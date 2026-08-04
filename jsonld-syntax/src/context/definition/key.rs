use crate::{CompactIri, Keyword, intern::intern_static};
use iri_rs::Iri;
use rdfx::BlankId;
use std::{borrow::Borrow, cmp::Ordering, fmt, hash::Hash};

/// Context key.
///
/// Stores the canonical `&'static str` returned by the process-wide interner
/// directly, so [`Key::as_str`] is a single load and equality on equal logical
/// strings short-circuits via pointer equality (the interner guarantees a
/// unique allocation per interned string).
///
/// Interned strings are never freed: memory usage grows with the number of
/// distinct keys the process parses. See the [`crate::intern`] module
/// documentation for the implications when parsing untrusted input.
#[derive(Clone, Copy, Debug)]
pub struct Key(&'static str);

impl Key {
    /// Borrows this `Key` as IRI, if it is one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0).ok()
    }

    /// Borrows this `Key` as compact IRI, if it is one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(self.0).ok()
    }

    /// Borrows this `Key` as blank id, if it is one.
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(self.0).ok()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        self.0
    }

    /// Returns the number of entries of this `Key`.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks whether this `Key` is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes this `Key`, returning its string.
    pub fn into_string(self) -> String {
        self.0.to_owned()
    }

    /// Checks whether this `Key` is keyword like.
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.0)
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        // Interned strings are pointer-unique, so a pointer/length match is
        // sufficient and faster than byte comparison.
        std::ptr::eq(self.0.as_ptr(), other.0.as_ptr()) && self.0.len() == other.0.len()
    }
}

impl Eq for Key {}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other { Ordering::Equal } else { self.0.cmp(other.0) }
    }
}

impl From<jstrict::object::Key> for Key {
    fn from(k: jstrict::object::Key) -> Self {
        Self::from(k.into_string())
    }
}

#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash via the underlying string so that `Borrow<str>` lookups
        // (e.g. `HashMap::get(&"foo")`) stay equivalent.
        self.0.hash(state)
    }
}

impl From<String> for Key {
    fn from(k: String) -> Self {
        Self(intern_static(&k))
    }
}

impl<'a> From<&'a str> for Key {
    fn from(value: &'a str) -> Self {
        Self(intern_static(value))
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
            // default; the interner only needs `&str`, so one method covers
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
    /// Checks whether this `KeyRef` is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Checks whether this `KeyRef` is keyword like.
    pub fn is_keyword_like(&self) -> bool {
        crate::is_keyword_like(self.as_str())
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'a str {
        self.0
    }

    /// Clones this `KeyRef` into an owned one.
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

impl<'a> fmt::Display for KeyRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Context definition key that may also be a JSON-LD keyword.
pub enum KeyOrKeyword {
    /// A JSON-LD keyword.
    Keyword(Keyword),
    /// An entry key.
    Key(Key),
}

impl KeyOrKeyword {
    /// Checks whether this `KeyOrKeyword` is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Keyword(_) => false,
            Self::Key(k) => k.is_empty(),
        }
    }

    /// Consumes this `KeyOrKeyword`, returning its keyword.
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Keyword(k) => Some(k),
            Self::Key(_) => None,
        }
    }

    /// Consumes this `KeyOrKeyword`, returning its key.
    pub fn into_key(self) -> Option<Key> {
        match self {
            Self::Keyword(_) => None,
            Self::Key(k) => Some(k),
        }
    }

    /// Borrows this `KeyOrKeyword` as keyword, if it is one.
    pub fn as_keyword(&self) -> Option<Keyword> {
        match self {
            Self::Keyword(k) => Some(*k),
            Self::Key(_) => None,
        }
    }

    /// Borrows this `KeyOrKeyword` as key, if it is one.
    pub fn as_key(&self) -> Option<&Key> {
        match self {
            Self::Keyword(_) => None,
            Self::Key(k) => Some(k),
        }
    }

    /// Returns this value as a string slice.
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
        self.as_str().hash(state)
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
    /// An entry key.
    Key(KeyRef<'a>),
}

impl<'a> KeyOrKeywordRef<'a> {
    /// Clones this `KeyOrKeywordRef` into an owned one.
    pub fn to_owned(self) -> KeyOrKeyword {
        match self {
            Self::Keyword(k) => KeyOrKeyword::Keyword(k),
            Self::Key(k) => KeyOrKeyword::Key(k.to_owned()),
        }
    }

    /// Returns this value as a string slice.
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
    /// An entry key.
    Key(Key),
    /// The `@type` entry.
    Type,
}

impl KeyOrType {
    /// Returns this value as a string slice.
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

#[cfg(all(test, feature = "serde"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use jstrict::Parse;

    /// `#[serde(flatten)]` on `Definition::bindings` buffers entries into
    /// serde's internal `Content` representation and replays keys through
    /// `visit_str` (transient, not borrowed). The former
    /// `<&str>::deserialize`-based impl failed on that path.
    #[test]
    fn key_deserializes_through_serde_content_buffer() {
        let (value, _) = jstrict::Value::parse_str(r#"{"@vocab": "http://example.com/", "term": "http://example.com/term"}"#).unwrap();
        let definition: crate::context::Definition = crate::from_value(value).unwrap();
        assert!(definition.bindings.get(&Key::from("term")).is_some());
    }

    /// Owned-string input (`visit_string` → `visit_str`), as produced by
    /// escaped keys and reader-based formats.
    #[test]
    fn key_deserializes_from_owned_string() {
        let (value, _) = jstrict::Value::parse_str(r#""api""#).unwrap();
        let key: Key = crate::from_value(value).unwrap();
        assert_eq!(key.as_str(), "api");
    }
}
