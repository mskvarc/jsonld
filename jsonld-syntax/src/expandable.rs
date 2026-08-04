use std::fmt;

use crate::{Keyword, context::definition::KeyOrKeyword};

/// Borrowed value that IRI expansion can be applied to.
///
/// Keywords are recognised while borrowing, so that expansion does not have to
/// match against the keyword list again.
pub enum ExpandableRef<'a> {
    /// A JSON-LD keyword.
    Keyword(Keyword),

    /// Anything else: a term, a compact IRI, an IRI or a blank node
    /// identifier.
    String(&'a str),
}

impl<'a> From<&'a KeyOrKeyword> for ExpandableRef<'a> {
    fn from(k: &'a KeyOrKeyword) -> Self {
        match k {
            KeyOrKeyword::Keyword(k) => Self::Keyword(*k),
            KeyOrKeyword::Key(k) => Self::String(k.as_str()),
        }
    }
}

impl<'a> From<&'a str> for ExpandableRef<'a> {
    fn from(s: &'a str) -> Self {
        match Keyword::try_from(s) {
            Ok(k) => Self::Keyword(k),
            Err(_) => Self::String(s),
        }
    }
}

impl<'a> fmt::Display for ExpandableRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keyword(k) => k.fmt(f),
            Self::String(s) => s.fmt(f),
        }
    }
}
