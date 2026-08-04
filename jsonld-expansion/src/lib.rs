//! Implementation of the JSON-LD 1.1
//! [Expansion algorithm](https://www.w3.org/TR/json-ld11-api/#expansion-algorithms).
//!
//! Expansion rewrites a JSON-LD document into a regular form: terms and
//! compact IRIs become full IRIs, the defaults a context sets (`@base`,
//! `@vocab`, `@language`...) are applied to every value, and the `@context`
//! entries themselves disappear. It is the first step of almost every other
//! JSON-LD algorithm, since it removes the many ways the syntax offers to say
//! the same thing.
//!
//! The algorithm operates on the JSON syntax tree of the `jstrict` crate and
//! produces the [`ExpandedDocument`] type of the `jsonld-core` crate.
//!
//! # Usage
//!
//! Expansion is exposed by the [`Expand`] trait, implemented for
//! [`jstrict::Value`] (a parsed JSON document) and for [`RemoteDocument`]
//! (a parsed JSON document along with the URL it comes from, used to resolve
//! relative IRI references).
//!
//! Most users will not depend on this crate directly but on the
//! [`jsonld` crate](https://crates.io/crates/jsonld), which re-exports
//! [`Expand`] next to the other JSON-LD algorithms.
use std::hash::Hash;

// Re-exported because they appear in the public `Expand` signatures:
// standalone users should not need to depend on `jsonld-core` and
// `jsonld-context-processing` just to name them.
pub use jsonld_context_processing::Context;
pub use jsonld_core::{Environment, ExpandedDocument, Loader, RemoteDocument};

use jstrict::Value;
use rdfx::{
    BlankIdBuf,
    vocabulary::{self, BlankIdVocabulary, VocabularyMut},
};

mod array;
mod document;
mod element;
mod error;
mod expanded;
mod literal;
mod node;
mod options;
mod value;
mod warning;

pub use error::*;
pub use expanded::*;
pub use literal::{LiteralExpansionError, NotALiteral};
pub use options::*;
pub use value::InvalidValue;
pub use warning::*;

pub(crate) use array::expand_array;
pub(crate) use document::filter_top_level_item;
pub(crate) use element::{ActiveProperty, ExpandedEntry, expand_element};
pub(crate) use jsonld_context_processing::algorithm::expand_iri_simple as expand_iri;
pub(crate) use literal::{GivenLiteralValue, LiteralValue, expand_literal};
pub(crate) use node::{expand_node, node_id_of_term};
pub(crate) use value::expand_value;

/// Result of a document expansion, where `E` is the error type of the document
/// loader used to fetch the remote contexts.
pub type ExpansionResult<T, B, E> = Result<ExpandedDocument<T, B>, Error<E>>;

/// Handler for the warnings emitted while expanding a JSON-LD document.
///
/// This is an alias for a [`jsonld_core::warning::Handler`] of [`Warning`],
/// automatically implemented by every such handler. The unit type `()` is a
/// handler that discards every warning.
pub trait WarningHandler<N: BlankIdVocabulary>: jsonld_core::warning::Handler<N, Warning<N::BlankId>> {}

impl<N: BlankIdVocabulary, H> WarningHandler<N> for H where H: jsonld_core::warning::Handler<N, Warning<N::BlankId>> {}

/// Document expansion.
///
/// Provides the functions expanding a JSON-LD document into an
/// [`ExpandedDocument`]. It is implemented by [`jstrict::Value`], for a
/// document parsed from JSON, and by [`RemoteDocument`], for a parsed document
/// carrying the URL it comes from.
///
/// # Example
///
/// ```
/// use iri_rs::iri;
/// use jsonld::{Expand, NoLoader, syntax::Parse};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // Parse the input JSON-LD document.
/// let (json, _) = jsonld::syntax::Value::parse_str(
///   r#"
///   {
///     "@context": { "label": "http://example.org/vocab#label" },
///     "@id": "http://example.org/book",
///     "label": "Chapter One"
///   }
///   "#)
/// .unwrap();
///
/// // Expanding this document requires no remote context, so a loader that
/// // fetches nothing will do.
/// let loader = NoLoader;
///
/// let expanded = json.expand(&loader).await.unwrap();
///
/// // The `label` term has been replaced by the IRI it is defined as.
/// let object = expanded.into_iter().next().unwrap();
/// let label = object
///   .as_node().unwrap()
///   .get_any(&iri!("http://example.org/vocab#label")).unwrap()
///   .as_str().unwrap();
///
/// assert_eq!(label, "Chapter One");
/// # }
/// ```
pub trait Expand<Iri> {
    /// Returns the base URL the document is expanded against when none is
    /// given explicitly, as in [`Expand::expand`] and [`Expand::expand_with`].
    ///
    /// It is also the base URL of the empty initial context those two methods
    /// build.
    fn default_base_url(&self) -> Option<&Iri>;

    /// Expands the document, controlling every parameter of the algorithm.
    ///
    /// - `vocabulary` interprets the identifiers of the input and mints those
    ///   of the result;
    /// - `context` is the initial active context;
    /// - `base_url` is the initial base URL, against which relative IRI
    ///   references are resolved;
    /// - `loader` fetches the remote documents (contexts, mostly) the input
    ///   refers to;
    /// - `options` tune the algorithm: processing mode, key expansion policy
    ///   and entry ordering;
    /// - `warnings_handler` is called for each warning raised during
    ///   expansion, including the warnings raised while processing scoped and
    ///   local `@context`s, which arrive wrapped in
    ///   [`Warning::ContextProcessing`].
    async fn expand_full<N, L, W>(
        &self,
        vocabulary: &mut N,
        context: Context<Iri, N::BlankId>,
        base_url: Option<&N::Iri>,
        loader: &L,
        options: Options,
        warnings_handler: W,
    ) -> ExpansionResult<N::Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut<Iri = Iri>,
        Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
        W: WarningHandler<N>;

    /// Expands the document, interpreting identifiers with the given
    /// `vocabulary`.
    ///
    /// The algorithm starts from an empty context whose base URL is
    /// [`Expand::default_base_url`], runs with the default [`Options`] and
    /// discards warnings. The given `loader` fetches the remote documents
    /// (contexts, mostly) the input refers to.
    async fn expand_with<'a, N, L>(&'a self, vocabulary: &'a mut N, loader: &'a L) -> ExpansionResult<Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut<Iri = Iri>,
        Iri: 'a + Clone + Eq + Hash,
        N::BlankId: 'a + Clone + Eq + Hash,
        L: Loader,
    {
        self.expand_full(
            vocabulary,
            Context::<N::Iri, N::BlankId>::new(self.default_base_url().cloned()),
            self.default_base_url(),
            loader,
            Options::default(),
            (),
        )
        .await
    }

    /// Expands the document, keeping identifiers as `IriBuf` and
    /// [`BlankIdBuf`] values instead of interning them in a vocabulary.
    ///
    /// Otherwise behaves like [`Expand::expand_with`]: the algorithm starts
    /// from an empty context whose base URL is [`Expand::default_base_url`],
    /// runs with the default [`Options`] and discards warnings. The given
    /// `loader` fetches the remote documents (contexts, mostly) the input
    /// refers to.
    async fn expand<'a, L>(&'a self, loader: &'a L) -> ExpansionResult<Iri, BlankIdBuf, L::Error>
    where
        (): VocabularyMut<Iri = Iri>,
        Iri: 'a + Clone + Eq + Hash,
        L: Loader,
    {
        self.expand_with(vocabulary::no_vocabulary_mut(), loader).await
    }
}

/// A parsed JSON document has no URL of its own, hence no default base URL:
/// relative IRI references it contains can only be resolved against a base URL
/// given explicitly to [`Expand::expand_full`].
impl<Iri> Expand<Iri> for Value {
    fn default_base_url(&self) -> Option<&Iri> {
        None
    }

    async fn expand_full<N, L, W>(
        &self,
        vocabulary: &mut N,
        context: Context<Iri, N::BlankId>,
        base_url: Option<&Iri>,
        loader: &L,
        options: Options,
        mut warnings_handler: W,
    ) -> ExpansionResult<Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut<Iri = Iri>,
        Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
        W: WarningHandler<N>,
    {
        document::expand(
            Environment {
                vocabulary,
                loader,
                warnings: &mut warnings_handler,
            },
            self,
            context,
            base_url,
            options,
        )
        .await
    }
}

/// A remote document knows where it comes from: its own URL is used as default
/// base URL, so relative IRI references resolve as they would for a consumer
/// fetching the document.
impl<Iri> Expand<Iri> for RemoteDocument<Iri> {
    fn default_base_url(&self) -> Option<&Iri> {
        self.url()
    }

    async fn expand_full<N, L, W>(
        &self,
        vocabulary: &mut N,
        context: Context<Iri, N::BlankId>,
        base_url: Option<&Iri>,
        loader: &L,
        options: Options,
        warnings_handler: W,
    ) -> ExpansionResult<Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut<Iri = Iri>,
        Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
        W: WarningHandler<N>,
    {
        self.document()
            .expand_full(vocabulary, context, base_url, loader, options, warnings_handler)
            .await
    }
}
