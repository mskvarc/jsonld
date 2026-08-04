pub use langtag::{InvalidLangTag, LangTag, LangTagBuf};
use std::{borrow::Borrow, fmt, hash::Hash, ops::Deref};

use crate::utils::{case_insensitive_cmp, case_insensitive_eq, case_insensitive_hash};

/// Language tag that may not be well-formed.
///
/// JSON-LD accepts any string as a `@language` value, so an ill-formed tag must
/// be preserved rather than rejected. Comparison, ordering and hashing are
/// ASCII-case-insensitive, as language tags are case-insensitive.
// `repr(transparent)` guarantees the layout assumed by the `&str` transmutes
// in `new` and `as_lenient_lang_tag_ref`.
#[derive(Debug)]
#[repr(transparent)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct LenientLangTag(str);

impl LenientLangTag {
    /// Borrows the given string as a language tag, along with the error
    /// produced by validating it, if it is not well-formed.
    #[must_use]
    pub fn new(s: &str) -> (&Self, Option<InvalidLangTag<&str>>) {
        let err = LangTag::new(s).err();
        // SAFETY: `Self` is `repr(transparent)` over `str`, and this type
        // accepts any string, well-formed or not.
        (unsafe { &*(std::ptr::from_ref::<str>(s) as *const Self) }, err)
    }

    /// Returns this value as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Returns this value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks whether this tag is a well-formed language tag.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        LangTag::new(self.as_str()).is_ok()
    }

    /// Borrows this tag as a well-formed [`LangTag`], or returns `None` if it
    /// is not well-formed.
    #[must_use]
    pub fn as_well_formed(&self) -> Option<&LangTag> {
        LangTag::new(self.as_str()).ok()
    }
}

impl PartialEq for LenientLangTag {
    fn eq(&self, other: &Self) -> bool {
        case_insensitive_eq(self.as_bytes(), other.as_bytes())
    }
}

impl Eq for LenientLangTag {}

impl PartialOrd for LenientLangTag {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LenientLangTag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        case_insensitive_cmp(self.as_bytes(), other.as_bytes())
    }
}

impl Hash for LenientLangTag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        case_insensitive_hash(self.as_bytes(), state);
    }
}

impl ToOwned for LenientLangTag {
    type Owned = LenientLangTagBuf;

    fn to_owned(&self) -> Self::Owned {
        LenientLangTagBuf(self.0.to_owned())
    }
}

// No `Borrow<str>` impl: `LenientLangTag`'s `Eq`/`Ord`/`Hash` are
// case-insensitive while `str`'s are not, and the `Borrow` contract requires
// them to agree. Use `AsRef<str>`/`as_str` to reach the raw string.
impl AsRef<str> for LenientLangTag {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for LenientLangTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Owned language tag that may not be well-formed.
#[derive(Debug, Clone)]
pub struct LenientLangTagBuf(String);

impl LenientLangTagBuf {
    /// Takes ownership of the given string as a language tag, along with the
    /// error produced by validating it, if it is not well-formed.
    #[must_use]
    pub fn new(s: String) -> (Self, Option<InvalidLangTag<String>>) {
        let err = LangTag::new(s.as_str()).err().map(|InvalidLangTag(s)| InvalidLangTag(s.to_owned()));
        (Self(s), err)
    }

    /// Borrows this value as a [`LenientLangTag`].
    #[must_use]
    pub fn as_lenient_lang_tag_ref(&self) -> &LenientLangTag {
        // SAFETY: `LenientLangTag` is `repr(transparent)` over `str`.
        unsafe { &*(std::ptr::from_ref::<str>(self.0.as_str()) as *const LenientLangTag) }
    }

    /// Unwraps the underlying string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Converts this tag into a well-formed [`LangTagBuf`], failing if it is
    /// not well-formed.
    ///
    /// # Errors
    ///
    /// Returns the string unchanged when it is not a well-formed language tag.
    pub fn into_well_formed(self) -> Result<LangTagBuf, InvalidLangTag<String>> {
        LangTagBuf::new(self.0)
    }
}

impl Deref for LenientLangTagBuf {
    type Target = LenientLangTag;

    fn deref(&self) -> &Self::Target {
        self.as_lenient_lang_tag_ref()
    }
}

impl PartialEq for LenientLangTagBuf {
    fn eq(&self, other: &Self) -> bool {
        self.as_lenient_lang_tag_ref().eq(other.as_lenient_lang_tag_ref())
    }
}

impl Eq for LenientLangTagBuf {}

impl PartialOrd for LenientLangTagBuf {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LenientLangTagBuf {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_lenient_lang_tag_ref().cmp(other.as_lenient_lang_tag_ref())
    }
}

impl Hash for LenientLangTagBuf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_lenient_lang_tag_ref().hash(state);
    }
}

impl Borrow<LenientLangTag> for LenientLangTagBuf {
    fn borrow(&self) -> &LenientLangTag {
        self.as_lenient_lang_tag_ref()
    }
}

impl AsRef<LenientLangTag> for LenientLangTagBuf {
    fn as_ref(&self) -> &LenientLangTag {
        self.as_lenient_lang_tag_ref()
    }
}

impl From<LangTagBuf> for LenientLangTagBuf {
    fn from(tag: LangTagBuf) -> Self {
        Self(tag.into_string())
    }
}

impl From<String> for LenientLangTagBuf {
    fn from(tag: String) -> Self {
        Self(tag)
    }
}

impl fmt::Display for LenientLangTagBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for LenientLangTagBuf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LenientLangTagBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(String::deserialize(deserializer)?))
    }
}
