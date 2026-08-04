//! JSON-LD context processing types and algorithms.
use algorithm::{Action, RejectVocab};
pub use jsonld_core::{Context, ProcessingMode, warning};
use jsonld_core::{ExtractContextError, LoadError, Loader};
use jsonld_syntax::ErrorCode;
use rdfx::vocabulary::VocabularyMut;
use std::{fmt, hash::Hash};

/// The context processing algorithm itself.
pub mod algorithm;
mod cache;
mod processed;
mod stack;

pub use cache::ProcessingCache;
pub use processed::*;
pub use stack::{MAX_REMOTE_CONTEXTS, ProcessingStack};

/// Warnings that can be raised during context processing.
pub enum Warning {
    /// A term looks like a keyword and was ignored.
    KeywordLikeTerm(String),
    /// A value looks like a keyword and was ignored.
    KeywordLikeValue(String),
    /// A term expanded to a malformed IRI.
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

/// Handlers collecting the warnings raised by context processing.
pub trait WarningHandler<N>: jsonld_core::warning::Handler<N, Warning> {}

impl<N, H> WarningHandler<N> for H where H: jsonld_core::warning::Handler<N, Warning> {}

/// Errors that can happen during context processing.
#[derive(Debug, thiserror::Error)]
pub enum Error<E = std::convert::Infallible> {
    #[error("Invalid context nullification")]
    /// Invalid context nullification.
    InvalidContextNullification,

    #[error("Remote document loading failed")]
    /// Remote document loading failed.
    LoadingDocumentFailed,

    #[error("Processing mode conflict")]
    /// Processing mode conflict.
    ProcessingModeConflict,

    #[error("Invalid `@context` entry")]
    /// Invalid `@context` entry.
    InvalidContextEntry,

    #[error("Invalid `@import` value")]
    /// Invalid `@import` value.
    InvalidImportValue,

    #[error("Invalid remote context")]
    /// Invalid remote context.
    InvalidRemoteContext,

    #[error("Invalid base IRI")]
    /// Invalid base IRI.
    InvalidBaseIri,

    #[error("Invalid vocabulary mapping")]
    /// Invalid vocabulary mapping.
    InvalidVocabMapping,

    #[error("Cyclic IRI mapping")]
    /// Cyclic IRI mapping.
    CyclicIriMapping,

    #[error("Invalid term definition")]
    /// Invalid term definition.
    InvalidTermDefinition,

    #[error("Keyword redefinition")]
    /// Keyword redefinition.
    KeywordRedefinition,

    #[error("Invalid `@protected` value")]
    /// Invalid `@protected` value.
    InvalidProtectedValue,

    #[error("Invalid type mapping")]
    /// Invalid type mapping.
    InvalidTypeMapping,

    #[error("Invalid reverse property")]
    /// Invalid reverse property.
    InvalidReverseProperty,

    #[error("Invalid IRI mapping")]
    /// Invalid IRI mapping.
    InvalidIriMapping,

    #[error("Invalid keyword alias")]
    /// Invalid keyword alias.
    InvalidKeywordAlias,

    #[error("Invalid container mapping")]
    /// Invalid container mapping.
    InvalidContainerMapping,

    #[error("Invalid scoped context")]
    /// Invalid scoped context.
    InvalidScopedContext,

    #[error("Protected term redefinition")]
    /// Protected term redefinition.
    ProtectedTermRedefinition,

    #[error(transparent)]
    /// A referenced context could not be loaded.
    ContextLoadingFailed(#[from] LoadError<E>),

    #[error("Unable to extract JSON-LD context: {0}")]
    /// Unable to extract JSON-LD context: the given value.
    ContextExtractionFailed(ExtractContextError),

    #[error("Recursive context inclusion")]
    /// A remote context includes itself, directly or indirectly.
    ///
    /// JSON-LD 1.0 only: 1.1 permits a context to be reloaded (scoped contexts
    /// rely on it) and reports [`ErrorCode::ContextOverflow`] instead.
    RecursiveContextInclusion,

    #[error("Context overflow")]
    /// The chain of remote contexts exceeded the processor limit
    /// ([`MAX_REMOTE_CONTEXTS`]), as mandated by the [context processing
    /// algorithm][1] to protect against unbounded remote context chains.
    ///
    /// [1]: <https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm>
    ContextOverflow,

    #[error("Use of forbidden `@vocab`")]
    /// Use of forbidden `@vocab`.
    ForbiddenVocab,
}

impl<E> From<RejectVocab> for Error<E> {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

impl<E> Error<E> {
    /// Returns the code of this `Error`.
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
            Self::InvalidProtectedValue => ErrorCode::InvalidPropagateValue,
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

/// Result of context processing functions.
pub type ProcessingResult<'a, T, B, E> = Result<Processed<'a, T, B>, Error<E>>;

/// Contexts that can be processed into an active context.
pub trait Process {
    /// Process the local context with specific options.
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

    /// Process the local context, consulting `cache` first and storing the
    /// result on a miss.
    ///
    /// The cache must outlive the active context and local context references
    /// to remain sound. See [`ProcessingCache`] for the soundness contract.
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

    /// Process the local context, consulting `cache` first and storing the
    /// result on a miss. Convenience wrapper using the default warning handler.
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

    /// Process the local context with specific options.
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

    /// Process the local context with the given initial active context with the default options:
    /// `is_remote` is `false`, `override_protected` is `false` and `propagate` is `true`.
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

/// Options of the Context Processing Algorithm.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The processing mode
    pub processing_mode: ProcessingMode,

    /// Override protected definitions.
    pub override_protected: bool,

    /// Propagate the processed context.
    pub propagate: bool,

    /// Forbid the use of `@vocab` to expand terms.
    pub vocab: Action,
}

impl Options {
    /// Return the same set of options, but with `override_protected` set to `true`.
    #[must_use]
    pub fn with_override(&self) -> Options {
        let mut opt = *self;
        opt.override_protected = true;
        opt
    }

    /// Return the same set of options, but with `override_protected` set to `false`.
    #[must_use]
    pub fn with_no_override(&self) -> Options {
        let mut opt = *self;
        opt.override_protected = false;
        opt
    }

    /// Return the same set of options, but with `propagate` set to `false`.
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
