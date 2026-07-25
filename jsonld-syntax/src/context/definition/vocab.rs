use crate::{CompactIri, ExpandableRef};
use iri_rs::Iri;
use rdfx::BlankId;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
/// Value of the `@vocab` entry.
pub struct Vocab(String);

impl Vocab {
    /// Borrows this `Vocab` as IRI, if it is one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Borrows this `Vocab` as compact IRI, if it is one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(&self.0).ok()
    }

    /// Borrows this `Vocab` as blank id, if it is one.
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(&self.0).ok()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes this `Vocab`, returning its string.
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
