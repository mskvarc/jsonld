use crate::{
    Action,
    ActiveProperty,
    Error,
    Expanded,
    ExpandedEntry,
    LiteralValue,
    Options,
    Warning,
    WarningHandler,
    expand_element,
    expand_iri,
    expand_literal,
    filter_top_level_item,
};
use contextual::WithContext;
use jsonld_context_processing::{Options as ProcessingOptions, Process, ProcessingCache};
use jsonld_core::{
    Container,
    Context,
    ContextRef,
    Environment,
    Id,
    Indexed,
    IndexedObject,
    LangString,
    Loader,
    Node,
    Object,
    ProcessingMode,
    Term,
    Type,
    Value,
    object,
    object::value::Literal,
};
use jsonld_syntax::{ContainerKind, Keyword, LenientLangTagBuf, Nullable};
use jstrict::object::Entry;
use rdfx::vocabulary::VocabularyMut;
use smallvec::SmallVec;
use std::{hash::Hash, sync::Arc};

/// Turns an expanded term into a node identifier.
///
/// Returns `None` for `Term::Null`, which is what a null value or a
/// keyword-like string expands to. A keyword becomes an invalid identifier
/// holding the keyword itself, which the key expansion
/// [`Policy`](crate::Policy) then decides to keep, drop or reject.
pub(crate) fn node_id_of_term<T: Clone, B: Clone>(term: Arc<Term<T, B>>) -> Option<Id<T, B>> {
    let term = Arc::try_unwrap(term).unwrap_or_else(|a| (*a).clone());
    match term {
        Term::Null => None,
        Term::Id(prop) => Some(prop),
        Term::Keyword(kw) => Some(Id::Invalid(kw.into_str().to_string())),
    }
}

/// Expands a node object: a map that is neither a value, list nor set object,
/// and whose entries have already been expanded.
///
/// Returns `None` when the node is dropped: when its entries expanded to
/// nothing, or when it is free-floating and carries no more than an identifier.
pub(crate) async fn expand_node<'a, N, L, W>(
    env: Environment<'a, N, L, W>,
    active_context: &'a Context<N::Iri, N::BlankId>,
    type_scoped_context: &'a Context<N::Iri, N::BlankId>,
    active_property: ActiveProperty<'a>,
    expanded_entries: Vec<ExpandedEntry<'a, N::Iri, N::BlankId>>,
    base_url: Option<&'a N::Iri>,
    options: Options,
    cache: Option<&'a ProcessingCache<N::Iri, N::BlankId>>,
) -> Result<Option<Indexed<Node<N::Iri, N::BlankId>>>, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // The specification initializes two empty maps here, `result` and `nests`.
    // No `nests` map is needed: the `@nest` entries are expanded into `result`
    // as they are met, in the `Keyword::Nest` arm of `expand_node_entries`.
    let (result, has_value_object_entries) = Box::pin(expand_node_entries(
        env,
        Indexed::new(Node::new(), None),
        false,
        active_context,
        type_scoped_context,
        active_property,
        expanded_entries,
        base_url,
        options,
        cache,
    ))
    .await?;

    // The specification post-processes a result carrying `@value`, `@type`,
    // `@set` or `@list` entries here. None of it applies: `result` is a
    // `Node`, a type that cannot hold `@value`, `@set` or `@list` entries —
    // maps with those entries never reach this function — and whose `@type`
    // values are already stored as a list.

    // The specification drops a result whose only entry is `@language`. Here
    // that is a node which saw an `@language` or `@direction` entry — neither
    // of which a node object can carry — and expanded to nothing else.
    if has_value_object_entries && result.is_empty() && result.id.is_none() {
        return Ok(None);
    }

    // If active property is null or @graph, drop free-floating
    // values as follows:
    if active_property.is_none() || active_property == Keyword::Graph {
        // If `result` is a map which is empty, set `result` to null. The
        // specification also covers a map containing only the entries `@value`
        // or `@list`, which a node object never does.
        // Otherwise, if result is a map whose only entry is @id, set result to null.
        if result.is_empty() && result.index().is_none() {
            // `Node::is_empty` ignores `@id`, so both cases hold at once: an
            // empty node, and a node carrying nothing but an identifier.
            return Ok(None);
        }
    }

    Ok(Some(result))
}

/// Node being expanded, paired with a flag set to `true` when the map it comes
/// from had an entry only a value object may carry, namely `@language` or
/// `@direction`.
type ExpandedNode<T, B> = (Indexed<Node<T, B>>, bool);

/// Result of expanding the entries of a node object.
type NodeEntriesExpensionResult<T, B, E> = Result<ExpandedNode<T, B>, Error<E>>;

/// Expands the given entries into `result`, one entry at a time.
///
/// Takes the node under construction and returns it, so that the `@nest` arm
/// can call this function recursively and have the nested entries land in the
/// same node.
async fn expand_node_entries<'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    mut result: Indexed<Node<N::Iri, N::BlankId>>,
    mut has_value_object_entries: bool,
    active_context: &'a Context<N::Iri, N::BlankId>,
    type_scoped_context: &'a Context<N::Iri, N::BlankId>,
    active_property: ActiveProperty<'a>,
    expanded_entries: Vec<ExpandedEntry<'a, N::Iri, N::BlankId>>,
    base_url: Option<&'a N::Iri>,
    options: Options,
    cache: Option<&'a ProcessingCache<N::Iri, N::BlankId>>,
) -> NodeEntriesExpensionResult<N::Iri, N::BlankId, L::Error>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // For each `key` and `value` in `element`, ordered lexicographically by key
    // if `ordered` is `true`:
    for ExpandedEntry(key, expanded_key, value) in expanded_entries {
        // The active context holds references of its own to this term, in its
        // term definitions and in its term-resolution cache, so
        // `Arc::try_unwrap` would fail and clone the whole term for every key —
        // one allocation per key under an `IriBuf` vocabulary. Match through
        // the `Arc` instead, and clone only where an owned identifier is
        // required.
        //
        // The specification IRI-expands `key` into `expanded_property` at this
        // point, then drops the key when `expanded_property` is `null` or
        // neither contains a colon (`:`) nor is a keyword. Here the keys arrive
        // already expanded: those that expanded to nothing were left out by the
        // caller, those that expanded to `Term::Null` are dropped by the first
        // arm below, and those that expanded to an identifier without a colon by
        // the last one.
        match &*expanded_key {
            Term::Null => (),

            // If key is @context, continue to the next key.
            Term::Keyword(Keyword::Context) => (),

            // If `expanded_property` is a keyword:
            Term::Keyword(expanded_property) => {
                let expanded_property = *expanded_property;
                // If `active_property` equals `@reverse`, an invalid reverse property
                // map error has been detected and processing is aborted.
                if active_property == Keyword::Reverse {
                    return Err(Error::InvalidReversePropertyMap);
                }

                // If `result` already has an `expanded_property` entry, other than
                // `@included` or `@type` (unless processing mode is json-ld-1.0), a
                // colliding keywords error has been detected and processing is
                // aborted.
                if (options.processing_mode == ProcessingMode::JsonLd1_0 || (expanded_property != Keyword::Included && expanded_property != Keyword::Type))
                    && result.has_key(&Term::Keyword(expanded_property))
                {
                    return Err(Error::CollidingKeywords);
                }

                match expanded_property {
                    // If `expanded_property` is @id:
                    Keyword::Id => {
                        // If `value` is not a string, an invalid @id value error has
                        // been detected and processing is aborted.
                        if let Some(str_value) = value.as_str() {
                            // Otherwise, set `expanded_value` to the result of IRI
                            // expanding value using true for document relative and
                            // false for vocab.
                            result.id = expand_iri(&mut env, active_context, Nullable::Some(str_value.into()), true, None)?.and_then(node_id_of_term);
                        } else {
                            return Err(Error::InvalidIdValue);
                        }
                    }
                    // If expanded property is @type:
                    Keyword::Type => {
                        // If value is neither a string nor an array of strings, an
                        // invalid type value error has been detected and processing
                        // is aborted.
                        let value = jstrict::Value::force_as_array(value);
                        // Set `expanded_value` to the result of IRI expanding each
                        // of its values using `type_scoped_context` for active
                        // context, and true for document relative.
                        for ty in value {
                            if let Some(str_ty) = ty.as_str() {
                                if let Some(ty_arc) =
                                    expand_iri(&mut env, type_scoped_context, Nullable::Some(str_ty.into()), true, Some(options.policy.vocab))?
                                {
                                    let ty = Arc::try_unwrap(ty_arc).unwrap_or_else(|a| (*a).clone());
                                    if let Ok(ty) = ty.try_into() {
                                        if let Id::Invalid(_) = &ty {
                                            match options.policy.invalid {
                                                Action::Keep => (),
                                                Action::Drop => continue,
                                                Action::Reject => {
                                                    return Err(Error::InvalidTypeValue);
                                                }
                                            }
                                        }

                                        result.types_mut_or_default().push(ty);
                                    } else {
                                        return Err(Error::InvalidTypeValue);
                                    }
                                }
                            } else {
                                return Err(Error::InvalidTypeValue);
                            }
                        }
                    }
                    // If expanded property is @graph
                    Keyword::Graph => {
                        // Set `expanded_value` to the result of using this algorithm
                        // recursively passing `active_context`, `@graph` for active
                        // property, `value` for element, `base_url`, and the
                        // `frame_expansion` and `ordered` flags, ensuring that
                        // `expanded_value` is an array of one or more maps.
                        let expanded_value = Box::pin(expand_element(
                            Environment {
                                vocabulary: env.vocabulary,
                                loader: env.loader,
                                warnings: env.warnings,
                            },
                            active_context,
                            ActiveProperty::Some("@graph"),
                            value,
                            base_url,
                            options,
                            false,
                            cache,
                        ))
                        .await?;

                        result.set_graph_entry(Some(expanded_value.into_iter().filter(filter_top_level_item).collect()));
                    }
                    // If expanded property is @included:
                    Keyword::Included => {
                        // If processing mode is json-ld-1.0, continue with the next
                        // key from element.
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            continue;
                        }

                        // Set `expanded_value` to the result of using this algorithm
                        // recursively passing `active_context`, `active_property`,
                        // `value` for element, `base_url`, and the `frame_expansion`
                        // and `ordered` flags, ensuring that the result is an array.
                        let expanded_value = Box::pin(expand_element(
                            Environment {
                                vocabulary: env.vocabulary,
                                loader: env.loader,
                                warnings: env.warnings,
                            },
                            active_context,
                            ActiveProperty::Some("@included"),
                            value,
                            base_url,
                            options,
                            false,
                            cache,
                        ))
                        .await?;
                        let mut expanded_nodes = Vec::new();
                        for obj in expanded_value {
                            match obj.try_cast::<Node<N::Iri, N::BlankId>>() {
                                Ok(node) => expanded_nodes.push(node),
                                Err(_) => {
                                    return Err(Error::InvalidIncludedValue);
                                }
                            }
                        }

                        if let Some(included) = result.included_entry_mut() {
                            included.extend(expanded_nodes);
                        } else {
                            result.set_included(Some(expanded_nodes.into_iter().collect()));
                        }
                    }
                    // A node object has nowhere to put `@language` or
                    // `@direction`, both of which belong to value objects. Only
                    // record that one was seen, so that `expand_node` can drop
                    // the node if it holds nothing else.
                    // If expanded property is @language:
                    Keyword::Language => has_value_object_entries = true,
                    // If expanded property is @direction:
                    Keyword::Direction => has_value_object_entries = true,
                    // If expanded property is @index:
                    Keyword::Index => {
                        if let Some(index) = value.as_str() {
                            result.set_index(Some(index.to_string()));
                        } else {
                            // If value is not a string, an invalid @index value
                            // error has been detected and processing is aborted.
                            return Err(Error::InvalidIndexValue);
                        }
                    }
                    // If expanded property is @reverse:
                    Keyword::Reverse => {
                        // If value is not a map, an invalid @reverse value error
                        // has been detected and processing is aborted.
                        if let Some(value) = value.as_object() {
                            let mut reverse_entries: SmallVec<[&Entry; 8]> = value.iter().collect();

                            if options.ordered {
                                reverse_entries.sort_by_key(|entry| &entry.key);
                            }

                            for Entry {
                                key: reverse_key,
                                value: reverse_value,
                            } in reverse_entries
                            {
                                let reverse_expanded_arc = expand_iri(
                                    &mut env,
                                    active_context,
                                    Nullable::Some(reverse_key.as_str().into()),
                                    false,
                                    Some(options.policy.vocab),
                                )?;
                                let reverse_expanded = reverse_expanded_arc.map(|arc| Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone()));
                                match reverse_expanded {
                                    Some(Term::Keyword(_)) => {
                                        return Err(Error::InvalidReversePropertyMap);
                                    }
                                    Some(Term::Id(reverse_prop)) if reverse_prop.with(&*env.vocabulary).as_str().contains(':') => {
                                        if !reverse_prop.is_valid() {
                                            match options.policy.invalid {
                                                Action::Keep => (),
                                                Action::Drop => continue,
                                                Action::Reject => {
                                                    return Err(Error::KeyExpansionFailed(reverse_key.to_string()));
                                                }
                                            }
                                        }

                                        let reverse_expanded_value = Box::pin(expand_element(
                                            Environment {
                                                vocabulary: env.vocabulary,
                                                loader: env.loader,
                                                warnings: env.warnings,
                                            },
                                            active_context,
                                            ActiveProperty::Some(reverse_key.as_ref()),
                                            reverse_value,
                                            base_url,
                                            options,
                                            false,
                                            cache,
                                        ))
                                        .await?;

                                        let is_double_reversed = if let Some(reverse_key_definition) = active_context.get(reverse_key.as_str()) {
                                            reverse_key_definition.reverse_property()
                                        } else {
                                            false
                                        };

                                        if is_double_reversed {
                                            result.insert_all(reverse_prop, reverse_expanded_value.into_iter());
                                        } else {
                                            let mut reverse_expanded_nodes = Vec::new();
                                            for object in reverse_expanded_value {
                                                match object.try_cast::<Node<N::Iri, N::BlankId>>() {
                                                    Ok(node) => reverse_expanded_nodes.push(node),
                                                    Err(_) => {
                                                        return Err(Error::InvalidReversePropertyValue);
                                                    }
                                                }
                                            }

                                            result.reverse_properties_or_default().insert_all(reverse_prop, reverse_expanded_nodes);
                                        }
                                    }
                                    _ => {
                                        if options.policy.invalid.is_reject() {
                                            return Err(Error::KeyExpansionFailed(reverse_key.to_string()));
                                        }

                                        if !options.policy.allow_undefined {
                                            return Err(Error::KeyExpansionFailed(reverse_key.to_string()));
                                        }

                                        // Otherwise the key is dropped.
                                    }
                                }
                            }
                        } else {
                            return Err(Error::InvalidReverseValue);
                        }
                    }
                    // If expanded property is @nest
                    Keyword::Nest => {
                        let nesting_key = key;
                        // Recursively repeat steps 3, 8, 13, and 14 of the
                        // expansion algorithm using `nesting_key` for active
                        // property, and nested value for element.
                        let value = jstrict::Value::force_as_array(value);
                        for nested_value in value {
                            // Expansion algorithm step 3, applied again for
                            // the nesting key: pick up its property-scoped
                            // context.
                            let mut property_scoped_base_url = None;
                            let property_scoped_context = match active_context.get(nesting_key) {
                                Some(definition) => {
                                    if let Some(base_url) = definition.base_url() {
                                        property_scoped_base_url = Some(base_url.clone());
                                    }

                                    definition.context()
                                }
                                None => None,
                            };

                            // Expansion algorithm step 8, applied again: process
                            // that context on top of the active one.
                            let active_context = match property_scoped_context {
                                Some(property_scoped_context) => {
                                    let options: ProcessingOptions = options.into();
                                    let processed = match cache {
                                        Some(cache) => Box::pin(property_scoped_context.process_full_with_cache(
                                            env.vocabulary,
                                            active_context,
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
                                            active_context,
                                            env.loader,
                                            property_scoped_base_url,
                                            options.with_override(),
                                            crate::warning::ContextWarnings(&mut *env.warnings),
                                        ))
                                        .await?
                                        .into_processed(),
                                    };
                                    ContextRef::owned(processed)
                                }
                                None => ContextRef::Borrowed(active_context),
                            };

                            // Steps 13 and 14 again: expand the nested entries
                            // into the same `result`.
                            if let Some(nested_value) = nested_value.as_object() {
                                let mut nested_entries: SmallVec<[&Entry; 8]> = SmallVec::new();

                                for entry in nested_value {
                                    nested_entries.push(entry);
                                }

                                if options.ordered {
                                    nested_entries.sort_by_key(|entry| &entry.key);
                                }

                                let nested_expanded_entries = nested_entries
                                    .into_iter()
                                    .filter_map(|Entry { key, value }| {
                                        expand_iri(
                                            &mut env,
                                            active_context.as_ref(),
                                            Nullable::Some(key.as_str().into()),
                                            false,
                                            Some(options.policy.vocab),
                                        )
                                        .map(|e| e.map(|expanded_key| ExpandedEntry(key, expanded_key, value)))
                                        .transpose()
                                    })
                                    .collect::<Result<_, _>>()?;

                                let (new_result, new_has_value_object_entries) = Box::pin(expand_node_entries(
                                    Environment {
                                        vocabulary: env.vocabulary,
                                        loader: env.loader,
                                        warnings: env.warnings,
                                    },
                                    result,
                                    has_value_object_entries,
                                    active_context.as_ref(),
                                    type_scoped_context,
                                    active_property,
                                    nested_expanded_entries,
                                    base_url,
                                    options,
                                    cache,
                                ))
                                .await?;

                                result = new_result;
                                has_value_object_entries = new_has_value_object_entries;
                            } else {
                                return Err(Error::InvalidNestValue);
                            }
                        }
                    }
                    // A node object never has a `@value` entry: reaching this
                    // arm means one turned up inside the value of an `@nest`
                    // entry, which the specification forbids.
                    Keyword::Value => return Err(Error::InvalidNestValue),
                    // Any other keyword (`@list`, `@set`, `@none`, a context
                    // keyword...) has no node object entry to fill and is
                    // ignored. Maps with a `@list` or `@set` entry are expanded
                    // as list and set objects, before this function is reached.
                    _ => (),
                }
            }

            Term::Id(prop) if prop.with(&*env.vocabulary).as_str().contains(':') => {
                // Inserting into `result` hands over the property identifier, so
                // this is the one arm that has to clone it out of the `Arc`.
                let prop = prop.clone();
                if let Id::Invalid(name) = &prop {
                    match options.policy.invalid {
                        Action::Keep => (),
                        Action::Drop => continue,
                        Action::Reject => return Err(Error::KeyExpansionFailed(name.to_owned())),
                    }
                }

                let mut container_mapping = Container::new();

                let key_definition = active_context.get(key);
                let mut is_reverse_property = false;
                let mut is_json = false;

                if let Some(key_definition) = key_definition {
                    is_reverse_property = key_definition.reverse_property();

                    // Initialize container mapping to key's container mapping in active context.
                    container_mapping = key_definition.container();

                    // If key's term definition in `active_context` has a type mapping of `@json`,
                    // set expanded value to a new map,
                    // set the entry `@value` to `value`, and set the entry `@type` to `@json`.
                    if key_definition.typ() == Some(&Type::Json) {
                        is_json = true;
                    }
                }

                let mut expanded_value = if is_json {
                    Expanded::Object(Object::Value(Value::Json(value.clone())).into())
                } else {
                    match value.as_object() {
                        Some(value) if container_mapping.contains(ContainerKind::Language) => {
                            // Otherwise, if container mapping includes @language and value is a map then
                            // value is expanded from a language map as follows:
                            // Initialize expanded value to an empty array.
                            let mut expanded_value = Vec::new();

                            // Initialize direction to the default base direction from active context.
                            let mut direction = active_context.default_base_direction();

                            // If key's term definition in active context has a
                            // direction mapping, update direction with that value.
                            if let Some(key_definition) = key_definition
                                && let Some(key_direction) = key_definition.direction()
                            {
                                direction = key_direction.option();
                            }

                            // For each key-value pair language-language value in
                            // value, ordered lexicographically by language if ordered is true:
                            let mut language_entries: SmallVec<[&Entry; 8]> = SmallVec::with_capacity(value.len());
                            for language_entry in value {
                                language_entries.push(language_entry);
                            }

                            if options.ordered {
                                language_entries.sort_by_key(|entry| &entry.key);
                            }

                            for Entry {
                                key: language,
                                value: language_value,
                            } in language_entries
                            {
                                // If language value is not an array set language value to
                                // an array containing only language value.
                                let language_value = jstrict::Value::force_as_array(language_value);

                                // For each item in language value:
                                for item in language_value {
                                    match item {
                                        // If item is null, continue to the next entry in
                                        // language value.
                                        jstrict::Value::Null => (),
                                        jstrict::Value::String(item) => {
                                            // If language is @none, or expands to
                                            // @none, remove @language from v.
                                            let language = if expand_iri(
                                                &mut env,
                                                active_context,
                                                Nullable::Some(language.as_str().into()),
                                                false,
                                                Some(options.policy.vocab),
                                            )?
                                            .as_deref()
                                                == Some(&Term::Keyword(Keyword::None))
                                            {
                                                None
                                            } else {
                                                let (language, error) = LenientLangTagBuf::new(language.to_string());

                                                if let Some(error) = error {
                                                    env.warnings.handle(env.vocabulary, Warning::MalformedLanguageTag(language.to_string(), error));
                                                }

                                                Some(language)
                                            };

                                            // initialize a new map v consisting of two
                                            // key-value pairs: (@value-item) and
                                            // (@language-language).
                                            if let Ok(v) = LangString::new(item.clone(), language, direction) {
                                                // The specification asks for a warning when the
                                                // language is neither @none nor well-formed
                                                // according to section 2.2.9 of [BCP47]. It is
                                                // raised just above, where the tag is turned
                                                // into a `LenientLangTagBuf`.

                                                // Append v to expanded value.
                                                expanded_value.push(Object::Value(Value::LangString(v)).into());
                                            } else {
                                                expanded_value.push(Object::Value(Value::Literal(Literal::String(item.clone()), None)).into());
                                            }
                                        }
                                        _ => {
                                            // item must be a string, otherwise an
                                            // invalid language map value error has
                                            // been detected and processing is aborted.
                                            return Err(Error::InvalidLanguageMapValue);
                                        }
                                    }
                                }
                            }

                            Expanded::Array(expanded_value)
                        }
                        Some(value)
                            if container_mapping.contains(ContainerKind::Index)
                                || container_mapping.contains(ContainerKind::Type)
                                || container_mapping.contains(ContainerKind::Id) =>
                        {
                            // Otherwise, if container mapping includes @index, @type, or @id and value
                            // is a map then value is expanded from a map as follows:

                            // Initialize expanded value to an empty array.
                            let mut expanded_value: Vec<IndexedObject<N::Iri, N::BlankId>> = Vec::new();

                            // Initialize `index_key` to the key's index mapping in
                            // `active_context`, or @index, if it does not exist.
                            let index_key = if let Some(key_definition) = key_definition {
                                if let Some(index) = key_definition.index() { index.as_str() } else { "@index" }
                            } else {
                                "@index"
                            };

                            // For each key-value pair index-index value in value,
                            // ordered lexicographically by index if ordered is true:
                            let mut entries: SmallVec<[&Entry; 8]> = SmallVec::with_capacity(value.len());
                            for entry in value {
                                entries.push(entry);
                            }

                            if options.ordered {
                                entries.sort_by_key(|entry| &entry.key);
                            }

                            for Entry {
                                key: index,
                                value: index_value,
                            } in entries
                            {
                                // If container mapping includes @id or @type,
                                // initialize `map_context` to the `previous_context`
                                // from `active_context` if it exists, otherwise, set
                                // `map_context` to `active_context`.
                                let mut map_context = ContextRef::Borrowed(active_context);
                                if (container_mapping.contains(ContainerKind::Type) || container_mapping.contains(ContainerKind::Id))
                                    && let Some(previous_context) = active_context.previous_context()
                                {
                                    map_context = ContextRef::Borrowed(previous_context);
                                }

                                // If container mapping includes @type and
                                // index's term definition in map context has a
                                // local context, update map context to the result of
                                // the Context Processing algorithm, passing
                                // map context as active context the value of the
                                // index's local context as local context and base URL
                                // from the term definition for index in map context.
                                if container_mapping.contains(ContainerKind::Type)
                                    && let Some(index_definition) = map_context.get(index.as_str())
                                    && let Some(local_context) = index_definition.context()
                                {
                                    let base_url = index_definition.base_url().cloned();
                                    let processed = match cache {
                                        Some(cache) => Box::pin(local_context.process_full_with_cache(
                                            env.vocabulary,
                                            map_context.as_ref(),
                                            env.loader,
                                            base_url,
                                            options.into(),
                                            crate::warning::ContextWarnings(&mut *env.warnings),
                                            cache,
                                        ))
                                        .await?
                                        .into_processed(),
                                        None => Box::pin(local_context.process_full(
                                            env.vocabulary,
                                            map_context.as_ref(),
                                            env.loader,
                                            base_url,
                                            options.into(),
                                            crate::warning::ContextWarnings(&mut *env.warnings),
                                        ))
                                        .await?
                                        .into_processed(),
                                    };
                                    map_context = ContextRef::owned(processed);
                                }

                                // Spec step 13.8.3 says "Otherwise, set map
                                // context to active context", which would
                                // discard the `previous_context` selected in
                                // step 13.8.1 whenever the index term has no
                                // type-scoped local context. This
                                // implementation keeps the context chosen in
                                // step 13.8.1 instead: reverting would make
                                // that assignment observable only through
                                // 13.8.2, which reads as a spec editing
                                // artifact rather than intent. No W3C
                                // expansion test covers the difference.

                                // Initialize `expanded_index` to the result of IRI
                                // expanding index. It is left `None` when index
                                // expands to `@none` or to nothing, which is how
                                // the "expanded index is not @none" condition of
                                // the steps below is enforced.
                                let expanded_index = match expand_iri(
                                    &mut env,
                                    active_context,
                                    Nullable::Some(index.as_str().into()),
                                    false,
                                    Some(options.policy.vocab),
                                )? {
                                    Some(arc) if matches!(arc.as_ref(), Term::Null | Term::Keyword(Keyword::None)) => None,
                                    key => key.map(|arc| Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone())),
                                };

                                // The specification wraps a non-array index value
                                // in an array here; `expand_element` accepts both
                                // and yields the same expansion either way.

                                // Initialize index value to the result of using this
                                // algorithm recursively, passing map context as
                                // active context, key as active property,
                                // index value as element, base URL, and the
                                // frameExpansion and ordered flags.
                                // And `true` for `from_map`.
                                let expanded_index_value = Box::pin(expand_element(
                                    Environment {
                                        vocabulary: env.vocabulary,
                                        loader: env.loader,
                                        warnings: env.warnings,
                                    },
                                    map_context.as_ref(),
                                    ActiveProperty::Some(key),
                                    index_value,
                                    base_url,
                                    options,
                                    true,
                                    cache,
                                ))
                                .await?;
                                // For each item in index value:
                                for mut item in expanded_index_value {
                                    // If container mapping includes @graph,
                                    // and item is not a graph object, set item to
                                    // a new map containing the key-value pair
                                    // @graph-item, ensuring that the value is
                                    // represented using an array.
                                    if container_mapping.contains(ContainerKind::Graph) && !item.is_graph() {
                                        let mut node = Node::new();
                                        node.set_graph_entry(Some(vec![item]));
                                        item = Object::node(node).into();
                                    }

                                    if expanded_index.is_some() {
                                        // If `container_mapping` includes @index,
                                        // index key is not @index, and expanded index is
                                        // not @none:
                                        if container_mapping.contains(ContainerKind::Index) && index_key != "@index" {
                                            // Initialize re-expanded index to the result
                                            // of calling the Value Expansion algorithm,
                                            // passing the active context, index key as
                                            // active property, and index as value.
                                            let re_expanded_index = expand_literal(
                                                Environment {
                                                    vocabulary: env.vocabulary,
                                                    loader: env.loader,
                                                    warnings: env.warnings,
                                                },
                                                options.policy.vocab,
                                                active_context,
                                                ActiveProperty::Some(index_key),
                                                LiteralValue::Inferred(index.as_str().into()),
                                            )?;

                                            // Initialize expanded index key to the result
                                            // of IRI expanding index key.
                                            let expanded_index_key = match expand_iri(
                                                &mut env,
                                                active_context,
                                                Nullable::Some(index_key.into()),
                                                false,
                                                Some(options.policy.vocab),
                                            )? {
                                                Some(arc) if matches!(arc.as_ref(), Term::Id(_)) => {
                                                    match Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone()) {
                                                        Term::Id(prop) => prop,
                                                        // Unreachable: the guard above
                                                        // matched `Term::Id`. Skipping
                                                        // keeps the match total.
                                                        _ => continue,
                                                    }
                                                }
                                                _ => continue,
                                            };

                                            // Add the key-value pair (expanded index
                                            // key-index property values) to item.
                                            if let Object::Node(node) = item.inner_mut() {
                                                node.insert(expanded_index_key, re_expanded_index);
                                            } else {
                                                // If item is a value object, it MUST NOT
                                                // contain any extra properties; an invalid
                                                // value object error has been detected and
                                                // processing is aborted.
                                                return Err(Error::Value(crate::InvalidValue::ValueObject));
                                            }
                                        } else if container_mapping.contains(ContainerKind::Index) && item.index().is_none() {
                                            // Otherwise, if container mapping includes
                                            // @index, item does not have an entry @index,
                                            // and expanded index is not @none, add the
                                            // key-value pair (@index-index) to item.
                                            item.set_index(Some((*index).to_string()));
                                        } else if container_mapping.contains(ContainerKind::Id) && item.id().is_none() {
                                            // Otherwise, if container mapping includes
                                            // @id item does not have the entry @id,
                                            // and expanded index is not @none, add the
                                            // key-value pair (@id-expanded index) to
                                            // item, where expanded index is set to the
                                            // result of IRI expanding index using true for
                                            // document relative and false for vocab.
                                            if let Object::Node(ref mut node) = *item {
                                                node.id = expand_iri(&mut env, active_context, Nullable::Some(index.as_str().into()), true, None)?
                                                    .and_then(node_id_of_term);
                                            }
                                        } else if container_mapping.contains(ContainerKind::Type) {
                                            // Otherwise, if container mapping includes
                                            // @type and expanded index is not @none,
                                            // initialize types to a new array consisting
                                            // of expanded index followed by any existing
                                            // values of @type in item. Add the key-value
                                            // pair (@type-types) to item.
                                            if let Some(ei) = expanded_index.as_ref() {
                                                if let Ok(typ) = ei.clone().try_into() {
                                                    if let Object::Node(ref mut node) = *item {
                                                        node.types_mut_or_default().insert(0, typ);
                                                    }
                                                } else {
                                                    return Err(Error::InvalidTypeValue);
                                                }
                                            }
                                        }
                                    }

                                    // Append item to expanded value.
                                    expanded_value.push(item);
                                }
                            }

                            Expanded::Array(expanded_value)
                        }
                        _ => {
                            // Otherwise, initialize expanded value to the result of using this
                            // algorithm recursively, passing active context, key for active property,
                            // value for element, base URL, and the frameExpansion and ordered flags.
                            Box::pin(expand_element(
                                Environment {
                                    vocabulary: env.vocabulary,
                                    loader: env.loader,
                                    warnings: env.warnings,
                                },
                                active_context,
                                ActiveProperty::Some(key),
                                value,
                                base_url,
                                options,
                                false,
                                cache,
                            ))
                            .await?
                        }
                    }
                };

                // If container mapping includes @list and expanded value is
                // not already a list object, convert expanded value to a list
                // object by first setting it to an array containing only
                // expanded value if it is not already an array, and then by
                // setting it to a map containing the key-value pair
                // @list-expanded value.
                if container_mapping.contains(ContainerKind::List) && !expanded_value.is_list() {
                    // JSON-LD 1.0 forbids wrapping list objects in another list;
                    // 1.1 lifted the restriction (`expand#ter24`).
                    if options.processing_mode == ProcessingMode::JsonLd1_0 && expanded_value.iter().any(|item| item.is_list()) {
                        return Err(Error::ListOfLists);
                    }

                    expanded_value = Expanded::Object(Object::List(object::List::new(expanded_value.into_iter().collect())).into());
                }

                // If container mapping includes @graph, and includes neither
                // @id nor @index, convert expanded value into an array, if
                // necessary, then convert each value ev in expanded value
                // into a graph object:
                if container_mapping.contains(ContainerKind::Graph)
                    && !container_mapping.contains(ContainerKind::Id)
                    && !container_mapping.contains(ContainerKind::Index)
                {
                    expanded_value = Expanded::Array(
                        expanded_value
                            .into_iter()
                            .map(|ev| {
                                let mut node = Node::new();
                                node.set_graph_entry(Some(vec![ev]));
                                Object::node(node).into()
                            })
                            .collect(),
                    );
                }

                if !expanded_value.is_null() {
                    // If the term definition associated to key indicates that it
                    // is a reverse property:
                    if is_reverse_property {
                        // The values become the subjects of the reversed
                        // relation, so each of them must be a node object.
                        let mut reverse_expanded_nodes = Vec::new();
                        for object in expanded_value {
                            match object.try_cast::<Node<N::Iri, N::BlankId>>() {
                                Ok(node) => reverse_expanded_nodes.push(node),
                                Err(_) => return Err(Error::InvalidReversePropertyValue),
                            }
                        }

                        result.reverse_properties_or_default().insert_all(prop, reverse_expanded_nodes);
                    } else {
                        // Otherwise, key is not a reverse property use add value
                        // to add expanded value to the expanded property entry in
                        // result using true for as array.
                        result.insert_all(prop, expanded_value.into_iter());
                    }
                }
            }

            Term::Id(prop) => {
                // A key that is no keyword and whose expansion contains no `:`
                // cannot become an IRI, and the entry is dropped: nothing is
                // inserted into `result` here. The policy only decides whether
                // that silent loss is an error instead.
                if let Id::Invalid(name) = prop {
                    match options.policy.invalid {
                        Action::Drop | Action::Keep => (),
                        Action::Reject => return Err(Error::KeyExpansionFailed(name.to_owned())),
                    }
                }

                if !options.policy.allow_undefined {
                    return Err(Error::KeyExpansionFailed(prop.with(env.vocabulary).to_string()));
                }
            }
        }
    }

    Ok((result, has_value_object_entries))
}
