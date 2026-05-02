use super::Loader;
use crate::{LoadError, loader::RemoteDocument};
use iri_rs::{Iri, IriBuf};

/// Dummy loader.
///
/// A dummy loader that does not load anything.
/// Can be useful when you know that you will never need to load remote resource.
///
/// Raises an `LoadingDocumentFailed` at every attempt to load a resource.
#[derive(Debug, Default)]
pub struct NoLoader;

#[derive(Debug, thiserror::Error)]
#[error("no loader")]
pub struct CannotLoad;

impl Loader for NoLoader {
    type Error = CannotLoad;

    #[inline(always)]
    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        Err(LoadError::new(url.into(), CannotLoad))
    }
}
