use super::{Loader, RemoteDocument};
use crate::{LoadError, LoadingResult};
use iri_rs::{Iri, IriBuf};
use std::collections::{BTreeMap, HashMap};

/// Error returned using [`HashMap`] or [`BTreeMap`] as a [`Loader`] with the
/// requested document is not found.
#[derive(Debug, thiserror::Error)]
#[error("document not found")]
pub struct EntryNotFound;

impl Loader for HashMap<IriBuf, RemoteDocument> {
	async fn load(&self, url: Iri<&str>) -> LoadingResult<IriBuf> {
		match self.get(url.as_str()) {
			Some(document) => Ok(document.clone()),
			None => Err(LoadError::new(url.into(), EntryNotFound)),
		}
	}
}

impl Loader for BTreeMap<IriBuf, RemoteDocument> {
	async fn load(&self, url: Iri<&str>) -> LoadingResult<IriBuf> {
		match self.get(url.as_str()) {
			Some(document) => Ok(document.clone()),
			None => Err(LoadError::new(url.into(), EntryNotFound)),
		}
	}
}
