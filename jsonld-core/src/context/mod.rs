//! Context processing algorithm and related types.
mod definition;
/// Inverse context, used to pick terms during compaction.
pub mod inverse;

use crate::{Direction, LenientLangTag, LenientLangTagBuf, ProcessingMode, Term, ValidId as Id};
use contextual::WithContext;
use iri_rs::IriBuf;
use jsonld_syntax::{Keyword, KeywordType, Nullable};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use rdfx::{BlankIdBuf, vocabulary::Vocabulary};
use std::{borrow::Borrow, hash::Hash, sync::Arc};

pub use jsonld_syntax::context::{
    definition::{Key, KeyOrType, Type},
    term_definition::Nest,
};

pub use definition::*;
pub use inverse::InverseContext;

/// Error returned when a context vocabulary is set to an invalid value.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum InvalidVocab {
    /// The vocabulary is a JSON-LD keyword.
    #[error("vocabulary cannot be a keyword")]
    Keyword,
}

/// Error returned when a context key cannot be expressed in syntax form.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid context key")]
pub struct InvalidContextKey;

/// Error returned by [`Context::into_syntax_definition`] and related
/// conversions.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum InvalidContextError {
    #[error(transparent)]
    /// The `@vocab` entry is invalid.
    InvalidVocab(#[from] InvalidVocab),

    #[error(transparent)]
    /// A context key is invalid.
    InvalidContextKey(#[from] InvalidContextKey),
}

/// Processed JSON-LD context.
///
/// Represents the result of the [context processing algorithm][1] implemented
/// by the [`json-ld-context-processing`] crate.
///
/// [1]: <https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm>
/// [`json-ld-context-processing`]: <https://crates.io/crates/json-ld-context-processing>
/// Cache key for [`Context::compact_iri_cache`]: `(var, vocab, reverse)`.
pub type CompactIriKey<T, B> = (Term<T, B>, bool, bool);

/// Borrowed view over a [`CompactIriKey`] for cache lookups that don't need
/// to allocate a fresh `Term`. Hashes byte-for-byte identically to the
/// owned tuple, so `HashMap::get` finds the same bucket.
pub struct CompactIriKeyRef<'a, T, B>(pub &'a Term<T, B>, pub bool, pub bool);

impl<'a, T: std::hash::Hash, B: std::hash::Hash> std::hash::Hash for CompactIriKeyRef<'a, T, B> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
    }
}

impl<'a, T: PartialEq, B: PartialEq> hashbrown::Equivalent<CompactIriKey<T, B>> for CompactIriKeyRef<'a, T, B> {
    #[inline]
    fn equivalent(&self, key: &CompactIriKey<T, B>) -> bool {
        self.0 == &key.0 && self.1 == key.1 && self.2 == key.2
    }
}

/// Cached fixed-keyword aliases for an active context.
///
/// 13 keywords appear repeatedly during compaction (`@id`, `@type`, `@value`,
/// `@list`, `@set`, `@graph`, `@index`, `@language`, `@direction`, `@reverse`,
/// `@none`, `@included`, `@json`). Their compact-IRI form is a pure function
/// of the active context, so the result is computed once and reused — saving
/// per-element [`Mutex`] traffic + cache lookups + alias-selection work in
/// `compact_iri_full`.
pub struct KeywordAliases {
    aliases: [Box<str>; 13],
}

impl KeywordAliases {
    /// Creates a new `KeywordAliases`.
    pub fn new(aliases: [Box<str>; 13]) -> Self {
        Self { aliases }
    }

    /// Returns the cached alias for the given keyword. For one of the 13
    /// supported keywords, returns the alias selected during cache
    /// initialization; for any other keyword returns its literal name
    /// (e.g. `"@base"`).
    #[inline]
    pub fn get(&self, k: Keyword) -> &str {
        match keyword_alias_index(k) {
            Some(i) => self.aliases[i].as_ref(),
            None => k.into_str(),
        }
    }
}

/// 13 keywords supported by [`KeywordAliases`], in cache-array order.
pub const CACHED_KEYWORDS: [Keyword; 13] = [
    Keyword::Id,
    Keyword::Type,
    Keyword::Value,
    Keyword::List,
    Keyword::Set,
    Keyword::Graph,
    Keyword::Index,
    Keyword::Language,
    Keyword::Direction,
    Keyword::Reverse,
    Keyword::None,
    Keyword::Included,
    Keyword::Json,
];

#[inline]
fn keyword_alias_index(k: Keyword) -> Option<usize> {
    Some(match k {
        Keyword::Id => 0,
        Keyword::Type => 1,
        Keyword::Value => 2,
        Keyword::List => 3,
        Keyword::Set => 4,
        Keyword::Graph => 5,
        Keyword::Index => 6,
        Keyword::Language => 7,
        Keyword::Direction => 8,
        Keyword::Reverse => 9,
        Keyword::None => 10,
        Keyword::Included => 11,
        Keyword::Json => 12,
        _ => return None,
    })
}

type CompactIriCache<T, B> = Mutex<crate::HashMap<CompactIriKey<T, B>, Option<Arc<str>>>>;
type TermResolutionCache<T, B> = Mutex<crate::HashMap<Box<str>, Arc<Term<T, B>>>>;

/// Active context: everything the algorithms need to expand or compact
/// against the current scope.
pub struct Context<T = IriBuf, B = BlankIdBuf> {
    original_base_url: Option<T>,
    base_iri: Option<T>,
    vocabulary: Option<Term<T, B>>,
    default_language: Option<LenientLangTagBuf>,
    default_base_direction: Option<Direction>,
    previous_context: Option<Arc<Self>>,
    definitions: Arc<Definitions<T, B>>,
    /// Mode this context was processed under.
    ///
    /// A handful of algorithm steps behave differently in JSON-LD 1.0 — most
    /// notably that any term definition may serve as a compact-IRI prefix
    /// during IRI expansion, the `@prefix` flag being a 1.1 addition. Carrying
    /// the mode here keeps those checks out of every IRI-expansion signature.
    processing_mode: ProcessingMode,
    // Caches are wrapped in `Arc` so that `Clone` shares them across
    // copies of a processed context. This is critical for the
    // `ProcessingCache` hit path, which clones a stored `Arc<Context>`
    // for each entity expansion — without sharing, every entity would
    // start with empty caches and the per-context memoization would
    // never see cross-entity hits.
    //
    // Invalidation (via [`Self::invalidate_iri_caches`]) replaces the
    // `Arc` with a fresh one, so other holders of the original `Arc`
    // keep their (still-valid) cached entries.
    inverse: Arc<OnceCell<InverseContext<T, B>>>,
    prefix_terms: Arc<OnceCell<Vec<Key>>>,
    compact_iri_cache: Arc<OnceCell<CompactIriCache<T, B>>>,
    term_resolution_cache: Arc<OnceCell<TermResolutionCache<T, B>>>,
    keyword_aliases: Arc<OnceCell<KeywordAliases>>,
}

impl<T, B> Default for Context<T, B> {
    fn default() -> Self {
        Self {
            original_base_url: None,
            base_iri: None,
            vocabulary: None,
            default_language: None,
            default_base_direction: None,
            previous_context: None,
            definitions: Arc::new(Definitions::default()),
            processing_mode: ProcessingMode::default(),
            inverse: Arc::new(OnceCell::new()),
            prefix_terms: Arc::new(OnceCell::new()),
            compact_iri_cache: Arc::new(OnceCell::new()),
            term_resolution_cache: Arc::new(OnceCell::new()),
            keyword_aliases: Arc::new(OnceCell::new()),
        }
    }
}

/// Binding of a context definition, key and term definition together.
pub type DefinitionEntryRef<'a, T = IriBuf, B = BlankIdBuf> = (&'a Key, &'a TermDefinition<T, B>);

impl<T, B> Context<T, B> {
    /// Create a new context with the given base IRI.
    pub fn new(base_iri: Option<T>) -> Self
    where
        T: Clone,
    {
        Self {
            original_base_url: base_iri.clone(),
            base_iri,
            vocabulary: None,
            default_language: None,
            default_base_direction: None,
            previous_context: None,
            definitions: Arc::new(Definitions::default()),
            processing_mode: ProcessingMode::default(),
            inverse: Arc::new(OnceCell::new()),
            prefix_terms: Arc::new(OnceCell::new()),
            compact_iri_cache: Arc::new(OnceCell::new()),
            term_resolution_cache: Arc::new(OnceCell::new()),
            keyword_aliases: Arc::new(OnceCell::new()),
        }
    }

    /// Returns a copy of this context whose IRI memoisation caches are private
    /// to the copy.
    ///
    /// [`Self::term_resolution_cache`][c] maps a term to an already-resolved
    /// [`Term`], which carries `T`-typed identifiers. Sharing it across
    /// vocabulary forks is unsound: a term that one fork resolved by interning
    /// a *new* IRI would be served to another fork, where that identifier
    /// denotes something else entirely — or nothing. Concurrent expansion
    /// therefore gives each task a context of its own.
    ///
    /// Only the caches are fresh. Definitions, prefix terms and keyword
    /// aliases are shared through their `Arc`s: they are derived from the
    /// context's own definitions, whose identifiers every fork inherits, so
    /// they stay valid. The same reasoning covers the inverse context.
    ///
    /// [c]: Self::term_resolution_cache
    pub fn with_private_caches(&self) -> Context<T, B>
    where
        T: Clone,
        B: Clone,
    {
        Context {
            original_base_url: self.original_base_url.clone(),
            base_iri: self.base_iri.clone(),
            vocabulary: self.vocabulary.clone(),
            default_language: self.default_language.clone(),
            default_base_direction: self.default_base_direction,
            // A previous context is consulted on the same code paths, so its
            // caches have to be private too.
            previous_context: self.previous_context.as_ref().map(|previous| Arc::new(previous.with_private_caches())),
            definitions: Arc::clone(&self.definitions),
            processing_mode: self.processing_mode,
            inverse: Arc::clone(&self.inverse),
            prefix_terms: Arc::clone(&self.prefix_terms),
            compact_iri_cache: Arc::new(OnceCell::new()),
            term_resolution_cache: Arc::new(OnceCell::new()),
            keyword_aliases: Arc::clone(&self.keyword_aliases),
        }
    }

    /// Returns the processing mode this context was processed under.
    #[inline(always)]
    pub fn processing_mode(&self) -> ProcessingMode {
        self.processing_mode
    }

    /// Sets the processing mode of this context.
    #[inline(always)]
    pub fn set_processing_mode(&mut self, mode: ProcessingMode) {
        self.processing_mode = mode
    }

    /// Returns a reference to the given `term` definition, if any.
    pub fn get<Q>(&self, term: &Q) -> Option<TermDefinitionRef<'_, T, B>>
    where
        Key: Borrow<Q>,
        KeywordType: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.definitions.get(term)
    }

    /// Returns a reference to the given `term` normal definition, if any.
    pub fn get_normal<Q>(&self, term: &Q) -> Option<&NormalTermDefinition<T, B>>
    where
        Key: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.definitions.get_normal(term)
    }

    /// Returns a reference to the `@type` definition, if any.
    pub fn get_type(&self) -> Option<&TypeTermDefinition> {
        self.definitions.get_type()
    }

    /// Checks if the given `term` is defined.
    pub fn contains_term<Q>(&self, term: &Q) -> bool
    where
        Key: Borrow<Q>,
        KeywordType: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.definitions.contains_term(term)
    }

    /// Returns the original base URL of the context.
    pub fn original_base_url(&self) -> Option<&T> {
        self.original_base_url.as_ref()
    }

    /// Returns the base IRI of the context.
    pub fn base_iri(&self) -> Option<&T> {
        self.base_iri.as_ref()
    }

    /// Returns the `@vocab` value, if any.
    pub fn vocabulary(&self) -> Option<&Term<T, B>> {
        match &self.vocabulary {
            Some(v) => Some(v),
            None => None,
        }
    }

    /// Returns the default `@language` value.
    pub fn default_language(&self) -> Option<&LenientLangTag> {
        self.default_language.as_ref().map(|tag| tag.as_lenient_lang_tag_ref())
    }

    /// Returns the default `@direction` value.
    pub fn default_base_direction(&self) -> Option<Direction> {
        self.default_base_direction
    }

    /// Returns a reference to the previous context.
    pub fn previous_context(&self) -> Option<&Self> {
        match &self.previous_context {
            Some(c) => Some(c),
            None => None,
        }
    }

    /// Returns the address of the `Arc` backing the term definitions.
    ///
    /// Two contexts that share this pointer share the exact same definitions
    /// (copy-on-write via [`Arc::make_mut`]). Useful as a cheap fingerprint
    /// for memoization keys.
    pub fn definitions_arc_ptr(&self) -> *const Definitions<T, B> {
        Arc::as_ptr(&self.definitions)
    }

    /// Returns the address of the `Arc` backing the previous context, or null
    /// when there is none. See [`Self::definitions_arc_ptr`].
    pub fn previous_context_arc_ptr(&self) -> *const Self {
        self.previous_context.as_ref().map(Arc::as_ptr).unwrap_or(std::ptr::null())
    }

    /// Returns the number of terms defined.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Checks if no terms are defined.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Returns a handle to the term definitions.
    pub fn definitions(&self) -> &Definitions<T, B> {
        &self.definitions
    }

    /// Checks if the context has a protected definition.
    pub fn has_protected_items(&self) -> bool {
        for binding in self.definitions() {
            if binding.definition().protected() {
                return true;
            }
        }

        false
    }

    /// Returns the inverse of this context.
    pub fn inverse(&self) -> &InverseContext<T, B>
    where
        T: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
    {
        self.inverse.get_or_init(|| self.into())
    }

    /// Returns the keys of the term definitions whose `prefix` flag is `true`.
    ///
    /// Lazily computed and cached; invalidated whenever a definition is added,
    /// removed, or replaced.
    pub fn prefix_term_keys(&self) -> &[Key] {
        self.prefix_terms.get_or_init(|| {
            self.definitions
                .iter()
                .filter_map(|binding| match binding {
                    BindingRef::Normal(key, def) if def.prefix => Some(*key),
                    _ => None,
                })
                .collect()
        })
    }

    /// Returns the per-context memoization map for the compact-IRI algorithm.
    ///
    /// The cache stores results keyed on `(var, vocab, reverse)`. It is
    /// invalidated whenever the context's term definitions, base IRI,
    /// vocabulary, language, or direction change, since those affect
    /// compaction output.
    pub fn compact_iri_cache(&self) -> &CompactIriCache<T, B> {
        self.compact_iri_cache.get_or_init(|| Mutex::new(crate::HashMap::default()))
    }

    /// Returns the per-context memoization map for IRI expansion of term keys.
    ///
    /// Maps the input string to the resolved [`Arc<Term>`]. Populated only by
    /// the dominant key-expansion call shape (`vocab=Some(Keep)`,
    /// `document_relative=false`); other shapes bypass this cache. Invalidated
    /// alongside the compact-IRI / inverse caches whenever the context's term
    /// definitions, base IRI, vocabulary, language, or direction change.
    pub fn term_resolution_cache(&self) -> &TermResolutionCache<T, B> {
        self.term_resolution_cache.get_or_init(|| Mutex::new(crate::HashMap::default()))
    }

    /// Returns the cached keyword aliases for this context, computing them via
    /// `init` on first access. The closure runs once per context lifetime;
    /// subsequent calls return the cached value.
    pub fn keyword_aliases_or_init<F: FnOnce() -> KeywordAliases>(&self, init: F) -> &KeywordAliases {
        self.keyword_aliases.get_or_init(init)
    }

    /// Drops the inverse-context and compact-IRI caches if they are populated.
    ///
    /// Replaces each cache `Arc` with a fresh empty one.
    ///
    /// We can't `take()` from a shared `Arc<OnceCell<_>>` (it requires `&mut`
    /// access to the OnceCell, which the Arc doesn't grant). Replacing the
    /// `Arc` itself diverges this context's caches from any sharers — which
    /// is exactly what we want: other holders had a context with the
    /// pre-mutation state and their caches are still valid for that state.
    #[inline]
    fn invalidate_iri_caches(&mut self) {
        if self.inverse.get().is_some() {
            self.inverse = Arc::new(OnceCell::new());
        }
        if self.compact_iri_cache.get().is_some() {
            self.compact_iri_cache = Arc::new(OnceCell::new());
        }
        if self.term_resolution_cache.get().is_some() {
            self.term_resolution_cache = Arc::new(OnceCell::new());
        }
        if self.keyword_aliases.get().is_some() {
            self.keyword_aliases = Arc::new(OnceCell::new());
        }
    }

    /// Sets the normal definition for the given term `key`.
    pub fn set_normal(&mut self, key: Key, definition: Option<NormalTermDefinition<T, B>>) -> Option<NormalTermDefinition<T, B>>
    where
        T: Clone,
        B: Clone,
    {
        self.invalidate_iri_caches();
        if self.prefix_terms.get().is_some() {
            self.prefix_terms = Arc::new(OnceCell::new());
        }
        Arc::make_mut(&mut self.definitions).set_normal(key, definition)
    }

    /// Sets the `@type` definition.
    pub fn set_type(&mut self, type_: Option<TypeTermDefinition>) -> Option<TypeTermDefinition>
    where
        T: Clone,
        B: Clone,
    {
        if self.compact_iri_cache.get().is_some() {
            self.compact_iri_cache = Arc::new(OnceCell::new());
        }
        if self.term_resolution_cache.get().is_some() {
            self.term_resolution_cache = Arc::new(OnceCell::new());
        }
        if self.keyword_aliases.get().is_some() {
            self.keyword_aliases = Arc::new(OnceCell::new());
        }
        Arc::make_mut(&mut self.definitions).set_type(type_)
    }

    /// Sets the base IRI.
    pub fn set_base_iri(&mut self, iri: Option<T>) {
        self.invalidate_iri_caches();
        self.base_iri = iri
    }

    /// Sets the `@vocab` value.
    pub fn set_vocabulary(&mut self, vocab: Option<Term<T, B>>) {
        self.invalidate_iri_caches();
        self.vocabulary = vocab;
    }

    /// Sets the default `@language` value.
    pub fn set_default_language(&mut self, lang: Option<LenientLangTagBuf>) {
        self.invalidate_iri_caches();
        self.default_language = lang;
    }

    /// Sets the default `@direction` value.
    pub fn set_default_base_direction(&mut self, dir: Option<Direction>) {
        self.invalidate_iri_caches();
        self.default_base_direction = dir;
    }

    /// Sets the previous context.
    pub fn set_previous_context(&mut self, previous: Self) {
        self.invalidate_iri_caches();
        self.previous_context = Some(Arc::new(previous))
    }

    /// Converts this context into its syntactic definition.
    pub fn into_syntax_definition(self, vocabulary: &impl Vocabulary<Iri = T, BlankId = B>) -> Result<jsonld_syntax::context::Definition, InvalidContextError>
    where
        T: Clone,
        B: Clone,
    {
        let (bindings, type_) = Arc::unwrap_or_clone(self.definitions).into_parts();

        let vocab = match self.vocabulary {
            Some(Term::Null) => Some(Nullable::Null),
            Some(Term::Id(r)) => Some(Nullable::Some(r.with(vocabulary).to_string().into())),
            Some(Term::Keyword(_)) => return Err(InvalidContextError::InvalidVocab(InvalidVocab::Keyword)),
            None => None,
        };

        Ok(jsonld_syntax::context::Definition {
            base: self.base_iri.map(|i| {
                // SAFETY: `i` was inserted into `vocabulary` when this context
                // was built.
                let iri: iri_rs::IriBuf = unsafe { vocabulary.iri(&i).unwrap_unchecked() }.into();
                Nullable::Some(iri.into())
            }),
            import: None,
            language: self.default_language.map(Nullable::Some),
            direction: self.default_base_direction.map(Nullable::Some),
            propagate: None,
            protected: None,
            type_: type_.map(TypeTermDefinition::into_syntax_definition),
            version: None,
            vocab,
            bindings: bindings
                .into_iter()
                .map(|(key, definition)| definition.into_syntax_definition(vocabulary).map(|d| (key, d)))
                .collect::<Result<_, _>>()?,
        })
    }

    /// Rewrites every IRI and identifier of this context with the given
    /// functions.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> Context<U, C>
    where
        T: Clone,
        B: Clone,
    {
        self.map_ids_with(&mut map_iri, &mut map_id)
    }

    fn map_ids_with<U, C>(self, map_iri: &mut impl FnMut(T) -> U, map_id: &mut impl FnMut(Id<T, B>) -> Id<U, C>) -> Context<U, C>
    where
        T: Clone,
        B: Clone,
    {
        Context {
            original_base_url: self.original_base_url.map(&mut *map_iri),
            base_iri: self.base_iri.map(&mut *map_iri),
            vocabulary: self.vocabulary.map(|v| v.map_id(&mut *map_id)),
            default_language: self.default_language,
            default_base_direction: self.default_base_direction,
            previous_context: self.previous_context.map(|c| Arc::new(Arc::unwrap_or_clone(c).map_ids_with(map_iri, map_id))),
            definitions: Arc::new(Arc::unwrap_or_clone(self.definitions).map_ids(map_iri, map_id)),
            processing_mode: self.processing_mode,
            inverse: Arc::new(OnceCell::new()),
            prefix_terms: Arc::new(OnceCell::new()),
            compact_iri_cache: Arc::new(OnceCell::new()),
            term_resolution_cache: Arc::new(OnceCell::new()),
            keyword_aliases: Arc::new(OnceCell::new()),
        }
    }
}

/// Context fragment to syntax method.
pub trait IntoSyntax<T = IriBuf, B = BlankIdBuf> {
    /// Consumes this `IntoSyntax`, returning its syntax.
    fn into_syntax(self, vocabulary: &impl Vocabulary<Iri = T, BlankId = B>) -> Result<jsonld_syntax::context::Context, InvalidContextError>;
}

impl<T, B> IntoSyntax<T, B> for jsonld_syntax::context::Context {
    fn into_syntax(self, _namespace: &impl Vocabulary<Iri = T, BlankId = B>) -> Result<jsonld_syntax::context::Context, InvalidContextError> {
        Ok(self)
    }
}

impl<T: Clone, B: Clone> IntoSyntax<T, B> for Context<T, B> {
    fn into_syntax(self, vocabulary: &impl Vocabulary<Iri = T, BlankId = B>) -> Result<jsonld_syntax::context::Context, InvalidContextError> {
        Ok(jsonld_syntax::context::Context::One(jsonld_syntax::ContextEntry::Definition(
            self.into_syntax_definition(vocabulary)?,
        )))
    }
}

impl<T: Clone, B: Clone> Clone for Context<T, B> {
    fn clone(&self) -> Self {
        // Share caches across clones via `Arc::clone`. Sound because processed
        // contexts are not mutated post-processing — the only mutating APIs
        // (`set_normal`, `set_type`, `set_base_iri`, etc.) call
        // `invalidate_iri_caches`, which replaces the `Arc` with a fresh one
        // so the mutated copy diverges from any sharers.
        Self {
            original_base_url: self.original_base_url.clone(),
            base_iri: self.base_iri.clone(),
            vocabulary: self.vocabulary.clone(),
            default_language: self.default_language.clone(),
            default_base_direction: self.default_base_direction,
            previous_context: self.previous_context.clone(),
            definitions: Arc::clone(&self.definitions),
            processing_mode: self.processing_mode,
            inverse: Arc::clone(&self.inverse),
            prefix_terms: Arc::clone(&self.prefix_terms),
            compact_iri_cache: Arc::clone(&self.compact_iri_cache),
            term_resolution_cache: Arc::clone(&self.term_resolution_cache),
            keyword_aliases: Arc::clone(&self.keyword_aliases),
        }
    }
}

impl<T: PartialEq, B: PartialEq> PartialEq for Context<T, B> {
    fn eq(&self, other: &Self) -> bool {
        self.original_base_url == other.original_base_url
            && self.base_iri == other.base_iri
            && self.vocabulary == other.vocabulary
            && self.default_language == other.default_language
            && self.default_base_direction == other.default_base_direction
            && self.previous_context == other.previous_context
    }
}
