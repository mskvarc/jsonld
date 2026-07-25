use crate::{CompactIri, ExpandableRef, Keyword, context::definition::KeyOrKeywordRef};
use iri_rs::Iri;
use rdfx::BlankId;
use std::{fmt, hash::Hash};

#[derive(Clone, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
/// Value of the `@id` entry of a term definition.
pub enum Id {
    /// A JSON-LD keyword.
    Keyword(Keyword),
    /// A term defined by the active context.
    Term(String),
}

impl Id {
    /// Borrows this `Id` as IRI, if it is one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        match self {
            Self::Term(t) => Iri::parse(t.as_str()).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Borrows this `Id` as blank id, if it is one.
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        match self {
            Self::Term(t) => BlankId::new(t).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Borrows this `Id` as compact IRI, if it is one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        match self {
            Self::Term(t) => CompactIri::new(t).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Borrows this `Id` as keyword, if it is one.
    pub fn as_keyword(&self) -> Option<Keyword> {
        match self {
            Self::Keyword(k) => Some(*k),
            Self::Term(_) => None,
        }
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Term(t) => t.as_str(),
            Self::Keyword(k) => k.into_str(),
        }
    }

    /// Consumes this `Id`, returning its string.
    pub fn into_string(self) -> String {
        match self {
            Self::Term(t) => t,
            Self::Keyword(k) => k.to_string(),
        }
    }

    /// Checks whether this `Id` is keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(self, Self::Keyword(_))
    }

    /// Checks whether this `Id` is keyword like.
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.as_str())
    }

    /// Borrows this `Id` as id ref, if it is one.
    pub fn as_id_ref(&self) -> IdRef<'_> {
        match self {
            Self::Term(t) => IdRef::Term(t),
            Self::Keyword(k) => IdRef::Keyword(*k),
        }
    }
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Term(a), Self::Term(b)) => a == b,
            (Self::Keyword(a), Self::Keyword(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Id {}

impl Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        match Keyword::try_from(s.as_str()) {
            Ok(k) => Self::Keyword(k),
            Err(_) => Self::Term(s),
        }
    }
}

impl<'a> From<&'a Id> for ExpandableRef<'a> {
    fn from(i: &'a Id) -> Self {
        match i {
            Id::Term(t) => Self::String(t),
            Id::Keyword(k) => Self::Keyword(*k),
        }
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Term(t) => t.fmt(f),
            Self::Keyword(k) => k.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Borrowed value of the `@id` entry of a term definition.
pub enum IdRef<'a> {
    /// A term defined by the active context.
    Term(&'a str),
    /// A JSON-LD keyword.
    Keyword(Keyword),
}

impl<'a> IdRef<'a> {
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Term(t) => t,
            Self::Keyword(k) => k.into_str(),
        }
    }

    /// Checks whether this `IdRef` is keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(self, Self::Keyword(_))
    }

    /// Checks whether this `IdRef` is keyword like.
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.as_str())
    }
}

impl<'a> From<&'a str> for IdRef<'a> {
    fn from(s: &'a str) -> Self {
        match Keyword::try_from(s) {
            Ok(k) => Self::Keyword(k),
            Err(_) => Self::Term(s),
        }
    }
}

impl<'a> From<IdRef<'a>> for ExpandableRef<'a> {
    fn from(i: IdRef<'a>) -> Self {
        match i {
            IdRef::Term(t) => Self::String(t),
            IdRef::Keyword(k) => Self::Keyword(k),
        }
    }
}

impl<'a> fmt::Display for IdRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<'a> From<IdRef<'a>> for KeyOrKeywordRef<'a> {
    fn from(i: IdRef<'a>) -> Self {
        match i {
            IdRef::Term(t) => Self::Key(t.into()),
            IdRef::Keyword(k) => Self::Keyword(k),
        }
    }
}
