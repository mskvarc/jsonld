use crate::LoadError;
use iri_rs::{Iri, IriBuf};

use super::{Loader, RemoteDocument};

/// * [`ChainLoader`]: loads document from the first loader, otherwise falls back to the second one.
///
/// This can be useful for combining, for example,
/// an [`FsLoader`](super::FsLoader) for loading some contexts from a local cache,
/// and a [`ReqwestLoader`](super::ReqwestLoader) for loading any other context from the web.
///
/// Note that it is also possible to nest several [`ChainLoader`]s,
/// to combine more than two loaders.
#[derive(Debug, Clone)]
pub struct ChainLoader<L1, L2>(L1, L2);

impl<L1, L2> ChainLoader<L1, L2> {
    /// Build a new chain loader
    pub fn new(l1: L1, l2: L2) -> Self {
        ChainLoader(l1, l2)
    }
}

impl<L1, L2> Loader for ChainLoader<L1, L2>
where
    L1: Loader,
    L2: Loader,
{
    type Error = ChainError<L1::Error, L2::Error>;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        match self.0.load(url).await {
            Ok(doc) => Ok(doc),
            Err(LoadError { source: e1, .. }) => match self.1.load(url).await {
                Ok(doc) => Ok(doc),
                Err(LoadError { target, source: e2 }) => Err(LoadError::new(target, ChainError::Both(e1, e2))),
            },
        }
    }
}

/// Combined error from two chained loaders.
///
/// The second loader is only consulted after the first has failed, so a
/// chain failure always carries both errors.
#[derive(Debug, thiserror::Error)]
pub enum ChainError<A, B> {
    /// Both loaders failed.
    #[error("both loaders failed: {0}, then {1}")]
    Both(A, B),
}
