use crate::{Error, IriConfusedWithPrefix, Options, add_value, compact_iri, compact_property, iri::keyword_alias};
use contextual::WithContext;
use jsonld_context_processing::{Options as ProcessingOptions, Process, ProcessingMode};
use jsonld_core::{Container, ContainerKind, Context, ContextRef, Id, Loader, Node, Term, Type};
use jsonld_syntax::Keyword;
use rdfx::vocabulary::{Vocabulary, VocabularyMut};
use std::hash::Hash;

/// Turns an optional string into a JSON string, or JSON `null` when absent.
///
/// IRI compaction returns `None` for a null term, which the specification
/// requires to be written out as `null` rather than omitted.
fn optional_string(s: Option<&str>) -> jstrict::Value {
    s.map_or(jstrict::Value::Null, |s| jstrict::Value::String(s.into()))
}

/// Compacts a node object, following the [compaction algorithm][1]'s node
/// object case.
///
/// Emits `@id`, `@type`, `@reverse`, `@index`, `@graph`, `@included` and every
/// ordinary property into a single JSON object, keyed by whatever each expanded
/// property compacts to. `index` is the `@index` the node was reached through,
/// and is dropped when the active property's container mapping already turned it
/// into a map key.
///
/// A node object consisting of nothing but an `@id` is a node reference and
/// is compacted by [`compact_node_reference`] once the active property's
/// scoped context is in force.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#compaction-algorithm
pub async fn compact_indexed_node_with<N, L>(
    vocabulary: &mut N,
    node: &Node<N::Iri, N::BlankId>,
    index: Option<&str>,
    mut active_context: &Context<N::Iri, N::BlankId>,
    type_scoped_context: &Context<N::Iri, N::BlankId>,
    active_property: Option<&str>,
    loader: &L,
    options: Options,
) -> Result<jstrict::Value, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    L: Loader,
{
    // If active context has a previous context, the active context is not propagated.
    // If element does not contain an @value entry, and element does not consist of
    // a single @id entry, set active context to previous context from active context,
    // as the scope of a term-scoped context does not apply when processing new node objects.
    if !(node.is_empty() && node.id.is_some()) {
        // does not consist of a single @id entry
        if let Some(previous_context) = active_context.previous_context() {
            active_context = previous_context;
        }
    }

    // If the term definition for active property in active context has a local context:
    //
    // Known deviation from the specification text, which says to look the term
    // definition up in the active context. This looks it up in
    // `type_scoped_context` instead, which is what makes the W3C compaction test
    // suite pass; the spec text is believed to be in error. See
    // https://github.com/w3c/json-ld-api/issues/502.
    let mut active_context = ContextRef::Borrowed(active_context);
    if let Some(active_property) = active_property
        && let Some(active_property_definition) = type_scoped_context.get(active_property)
        && let Some(local_context) = active_property_definition.context()
    {
        active_context = ContextRef::owned(
            local_context
                .process_with(
                    vocabulary,
                    active_context.as_ref(),
                    loader,
                    active_property_definition.base_url().cloned(),
                    ProcessingOptions::from(options).with_override(),
                )
                .await?
                .into_processed(),
        );
    }

    if node.is_empty()
        && let Some(id) = &node.id
    {
        return Ok(compact_node_reference(
            vocabulary,
            id,
            index,
            active_context.as_ref(),
            active_property,
            options,
        )?);
    }

    let mut result = jstrict::Object::default();

    if !node.types().is_empty() {
        // If element has an @type entry, create a new array compacted types initialized by
        // transforming each expanded type of that entry into its compacted form by IRI
        // compacting expanded type. Then, for each term in compacted types ordered
        // lexicographically:
        let mut compacted_types = Vec::new();
        for ty in node.types() {
            let compacted_ty = compact_iri(vocabulary, type_scoped_context, &ty.clone().into_term(), true, false, options)?;
            compacted_types.push(compacted_ty);
        }

        compacted_types.sort_by(|a, b| a.as_deref().cmp(&b.as_deref()));

        for term in compacted_types.iter().flatten() {
            if let Some(term_definition) = type_scoped_context.get(&**term)
                && let Some(local_context) = term_definition.context()
            {
                let processing_options = ProcessingOptions::from(options).without_propagation();
                active_context = ContextRef::owned(
                    local_context
                        .process_with(
                            vocabulary,
                            active_context.as_ref(),
                            loader,
                            term_definition.base_url().cloned(),
                            processing_options,
                        )
                        .await?
                        .into_processed(),
                );
            }
        }
    }

    // For each key expanded property and value expanded value in element, ordered
    // lexicographically by expanded property if ordered is true:
    let expanded_entries: Vec<_> = if options.ordered {
        // Decorate-sort-undecorate: cache `as_str()` so the comparator does
        // not call `with(vocabulary).as_str()` O(P log P) times. The cached
        // `&str` borrows from `entry.0` (which lives in `node.properties()`)
        // and `vocabulary`, both of which outlive `decorated`, so we avoid
        // the per-entry `to_string()` allocation.
        let vocabulary: &N = vocabulary;
        let mut decorated: Vec<(&str, _)> = node.properties().iter().map(|entry| (entry.0.with(vocabulary).as_str(), entry)).collect();
        decorated.sort_by(|a, b| a.0.cmp(b.0));
        decorated.into_iter().map(|(_, entry)| entry).collect()
    } else {
        node.properties().iter().collect()
    };

    // If expanded property is @id:
    if let Some(id_entry) = &node.id {
        insert_id(vocabulary, &mut result, id_entry, active_context.as_ref(), options)?;
    }

    compact_types(
        vocabulary,
        &mut result,
        node.types.as_deref(),
        active_context.as_ref(),
        type_scoped_context,
        options,
    )?;

    // If expanded property is @reverse:
    if let Some(reverse_properties) = node.reverse_properties_entry()
        && !reverse_properties.is_empty()
    {
        // Initialize compacted value to the result of using this algorithm recursively,
        // passing active context, @reverse for active property,
        // expanded value for element, and the compactArrays and ordered flags.
        let active_property = "@reverse";
        if let Some(active_property_definition) = active_context.get(active_property)
            && let Some(local_context) = active_property_definition.context()
        {
            active_context = ContextRef::owned(
                local_context
                    .process_with(
                        vocabulary,
                        active_context.as_ref(),
                        loader,
                        active_property_definition.base_url().cloned(),
                        ProcessingOptions::from(options).with_override(),
                    )
                    .await?
                    .into_processed(),
            );
        }

        // Every property compacts into one shared object, deliberately. Giving
        // each property its own object and merging afterwards — the obvious way
        // to make these iterations independent of one another — does not
        // reproduce the same output: `add_value` cannot merge the fragments
        // that `compact_property` produces for `@nest` sub-objects, for
        // `@index`/`@id`/`@type`/`@language` container maps, or for graph
        // fragments, because those are objects that have to be merged key by
        // key rather than appended. Doing it correctly would mean either
        // restructuring `compact_property` to emit a flat log of insertions, or
        // consulting the active context per key to decide between a recursive
        // merge and `add_value`.
        let mut reverse_result = jstrict::Object::default();
        for (expanded_property, expanded_value) in reverse_properties {
            compact_property(
                vocabulary,
                &mut reverse_result,
                expanded_property.clone().into(),
                expanded_value.iter(),
                active_context.as_ref(),
                loader,
                true,
                options,
            )
            .await?;
        }

        // For each property and value in compacted value:
        let mut reverse_map = jstrict::Object::default();
        for (property, mapped_value) in &mut reverse_result {
            let mut value = jstrict::Value::Null;
            std::mem::swap(&mut value, &mut *mapped_value);

            // If the term definition for property in the active context indicates that
            // property is a reverse property
            if let Some(term_definition) = active_context.get(property.as_str())
                && term_definition.reverse_property()
            {
                // Initialize as array to true if the container mapping for property in
                // the active context includes @set, otherwise the negation of compactArrays.
                let as_array = term_definition.container().contains(ContainerKind::Set) || !options.compact_arrays;

                // Use add value to add value to the property entry in result using as array.
                add_value(&mut result, property, value, as_array);
                continue;
            }

            reverse_map.insert(property.clone(), value);
        }

        if !reverse_map.is_empty() {
            // Initialize alias by IRI compacting @reverse.
            let alias = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Reverse);

            // Set the value of the alias entry of result to compacted value.
            result.insert(alias.into(), reverse_map.into());
        }
    }

    insert_index(vocabulary, &mut result, index, active_context.as_ref(), active_property, options);

    if let Some(graph_entry) = node.graph_entry() {
        compact_property(
            vocabulary,
            &mut result,
            Term::Keyword(Keyword::Graph),
            graph_entry.iter(),
            active_context.as_ref(),
            loader,
            false,
            options,
        )
        .await?;
    }

    for (expanded_property, expanded_value) in expanded_entries {
        compact_property(
            vocabulary,
            &mut result,
            expanded_property.clone().into(),
            expanded_value.iter(),
            active_context.as_ref(),
            loader,
            false,
            options,
        )
        .await?;
    }

    if let Some(included_entry) = node.included_entry() {
        compact_property(
            vocabulary,
            &mut result,
            Term::Keyword(Keyword::Included),
            included_entry.iter(),
            active_context.as_ref(),
            loader,
            false,
            options,
        )
        .await?;
    }

    Ok(result.into())
}

/// Compacts a node reference, a node object whose only entry is `@id`,
/// against an active context in which the active property's scoped context
/// is already in force.
///
/// Follows step 7 of the [value compaction algorithm][1]: under a term whose
/// type mapping is `@id` the reference becomes the `@id` compacted as an IRI
/// reference (`vocab` set to `false`), under a term whose type mapping is
/// `@vocab` it becomes the `@id` compacted with `vocab` set to `true`, and in
/// either case `index` is dropped. Otherwise the result is an object holding
/// the `@id`, compacted with `vocab` set to `false`, under the active
/// context's alias of `@id`, plus `index` under the alias of `@index` unless
/// the active property's container mapping includes `@index`.
///
/// `active_property` is the compacted term the reference is written under,
/// or `None` at the top level. When that term defines a scoped `@context`,
/// `active_context` must be the result of processing it on top of the
/// context the term was selected in; a node reference does not revert a
/// term-scoped context to the previous context, and type-scoped contexts do
/// not apply because a reference has no `@type`. This is synchronous and
/// needs no loader.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#value-compaction
///
/// # Errors
///
/// Returns [`IriConfusedWithPrefix`] when the `@id` can only be written out
/// in full and would then be read back as a compact IRI.
pub fn compact_node_reference<N>(
    vocabulary: &N,
    id: &Id<N::Iri, N::BlankId>,
    index: Option<&str>,
    active_context: &Context<N::Iri, N::BlankId>,
    active_property: Option<&str>,
    options: Options,
) -> Result<jstrict::Value, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    // This covers step 7 of the compaction algorithm:
    // If element has an @value or @id entry and the result of using the
    // Value Compaction algorithm, passing active context, active property,
    // and element as value is a scalar, or the term definition for active property
    // has a type mapping of @json, return that result.
    //
    // together with step 7 of the value compaction algorithm:
    // If value has an @id entry and has no other entries other than @index:
    //
    // If the type mapping of active property is set to @id,
    // set result to the result of IRI compacting the value associated with the
    // @id entry using false for vocab.
    let type_mapping = match active_property {
        Some(prop) => match active_context.get(prop) {
            Some(def) => def.typ(),
            None => None,
        },
        None => None,
    };

    if type_mapping == Some(&Type::Id) {
        let compacted_value = compact_iri(vocabulary, active_context, &id.clone().into_term(), false, false, options)?;
        return Ok(optional_string(compacted_value.as_deref()));
    }

    // Otherwise, if the type mapping of active property is set to @vocab,
    // set result to the result of IRI compacting the value associated with the @id entry.
    if type_mapping == Some(&Type::Vocab) {
        let compacted_value = compact_iri(vocabulary, active_context, &id.clone().into_term(), true, false, options)?;
        return Ok(optional_string(compacted_value.as_deref()));
    }

    let mut result = jstrict::Object::default();
    insert_id(vocabulary, &mut result, id, active_context, options)?;
    insert_index(vocabulary, &mut result, index, active_context, active_property, options);
    Ok(result.into())
}

/// Inserts a node's `@id`, compacted as an IRI reference, into `result` under
/// whatever `@id` compacts to.
fn insert_id<N>(
    vocabulary: &N,
    result: &mut jstrict::Object,
    id: &Id<N::Iri, N::BlankId>,
    active_context: &Context<N::Iri, N::BlankId>,
    options: Options,
) -> Result<(), IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    // If expanded value is a string, then initialize compacted value by IRI
    // compacting expanded value with vocab set to false.
    let compacted_value = compact_iri(vocabulary, active_context, &id.clone().into_term(), false, false, options)?;

    // Initialize alias by IRI compacting expanded property.
    let alias = keyword_alias(vocabulary, active_context, options, Keyword::Id);
    result.insert(alias.into(), optional_string(compacted_value.as_deref()));
    Ok(())
}

/// Inserts the `@index` a node was reached through into `result`, unless the
/// active property's container mapping includes `@index`.
fn insert_index<N>(
    vocabulary: &N,
    result: &mut jstrict::Object,
    index: Option<&str>,
    active_context: &Context<N::Iri, N::BlankId>,
    active_property: Option<&str>,
    options: Options,
) where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    // If expanded property is @index and active property has a container mapping in
    // active context that includes @index,
    if let Some(index_entry) = index {
        let mut index_container = false;
        if let Some(active_property) = active_property
            && let Some(active_property_definition) = active_context.get(active_property)
            && active_property_definition.container().contains(ContainerKind::Index)
        {
            // then the compacted result will be inside of an @index container,
            // drop the @index entry by continuing to the next expanded property.
            index_container = true;
        }

        if !index_container {
            // Initialize alias by IRI compacting expanded property.
            let alias = keyword_alias(vocabulary, active_context, options, Keyword::Index);

            // Add an entry alias to result whose value is set to expanded value and continue with the next expanded property.
            result.insert(alias.into(), index_entry.into());
        }
    }
}

/// Checks whether the `@type` values of a node object are written as an array
/// even when there is only one.
///
/// `type_alias` is what `@type` compacts to under `active_context`, as
/// [`keyword_alias`][crate::keyword_alias] returns it. Follows the *as array*
/// rule the [compaction algorithm][1] applies to `@type`: `true` when
/// processing JSON-LD 1.1 and the
/// alias's container mapping includes `@set`, or when `options.compact_arrays`
/// is off.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#compaction-algorithm
#[must_use]
pub fn types_as_array<I, B>(active_context: &Context<I, B>, type_alias: &str, options: Options) -> bool {
    // Initialize as array to true if processing mode is json-ld-1.1 and the
    // container mapping for alias in the active context includes @set,
    // otherwise to the negation of compactArrays.
    let container_mapping = match active_context.get(type_alias) {
        Some(def) => def.container(),
        None => Container::None,
    };
    (options.processing_mode == ProcessingMode::JsonLd1_1 && container_mapping.contains(ContainerKind::Set)) || !options.compact_arrays
}

/// Compacts a node object's `@type` values and inserts them into `result` under
/// whatever `@type` itself compacts to.
///
/// Each type is compacted against `type_scoped_context`, as the specification
/// requires, while the key for `@type` comes from `active_context`. A single type
/// becomes a bare string unless the term for `@type` has an `@set` container (in
/// JSON-LD 1.1) or `compact_arrays` is off.
fn compact_types<N, E>(
    vocabulary: &mut N,
    result: &mut jstrict::Object,
    types: Option<&[Id<N::Iri, N::BlankId>]>,
    active_context: &Context<N::Iri, N::BlankId>,
    type_scoped_context: &Context<N::Iri, N::BlankId>,
    options: Options,
) -> Result<(), Error<E>>
where
    N: VocabularyMut,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    // If expanded property is @type:
    if let Some(types) = types
        && !types.is_empty()
    {
        // If expanded value is a string,
        // then initialize compacted value by IRI compacting expanded value using
        // type-scoped context for active context.
        let compacted_value = if types.len() == 1 {
            let arc = compact_iri(vocabulary, type_scoped_context, &types[0].clone().into_term(), true, false, options)?;
            optional_string(arc.as_deref())
        } else {
            // Otherwise, expanded value must be a @type array:
            // Initialize compacted value to an empty array.
            let mut compacted_value = Vec::with_capacity(types.len());

            // For each item expanded type in expanded value:
            for ty in types {
                let ty = ty.clone().into_term();

                // Set term by IRI compacting expanded type using type-scoped context for active context.
                let compacted_ty = compact_iri(vocabulary, type_scoped_context, &ty, true, false, options)?;

                // Append term, to compacted value.
                compacted_value.push(optional_string(compacted_ty.as_deref()));
            }

            jstrict::Value::Array(compacted_value.into_iter().collect())
        };

        // Initialize alias by IRI compacting expanded property.
        let alias = keyword_alias(vocabulary, active_context, options, Keyword::Type);

        let as_array = types_as_array(active_context, alias, options);

        // Use add value to add compacted value to the alias entry in result using as array.
        add_value(result, alias, compacted_value, as_array);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::{
        CompactFragment,
        test_fixtures::{VALUE_CONTEXT, compact_one_value_document, expanded_object, json, processed_context},
    };
    use jsonld_core::NoLoader;
    use rdfx::vocabulary::{no_vocabulary, no_vocabulary_mut};

    /// Compacting one node reference on its own gives what the full compaction
    /// algorithm writes for it as the only value of a property, under `@id`-
    /// and `@vocab`-typed terms as well as under an untyped term.
    #[tokio::test]
    async fn compact_node_reference_matches_the_compaction_of_a_one_value_document() {
        let context = processed_context(VALUE_CONTEXT).await;

        // (property, expanded node reference, expected key, expected compacted value)
        let cases = [
            (
                "https://example.com/object",
                r#"{ "@id": "https://example.com/Building" }"#,
                "object",
                r#""ex:Building""#,
            ),
            (
                "https://example.com/objectVocab",
                r#"{ "@id": "https://example.com/Building" }"#,
                "objectVocab",
                r#""Building""#,
            ),
            (
                "https://example.com/objectVocab",
                r#"{ "@id": "https://vocab.example/Thing" }"#,
                "objectVocab",
                r#""Thing""#,
            ),
            (
                "https://example.com/text",
                r#"{ "@id": "urn:ngsi-ld:Thing:1" }"#,
                "text",
                r#"{ "id": "urn:ngsi-ld:Thing:1" }"#,
            ),
            ("https://example.com/text", r#"{ "@id": "_:b0" }"#, "text", r#"{ "id": "_:b0" }"#),
            ("https://example.com/object", r#"{ "@id": "urn:x", "@index": "i" }"#, "object", r#""urn:x""#),
            (
                "https://example.com/text",
                r#"{ "@id": "urn:x", "@index": "i" }"#,
                "text",
                r#"{ "id": "urn:x", "@index": "i" }"#,
            ),
        ];

        for (property, reference, expected_key, expected) in cases {
            let (key, written) = compact_one_value_document(&context, property, reference).await;
            assert_eq!(key, expected_key, "key of {reference}");

            let object = expanded_object(reference);
            let compacted = compact_node_reference(no_vocabulary(), object.id().unwrap(), object.index(), &context, Some(&key), Options::default()).unwrap();
            assert_eq!(compacted, json(expected), "compaction of {reference} under {key}");
            assert_eq!(compacted, written, "{reference} compacted alone and in a document");
        }
    }

    /// A single `@type` is written bare unless the term `@type` compacts to has
    /// an `@set` container.
    #[tokio::test]
    async fn types_as_array_follows_the_container_of_the_type_alias() {
        let node = expanded_object(r#"{ "@id": "urn:x", "@type": ["https://example.com/Building"] }"#);
        let cases = [
            (
                r#"{ "type": "@type", "ex": "https://example.com/" }"#,
                false,
                r#"{ "@id": "urn:x", "type": "ex:Building" }"#,
            ),
            (
                r#"{ "type": { "@id": "@type", "@container": "@set" }, "ex": "https://example.com/" }"#,
                true,
                r#"{ "@id": "urn:x", "type": ["ex:Building"] }"#,
            ),
        ];

        for (source, as_array, expected) in cases {
            let context = processed_context(source).await;
            let alias = keyword_alias(no_vocabulary(), &context, Options::default(), Keyword::Type);
            assert_eq!(types_as_array(&context, alias, Options::default()), as_array, "{source}");

            let compacted = node
                .compact_fragment_full(no_vocabulary_mut(), &context, &context, None, &NoLoader, Options::default())
                .await
                .unwrap();
            assert_eq!(compacted, json(expected), "{source}");
        }
    }
}
