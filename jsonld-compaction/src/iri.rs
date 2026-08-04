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
    context::{
        CACHED_KEYWORDS,
        CompactIriKeyRef,
        KeywordAliases,
        inverse::{LangSelection, Selection, TypeSelection},
    },
    object,
};
use jsonld_syntax::{Keyword, is_keyword, is_keyword_like};
use rdfx::vocabulary::Vocabulary;
use smallvec::SmallVec;
use std::{hash::Hash, sync::Arc};

/// An IRI cannot be written out because it would be read back as a compact IRI.
///
/// Raised when neither a term nor a compact IRI could be found for an IRI whose
/// scheme is itself a term of the active context: emitting the IRI verbatim
/// would make a reader expand `scheme:rest` through that term instead.
///
/// The [specification][1] narrows this to terms whose `prefix` flag is set and
/// to IRIs with no authority component (no `//`); the check here does not
/// distinguish those cases and so can fire slightly more often.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#iri-compaction
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("IRI confused with prefix")]
pub struct IriConfusedWithPrefix;

/// Checks whether the compact IRI `key:suffix` is a better candidate than the
/// current best `current`.
///
/// The rule is *shortest first, then lexicographically least*: a shorter
/// candidate always wins, and among candidates of equal length the byte-wise
/// smallest wins. This is the ordering the JSON-LD API specification prescribes
/// for [IRI compaction][1] ("shorter or the same length but lexicographically
/// less than compact IRI"), and it matches what `jsonld.js` produces.
///
/// Note that this is *not* the same as `candidate.len() <= current.len() &&
/// candidate < current`: that stricter test rejects a shorter candidate whose
/// first differing byte is larger, for example `zz:a` against `aaaa:a`.
///
/// `candidate_len` is `key.len() + 1 + suffix.len()`, passed in so the length
/// comparison — which settles almost every call — costs nothing. The
/// concatenation is only walked, never materialized, so no allocation happens
/// on this path.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#iri-compaction
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

/// One-entry memo over a run of values compacted against the same term.
///
/// The container list and the type/language selection are the only things
/// [`compact_iri_full`] derives from its `value` argument; everything after
/// them — the inverse-context search and the compact-IRI fallback — reads the
/// value only through whether it is present at all. So two values producing the
/// same pair produce the same term, and consecutive values of one property
/// almost always do: an array of value objects, an array of node references.
///
/// Both halves of the key are handed over by move once the algorithm is done
/// with them, so a miss costs a comparison and no allocation.
///
/// The key covers the value **only**: a memo is valid while `vocabulary`,
/// `active_context`, `var`, `vocab`, `reverse` and `options` are held fixed.
/// Build one immediately before such a loop and drop it after.
pub(crate) struct CompactIriMemo<'a, T> {
    key: Option<MemoKey<'a, T>>,
    result: Option<Arc<str>>,
}

struct MemoKey<'a, T> {
    containers: SmallVec<[Container; 8]>,
    selection: Selection<'a, T>,
}

impl<T> CompactIriMemo<'_, T> {
    pub fn new() -> Self {
        Self { key: None, result: None }
    }
}

impl<'a, T: PartialEq> CompactIriMemo<'a, T> {
    // The two layers mean different things: the outer `None` is a memo miss,
    // the inner `None` is a remembered "no compact IRI applies". Collapsing them
    // would recompute the negative case on every lookup.
    #[allow(clippy::option_option)]
    fn get(&self, containers: &[Container], selection: &Selection<'a, T>) -> Option<Option<Arc<str>>> {
        let key = self.key.as_ref()?;
        (key.containers.as_slice() == containers && key.selection == *selection).then(|| self.result.clone())
    }

    fn store(&mut self, containers: SmallVec<[Container; 8]>, selection: Selection<'a, T>, result: Option<Arc<str>>) {
        self.key = Some(MemoKey { containers, selection });
        self.result = result;
    }
}

/// Compacts `var` into a term, compact IRI or relative IRI reference, without
/// considering any value.
///
/// This is [`compact_iri_full`] with no `value`, which is how keys, `@type`
/// values and `@id` values are compacted. Memoized per active context: a
/// repeated `(var, vocab, reverse, processing mode)` lookup returns the cached
/// result instead of rerunning the algorithm. The processing mode belongs in
/// the key because term selection depends on it — an `@index` container term,
/// for instance, is only eligible for an index-less value under JSON-LD 1.1.
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
        if let Some(hit) = guard.get(&CompactIriKeyRef(var, vocab, reverse, options.processing_mode)) {
            return Ok(hit.clone());
        }
    }

    let result = compact_iri_full::<N, Object<N::Iri, N::BlankId>>(vocabulary, active_context, var, None, vocab, reverse, options, None)?;

    cache.lock().insert((var.clone(), vocab, reverse, options.processing_mode), result.clone());
    Ok(result)
}

/// Returns the key a keyword compacts to under the active context, which is
/// either an alias the context defines for it or the keyword itself.
///
/// Restricted to the keywords compaction emits (see [`CACHED_KEYWORDS`]). All
/// of their aliases are resolved on first use through [`compact_iri`] and
/// cached on the active context, once per processing mode, so that the many
/// keyword call sites throughout compaction cost a slice index instead of a
/// lock, a hash lookup and a walk through [`compact_iri_full`].
///
/// A keyword with no alias falls back to its own spelling rather than to
/// `None`, so callers always get a usable key.
pub(crate) fn keyword_alias<'a, N>(vocabulary: &N, active_context: &'a Context<N::Iri, N::BlankId>, options: Options, k: Keyword) -> &'a str
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    let aliases = active_context.keyword_aliases_or_init(options.processing_mode, || {
        let arr: [Box<str>; 13] = std::array::from_fn(|i| {
            let kw = CACHED_KEYWORDS[i];
            match compact_iri(vocabulary, active_context, &Term::Keyword(kw), true, false, options).ok().flatten() {
                Some(arc) => Box::<str>::from(&*arc),
                None => Box::<str>::from(kw.into_str()),
            }
        });
        KeywordAliases::new(arr)
    });
    aliases.get(k)
}

/// Compacts `var` into the term best suited to holding `value`.
///
/// This is [`compact_iri_full`] with a value, so the inverse-context search may
/// prefer a term whose container, type or language mapping matches `value`.
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
    compact_iri_full(vocabulary, active_context, var, Some(value), vocab, reverse, options, None)
}

/// Compacts `var` into the term best suited to holding `value`, reusing `memo`
/// across values that select the same term.
///
/// See [`CompactIriMemo`] for the conditions under which one memo may be shared
/// by successive calls.
pub(crate) fn compact_iri_with_memo<'a, N, O>(
    vocabulary: &N,
    active_context: &'a Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    value: &'a Indexed<O>,
    vocab: bool,
    reverse: bool,
    options: Options,
    memo: &mut CompactIriMemo<'a, N::Iri>,
) -> Result<Option<Arc<str>>, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    O: object::Any<N::Iri, N::BlankId>,
{
    compact_iri_full(vocabulary, active_context, var, Some(value), vocab, reverse, options, Some(memo))
}

/// Runs the [IRI compaction algorithm][1] on `var`.
///
/// Returns the shortest key that expands back to `var` under `active_context`:
/// a term from the inverse context if one is compatible, otherwise a compact
/// IRI, a `@vocab`-relative suffix, a base-relative IRI reference, or `var`
/// itself. Returns `None` when `var` is null.
///
/// `vocab` says the result will be used where `@vocab` applies — as a key, an
/// `@type` value, or the value of a `@vocab`-typed term — which is what makes
/// the inverse-context search and `@vocab` suffixing eligible at all. `reverse`
/// says `var` is being compacted as a reverse property, restricting selection to
/// terms declared with `@reverse`. `value`, when given, lets the search prefer a
/// term whose container, type or language mapping fits it. `memo` short-circuits
/// the search for a run of values that all select the same term; see
/// [`CompactIriMemo`].
///
/// [1]: https://www.w3.org/TR/json-ld-api/#iri-compaction
pub(crate) fn compact_iri_full<'a, N, O>(
    vocabulary: &N,
    active_context: &'a Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    value: Option<&'a Indexed<O>>,
    vocab: bool,
    reverse: bool,
    options: Options,
    mut memo: Option<&mut CompactIriMemo<'a, N::Iri>>,
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

    if vocab && let Some(entry) = active_context.inverse().get(var) {
        // Initialize containers to an empty array.
        // This array will be used to keep track of an ordered list of preferred container
        // mapping for a term, based on what is compatible with value.
        let mut containers: SmallVec<[Container; 8]> = SmallVec::new();
        let mut type_lang_value = None;

        if let Some(value) = value
            && value.index().is_some()
            && !value.is_graph()
        {
            containers.push(Container::Index);
            containers.push(Container::IndexSet);
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
                        common_lang_dir = Some(Nullable::Some((active_context.default_language(), active_context.default_base_direction())));
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
                                common_lang_dir = item_lang_dir;
                            } else if is_value && common_lang_dir != item_lang_dir {
                                common_lang_dir = Some(Nullable::Some((None, None)));
                            }

                            if common_type.is_none() {
                                common_type = Some(item_type);
                            } else if common_type.as_ref().is_some_and(|t| *t != item_type) {
                                common_type = Some(None);
                            }

                            if common_lang_dir == Some(Nullable::Some((None, None))) && common_type == Some(None) {
                                break;
                            }
                        }
                    }

                    let common_lang_dir = common_lang_dir.unwrap_or(Nullable::Some((None, None)));
                    let common_type = common_type.unwrap_or(None);

                    if let Some(common_type) = common_type {
                        type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(common_type)));
                    } else {
                        type_lang_value = Some(TypeLangValue::Lang(LangSelection::Lang(common_lang_dir)));
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

                    type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(Type::Id)));
                }
                Some(object::Ref::Value(v)) => {
                    // If value is a value object:
                    if (v.direction().is_some() || v.language().is_some()) && !has_index {
                        type_lang_value = Some(TypeLangValue::Lang(LangSelection::Lang(Nullable::Some((v.language(), v.direction())))));
                        containers.push(Container::Language);
                        containers.push(Container::LanguageSet);
                    } else if let Some(ty) = v.typ() {
                        type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(ty.as_syntax_type().cloned())));
                    } else {
                        is_simple_value = v.direction().is_none() && v.language().is_none() && !has_index;
                    }

                    containers.push(Container::Set);
                }
                _ => {
                    // Otherwise, set type/language to @type and set type/language value
                    // to @id, and append @id, @id@set, @type, and @set@type, to containers.
                    type_lang_value = Some(TypeLangValue::Type(TypeSelection::Type(Type::Id)));
                    containers.push(Container::Id);
                    containers.push(Container::IdSet);
                    containers.push(Container::Type);
                    containers.push(Container::SetType);

                    containers.push(Container::Set);
                }
            }
        }

        containers.push(Container::None);

        if options.processing_mode != ProcessingMode::JsonLd1_0 && !has_index {
            containers.push(Container::Index);
            containers.push(Container::IndexSet);
        }

        if options.processing_mode != ProcessingMode::JsonLd1_0 && is_simple_value {
            containers.push(Container::Language);
            containers.push(Container::LanguageSet);
        }

        let mut is_empty_list = false;
        if let Some(value) = value
            && let object::Ref::List(list) = value.inner().as_ref()
            && list.is_empty()
        {
            is_empty_list = true;
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
                    if let Some(value) = value
                        && let Some(id) = value.id()
                        && (type_value == TypeSelection::Type(Type::Id) || type_value == TypeSelection::Reverse)
                    {
                        has_id_type = true;
                        let mut vocab = false;
                        if let Some(compacted_iri) = compact_iri(vocabulary, active_context, &id.clone().into_term(), true, false, options)?
                            && let Some(def) = active_context.get(&*compacted_iri)
                            && let Some(iri_mapping) = def.value()
                        {
                            vocab = iri_mapping == id;
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

        if let Some(memo) = memo.as_deref_mut()
            && let Some(hit) = memo.get(&containers, &selection)
        {
            return Ok(hit);
        }

        let result = match entry.select(&containers, &selection) {
            Some(term) => Some(Arc::from(term.as_str())),
            // No term was selected. What follows does not read `value`
            // beyond whether it is present, which the memo holds fixed.
            None => compact_iri_fallback(vocabulary, active_context, var, value.is_none(), vocab)?,
        };

        if let Some(memo) = memo {
            memo.store(containers, selection, result.clone());
        }

        return Ok(result);
    }

    compact_iri_fallback(vocabulary, active_context, var, value.is_none(), vocab)
}

/// Tail of [`compact_iri_full`], reached when no term could be selected from the
/// inverse context.
///
/// Tries, in order: a suffix relative to the active context's `@vocab`, the
/// shortest compact IRI built from a prefix term, then either an IRI reference
/// relative to the base IRI (when `vocab` is false) or `var` written out in
/// full.
///
/// `no_value` is `true` when the caller passed no value; it only affects whether
/// a compact IRI may collide with an existing term definition. Reading the value
/// through this one flag is what lets [`compact_iri_full`] memoize across this
/// function.
fn compact_iri_fallback<N>(
    vocabulary: &N,
    active_context: &Context<N::Iri, N::BlankId>,
    var: &Term<N::Iri, N::BlankId>,
    no_value: bool,
    vocab: bool,
) -> Result<Option<Arc<str>>, IriConfusedWithPrefix>
where
    N: Vocabulary,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
{
    if vocab {
        // At this point, there is no simple term that var can be compacted to.
        // If vocab is true and active context has a vocabulary mapping:
        if let Some(vocab_mapping) = active_context.vocabulary() {
            // If var begins with the vocabulary mapping's value but is longer, then initialize
            // suffix to the substring of var that does not match. If suffix does not have a term
            // definition in active context, then return suffix.
            if let Some(suffix) = var.with(vocabulary).as_str().strip_prefix(vocab_mapping.with(vocabulary).as_str())
                && !suffix.is_empty()
                && active_context.get(suffix).is_none()
            {
                return Ok(Some(Arc::from(suffix)));
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

    // Iterate only term definitions whose `prefix` flag is true, with their IRI
    // mappings (cached on the active context).
    for (key, iri_mapping) in active_context.prefix_terms() {
        let Some(suffix) = var_str.strip_prefix((**iri_mapping).with(vocabulary).as_str()) else {
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
            Some(def) => def.value() == Some(var) && no_value,
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
    //
    // The test below is broader than that: it looks only at whether the scheme
    // is a term of the active context, ignoring the term's `prefix` flag and
    // the absence of an authority component. The W3C compaction suite passes
    // either way, so the extra strictness has never been observed to matter.
    if let Some(iri) = var.as_iri()
        && let Some(iri) = vocabulary.iri(iri)
        && active_context.contains_term(iri.scheme())
    {
        return Err(IriConfusedWithPrefix);
    }

    // If vocab is false, transform var to a relative IRI reference using the
    // base IRI from active context, if it exists.
    if !vocab
        && let Some(base_iri) = active_context.base_iri()
        && let Some(iri) = var.as_iri()
    {
        let Some(iri) = vocabulary.iri(iri) else {
            return Ok(None);
        };
        let Some(base) = vocabulary.iri(base_iri) else {
            return Ok(None);
        };
        let rel = iri.relative_to(&base);
        let s = rel.as_str();
        // RFC 3986 relativization yields "" when the target equals the base;
        // the W3C compaction test `compact#t0076` expects the last path segment
        // of the base instead. When the base itself ends in `/` that segment is
        // empty too, so fall back to "./" — which also resolves back to the
        // base — rather than emitting `"@id": ""`.
        let out = if s.is_empty() {
            match base.as_str().rsplit('/').next() {
                Some(segment) if !segment.is_empty() => segment.to_string(),
                _ => "./".to_string(),
            }
        } else {
            s.to_string()
        };
        return Ok(Some(Arc::from(disambiguate_keyword(out))));
    }

    // Finally, return var as is.
    Ok(Some(Arc::from(var.with(vocabulary).to_string())))
}

/// Prefixes `./` to a relative IRI reference that would otherwise be read back
/// as a keyword.
///
/// A relative reference such as `@foo` is keyword-like but is not a keyword, so
/// re-expanding it would silently drop the entry. `./@foo` resolves to the same
/// IRI while staying a plain reference. Real keywords are left alone, since
/// nothing else can relativize to one.
fn disambiguate_keyword(s: String) -> String {
    if is_keyword_like(&s) && !is_keyword(&s) { "./".to_string() + &s } else { s }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use jsonld_core::{Id, ValidId, context::NormalTermDefinition};
    use rdfx::{BlankIdBuf, IriBuf, vocabulary::no_vocabulary};

    /// One processed context compacted under 1.1 and then 1.0: the
    /// per-context compact-IRI cache must not serve the 1.1 result to the
    /// 1.0 run (selection is mode-dependent — an `@index` container term is
    /// eligible for an index-less value in 1.1 only).
    #[test]
    fn compact_iri_cache_distinguishes_processing_modes() {
        let iri = IriBuf::new("http://example.com/t".to_string()).unwrap();
        let term = Term::<IriBuf, BlankIdBuf>::Id(Id::Valid(ValidId::Iri(iri)));

        let mut context: Context<IriBuf, BlankIdBuf> = Context::new(None);
        context.set_normal(
            "t".into(),
            Some(NormalTermDefinition {
                value: Some(Arc::new(term.clone())),
                container: Container::Index,
                ..Default::default()
            }),
        );

        let options_1_1 = Options {
            processing_mode: ProcessingMode::JsonLd1_1,
            ..Default::default()
        };
        let options_1_0 = Options {
            processing_mode: ProcessingMode::JsonLd1_0,
            ..Default::default()
        };

        let compacted_1_1 = compact_iri(no_vocabulary(), &context, &term, true, false, options_1_1).unwrap();
        let compacted_1_0 = compact_iri(no_vocabulary(), &context, &term, true, false, options_1_0).unwrap();

        assert_eq!(compacted_1_1.as_deref(), Some("t"));
        assert_eq!(compacted_1_0.as_deref(), Some("http://example.com/t"));
    }

    /// Relativizing an IRI equal to a base ending in `/` must not produce
    /// the empty string.
    #[test]
    fn relativization_of_base_with_trailing_slash_is_not_empty() {
        let base = IriBuf::new("http://example.com/dir/".to_string()).unwrap();
        let iri = IriBuf::new("http://example.com/dir/".to_string()).unwrap();
        let term = Term::<IriBuf, BlankIdBuf>::Id(Id::Valid(ValidId::Iri(iri)));

        let mut context: Context<IriBuf, BlankIdBuf> = Context::new(None);
        context.set_base_iri(Some(base));

        let compacted = compact_iri(no_vocabulary(), &context, &term, false, false, Options::default()).unwrap();
        assert_eq!(compacted.as_deref(), Some("./"));
    }
}
