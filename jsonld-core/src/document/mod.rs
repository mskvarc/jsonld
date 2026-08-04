use std::{borrow::Borrow, hash::Hash, ops::Deref};

use iri_rs::IriBuf;
use rdfx::BlankIdBuf;

/// Expanded documents.
pub mod expanded;
/// Flattened documents.
pub mod flattened;

pub use expanded::ExpandedDocument;
pub use flattened::FlattenedDocument;

use crate::RemoteDocument;

/// Remote document paired with its expanded form.
pub type DocumentParts<I, B> = (RemoteDocument<I>, ExpandedDocument<I, B>);

/// JSON-LD document in both compact and expanded form.
#[derive(Debug, Clone)]
pub struct Document<I = IriBuf, B = BlankIdBuf> {
    remote: RemoteDocument<I>,
    expanded: ExpandedDocument<I, B>,
}

impl<I, B> Document<I, B> {
    /// Creates a new `Document`.
    pub fn new(remote: RemoteDocument<I>, expanded: ExpandedDocument<I, B>) -> Self {
        Self { remote, expanded }
    }

    /// Consumes the document, returning its remote (compact) form.
    pub fn into_remote(self) -> RemoteDocument<I> {
        self.remote
    }

    /// Consumes the document, returning its compact form as a JSON value.
    pub fn into_compact(self) -> jsonld_syntax::Value {
        self.remote.into_document()
    }

    /// Consumes the document, returning its expanded form.
    pub fn into_expanded(self) -> ExpandedDocument<I, B> {
        self.expanded
    }

    /// Consumes the document, returning its remote and expanded forms.
    pub fn into_parts(self) -> DocumentParts<I, B> {
        (self.remote, self.expanded)
    }

    /// Returns the remote (compact) form of the document.
    pub fn as_remote(&self) -> &RemoteDocument<I> {
        &self.remote
    }

    /// Returns the compact form of the document as a JSON value.
    pub fn as_compact(&self) -> &jsonld_syntax::Value {
        self.remote.document()
    }

    /// Returns the expanded form of the document.
    pub fn as_expanded(&self) -> &ExpandedDocument<I, B> {
        &self.expanded
    }
}

impl<I, B> Deref for Document<I, B> {
    type Target = ExpandedDocument<I, B>;

    fn deref(&self) -> &Self::Target {
        &self.expanded
    }
}

impl<I, B> Borrow<RemoteDocument<I>> for Document<I, B> {
    fn borrow(&self) -> &RemoteDocument<I> {
        &self.remote
    }
}

impl<I, B> Borrow<jsonld_syntax::Value> for Document<I, B> {
    fn borrow(&self) -> &jsonld_syntax::Value {
        self.remote.document()
    }
}

impl<I, B> Borrow<ExpandedDocument<I, B>> for Document<I, B> {
    fn borrow(&self) -> &ExpandedDocument<I, B> {
        &self.expanded
    }
}

impl<I: Eq + Hash, B: Eq + Hash> PartialEq for Document<I, B> {
    fn eq(&self, other: &Self) -> bool {
        self.expanded.eq(&other.expanded)
    }
}

impl<I: Eq + Hash, B: Eq + Hash> Eq for Document<I, B> {}

#[cfg(feature = "serde")]
impl<I, B> serde::Serialize for Document<I, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.remote.document().serialize(serializer)
    }
}

// TODO: `Document` lost its `LinkedData` / `LinkedDataGraph` impls when the
// serialization layer moved to `ld_core`; they need to be rewritten against
// `ld_core`'s `Interpretation`-based API.
