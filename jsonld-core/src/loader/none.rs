use super::Loader;
use crate::{LoadError, loader::RemoteDocument};
use iri_rs::{Iri, IriBuf};

/// Loader that never loads anything.
///
/// Useful when the documents being processed are known not to reference any
/// remote context. Every load attempt fails with [`CannotLoad`].
#[derive(Debug, Default)]
pub struct NoLoader;

#[derive(Debug, thiserror::Error)]
#[error("no loader")]
/// Error raised by a loader that never loads anything.
pub struct CannotLoad;

impl Loader for NoLoader {
    type Error = CannotLoad;

    #[inline(always)]
    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        Err(LoadError::new(url.into(), CannotLoad))
    }
}
