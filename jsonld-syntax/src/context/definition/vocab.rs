use crate::{CompactIri, ExpandableRef};
use iri_rs::Iri;
use rdfx::BlankId;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
/// Value of the `@vocab` entry.
///
/// Kept as an unvalidated string: it may be an IRI, a compact IRI, a blank node
/// identifier or a term of the surrounding context, and which one it is can
/// only be told once the context is processed.
pub struct Vocab(String);

impl Vocab {
    /// Parses this value as an IRI, returning `None` if it is not one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Parses this value as a compact IRI, returning `None` if it is not one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(&self.0).ok()
    }

    /// Parses this value as a blank node identifier, returning `None` if it is
    /// not one.
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(&self.0).ok()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Unwraps the underlying string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for Vocab {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl<'a> From<&'a Vocab> for ExpandableRef<'a> {
    fn from(v: &'a Vocab) -> Self {
        ExpandableRef::String(&v.0)
    }
}
