//! Invoking the `LoadDocumentCallback` from inside an algorithm.
//!
//! The algorithms recurse once per nesting level of their input, and keep that
//! recursion off the caller's native stack by checking, at every recursion
//! point, that at least [`RED_ZONE`] of it is left
//! ([`native_stack`](jsonld_syntax::native_stack)). Dereferencing a document
//! leaves the algorithm for the embedder's [`Loader`]: an HTTP client behind a
//! cache, say, whose native stack per poll the crate cannot bound. Polled
//! without a check of its own, a loader runs on whatever the last recursion
//! point left, which may be only a little more than the red zone.
//!
//! [`Dereference::dereference`] is therefore the one way an algorithm invokes a
//! loader: every poll of the loader runs as a recursion point does, so it
//! always starts with at least [`RED_ZONE`] of native stack. The workspace
//! `clippy.toml` disallows calling [`Loader::load`] anywhere else. It is also
//! where a loaded document's IRIs enter the caller's vocabulary.
//!
//! [`RED_ZONE`]: jsonld_syntax::native_stack::RED_ZONE

use crate::loader::{Loader, LoadingResult};
use jsonld_syntax::native_stack::Recursion;
use rdfx::vocabulary::IriVocabularyMut;
use std::{future::Future, hash::Hash};

/// Dereferencing a document through a [`Loader`] on sufficient native stack.
///
/// Implemented for every loader; an algorithm calls [`Self::dereference`]
/// where the JSON-LD 1.1 API invokes the `LoadDocumentCallback`.
pub trait Dereference: Loader {
    /// Loads the document behind `url`, using `vocabulary`, polling the loader
    /// on at least [`RED_ZONE`](jsonld_syntax::native_stack::RED_ZONE) of
    /// native stack.
    ///
    /// # Errors
    ///
    /// Returns the loader's error when the document cannot be fetched or
    /// parsed.
    ///
    /// # Panics
    ///
    /// Panics if `url` does not resolve in `vocabulary`, i.e. if it was
    /// obtained from a different vocabulary.
    fn dereference<V>(&self, vocabulary: &mut V, url: V::Iri) -> impl Future<Output = LoadingResult<V::Iri, Self::Error>>
    where
        V: IriVocabularyMut,
        V::Iri: Clone + Eq + Hash;
}

impl<L: Loader + ?Sized> Dereference for L {
    fn dereference<V>(&self, vocabulary: &mut V, url: V::Iri) -> impl Future<Output = LoadingResult<V::Iri, Self::Error>>
    where
        V: IriVocabularyMut,
        V::Iri: Clone + Eq + Hash,
    {
        Recursion::new(async move {
            #[allow(clippy::expect_used)]
            let lexical_url = vocabulary.iri(&url).expect("`url` does not resolve in the given vocabulary");
            #[expect(clippy::disallowed_methods, reason = "the one guarded invocation of a loader every algorithm goes through")]
            let document = self.load(lexical_url).await?;
            Ok(document.map_iris(|i| vocabulary.insert_owned(i)))
        })
    }
}
