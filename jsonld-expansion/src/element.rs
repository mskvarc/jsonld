use crate::{
    Error,
    Expanded,
    GivenLiteralValue,
    LiteralValue,
    Loader,
    Options,
    Warning,
    WarningHandler,
    expand_array,
    expand_iri,
    expand_literal,
    expand_node,
    expand_value,
};
use jsonld_context_processing::{Options as ProcessingOptions, Process, ProcessingCache};
use jsonld_core::{Context, ContextRef, Environment, Id, Indexed, Object, ProcessingMode, Term, ValidId, object};
use jsonld_syntax::{Keyword, Nullable};
use jstrict::{Value, object::Entry};
use rdfx::vocabulary::VocabularyMut;
use smallvec::SmallVec;
use std::{borrow::Cow, hash::Hash, sync::Arc};

/// Entry of a JSON object whose key has been expanded: the key as written, the
/// term it expanded to, and the value left untouched.
pub(crate) struct ExpandedEntry<'a, T, B>(pub &'a str, pub Arc<Term<T, B>>, pub &'a Value);

/// Property whose value is being expanded, as written in the input document.
///
/// It is `None` at the top level of a document, where the element being
/// expanded is not the value of any property.
pub(crate) enum ActiveProperty<'a> {
    Some(&'a str),
    None,
}

impl<'a> ActiveProperty<'a> {
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns the definition of this property in the given context, if the
    /// context defines it.
    pub fn get_from<'c, T, B>(&self, context: &'c Context<T, B>) -> Option<jsonld_core::context::TermDefinitionRef<'c, T, B>> {
        match self {
            Self::Some(s) => context.get(*s),
            Self::None => None,
        }
    }
}

impl<'a> Clone for ActiveProperty<'a> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> Copy for ActiveProperty<'a> {}

impl<'a> PartialEq<Keyword> for ActiveProperty<'a> {
    fn eq(&self, other: &Keyword) -> bool {
        match self {
            Self::Some(s) => *s == other.into_str(),
            _ => false,
        }
    }
}

/// Result of the expansion of a single element in a JSON-LD document.
pub(crate) type ElementExpansionResult<T, B, E> = Result<Expanded<T, B>, Error<E>>;

/// Expands one element of a JSON-LD document — a value, an array or a map —
/// against the given active context.
///
/// Implements the [Expansion
/// algorithm](https://www.w3.org/TR/json-ld11-api/#expansion-algorithm) and is
/// called recursively for every nested element. `from_map` tells whether this
/// element is the value of an entry of an index, id, type or language map,
/// which suspends the reverting to a previous context; the specification
/// defaults it to `false`.
pub(crate) async fn expand_element<'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a Context<N::Iri, N::BlankId>,
    active_property: ActiveProperty<'a>,
    element: &'a Value,
    base_url: Option<&'a N::Iri>,
    options: Options,
    from_map: bool,
    cache: Option<&'a ProcessingCache<N::Iri, N::BlankId>>,
) -> ElementExpansionResult<N::Iri, N::BlankId, L::Error>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // If `element` is null, return null.
    if element.is_null() {
        return Ok(Expanded::Null);
    }

    let active_property_definition = active_property.get_from(active_context);

    // If `active_property` has a term definition in `active_context` with a local context,
    // initialize property-scoped context to that local context.
    let mut property_scoped_base_url = None;
    let property_scoped_context = if let Some(definition) = active_property_definition {
        if let Some(base_url) = definition.base_url() {
            property_scoped_base_url = Some(base_url.clone());
        }

        definition.context()
    } else {
        None
    };

    match element {
        // A null element already returned at the top of this function; this arm
        // only keeps the match total, without a panic.
        Value::Null => Ok(Expanded::Null),
        Value::Array(element) => {
            expand_array(
                env,
                active_context,
                active_property,
                active_property_definition,
                element,
                base_url,
                options,
                from_map,
                cache,
            )
            .await
        }

        Value::Object(element) => {
            // Otherwise `element` is a map.
            //
            // If `active_context` has a `previous_context`, the active context
            // is not propagated: revert to `previous_context`, unless this map
            // comes from a map expansion (`from_map`), contains an entry
            // expanding to `@value`, or consists of a single entry expanding to
            // `@id`. A term-scoped context does not reach into a new node
            // object. Deciding this means IRI-expanding every key, so the two
            // entry checks are only computed when there is a
            // `previous_context` to revert to.
            let mut active_context = ContextRef::Borrowed(active_context);
            if !from_map && active_context.previous_context().is_some() {
                let mut has_value_entry = false;
                let mut has_id_entry = false;
                for Entry { key, value: _ } in element.entries() {
                    match expand_iri(
                        &mut env,
                        active_context.as_ref(),
                        Nullable::Some(key.as_str().into()),
                        false,
                        Some(options.policy.vocab),
                    )?
                    .as_deref()
                    {
                        Some(Term::Keyword(Keyword::Value)) => {
                            has_value_entry = true;
                        }
                        Some(Term::Keyword(Keyword::Id)) => has_id_entry = true,
                        _ => (),
                    }
                }
                if !(has_value_entry || element.len() == 1 && has_id_entry)
                    && let Some(previous_context) = active_context.previous_context()
                {
                    active_context = ContextRef::owned(previous_context.clone());
                }
            }

            // If `property_scoped_context` is defined, set `active_context` to the result of
            // the Context Processing algorithm, passing `active_context`,
            // `property_scoped_context` as `local_context`, `base_url` from the term
            // definition for `active_property`, in `active_context` and `true` for
            // `override_protected`.
            if let Some(property_scoped_context) = property_scoped_context {
                let options: ProcessingOptions = options.into();
                let processed = match cache {
                    Some(cache) => Box::pin(property_scoped_context.process_full_with_cache(
                        env.vocabulary,
                        active_context.as_ref(),
                        env.loader,
                        property_scoped_base_url,
                        options.with_override(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                        cache,
                    ))
                    .await?
                    .into_processed(),
                    None => Box::pin(property_scoped_context.process_full(
                        env.vocabulary,
                        active_context.as_ref(),
                        env.loader,
                        property_scoped_base_url,
                        options.with_override(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                    ))
                    .await?
                    .into_processed(),
                };
                active_context = ContextRef::owned(processed);
            }

            // If `element` contains the entry `@context`, set `active_context` to the result
            // of the Context Processing algorithm, passing `active_context`, the value of the
            // `@context` entry as `local_context` and `base_url`.
            if let Some(local_context) = element.get_unique("@context").map_err(Error::duplicate_key_ref)? {
                use jsonld_syntax::TryFromJson;
                let local_context = jsonld_syntax::context::Context::try_from_json(local_context)?;

                let processed = match cache {
                    Some(cache) => Box::pin(local_context.process_full_with_cache(
                        env.vocabulary,
                        active_context.as_ref(),
                        env.loader,
                        base_url.cloned(),
                        options.into(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                        cache,
                    ))
                    .await?
                    .into_processed(),
                    None => Box::pin(local_context.process_full(
                        env.vocabulary,
                        active_context.as_ref(),
                        env.loader,
                        base_url.cloned(),
                        options.into(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                    ))
                    .await?
                    .into_processed(),
                };
                active_context = ContextRef::owned(processed);
            }

            let entries: Cow<[Entry]> = if options.ordered {
                Cow::Owned(element.entries().to_vec())
            } else {
                Cow::Borrowed(element.entries())
            };

            // Single sweep over the entries: expand each key with the current
            // active context, build `expanded_entries`, and record the index of
            // every entry whose key expanded to `@type`. The specification
            // walks the entries twice here, once to collect the `@type` entries
            // driving type-scoped context processing and once to expand every
            // key; one pass plus a list of indices does the same work.
            let mut expanded_entries: Vec<ExpandedEntry<N::Iri, N::BlankId>> = Vec::with_capacity(entries.len());
            let mut type_indices: Vec<usize> = Vec::new();
            for Entry { key, value } in entries.iter() {
                if key.is_empty() {
                    env.warnings.handle(env.vocabulary, Warning::EmptyTerm);
                }
                let expanded_key = expand_iri(
                    &mut env,
                    active_context.as_ref(),
                    Nullable::Some(key.as_str().into()),
                    false,
                    Some(options.policy.vocab),
                )?;
                if let Some(expanded_key) = expanded_key {
                    if let Term::Keyword(Keyword::Type) = expanded_key.as_ref() {
                        type_indices.push(expanded_entries.len());
                    }
                    expanded_entries.push(ExpandedEntry(key, expanded_key, value));
                }
            }

            type_indices.sort_unstable_by(|&a, &b| expanded_entries[a].0.cmp(expanded_entries[b].0));

            // Initialize `type_scoped_context` to `active_context`.
            let type_scoped_context = active_context.as_ref();
            let mut active_context = ContextRef::Borrowed(active_context.as_ref());

            // For each entry whose key IRI-expands to `@type`, sorted
            // lexicographically by key, walk the `@type` values in
            // lexicographic order too and apply the type-scoped context of each
            // of them, if any, to `active_context`.
            for &i in &type_indices {
                let value = Value::force_as_array(expanded_entries[i].2);
                let mut sorted_value: SmallVec<[&str; 4]> = SmallVec::with_capacity(value.len());
                for term in value {
                    if let Some(s) = term.as_string() {
                        sorted_value.push(s);
                    }
                }
                sorted_value.sort_unstable();
                for term in sorted_value {
                    if let Some(term_definition) = type_scoped_context.get(term)
                        && let Some(local_context) = term_definition.context()
                    {
                        let base_url = term_definition.base_url().cloned();
                        let options: ProcessingOptions = options.into();
                        let processed = match cache {
                            Some(cache) => Box::pin(local_context.process_full_with_cache(
                                env.vocabulary,
                                active_context.as_ref(),
                                env.loader,
                                base_url,
                                options.without_propagation(),
                                crate::warning::ContextWarnings(&mut *env.warnings),
                                cache,
                            ))
                            .await?
                            .into_processed(),
                            None => Box::pin(local_context.process_full(
                                env.vocabulary,
                                active_context.as_ref(),
                                env.loader,
                                base_url,
                                options.without_propagation(),
                                crate::warning::ContextWarnings(&mut *env.warnings),
                            ))
                            .await?
                            .into_processed(),
                        };
                        active_context = ContextRef::owned(processed);
                    }
                }
            }

            // `input_type`: expansion of the last value of the lexicographically first
            // `@type` entry, IRI-expanded with the (possibly type-scoped) active context.
            let input_type = if let Some(&i) = type_indices.first() {
                let value = Value::force_as_array(expanded_entries[i].2);
                if let Some(input_type) = value.last() {
                    input_type
                        .as_string()
                        .map(|input_type_str| {
                            expand_iri(
                                &mut env,
                                active_context.as_ref(),
                                Nullable::Some(input_type_str.into()),
                                false,
                                Some(options.policy.vocab),
                            )
                        })
                        .transpose()?
                        .flatten()
                } else {
                    None
                }
            } else {
                None
            };

            // Type-scoped processing replaces `active_context` with an owned
            // one, under which a key may expand differently: the expansions
            // recorded during the single sweep above are then stale and have to
            // be redone.
            if active_context.is_owned() {
                expanded_entries.clear();
                for Entry { key, value } in entries.iter() {
                    let expanded_key = expand_iri(
                        &mut env,
                        active_context.as_ref(),
                        Nullable::Some(key.as_str().into()),
                        false,
                        Some(options.policy.vocab),
                    )?;
                    if let Some(expanded_key) = expanded_key {
                        expanded_entries.push(ExpandedEntry(key, expanded_key, value));
                    }
                }
            }

            // Detect list/set/value entries and emit blank-node-id warnings from the
            // final `expanded_entries`.
            let mut list_entry: Option<&Value> = None;
            let mut set_entry: Option<&Value> = None;
            let mut value_entry: Option<&Value> = None;
            for ExpandedEntry(_, expanded_key, value) in expanded_entries.iter() {
                match expanded_key.as_ref() {
                    Term::Keyword(Keyword::Value) => value_entry = Some(*value),
                    Term::Keyword(Keyword::List) if active_property.is_some() && active_property != Keyword::Graph => list_entry = Some(*value),
                    Term::Keyword(Keyword::Set) => set_entry = Some(*value),
                    Term::Id(Id::Valid(ValidId::Blank(id))) => {
                        env.warnings.handle(env.vocabulary, Warning::BlankNodeIdProperty(id.clone()));
                    }
                    _ => (),
                }
            }

            if let Some(list_entry) = list_entry {
                // List objects.
                let mut index = None;
                for ExpandedEntry(_, expanded_key, value) in expanded_entries {
                    match expanded_key.as_ref() {
                        Term::Keyword(Keyword::Index) => match value.as_string() {
                            Some(value) => index = Some(value.to_string()),
                            None => return Err(Error::InvalidIndexValue),
                        },
                        Term::Keyword(Keyword::List) => (),
                        _ => return Err(Error::InvalidSetOrListObject),
                    }
                }

                // Initialize expanded value to the result of using this algorithm
                // recursively passing active context, active property, value for element,
                // base URL, and the ordered flags, ensuring that the
                // result is an array.
                let mut result = Vec::new();
                let list_entry = Value::force_as_array(list_entry);
                for item in list_entry {
                    let e = Box::pin(expand_element(
                        Environment {
                            vocabulary: env.vocabulary,
                            loader: env.loader,
                            warnings: env.warnings,
                        },
                        active_context.as_ref(),
                        active_property,
                        item,
                        base_url,
                        options,
                        false,
                        cache,
                    ))
                    .await?;
                    result.extend(e)
                }

                // JSON-LD 1.0 forbids a list object among the items of another
                // list object; 1.1 lifted the restriction (`expand#ter32`).
                if options.processing_mode == ProcessingMode::JsonLd1_0 && result.iter().any(|item| item.is_list()) {
                    return Err(Error::ListOfLists);
                }

                Ok(Expanded::Object(Indexed::new(Object::List(object::List::new(result)), index)))
            } else if let Some(set_entry) = set_entry {
                // Set objects.
                for ExpandedEntry(_, expanded_key, _) in expanded_entries {
                    match expanded_key.as_ref() {
                        Term::Keyword(Keyword::Index) => {
                            // An `@index` entry alongside `@set` is tolerated,
                            // but ignored: a set has no order to index.
                        }
                        Term::Keyword(Keyword::Set) => (),
                        _ => return Err(Error::InvalidSetOrListObject),
                    }
                }

                // set expanded value to the result of using this algorithm recursively,
                // passing active context, active property, value for element, base URL,
                // and ordered flags.
                Box::pin(expand_element(
                    env,
                    active_context.as_ref(),
                    active_property,
                    set_entry,
                    base_url,
                    options,
                    false,
                    cache,
                ))
                .await
            } else if let Some(value_entry) = value_entry {
                // Value objects.
                let expanded_value = expand_value(
                    &mut env,
                    options.policy.vocab,
                    options.processing_mode,
                    input_type.as_deref(),
                    type_scoped_context,
                    expanded_entries,
                    value_entry,
                )?;

                if let Some(value) = expanded_value {
                    Ok(Expanded::Object(value))
                } else {
                    Ok(Expanded::Null)
                }
            } else {
                // Node objects.
                let e = Box::pin(expand_node(
                    env,
                    active_context.as_ref(),
                    type_scoped_context,
                    active_property,
                    expanded_entries,
                    base_url,
                    options,
                    cache,
                ))
                .await?;
                if let Some(result) = e {
                    Ok(Expanded::Object(result.cast::<Object<N::Iri, N::BlankId>>()))
                } else {
                    Ok(Expanded::Null)
                }
            }
        }

        _ => {
            // Otherwise `element` is a scalar: a boolean, a number or a string
            // (`null` returned at the top of this function).

            // If `active_property` is `null` or `@graph`, the scalar describes
            // nothing: drop it by returning null.
            if active_property.is_none() || active_property == Keyword::Graph {
                return Ok(Expanded::Null);
            }

            // If `property_scoped_context` is defined, set `active_context` to the result of the
            // Context Processing algorithm, passing `active_context`, `property_scoped_context` as
            // local context, and `base_url` from the term definition for `active_property` in
            // `active context`.
            let active_context = if let Some(property_scoped_context) = property_scoped_context {
                // A property-scoped context only exists when `active_property`
                // has a term definition, so `property_scoped_base_url` is the
                // base URL of that very definition, as the algorithm requires.
                let base_url = property_scoped_base_url;

                let result = match cache {
                    Some(cache) => Box::pin(property_scoped_context.process_full_with_cache(
                        env.vocabulary,
                        active_context,
                        env.loader,
                        base_url,
                        options.into(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                        cache,
                    ))
                    .await?
                    .into_processed(),
                    None => Box::pin(property_scoped_context.process_full(
                        env.vocabulary,
                        active_context,
                        env.loader,
                        base_url,
                        options.into(),
                        crate::warning::ContextWarnings(&mut *env.warnings),
                    ))
                    .await?
                    .into_processed(),
                };
                ContextRef::owned(result)
            } else {
                ContextRef::Borrowed(active_context)
            };

            // Return the result of the Value Expansion algorithm, passing the `active_context`,
            // `active_property`, and `element` as value.
            Ok(Expanded::Object(expand_literal(
                env,
                options.policy.vocab,
                active_context.as_ref(),
                active_property,
                LiteralValue::Given(GivenLiteralValue::new(element)?),
            )?))
        }
    }
}
