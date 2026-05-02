//! This library implements the [JSON-LD expansion algorithm](https://www.w3.org/TR/json-ld-api/#expansion-algorithms)
//! for the [`json-ld` crate](https://crates.io/crates/json-ld).
//!
//! # Usage
//!
//! The expansion algorithm is provided by the [`Expand`] trait.
use std::hash::Hash;

use jsonld_context_processing::Context;
use jsonld_core::{Environment, ExpandedDocument, Loader, RemoteDocument};
use jstrict::Value;
use rdf_rs::{
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

/// Sibling-count window for the `parallel` parallel branch in
/// [`array::expand_array`].
///
/// Below `PAR_LO` the per-task overhead of `FuturesOrdered` dominates the
/// useful work, so the sequential branch wins. Above `PAR_HI` cumulative
/// overhead (Box::pin allocations, queue maintenance, cache pressure)
/// outweighs the (apparent) parallel benefit observed for moderate widths.
/// Window tuned empirically against the bench corpus — see PR / bench
/// notes for the data.
#[cfg(feature = "parallel")]
pub(crate) const PAR_LO: usize = 32;
#[cfg(feature = "parallel")]
pub(crate) const PAR_HI: usize = 512;

pub use error::*;
pub use expanded::*;
pub use options::*;
pub use warning::*;

pub(crate) use array::*;
pub(crate) use document::filter_top_level_item;
pub(crate) use element::*;
pub(crate) use jsonld_context_processing::algorithm::expand_iri_simple as expand_iri;
pub(crate) use literal::*;
pub(crate) use node::*;
pub(crate) use value::*;

/// Result of the document expansion.
pub type ExpansionResult<T, B, E> = Result<ExpandedDocument<T, B>, Error<E>>;

/// Handler for the possible warnings emitted during the expansion
/// of a JSON-LD document.
pub trait WarningHandler<N: BlankIdVocabulary>: jsonld_core::warning::Handler<N, Warning<N::BlankId>> {}

impl<N: BlankIdVocabulary, H> WarningHandler<N> for H where H: jsonld_core::warning::Handler<N, Warning<N::BlankId>> {}

/// Document expansion.
///
/// This trait provides the functions necessary to expand
/// a JSON-LD document into an [`ExpandedDocument`].
/// It is implemented by [`jstrict::Value`] representing
/// a JSON object and [`RemoteDocument`].
///
/// # Example
///
/// ```
/// # mod json_ld { pub use jsonld_syntax as syntax; pub use jsonld_core::{RemoteDocument, ExpandedDocument, NoLoader}; pub use jsonld_expansion::Expand; };
///
/// use iri_rs::IriBuf;
/// use rdf_rs::BlankIdBuf;
/// use iri_rs::iri;
/// use jsonld::{syntax::Parse, RemoteDocument, Expand};
///
/// # #[async_std::test]
/// # async fn example() {
/// // Parse the input JSON(-LD) document.
/// let (json, _) = jsonld::syntax::Value::parse_str(
///   r##"
///   {
///     "@graph": [
///       {
///         "http://example.org/vocab#a": {
///           "@graph": [
///             {
///               "http://example.org/vocab#b": "Chapter One"
///             }
///           ]
///         }
///       }
///     ]
///   }
///   "##)
/// .unwrap();
///
/// // Prepare a dummy document loader using [`jsonld::NoLoader`],
/// // since we won't need to load any remote document while expanding this one.
/// let mut loader = jsonld::NoLoader;
///
/// // The `expand` method returns an [`jsonld::ExpandedDocument`].
/// json
///     .expand(&mut loader)
///     .await
///     .unwrap();
/// # }
/// ```
pub trait Expand<Iri> {
    /// Returns the default base URL passed to the expansion algorithm
    /// and used to initialize the default empty context when calling
    /// [`Expand::expand`] or [`Expand::expand_with`].
    fn default_base_url(&self) -> Option<&Iri>;

    /// Expand the document with full options.
    ///
    /// The `vocabulary` is used to interpret identifiers.
    /// The `context` is used as initial context.
    /// The `base_url` is the initial base URL used to resolve relative IRI references.
    /// The given `loader` is used to load remote documents (such as contexts)
    /// imported by the input and required during expansion.
    /// The `options` are used to tweak the expansion algorithm.
    /// The `warning_handler` is called each time a warning is emitted during expansion.
    #[allow(async_fn_in_trait)]
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
        N: VocabularyMut<Iri = Iri> + jsonld_core::ParallelSafeVocabulary,
        Iri: Clone + Eq + Hash,
        N::BlankId: Clone + Eq + Hash,
        L: Loader,
        W: WarningHandler<N>;

    /// Expand the input JSON-LD document with the given `vocabulary`
    /// to interpret identifiers.
    ///
    /// The given `loader` is used to load remote documents (such as contexts)
    /// imported by the input and required during expansion.
    /// The expansion algorithm is called with an empty initial context with
    /// a base URL given by [`Expand::default_base_url`].
    #[allow(async_fn_in_trait)]
    async fn expand_with<'a, N, L>(&'a self, vocabulary: &'a mut N, loader: &'a L) -> ExpansionResult<Iri, N::BlankId, L::Error>
    where
        N: VocabularyMut<Iri = Iri> + jsonld_core::ParallelSafeVocabulary,
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

    /// Expand the input JSON-LD document.
    ///
    /// The given `loader` is used to load remote documents (such as contexts)
    /// imported by the input and required during expansion.
    /// The expansion algorithm is called with an empty initial context with
    /// a base URL given by [`Expand::default_base_url`].
    #[allow(async_fn_in_trait)]
    async fn expand<'a, L>(&'a self, loader: &'a L) -> ExpansionResult<Iri, BlankIdBuf, L::Error>
    where
        (): VocabularyMut<Iri = Iri>,
        Iri: 'a + Clone + Eq + Hash,
        L: Loader,
    {
        self.expand_with(vocabulary::no_vocabulary_mut(), loader).await
    }
}

/// Value expansion without base URL.
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
        N: VocabularyMut<Iri = Iri> + jsonld_core::ParallelSafeVocabulary,
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

/// Remote document expansion.
///
/// The default base URL given to the expansion algorithm is the URL of
/// the remote document.
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
        N: VocabularyMut<Iri = Iri> + jsonld_core::ParallelSafeVocabulary,
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
