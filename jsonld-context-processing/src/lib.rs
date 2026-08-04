//! Implementation of the [JSON-LD context processing algorithm][1].
//!
//! Context processing turns a `@context` as it appears in a document — a
//! syntactic [`jsonld_syntax::context::Context`] — into an *active context*: a
//! [`Context`] holding the term definitions, base IRI, vocabulary mapping,
//! default language and default base direction that expansion and compaction
//! consult. Remote contexts referenced by IRI are fetched through a
//! [`Loader`], and `@import` is resolved by merging the imported definition
//! underneath the importing one.
//!
//! The entry point is the [`Process`] trait, implemented for
//! [`jsonld_syntax::context::Context`]. [`Process::process`] uses a fresh
//! active context and the default [`Options`];
//! [`Process::process_full`] takes every parameter, including the warning
//! handler. Documents that reuse the same `@context` across many nodes should
//! go through [`Process::process_with_cache`] with a [`ProcessingCache`], which
//! collapses repeated processing of an identical context to a single run.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
use algorithm::{Action, RejectVocab};
pub use jsonld_core::{Context, ProcessingMode, warning};
use jsonld_core::{ExtractContextError, LoadError, Loader};
use jsonld_syntax::ErrorCode;
use rdfx::vocabulary::VocabularyMut;
use std::{fmt, hash::Hash};

/// The context processing algorithm itself, and the pieces it is built from.
pub mod algorithm;
mod cache;
mod processed;
mod stack;

pub use cache::ProcessingCache;
pub use processed::*;
pub use stack::{MAX_REMOTE_CONTEXTS, ProcessingStack};

/// Warnings that can be raised during context processing.
///
/// None of these abort processing; the specification asks a processor to report
/// them and carry on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A term being expanded is not a keyword but has the shape of one (it
    /// starts with `@` and is otherwise alphabetic), so it expands to null.
    ///
    /// The specification allows this to be reported at that point; this crate
    /// currently drops such terms silently and does not raise it.
    KeywordLikeTerm(String),
    /// The `@id` or `@reverse` value of a term definition is not a keyword but
    /// has the shape of one, so the whole term definition is skipped.
    KeywordLikeValue(String),
    /// A value being expanded resolved to neither a well-formed IRI nor a blank
    /// node identifier, and is retained verbatim as an invalid identifier.
    MalformedIri(String),
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::KeywordLikeTerm(s) => write!(f, "keyword-like term `{s}`"),
            Self::KeywordLikeValue(s) => write!(f, "keyword-like value `{s}`"),
            Self::MalformedIri(s) => write!(f, "malformed IRI `{s}`"),
        }
    }
}

impl<N> contextual::DisplayWithContext<N> for Warning {
    fn fmt_with(&self, _: &N, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Handlers that can receive the [`Warning`]s raised during context processing.
///
/// A blanket alias for [`jsonld_core::warning::Handler<N, Warning>`][h], so any
/// handler of `Warning` qualifies — including [`warning::Print`] to print them
/// to standard error, `()` to discard them, and [`warning::WarningBuf`] to
/// collect them for inspection.
///
/// [h]: jsonld_core::warning::Handler
/// [`warning::Print`]: jsonld_core::warning::Print
/// [`warning::WarningBuf`]: jsonld_core::warning::WarningBuf
pub trait WarningHandler<N>: jsonld_core::warning::Handler<N, Warning> {}

impl<N, H> WarningHandler<N> for H where H: jsonld_core::warning::Handler<N, Warning> {}

/// Errors that can happen during context processing.
#[derive(Debug, thiserror::Error)]
pub enum Error<E = std::convert::Infallible> {
    #[error("Invalid context nullification")]
    /// A `null` context tried to clear an active context that holds protected
    /// term definitions, outside of a scope allowed to override them.
    InvalidContextNullification,

    #[error("Remote document loading failed")]
    /// A remote `@context` IRI could not be resolved against the base URL.
    ///
    /// Also used as a guard by the synchronous fast path, which refuses to
    /// continue if it meets a remote `@context` or an `@import` that the
    /// loader would have to fetch.
    LoadingDocumentFailed,

    #[error("Processing mode conflict")]
    /// A context declares `@version` while the processor is running in JSON-LD
    /// 1.0 mode.
    ProcessingModeConflict,

    #[error("Invalid `@context` entry")]
    /// A context entry is not permitted in the current processing mode.
    ///
    /// Raised for `@propagate`, `@import` and `@direction`, all of which are
    /// JSON-LD 1.1 additions, and for an `@import`ed context that itself
    /// contains `@import`.
    InvalidContextEntry,

    #[error("Invalid `@import` value")]
    /// An `@import` value could not be resolved into an IRI against the base
    /// URL.
    InvalidImportValue,

    #[error("Invalid remote context")]
    /// An `@import`ed document's `@context` is not a single context definition
    /// object.
    InvalidRemoteContext,

    #[error("Invalid base IRI")]
    /// A `@base` value is neither an absolute IRI nor resolvable against the
    /// context's current base IRI.
    InvalidBaseIri,

    #[error("Invalid vocabulary mapping")]
    /// A `@vocab` value did not expand to an IRI or blank node identifier.
    ///
    /// Also raised in JSON-LD 1.0 mode for a document-relative `@vocab`, which
    /// only became legal in 1.1.
    InvalidVocabMapping,

    #[error("Cyclic IRI mapping")]
    /// A term definition depends on itself, directly or through a chain of
    /// prefixes or `@id` values.
    CyclicIriMapping,

    #[error("Invalid term definition")]
    /// A term definition is malformed or uses a JSON-LD 1.1 entry in 1.0 mode.
    ///
    /// Raised for an empty term; for `@protected`, `@context`, `@nest` or
    /// `@prefix` under JSON-LD 1.0; for `@index` under JSON-LD 1.0 or without an
    /// `@index` container; for `@prefix` on a term containing `:` or `/`; for
    /// `@prefix` on a keyword alias; and for a stray `@propagate` inside a term
    /// definition.
    InvalidTermDefinition,

    #[error("Keyword redefinition")]
    /// A context tried to define a keyword as a term.
    ///
    /// `@type` is the one exception, and only under JSON-LD 1.1, where it may be
    /// given `@container: @set` and `@protected`.
    KeywordRedefinition,

    #[error("Invalid `@protected` value")]
    /// An `@protected` entry holds something other than a boolean.
    ///
    /// Rejected while parsing the context syntax, so context processing itself
    /// never raises this variant.
    InvalidProtectedValue,

    #[error("Invalid type mapping")]
    /// A `@type` entry in a term definition does not name a usable type.
    ///
    /// Raised when it expands to something that is not an IRI, `@id`, `@vocab`,
    /// `@json` or `@none`; when it is `@json` or `@none` under JSON-LD 1.0; and
    /// when a term with an `@type` container maps to a type other than `@id` or
    /// `@vocab`.
    InvalidTypeMapping,

    #[error("Invalid reverse property")]
    /// An `@reverse` term definition is malformed.
    ///
    /// Raised when it also carries `@id` or `@nest`, or when its `@container` is
    /// anything other than `@set`, `@index` or null.
    InvalidReverseProperty,

    #[error("Invalid IRI mapping")]
    /// A term could not be given an IRI mapping.
    ///
    /// Raised when an `@id` or `@reverse` value does not expand to an IRI or
    /// blank node identifier, when a term that looks like a compact IRI or a
    /// path does not expand back to its own `@id` (JSON-LD 1.1 only), and when a
    /// term has no `@id` and cannot be resolved through a prefix or the
    /// vocabulary mapping.
    InvalidIriMapping,

    #[error("Invalid keyword alias")]
    /// A term definition tried to alias `@context`, which must never be
    /// aliasable.
    InvalidKeywordAlias,

    #[error("Invalid container mapping")]
    /// A `@container` value is not one of the permitted keywords or
    /// combinations, or uses `@graph`, `@id`, `@type`, an array or null under
    /// JSON-LD 1.0.
    InvalidContainerMapping,

    #[error("Invalid scoped context")]
    /// A `@context` inside a term definition failed to process.
    ///
    /// The underlying error is deliberately replaced by this one, as the
    /// specification requires. The one exception is
    /// [`Self::ContextOverflow`], which is passed through unchanged: a
    /// resource limit says nothing about the scoped context's validity.
    InvalidScopedContext,

    #[error("Protected term redefinition")]
    /// A term marked `@protected` was redefined with a different meaning,
    /// outside of a scope allowed to override protected terms.
    ProtectedTermRedefinition,

    #[error(transparent)]
    /// The [`Loader`] failed to fetch a document referenced by a remote
    /// `@context` or by `@import`.
    ContextLoadingFailed(#[from] LoadError<E>),

    #[error("Unable to extract JSON-LD context: {0}")]
    /// A fetched document could not be reduced to a `@context`.
    ///
    /// Raised when the document is not JSON-LD, or has no top-level object with
    /// a `@context` entry.
    ContextExtractionFailed(ExtractContextError),

    #[error("Recursive context inclusion")]
    /// A remote context includes itself, directly or indirectly.
    ///
    /// JSON-LD 1.0 only. Under 1.1 a context already on the remote-context
    /// stack is skipped instead of being reprocessed — scoped contexts rely on
    /// being able to reference the same context more than once — so only the
    /// [`Self::ContextOverflow`] limit constrains the chain.
    RecursiveContextInclusion,

    #[error("Context overflow")]
    /// A processor-defined resource limit on context nesting was exceeded.
    ///
    /// Raised when the chain of remote contexts grows past
    /// [`MAX_REMOTE_CONTEXTS`], which is the limit the [context processing
    /// algorithm][1] mandates against unbounded remote context chains, and also
    /// when the synchronous fast path reaches its recursion depth limit on a
    /// deeply nested inline `@context`.
    ///
    /// [1]: <https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm>
    ContextOverflow,

    #[error("Use of forbidden `@vocab`")]
    /// A term had to be expanded through the vocabulary mapping while
    /// [`Options::vocab`] was set to [`Action::Reject`].
    ForbiddenVocab,
}

impl<E> From<RejectVocab> for Error<E> {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

impl<E> Error<E> {
    /// Returns the JSON-LD error code this error is reported under.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidContextNullification => ErrorCode::InvalidContextNullification,
            Self::LoadingDocumentFailed => ErrorCode::LoadingDocumentFailed,
            Self::ProcessingModeConflict => ErrorCode::ProcessingModeConflict,
            Self::InvalidContextEntry => ErrorCode::InvalidContextEntry,
            Self::InvalidImportValue => ErrorCode::InvalidImportValue,
            Self::InvalidRemoteContext => ErrorCode::InvalidRemoteContext,
            Self::InvalidBaseIri => ErrorCode::InvalidBaseIri,
            Self::InvalidVocabMapping => ErrorCode::InvalidVocabMapping,
            Self::CyclicIriMapping => ErrorCode::CyclicIriMapping,
            Self::InvalidTermDefinition => ErrorCode::InvalidTermDefinition,
            Self::KeywordRedefinition => ErrorCode::KeywordRedefinition,
            Self::InvalidProtectedValue => ErrorCode::InvalidProtectedValue,
            Self::InvalidTypeMapping => ErrorCode::InvalidTypeMapping,
            Self::InvalidReverseProperty => ErrorCode::InvalidReverseProperty,
            Self::InvalidIriMapping => ErrorCode::InvalidIriMapping,
            Self::InvalidKeywordAlias => ErrorCode::InvalidKeywordAlias,
            Self::InvalidContainerMapping => ErrorCode::InvalidContainerMapping,
            Self::InvalidScopedContext => ErrorCode::InvalidScopedContext,
            Self::ProtectedTermRedefinition => ErrorCode::ProtectedTermRedefinition,
            Self::ContextLoadingFailed(_) => ErrorCode::LoadingRemoteContextFailed,
            Self::ContextExtractionFailed(_) => ErrorCode::LoadingRemoteContextFailed,
            Self::RecursiveContextInclusion => ErrorCode::RecursiveContextInclusion,
            Self::ContextOverflow => ErrorCode::ContextOverflow,
            Self::ForbiddenVocab => ErrorCode::InvalidVocabMapping,
        }
    }
}

/// Result of context processing: the active context paired with the `@context` it
/// was built from, or the error that stopped the algorithm.
pub type ProcessingResult<'a, T, B, E> = Result<Processed<'a, T, B>, Error<E>>;

/// Contexts that can be processed into an active context.
pub trait Process {
    /// Processes this context on top of `active_context`, taking every parameter
    /// of the algorithm explicitly.
    ///
    /// `base_url` is the URL the context was written at; it is what relative
    /// `@context` IRIs, `@import` values and `@base` are resolved against.
    /// `loader` fetches remote contexts and `@import`ed documents. `warnings`
    /// receives the [`Warning`]s the algorithm reports without aborting.
    async fn process_full<N, L, W>(
        &self,
        vocabulary: &mut N,
        active_context: &Context<N::Iri, N::BlankId>,
        loader: &L,
        base_url: Option<N::Iri>,
        options: Options,
        warnings: W,
    ) -> Result<Processed<'_, N::Iri, N::BlankId>, Error<L::Error>>
    where
        N: VocabularyMut,
        N::Iri: Clone + Eq + Hash,
        N::BlankId: Clone + PartialEq,
        L: Loader,
        W: WarningHandler<N>;

    /// Processes this context as [`Process::process_full`] does, but consults
    /// `cache` first and stores the result on a miss.
    ///
    /// Worth using whenever the same `@context` is processed repeatedly against
    /// the same active context — the usual shape of a document with many nodes.
    /// See [`ProcessingCache`] for what makes a cached result reusable and for
    /// the lifetime the cache has to be given.
    async fn process_full_with_cache<N, L, W>(
        &self,
        vocabulary: &mut N,
        active_context: &Context<N::Iri, N::BlankId>,
        loader: &L,
        base_url: Option<N::Iri>,
        options: Options,
        warnings: W,
        cache: &ProcessingCache<N::Iri, N::BlankId>,
    ) -> Result<Processed<'_, N::Iri, N::BlankId>, Error<L::Error>>
    where
        N: VocabularyMut,
        N::Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
        W: WarningHandler<N>;

    /// Processes this context as [`Process::process_full_with_cache`] does,
    /// printing any warning to standard error.
    async fn process_with_cache<N, L>(
        &self,
        vocabulary: &mut N,
        active_context: &Context<N::Iri, N::BlankId>,
        loader: &L,
        base_url: Option<N::Iri>,
        options: Options,
        cache: &ProcessingCache<N::Iri, N::BlankId>,
    ) -> Result<Processed<'_, N::Iri, N::BlankId>, Error<L::Error>>
    where
        N: VocabularyMut,
        N::Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
    {
        self.process_full_with_cache(vocabulary, active_context, loader, base_url, options, warning::Print, cache)
            .await
    }

    /// Processes this context as [`Process::process_full`] does, printing any
    /// warning to standard error.
    async fn process_with<N, L>(
        &self,
        vocabulary: &mut N,
        active_context: &Context<N::Iri, N::BlankId>,
        loader: &L,
        base_url: Option<N::Iri>,
        options: Options,
    ) -> ProcessingResult<'_, N::Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut,
        N::Iri: Clone + Eq + Hash,
        N::BlankId: Clone + PartialEq,
        L: Loader,
    {
        self.process_full(vocabulary, active_context, loader, base_url, options, warning::Print).await
    }

    /// Processes this context on top of a fresh, empty active context, with the
    /// default [`Options`], printing any warning to standard error.
    ///
    /// This is the entry point for the `@context` of a document, as opposed to a
    /// scoped context layered on top of an existing one.
    async fn process<N, L>(&self, vocabulary: &mut N, loader: &L, base_url: Option<N::Iri>) -> Result<Processed<'_, N::Iri, N::BlankId>, Error<L::Error>>
    where
        N: VocabularyMut,
        N::Iri: Clone + Eq + Hash,
        N::BlankId: Clone + PartialEq,
        L: Loader,
    {
        let active_context = Context::default();
        self.process_full(vocabulary, &active_context, loader, base_url, Options::default(), warning::Print)
            .await
    }
}

/// Options of the context processing algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Which version of JSON-LD to enforce.
    ///
    /// Under [`ProcessingMode::JsonLd1_0`] the 1.1 additions — `@import`,
    /// `@propagate`, `@direction`, `@protected`, `@nest`, `@prefix`, scoped
    /// contexts, document-relative `@vocab`, and the `@graph`/`@id`/`@type`
    /// containers — are rejected rather than honoured.
    pub processing_mode: ProcessingMode,

    /// Whether this context may redefine terms marked `@protected`.
    ///
    /// Set for a scoped context, which is allowed to replace the definitions of
    /// the context it is scoped within.
    pub override_protected: bool,

    /// Whether the processed context stays in effect for nested node objects.
    ///
    /// When `false`, the context in force before processing is retained as the
    /// result's *previous context* and restored on descending into a new node
    /// object. This is what limits a type-scoped context to the node it was
    /// introduced on.
    pub propagate: bool,

    /// What to do when a term has to be expanded through the vocabulary
    /// mapping.
    ///
    /// The default, [`Action::Keep`], is the specified behaviour.
    /// [`Action::Drop`] and [`Action::Reject`] let a caller refuse terms that
    /// only resolve because of `@vocab`, which is useful for validating that a
    /// document's context covers every term it uses.
    pub vocab: Action,
}

impl Options {
    /// Returns these options with `override_protected` set to `true`.
    #[must_use]
    pub fn with_override(&self) -> Options {
        let mut opt = *self;
        opt.override_protected = true;
        opt
    }

    /// Returns these options with `override_protected` set to `false`.
    #[must_use]
    pub fn with_no_override(&self) -> Options {
        let mut opt = *self;
        opt.override_protected = false;
        opt
    }

    /// Returns these options with `propagate` set to `false`.
    #[must_use]
    pub fn without_propagation(&self) -> Options {
        let mut opt = *self;
        opt.propagate = false;
        opt
    }
}

impl Default for Options {
    fn default() -> Options {
        Options {
            processing_mode: ProcessingMode::default(),
            override_protected: false,
            propagate: true,
            vocab: Action::Keep,
        }
    }
}
