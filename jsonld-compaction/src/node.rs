use crate::{Error, Options, add_value, compact_iri, compact_property, iri::keyword_alias};
use contextual::WithContext;
use jsonld_context_processing::{Options as ProcessingOptions, Process, ProcessingMode};
use jsonld_core::{Container, ContainerKind, Context, Id, Loader, Node, ParallelSafeVocabulary, Term, Type};
use jsonld_syntax::Keyword;
use mown::Mown;
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

fn optional_string(s: Option<&str>) -> jstrict::Value {
    s.map(|s| jstrict::Value::String(s.into())).unwrap_or(jstrict::Value::Null)
}

/// Compact the given indexed node.
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
    N: VocabularyMut + ParallelSafeVocabulary,
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
            active_context = previous_context
        }
    }

    // If the term definition for active property in active context has a local context:
    // FIXME https://github.com/w3c/json-ld-api/issues/502
    //       Seems that the term definition should be looked up in `type_scoped_context`.
    let mut active_context = Mown::Borrowed(active_context);
    if let Some(active_property) = active_property
        && let Some(active_property_definition) = type_scoped_context.get(active_property)
        && let Some(local_context) = active_property_definition.context()
    {
        active_context = Mown::Owned(
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
        )
    }

    // let inside_reverse = active_property == Some("@reverse");
    let mut result = jstrict::Object::default();

    if !node.types().is_empty() {
        // If element has an @type entry, create a new array compacted types initialized by
        // transforming each expanded type of that entry into its compacted form by IRI
        // compacting expanded type. Then, for each term in compacted types ordered
        // lexicographically:
        let mut compacted_types = Vec::new();
        for ty in node.types() {
            let compacted_ty = compact_iri(vocabulary, type_scoped_context, &ty.clone().into_term(), true, false, options)?;
            compacted_types.push(compacted_ty)
        }

        compacted_types.sort_by(|a, b| a.as_deref().cmp(&b.as_deref()));

        for term in compacted_types.iter().flatten() {
            if let Some(term_definition) = type_scoped_context.get(&**term)
                && let Some(local_context) = term_definition.context()
            {
                let processing_options = ProcessingOptions::from(options).without_propagation();
                active_context = Mown::Owned(
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
                )
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
        let id = id_entry.clone().into_term();

        if node.is_empty() {
            // This captures step 7:
            // If element has an @value or @id entry and the result of using the
            // Value Compaction algorithm, passing active context, active property,
            // and element as value is a scalar, or the term definition for active property
            // has a type mapping of @json, return that result.
            //
            // in the Value Compaction Algorithm, step 7:
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
                let compacted_value = compact_iri(vocabulary, active_context.as_ref(), &id, false, false, options)?;
                return Ok(optional_string(compacted_value.as_deref()));
            }

            // Otherwise, if the type mapping of active property is set to @vocab,
            // set result to the result of IRI compacting the value associated with the @id entry.
            if type_mapping == Some(&Type::Vocab) {
                let compacted_value = compact_iri(vocabulary, active_context.as_ref(), &id, true, false, options)?;
                return Ok(optional_string(compacted_value.as_deref()));
            }
        }

        // If expanded value is a string, then initialize compacted value by IRI
        // compacting expanded value with vocab set to false.
        let compacted_value = compact_iri(vocabulary, active_context.as_ref(), &id, false, false, options)?;

        // Initialize alias by IRI compacting expanded property.
        let alias = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Id);
        result.insert(alias.into(), optional_string(compacted_value.as_deref()));
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
            active_context = Mown::Owned(
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
            )
        }

        // NOTE: Plan's two-phase parallel branch for the `@reverse` property loop is
        // deferred. The merge step described in the plan (`add_value` per fragment
        // entry) does not preserve byte-equal output across all W3C test cases —
        // notably `@nest` sub-objects, `@index`/`@id`/`@type`/`@language` container
        // maps, and graph fragments where the per-property output is itself an
        // `Object` that must be deep-merged rather than appended. A correct merge
        // would require either (a) restructuring `compact_property` to emit a flat
        // operation log, or (b) inspecting the active_context per key to choose
        // recurse-vs-`add_value` per top-level entry. Both exceed the prototype
        // scope; sequential execution preserves correctness for the prototype
        // baseline. Loops below intentionally retain the sequential implementation.
        let mut reverse_result = jstrict::Object::default();
        for (expanded_property, expanded_value) in reverse_properties.iter() {
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
        for (property, mapped_value) in reverse_result.iter_mut() {
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
            let alias = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Index);

            // Add an entry alias to result whose value is set to expanded value and continue with the next expanded property.
            result.insert(alias.into(), index_entry.into());
        }
    }

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
        .await?
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
        .await?
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
        .await?
    }

    Ok(result.into())
}

/// Compact the given list of types into the given `result` compacted object.
fn compact_types<N, E>(
    vocabulary: &mut N,
    result: &mut jstrict::Object,
    types: Option<&[Id<N::Iri, N::BlankId>]>,
    active_context: &Context<N::Iri, N::BlankId>,
    type_scoped_context: &Context<N::Iri, N::BlankId>,
    options: Options,
) -> Result<(), Error<E>>
where
    N: VocabularyMut + ParallelSafeVocabulary,
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
            for ty in types.iter() {
                let ty = ty.clone().into_term();

                // Set term by IRI compacting expanded type using type-scoped context for active context.
                let compacted_ty = compact_iri(vocabulary, type_scoped_context, &ty, true, false, options)?;

                // Append term, to compacted value.
                compacted_value.push(optional_string(compacted_ty.as_deref()))
            }

            jstrict::Value::Array(compacted_value.into_iter().collect())
        };

        // Initialize alias by IRI compacting expanded property.
        let alias = keyword_alias(vocabulary, active_context, options, Keyword::Type);

        // Initialize as array to true if processing mode is json-ld-1.1 and the
        // container mapping for alias in the active context includes @set,
        // otherwise to the negation of compactArrays.
        let container_mapping = match active_context.get(alias) {
            Some(def) => def.container(),
            None => Container::None,
        };
        let as_array = (options.processing_mode == ProcessingMode::JsonLd1_1 && container_mapping.contains(ContainerKind::Set)) || !options.compact_arrays;

        // Use add value to add compacted value to the alias entry in result using as array.
        add_value(result, alias, compacted_value, as_array)
    }

    Ok(())
}
