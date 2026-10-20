use crate::{CompactIri, Keyword};
use iri_rs::Iri;
use rdfx::BlankId;
use smol_str::SmolStr;
use std::{borrow::Borrow, fmt, hash::Hash};

/// Context key.
///
/// A key of up to 23 bytes is stored inline, so creating, cloning and dropping
/// it allocates nothing and touches no shared counter; nearly every term of a
/// real context is that short. A longer key is shared behind a reference
/// count.
///
/// A key owns its string and frees it when dropped, so the memory a process
/// spends on keys is bounded by the contexts it keeps.
///
/// Equality, ordering and hashing are those of the string, so a map keyed by
/// `Key` can be searched with a `&str`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(SmolStr);

impl Key {
    /// Parses this key as an IRI, returning `None` if it is not one.
    #[must_use]
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Parses this key as a compact IRI, returning `None` if it is not one.
    #[must_use]
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(self.0.as_str()).ok()
    }

    /// Parses this key as a blank node identifier, returning `None` if it is
    /// not one.
    #[must_use]
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(self.0.as_str()).ok()
    }

    /// Returns the string of this key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the length of this key, in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks whether this key is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Copies this key into a newly allocated `String`.
    #[must_use]
    pub fn into_string(self) -> String {
        String::from(self.0)
    }

    /// Checks whether this key is keyword-like: an `@` followed by ASCII
    /// letters. See [`crate::is_keyword_like`].
    #[must_use]
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.0.as_str())
    }
}

impl From<jstrict::object::Key> for Key {
    fn from(k: jstrict::object::Key) -> Self {
        Self::from(k.into_string())
    }
}

impl From<String> for Key {
    fn from(k: String) -> Self {
        Self(SmolStr::from(k))
    }
}

impl<'a> From<&'a str> for Key {
    fn from(value: &'a str) -> Self {
        Self(SmolStr::new(value))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Borrow<str> for Key {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Key;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string")
            }

            // `visit_borrowed_str` and `visit_string` forward here by
            // default; a key copies its string either way, so one method covers
            // borrowing formats, owned buffers (`from_reader`, escaped
            // keys) and serde's internal `Content` buffering
            // (`#[serde(flatten)]`) alike.
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Key, E> {
                Ok(Key::from(v))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// Borrowed context definition key.
pub struct KeyRef<'a>(&'a str);

impl<'a> KeyRef<'a> {
    /// Checks whether this key is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Checks whether this key is keyword-like: an `@` followed by ASCII
    /// letters. See [`crate::is_keyword_like`].
    #[must_use]
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.as_str())
    }

    /// Returns the borrowed string of this key.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.0
    }

    /// Copies this key into an owned [`Key`].
    #[must_use]
    pub fn to_owned(self) -> Key {
        Key::from(self.0)
    }
}

impl<'a> From<&'a str> for KeyRef<'a> {
    fn from(s: &'a str) -> Self {
        Self(s)
    }
}

impl<'a> From<&'a Key> for KeyRef<'a> {
    fn from(k: &'a Key) -> Self {
        Self(k.as_str())
    }
}

impl fmt::Display for KeyRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Context definition key that may also be a JSON-LD keyword.
pub enum KeyOrKeyword {
    /// A JSON-LD keyword.
    Keyword(Keyword),
    /// A term key.
    Key(Key),
}

impl KeyOrKeyword {
    /// Checks whether this is the empty key. A keyword is never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Keyword(_) => false,
            Self::Key(k) => k.is_empty(),
        }
    }

    /// Returns the keyword, or `None` if this is a term key.
    #[must_use]
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Keyword(k) => Some(k),
            Self::Key(_) => None,
        }
    }

    /// Returns the term key, or `None` if this is a keyword.
    #[must_use]
    pub fn into_key(self) -> Option<Key> {
        match self {
            Self::Keyword(_) => None,
            Self::Key(k) => Some(k),
        }
    }

    /// Returns the keyword, or `None` if this is a term key.
    #[must_use]
    pub fn as_keyword(&self) -> Option<Keyword> {
        match self {
            Self::Keyword(k) => Some(*k),
            Self::Key(_) => None,
        }
    }

    /// Borrows the term key, or returns `None` if this is a keyword.
    #[must_use]
    pub fn as_key(&self) -> Option<&Key> {
        match self {
            Self::Keyword(_) => None,
            Self::Key(k) => Some(k),
        }
    }

    /// Returns this value as it is spelled in the context.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Keyword(k) => k.into_str(),
            Self::Key(k) => k.as_str(),
        }
    }
}

#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for KeyOrKeyword {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Display for KeyOrKeyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(k) => k.fmt(f),
            Self::Keyword(k) => k.fmt(f),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Borrowed context definition key that may also be a keyword.
pub enum KeyOrKeywordRef<'a> {
    /// A JSON-LD keyword.
    Keyword(Keyword),
    /// A term key.
    Key(KeyRef<'a>),
}

impl<'a> KeyOrKeywordRef<'a> {
    /// Copies the term key, if any, into an owned [`KeyOrKeyword`].
    #[must_use]
    pub fn to_owned(self) -> KeyOrKeyword {
        match self {
            Self::Keyword(k) => KeyOrKeyword::Keyword(k),
            Self::Key(k) => KeyOrKeyword::Key(k.to_owned()),
        }
    }

    /// Returns this value as it is spelled in the context.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        match self {
            Self::Keyword(k) => k.into_str(),
            Self::Key(k) => k.as_str(),
        }
    }
}

impl<'a> From<&'a str> for KeyOrKeywordRef<'a> {
    fn from(s: &'a str) -> Self {
        match Keyword::try_from(s) {
            Ok(k) => Self::Keyword(k),
            Err(_) => Self::Key(s.into()),
        }
    }
}

impl<'a> From<&'a KeyOrKeyword> for KeyOrKeywordRef<'a> {
    fn from(k: &'a KeyOrKeyword) -> Self {
        match k {
            KeyOrKeyword::Keyword(k) => Self::Keyword(*k),
            KeyOrKeyword::Key(k) => Self::Key(k.into()),
        }
    }
}

impl<'a> From<KeyRef<'a>> for KeyOrKeywordRef<'a> {
    fn from(k: KeyRef<'a>) -> Self {
        Self::Key(k)
    }
}

impl<'a> From<&'a Key> for KeyOrKeywordRef<'a> {
    fn from(k: &'a Key) -> Self {
        Self::Key(k.into())
    }
}

/// Context definition key that may also be the `@type` entry.
pub enum KeyOrType {
    /// A term key.
    Key(Key),
    /// The `@type` entry.
    Type,
}

impl KeyOrType {
    /// Returns this value as it is spelled in the context.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Key(k) => k.as_str(),
            Self::Type => "@type",
        }
    }
}

impl fmt::Display for KeyOrType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::{TryFromJson, context::Context};
    use jstrict::Parse;
    use std::{
        collections::HashMap,
        hash::{BuildHasher, RandomState},
    };

    /// Parses `text` as a context holding a single context definition.
    fn parse_definition(text: &str) -> crate::context::Definition {
        let (value, _) = jstrict::Value::parse_str(text).unwrap();
        match Context::try_from_json(&value).unwrap() {
            Context::One(crate::ContextEntry::Definition(definition)) => Some(definition),
            Context::One(_) | Context::Many(_) => None,
        }
        .unwrap()
    }

    /// A key built from a borrowed string and one built from an owned string
    /// are equal and hash the same, and a map keyed by `Key` is searchable by
    /// `&str`, which is what the `Borrow<str>` contract requires.
    #[test]
    fn keys_from_borrowed_and_owned_strings_agree() {
        let borrowed = Key::from("speed");
        let owned = Key::from(String::from("speed"));
        assert_eq!(borrowed, owned);

        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&borrowed), hasher.hash_one(&owned));
        assert_eq!(hasher.hash_one(&borrowed), hasher.hash_one("speed"));

        let map = HashMap::from([(borrowed, 7_u8)]);
        assert_eq!(map.get("speed"), Some(&7));
    }

    /// Keys from two separately parsed contexts are equal and order equal, and
    /// a key as short as a typical term is stored without an allocation.
    #[test]
    fn keys_of_separately_parsed_contexts_are_equal() {
        let first = parse_definition(r#"{"speed": "https://example.org/speed"}"#);
        let second = parse_definition(r#"{"speed": "https://example.org/velocity"}"#);
        let first_key = first.bindings.get_key("speed").unwrap();
        let second_key = second.bindings.get_key("speed").unwrap();
        assert_eq!(first_key, second_key);
        assert_eq!(first_key.cmp(second_key), std::cmp::Ordering::Equal);
        assert!(!first_key.0.is_heap_allocated());
        assert_eq!(*first_key, first_key.clone());
    }

    /// `#[serde(flatten)]` on `Definition::bindings` buffers entries into
    /// serde's internal `Content` representation and replays keys through
    /// `visit_str` (transient, not borrowed). The former
    /// `<&str>::deserialize`-based impl failed on that path.
    #[cfg(feature = "serde")]
    #[test]
    fn key_deserializes_through_serde_content_buffer() {
        let (value, _) = jstrict::Value::parse_str(r#"{"@vocab": "http://example.com/", "term": "http://example.com/term"}"#).unwrap();
        let definition: crate::context::Definition = crate::from_value(value).unwrap();
        assert!(definition.bindings.get(&Key::from("term")).is_some());
    }

    /// Owned-string input (`visit_string` → `visit_str`), as produced by
    /// escaped keys and reader-based formats.
    #[cfg(feature = "serde")]
    #[test]
    fn key_deserializes_from_owned_string() {
        let (value, _) = jstrict::Value::parse_str(r#""api""#).unwrap();
        let key: Key = crate::from_value(value).unwrap();
        assert_eq!(key.as_str(), "api");
    }
}
