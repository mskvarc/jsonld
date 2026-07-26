//! Document-level parallel helpers.
//!
//! Single-thread I/O concurrency via [`futures::stream::FuturesOrdered`]:
//! each task owns an independent vocabulary fork and proceeds whenever any
//! other task awaits the loader, so I/O-bound workloads benefit. CPU-bound
//! batches see no speedup — true CPU parallelism would require changing the
//! [`crate::Loader`] trait so its `async fn` returns `Send` futures.

use crate::{
    Loader,
    RemoteDocument,
    expansion::{Expand, ExpansionResult},
};
use futures::stream::{FuturesOrdered, StreamExt};
use jsonld_core::{ForkableVocabulary, ParallelSafeVocabulary};
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Expand a batch of remote documents concurrently.
///
/// Each document gets its own vocabulary fork; the loader is shared by
/// reference. As a task finishes, its fork is merged back into `vocabulary` and
/// the document it produced is rewritten through the resulting remap, so every
/// returned document speaks `vocabulary`'s identifiers. Results come back in
/// input order.
pub async fn batch_expand<'a, I, N, L>(docs: Vec<RemoteDocument<I>>, vocabulary: &'a mut N, loader: &'a L) -> Vec<ExpansionResult<I, N::BlankId, L::Error>>
where
    N: VocabularyMut<Iri = I> + ParallelSafeVocabulary,
    I: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
{
    let mut stream: FuturesOrdered<_> = docs
        .into_iter()
        .map(|doc| {
            let mut vocab = vocabulary.fork();
            Box::pin(async move {
                let result = doc.expand_with(&mut vocab, loader).await;
                (result, vocab)
            })
        })
        .collect();

    let mut results = Vec::new();
    while let Some((result, vocab)) = stream.next().await {
        // The fork is folded back even when the task failed, so the vocabulary
        // still holds everything that was interned before the error.
        let remap = vocabulary.merge(vocab);
        results.push(match result {
            Ok(document) if !remap.is_identity() => Ok(document.map_ids(|iri| remap.iri(iri), |id| remap.id(id))),
            other => other,
        });
    }
    results
}
