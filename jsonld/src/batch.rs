//! Document-level parallel helpers.
//!
//! Single-thread I/O concurrency via [`futures::stream::FuturesOrdered`]:
//! each task owns an independent vocabulary clone and proceeds whenever any
//! other task awaits the loader, so I/O-bound workloads benefit. CPU-bound
//! batches see no speedup — true CPU parallelism would require changing the
//! [`crate::Loader`] trait so its `async fn` returns `Send` futures.

use crate::{
    Loader,
    RemoteDocument,
    expansion::{Expand, ExpansionResult},
};
use futures::stream::{FuturesOrdered, StreamExt};
use jsonld_core::ParallelSafeVocabulary;
use rdf_rs::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Expand a batch of remote documents concurrently.
///
/// Vocabulary is cloned once per document; loader is shared by reference.
/// Returns one result per input document, in input order.
pub async fn batch_expand<'a, I, N, L>(
    docs: Vec<RemoteDocument<I>>,
    vocabulary: &'a N,
    loader: &'a L,
) -> Vec<ExpansionResult<I, N::BlankId>>
where
    N: VocabularyMut<Iri = I> + ParallelSafeVocabulary,
    I: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
{
    let mut stream: FuturesOrdered<_> = docs
        .into_iter()
        .map(|doc| {
            let mut vocab: N = vocabulary.clone();
            Box::pin(async move {
                let result = doc.expand_with(&mut vocab, loader).await;
                // Drop `vocab` after the future completes; result borrows nothing from it.
                let _ = vocab;
                result
            })
        })
        .collect();

    let mut results = Vec::new();
    while let Some(r) = stream.next().await {
        results.push(r);
    }
    results
}
