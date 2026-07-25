use crate::{CompactIri, ExpandableRef, Keyword};
use iri_rs::Iri;
use std::hash::Hash;

#[derive(Clone, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
/// Value of the `@type` entry of a term definition.
pub enum Type {
    /// A JSON-LD keyword.
    Keyword(TypeKeyword),
    /// A term defined by the active context.
    Term(String),
}

impl Type {
    /// Borrows this `Type` as IRI, if it is one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        match self {
            Self::Term(t) => Iri::parse(t.as_str()).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Borrows this `Type` as compact IRI, if it is one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        match self {
            Self::Term(t) => CompactIri::new(t).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Borrows this `Type` as keyword, if it is one.
    pub fn as_keyword(&self) -> Option<TypeKeyword> {
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

    /// Consumes this `Type`, returning its string.
    pub fn into_string(self) -> String {
        match self {
            Self::Term(t) => t,
            Self::Keyword(k) => k.as_str().to_string(),
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Term(a), Self::Term(b)) => a == b,
            (Self::Keyword(a), Self::Keyword(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl From<String> for Type {
    fn from(s: String) -> Self {
        match TypeKeyword::try_from(s.as_str()) {
            Ok(k) => Self::Keyword(k),
            Err(_) => Self::Term(s),
        }
    }
}

/// Subset of keyword acceptable for as value for the `@type` entry
/// of an expanded term definition.
#[derive(Clone, Copy, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TypeKeyword {
    #[cfg_attr(feature = "serde", serde(rename = "@id"))]
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id,

    #[cfg_attr(feature = "serde", serde(rename = "@json"))]
    /// The `@json` type, marking the value as a JSON literal.
    Json,

    #[cfg_attr(feature = "serde", serde(rename = "@none"))]
    /// The `@none` entry, used as the index of values without one.
    None,

    #[cfg_attr(feature = "serde", serde(rename = "@vocab"))]
    /// The `@vocab` entry, setting the vocabulary against which terms expand.
    Vocab,
}

impl PartialEq for TypeKeyword {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Id, Self::Id) | (Self::Json, Self::Json) | (Self::None, Self::None) | (Self::Vocab, Self::Vocab)
        )
    }
}

impl Eq for TypeKeyword {}

impl Hash for TypeKeyword {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_str().hash(state)
    }
}

impl TypeKeyword {
    /// Returns the keyword of this `TypeKeyword`.
    pub fn keyword(&self) -> Keyword {
        self.into_keyword()
    }

    /// Consumes this `TypeKeyword`, returning its keyword.
    pub fn into_keyword(self) -> Keyword {
        self.into()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        self.into_keyword().into_str()
    }

    /// Consumes this `TypeKeyword`, returning its str.
    pub fn into_str(self) -> &'static str {
        self.into_keyword().into_str()
    }
}

/// Error raised when a keyword is not allowed as a `@type` value.
pub struct NotATypeKeyword(pub Keyword);

/// Error raised when a `@type` value is not a usable keyword.
pub enum InvalidTypeKeyword<T> {
    /// The value is not a keyword at all.
    NotAKeyword(T),
    /// The value is a keyword, but not one `@type` accepts.
    NotATypeKeyword(Keyword),
}

impl<T> From<NotATypeKeyword> for InvalidTypeKeyword<T> {
    fn from(NotATypeKeyword(k): NotATypeKeyword) -> Self {
        Self::NotATypeKeyword(k)
    }
}

impl<T> From<crate::NotAKeyword<T>> for InvalidTypeKeyword<T> {
    fn from(crate::NotAKeyword(t): crate::NotAKeyword<T>) -> Self {
        Self::NotAKeyword(t)
    }
}

impl From<TypeKeyword> for Keyword {
    fn from(k: TypeKeyword) -> Self {
        match k {
            TypeKeyword::Id => Self::Id,
            TypeKeyword::Json => Self::Json,
            TypeKeyword::None => Self::None,
            TypeKeyword::Vocab => Self::Vocab,
        }
    }
}

impl TryFrom<Keyword> for TypeKeyword {
    type Error = NotATypeKeyword;

    fn try_from(k: Keyword) -> Result<Self, Self::Error> {
        match k {
            Keyword::Id => Ok(Self::Id),
            Keyword::Json => Ok(Self::Json),
            Keyword::None => Ok(Self::None),
            Keyword::Vocab => Ok(Self::Vocab),
            _ => Err(NotATypeKeyword(k)),
        }
    }
}

impl<'a> TryFrom<&'a str> for TypeKeyword {
    type Error = InvalidTypeKeyword<&'a str>;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        Ok(Self::try_from(Keyword::try_from(s)?)?)
    }
}

// impl<'a> From<TypeRef<'a>> for context::definition::KeyOrKeywordRef<'a> {
// 	fn from(d: TypeRef<'a>) -> Self {
// 		match d {
// 			TypeRef::Term(t) => Self::Key(t.into()),
// 			TypeRef::Keyword(k) => Self::Keyword(k.into()),
// 		}
// 	}
// }

impl<'a> From<&'a Type> for ExpandableRef<'a> {
    fn from(d: &'a Type) -> Self {
        match d {
            Type::Term(t) => Self::String(t),
            Type::Keyword(k) => Self::Keyword((*k).into()),
        }
    }
}
