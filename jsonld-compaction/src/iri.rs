use crate::{Options, TypeLangValue};
use contextual::WithContext;
use jsonld_core::{
    Container,
    Context,
    Indexed,
    Nullable,
    Object,
    ProcessingMode,
    Term,
    Type,
    Value,
    context::{CACHED_KEYWORDS, CompactIriKeyRef, KeywordAliases, inverse::{LangSelection, Selection, TypeSelection}},
    object,
};
use jsonld_syntax::{Keyword, is_keyword, is_keyword_like};
use rdf_rs::vocabulary::Vocabulary;
use smallvec::SmallVec;
use std::{hash::Hash, sync::Arc};

pub struct IriConfusedWithPrefix;

/// Does `key:suffix` beat `current` under the (length, lex) ordering used by
/// the JSON-LD compact-IRI selection rule?
///
/// Equivalent to allocating `format!("{key}:{suffix}")` and testing
/// `(cand.len() <= current.len() && cand < current)`, but without the alloc.
fn candidate_beats(key: &str, suffix: &str, candidate_len: usize, current: &str) -> bool {
    use std::cmp::Ordering;
    match candidate_len.cmp(&current.len()) {
        Ordering::Less => true,
        Ordering::Greater => false,
        Ordering::Equal => {
            let cand = key.bytes().chain(std::iter::once(b':')).chain(suffix.bytes());
            cand.cmp(current.bytes()) == Ordering::Less
        }
    }
}

/// Compact the given term without considering any value.
///
/// Calls [`compact_iri_full`] with `None` for `value`. Memoized per active
/// context: repeated `(var, vocab, reverse)` lookups return the cached result
/// without rerunning the algorithm.
pub(crate) fn compact_iri<N>(
    vocabulary: &N,
    active_context: &Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    vocab: bool,
    reverse: bool,
    options: Options,
) -> Result<Option<Arc<str>>, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    let cache = active_context.compact_iri_cache();
    {
        let guard = cache.lock();
        if let Some(hit) = guard.get(&CompactIriKeyRef(var, vocab, reverse)) {
            return Ok(hit.clone());
        }
    }

    let result = compact_iri_full::<N, Object<N::Iri, N::BlankId>>(vocabulary, active_context, var, None, vocab, reverse, options)?;

    cache.lock().insert((var.clone(), vocab, reverse), result.clone());
    Ok(result)
}

/// Returns the cached compact alias for one of the 13 fixed keywords used
/// repeatedly during compaction (see [`CACHED_KEYWORDS`]). Computed once per
/// active context via [`compact_iri`] and reused — saves Mutex traffic +
/// HashMap lookups + the alias-selection walk in `compact_iri_full` for each
/// of the ~25 hot keyword call-sites.
pub(crate) fn keyword_alias<'a, N>(
    vocabulary: &N,
    active_context: &'a Context<N::Iri, N::BlankId>,
    options: Options,
    k: Keyword,
) -> &'a str
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    let aliases = active_context.keyword_aliases_or_init(|| {
        let arr: [Box<str>; 13] = std::array::from_fn(|i| {
            let kw = CACHED_KEYWORDS[i];
            match compact_iri(vocabulary, active_context, &Term::Keyword(kw), true, false, options).ok().flatten() {
                Some(arc) => Box::<str>::from(&*arc),
                None => Box::<str>::from(kw.into_str()),
            }
        });
        KeywordAliases::new(arr)
    });
    aliases.get(k).expect("keyword must be in CACHED_KEYWORDS")
}

/// Compact the given term considering the given value object.
///
/// Calls [`compact_iri_full`] with `Some(value)`.
pub(crate) fn compact_iri_with<N, O>(
    vocabulary: &N,
    active_context: &Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    value: &Indexed<O>,
    vocab: bool,
    reverse: bool,
    options: Options,
) -> Result<Option<Arc<str>>, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    O: object::Any<N::Iri, N::BlankId>,
{
    compact_iri_full(vocabulary, active_context, var, Some(value), vocab, reverse, options)
}

/// Compact the given term.
///
/// Default value for `value` is `None` and `false` for `vocab` and `reverse`.
pub(crate) fn compact_iri_full<N, O>(
    vocabulary: &N,
    active_context: &Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    value: Option<&Indexed<O>>,
    vocab: bool,
    reverse: bool,
    options: Options,
) -> Result<Option<Arc<str>>, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    O: object::Any<N::Iri, N::BlankId>,
{
    if var.is_null() {
        return Ok(None);
    }

    if vocab {
        if let Some(entry) = active_context.inverse().get(var) {
            // Initialize containers to an empty array.
            // This array will be used to keep track of an ordered list of preferred container
            // mapping for a term, based on what is compatible with value.
            let mut containers: SmallVec<[Container; 8]> = SmallVec::new();
            let mut type_lang_value = None;

            if let Some(value) = value {
                if value.index().is_some() && !value.is_graph() {
                    containers.push(Container::Index);
                    containers.push(Container::IndexSet);
                }
            }

            let mut has_index = false;
            let mut is_simple_value = false; // value object with no type, no index, no language and no direction.

            if reverse {
                type_lang_value = Some(TypeLangValue::Type(TypeSelection::Reverse));
                containers.push(Container::Set);
            } else {
                let value_ref = value.map(|v| {
                    has_index = v.index().is_some();
                    v.inner().as_ref()
                });

                match value_ref {
                    Some(object::Ref::List(list)) => {
                        if !has_index {
                            containers.push(Container::List);
                        }

                        let mut common_type = None;
                        let mut common_lang_dir = None;

                        if list.is_empty() {
                            common_lang_dir = Some(Nullable::Some((active_context.default_language(), active_context.default_base_direction())))
                        } else {
                            for item in list {
                                let mut item_type = None;
                                let mut item_lang_dir = None;
                                let mut is_value = false;

                                match item.inner() {
                                    Object::Value(value) => {
                                        is_value = true;
                                        match value {
                                            Value::LangString(lang_str) => item_lang_dir = Some(Nullable::Some((lang_str.language(), lang_str.direction()))),
                                            Value::Literal(_, Some(ty)) => item_type = Some(Type::Iri(ty.clone())),
                                            Value::Literal(_, None) => item_lang_dir = Some(Nullable::Null),
                                            Value::Json(_) => item_type = Some(Type::Json),
                                        }
                                    }
                                    _ => item_type = Some(Type::Id),
                                }

                                if common_lang_dir.is_none() {
                                    common_lang_dir = item_lang_dir
                                } else if is_value && common_lang_dir != item_lang_dir {
                                    common_lang_dir = Some(Nullable::Some((None, None)))
                                }

                                if common_type.is_none() {
                                    common_type = Some(item_type)
                                } else if *common_type.as_ref().unwrap() != item_type {
                                    common_type = Some(None)
                                }

                                if common_lang_dir == Some(Nullable::Some((None, None))) && common_type == Some(None) {
                                    break;
                                }
                            }
                        }

                        if common_lang_dir.is_none() {
                            common_lang_dir = Some(Nullable::Some((None, None)))
                        }
                        let common_lang_dir = common_lang_dir.unwrap();

                        if common_type.is_none() {
                            common_type = Some(None)
                        }
                        let common_type = common_type.unwrap();

                        if let Some(common_type) = common_type {
                            type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(common_type)))
                        } else {
                            type_lang_value = Some(TypeLangValue::Lang(LangSelection::Lang(common_lang_dir)))
                        }
                    }
                    Some(object::Ref::Node(node)) if node.is_graph() => {
                        // Otherwise, if value is a graph object, prefer a mapping most
                        // appropriate for the particular value.
                        if has_index {
                            // If value contains an @index entry, append the values
                            // @graph@index and @graph@index@set to containers.
                            containers.push(Container::GraphIndex);
                            containers.push(Container::GraphIndexSet);
                        }

                        if node.id.is_some() {
                            // If value contains an @id entry, append the values @graph@id and
                            // @graph@id@set to containers.
                            containers.push(Container::GraphId);
                            containers.push(Container::GraphIdSet);
                        }

                        // Append the values @graph, @graph@set, and @set to containers.
                        containers.push(Container::Graph);
                        containers.push(Container::GraphSet);
                        containers.push(Container::Set);

                        if !has_index {
                            // If value does not contain an @index entry, append the values
                            // @graph@index and @graph@index@set to containers.
                            containers.push(Container::GraphIndex);
                            containers.push(Container::GraphIndexSet);
                        }

                        if node.id.is_none() {
                            // If the value does not contain an @id entry, append the values
                            // @graph@id and @graph@id@set to containers.
                            containers.push(Container::GraphId);
                            containers.push(Container::GraphIdSet);
                        }

                        // Append the values @index and @index@set to containers.
                        containers.push(Container::Index);
                        containers.push(Container::IndexSet);

                        type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(Type::Id)))
                    }
                    Some(object::Ref::Value(v)) => {
                        // If value is a value object:
                        if (v.direction().is_some() || v.language().is_some()) && !has_index {
                            type_lang_value = Some(TypeLangValue::Lang(LangSelection::Lang(Nullable::Some((v.language(), v.direction())))));
                            containers.push(Container::Language);
                            containers.push(Container::LanguageSet)
                        } else if let Some(ty) = v.typ() {
                            type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(ty.as_syntax_type().cloned())))
                        } else {
                            is_simple_value = v.direction().is_none() && v.language().is_none() && !has_index
                        }

                        containers.push(Container::Set)
                    }
                    _ => {
                        // Otherwise, set type/language to @type and set type/language value
                        // to @id, and append @id, @id@set, @type, and @set@type, to containers.
                        type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(Type::Id)));
                        containers.push(Container::Id);
                        containers.push(Container::IdSet);
                        containers.push(Container::Type);
                        containers.push(Container::SetType);

                        containers.push(Container::Set)
                    }
                }
            }

            containers.push(Container::None);

            if options.processing_mode != ProcessingMode::JsonLd1_0 && !has_index {
                containers.push(Container::Index);
                containers.push(Container::IndexSet)
            }

            if options.processing_mode != ProcessingMode::JsonLd1_0 && is_simple_value {
                containers.push(Container::Language);
                containers.push(Container::LanguageSet)
            }

            let mut is_empty_list = false;
            if let Some(value) = value {
                if let object::Ref::List(list) = value.inner().as_ref() {
                    if list.is_empty() {
                        is_empty_list = true;
                    }
                }
            }

            // If type/language value is @reverse, append @reverse to preferred values.
            let selection = if is_empty_list {
                Selection::Any
            } else {
                match type_lang_value {
                    Some(TypeLangValue::Type(type_value)) => {
                        let mut selection: Vec<TypeSelection<N::Iri>> = Vec::new();

                        if type_value == TypeSelection::Reverse {
                            selection.push(TypeSelection::Reverse);
                        }

                        let mut has_id_type = false;
                        if let Some(value) = value {
                            if let Some(id) = value.id() {
                                if type_value == TypeSelection::Type(Type::Id) || type_value == TypeSelection::Reverse {
                                    has_id_type = true;
                                    let mut vocab = false;
                                    let compacted_iri = compact_iri(vocabulary, active_context, &id.clone().into_term(), true, false, options)?.unwrap();
                                    if let Some(def) = active_context.get(&*compacted_iri) {
                                        if let Some(iri_mapping) = def.value() {
                                            vocab = iri_mapping == id;
                                        }
                                    }

                                    if vocab {
                                        selection.push(TypeSelection::Type(Type::Vocab));
                                        selection.push(TypeSelection::Type(Type::Id));
                                    } else {
                                        selection.push(TypeSelection::Type(Type::Id));
                                        selection.push(TypeSelection::Type(Type::Vocab));
                                    }

                                    selection.push(TypeSelection::Type(Type::None));
                                }
                            }
                        }

                        if !has_id_type {
                            selection.push(type_value);
                            selection.push(TypeSelection::Type(Type::None));
                        }

                        selection.push(TypeSelection::Any);

                        Selection::Type(selection)
                    }
                    Some(TypeLangValue::Lang(lang_value)) => {
                        let mut selection = vec![lang_value, LangSelection::Lang(Nullable::Some((None, None))), LangSelection::Any];

                        if let LangSelection::Lang(Nullable::Some((Some(_), Some(dir)))) = lang_value {
                            selection.push(LangSelection::Lang(Nullable::Some((None, Some(dir)))));
                        }

                        Selection::Lang(selection)
                    }
                    None => Selection::Lang(vec![
                        LangSelection::Lang(Nullable::Null),
                        LangSelection::Lang(Nullable::Some((None, None))),
                        LangSelection::Any,
                    ]),
                }
            };

            if let Some(term) = entry.select(&containers, &selection) {
                return Ok(Some(Arc::from(term.as_str())));
            }
        }

        // At this point, there is no simple term that var can be compacted to.
        // If vocab is true and active context has a vocabulary mapping:
        if let Some(vocab_mapping) = active_context.vocabulary() {
            // If var begins with the vocabulary mapping's value but is longer, then initialize
            // suffix to the substring of var that does not match. If suffix does not have a term
            // definition in active context, then return suffix.
            if let Some(suffix) = var.with(vocabulary).as_str().strip_prefix(vocab_mapping.with(vocabulary).as_str()) {
                if !suffix.is_empty() && active_context.get(suffix).is_none() {
                    return Ok(Some(Arc::from(suffix)));
                }
            }
        }
    }

    // The var could not be compacted using the active context's vocabulary mapping.
    // Try to create a compact IRI, starting by initializing compact IRI to null.
    // This variable will be used to store the created compact IRI, if any.
    let mut compact_iri: Option<String> = None;
    let var_str = var.with(vocabulary).as_str();

    // Stack-inline scratch buffer reused across loop iterations. Allocates
    // only when the candidate exceeds the inline capacity. The accepted
    // winner is the only allocation paid per accept.
    let mut buf: SmallVec<[u8; 64]> = SmallVec::new();

    // Iterate only term definitions whose `prefix` flag is true (cached on the
    // active context).
    for key in active_context.prefix_term_keys() {
        let definition = match active_context.get_normal(key) {
            Some(d) => d,
            None => continue,
        };
        let Some(iri_mapping) = definition.value() else {
            continue;
        };
        let Some(suffix) = var_str.strip_prefix(iri_mapping.with(vocabulary).as_str()) else {
            continue;
        };
        if suffix.is_empty() {
            continue;
        }

        let key_str = key.as_str();
        let candidate_len = key_str.len() + 1 + suffix.len();

        // Cheap precondition: candidate must beat current best by (length, lex)
        // before we pay for materialization or the term-definition lookup.
        if let Some(current) = compact_iri.as_deref()
            && !candidate_beats(key_str, suffix, candidate_len, current)
        {
            continue;
        }

        // Build the candidate into the scratch buffer (no heap alloc when
        // the total fits in the inline capacity).
        buf.clear();
        buf.reserve(candidate_len);
        buf.extend_from_slice(key_str.as_bytes());
        buf.push(b':');
        buf.extend_from_slice(suffix.as_bytes());
        // SAFETY: `key_str` and `suffix` are `&str`, and ':' is ASCII, so
        // the concatenated bytes are valid UTF-8.
        let candidate_str: &str = unsafe { std::str::from_utf8_unchecked(&buf) };

        // If candidate has a term definition in active context, accept it
        // only when that definition's IRI mapping equals `var` and the
        // caller did not pass a `value`.
        let candidate_def = active_context.get(candidate_str);
        let definition_ok = match candidate_def {
            None => true,
            Some(def) => def.value() == Some(var) && value.is_none(),
        };
        if definition_ok {
            compact_iri = Some(candidate_str.to_owned());
        }
    }

    // If compact IRI is not null, return compact IRI.
    if let Some(compact_iri) = compact_iri {
        return Ok(Some(Arc::from(compact_iri)));
    }

    // To ensure that the IRI var is not confused with a compact IRI,
    // if the IRI scheme of var matches any term in active context with prefix flag set to true,
    // and var has no IRI authority (preceded by double-forward-slash (//),
    // an IRI confused with prefix error has been detected, and processing is aborted.
    if let Some(iri) = var.as_iri() {
        let iri = vocabulary.iri(iri).unwrap();
        if active_context.contains_term(iri.scheme()) {
            return Err(IriConfusedWithPrefix);
        }
    }

    // If vocab is false, transform var to a relative IRI reference using the
    // base IRI from active context, if it exists.
    if !vocab {
        if let Some(base_iri) = active_context.base_iri() {
            if let Some(iri) = var.as_iri() {
                let iri = vocabulary.iri(iri).unwrap();
                let base = vocabulary.iri(base_iri).unwrap();
                let rel = iri.relative_to(&base);
                let s = rel.as_str();
                // RFC 3986 relativization yields "" when target equals base;
                // JSON-LD test 0076 expects last path segment of base instead.
                let out = if s.is_empty() {
                    base.as_str().rsplit('/').next().unwrap_or("").to_string()
                } else {
                    s.to_string()
                };
                return Ok(Some(Arc::from(disambiguate_keyword(out))));
            }
        }
    }

    // Finally, return var as is.
    Ok(Some(Arc::from(var.with(vocabulary).to_string())))
}

fn disambiguate_keyword(s: String) -> String {
    if is_keyword_like(&s) && !is_keyword(&s) { "./".to_string() + &s } else { s }
}
