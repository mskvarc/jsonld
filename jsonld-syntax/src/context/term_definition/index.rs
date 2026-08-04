use crate::CompactIri;
use iri_rs::Iri;
use std::{fmt, hash::Hash};

#[derive(Clone, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
/// Value of the `@index` entry of a term definition: the property carrying the
/// index of the values in an index map.
pub struct Index(String);

impl Index {
    /// Parses this value as an IRI, returning `None` if it is not one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Parses this value as a compact IRI, returning `None` if it is not one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(&self.0).ok()
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

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Eq for Index {}

impl Hash for Index {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl From<String> for Index {
    fn from(s: String) -> Self {
        Self(s)
    }
}
