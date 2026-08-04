use super::{Loader, RemoteDocument};
use crate::LoadError;
use iri_rs::{Iri, IriBuf};
use std::collections::{BTreeMap, HashMap};

/// Error returned using [`HashMap`] or [`BTreeMap`] as a [`Loader`] when the
/// requested document is not found.
#[derive(Debug, thiserror::Error)]
#[error("document not found")]
pub struct EntryNotFound;

impl<S: std::hash::BuildHasher + Send + Sync> Loader for HashMap<IriBuf, RemoteDocument, S> {
    type Error = EntryNotFound;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        match self.get(url.as_str()) {
            Some(document) => Ok(document.clone()),
            None => Err(LoadError::new(url.into(), EntryNotFound)),
        }
    }
}

impl Loader for BTreeMap<IriBuf, RemoteDocument> {
    type Error = EntryNotFound;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        match self.get(url.as_str()) {
            Some(document) => Ok(document.clone()),
            None => Err(LoadError::new(url.into(), EntryNotFound)),
        }
    }
}
