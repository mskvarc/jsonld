use crate::{CompactIri, ExpandableRef, Keyword};
use iri_rs::Iri;
use std::hash::Hash;

#[derive(Clone, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
/// Value of the `@type` entry of a term definition: the type mapping of the
/// term.
pub enum Type {
    /// One of the keywords `@type` accepts.
    Keyword(TypeKeyword),
    /// A datatype given as an IRI, a compact IRI or another term of the
    /// context. Kept unvalidated, as telling those apart requires the
    /// processed context.
    Term(String),
}

impl Type {
    /// Parses this value as an IRI, returning `None` if it is not one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        match self {
            Self::Term(t) => Iri::parse(t.as_str()).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Parses this value as a compact IRI, returning `None` if it is not one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        match self {
            Self::Term(t) => CompactIri::new(t).ok(),
            Self::Keyword(_) => None,
        }
    }

    /// Returns the keyword, or `None` if this value is not one.
    pub fn as_keyword(&self) -> Option<TypeKeyword> {
        match self {
            Self::Keyword(k) => Some(*k),
            Self::Term(_) => None,
        }
    }

    /// Returns this value as it is spelled in the context.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Term(t) => t.as_str(),
            Self::Keyword(k) => k.into_str(),
        }
    }

    /// Converts this value into an owned `String`, allocating if it is a
    /// keyword.
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

/// The keywords accepted as the value of the `@type` entry of an expanded term
/// definition.
#[derive(Clone, Copy, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TypeKeyword {
    #[cfg_attr(feature = "serde", serde(rename = "@id"))]
    /// `@id`: the string values of the term are IRIs referencing nodes.
    Id,

    #[cfg_attr(feature = "serde", serde(rename = "@json"))]
    /// `@json`: the values of the term are JSON literals.
    Json,

    #[cfg_attr(feature = "serde", serde(rename = "@none"))]
    /// `@none`: accepted as a type mapping by the JSON-LD 1.1 grammar.
    None,

    #[cfg_attr(feature = "serde", serde(rename = "@vocab"))]
    /// `@vocab`: the string values of the term are expanded as terms against
    /// the vocabulary mapping.
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
    /// Returns this value as a general [`Keyword`].
    pub fn keyword(&self) -> Keyword {
        self.into_keyword()
    }

    /// Same as [`keyword`](Self::keyword), taking `self` by value.
    pub fn into_keyword(self) -> Keyword {
        self.into()
    }

    /// Returns the spelling of this keyword, such as `"@vocab"`.
    pub fn as_str(&self) -> &'static str {
        self.into_keyword().into_str()
    }

    /// Same as [`as_str`](Self::as_str), taking `self` by value.
    pub fn into_str(self) -> &'static str {
        self.into_keyword().into_str()
    }
}

/// Error raised when a keyword is not one of those a `@type` entry accepts.
pub struct NotATypeKeyword(pub Keyword);

/// Error raised when a string is not one of the keywords a `@type` entry
/// accepts.
pub enum InvalidTypeKeyword<T> {
    /// The string is not a keyword at all. Holds the rejected string.
    NotAKeyword(T),
    /// The string is a keyword, but not one `@type` accepts.
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

impl<'a> From<&'a Type> for ExpandableRef<'a> {
    fn from(d: &'a Type) -> Self {
        match d {
            Type::Term(t) => Self::String(t),
            Type::Keyword(k) => Self::Keyword((*k).into()),
        }
    }
}
