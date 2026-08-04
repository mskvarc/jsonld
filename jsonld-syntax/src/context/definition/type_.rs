use std::{hash::Hash, str::FromStr};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The `@type` entry of a context definition.
pub struct Type {
    #[cfg_attr(feature = "serde", serde(rename = "@container"))]
    /// The `@container` entry, declaring how values of the term are laid out.
    pub container: TypeContainer,

    #[cfg_attr(feature = "serde", serde(rename = "@protected", default, skip_serializing_if = "Option::is_none"))]
    /// The `@protected` entry, forbidding redefinition of the term.
    pub protected: Option<bool>,
}

impl Type {
    /// Returns an iterator over the entries of this `Type`.
    pub fn iter(&self) -> ContextTypeEntries {
        ContextTypeEntries {
            container: Some(self.container),
            protected: self.protected,
        }
    }
}

/// Iterator over the entries of a context `@type` definition.
pub struct ContextTypeEntries {
    container: Option<TypeContainer>,
    protected: Option<bool>,
}

impl Iterator for ContextTypeEntries {
    type Item = ContextTypeEntry;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = 0;

        if self.container.is_some() {
            len += 1;
        }

        if self.protected.is_some() {
            len += 1;
        }

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self.container.take() {
            Some(c) => Some(ContextTypeEntry::Container(c)),
            None => self.protected.take().map(ContextTypeEntry::Protected),
        }
    }
}

impl ExactSizeIterator for ContextTypeEntries {}

/// Entry of a context `@type` definition.
pub enum ContextTypeEntry {
    /// The `@container` entry.
    Container(TypeContainer),
    /// The `@protected` entry.
    Protected(bool),
}

impl ContextTypeEntry {
    /// Returns the key of this `ContextTypeEntry`.
    pub fn key(&self) -> ContextTypeKey {
        match self {
            Self::Container(_) => ContextTypeKey::Container,
            Self::Protected(_) => ContextTypeKey::Protected,
        }
    }
}

/// Key of a context `@type` definition entry.
pub enum ContextTypeKey {
    /// The `@container` key.
    Container,
    /// The `@protected` key.
    Protected,
}

impl ContextTypeKey {
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Container => "@container",
            Self::Protected => "@protected",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid JSON-LD `@type` container `{0}`")]
/// Error raised when a `@type` container is neither `@set` nor absent.
pub struct InvalidTypeContainer<T = String>(pub T);

#[derive(Clone, Copy, PartialOrd, Ord, Debug)]
/// Container allowed on a context `@type` definition.
pub enum TypeContainer {
    /// The `@set` container.
    Set,
}

impl TypeContainer {
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Set => "@set",
        }
    }

    /// Consumes this `TypeContainer`, returning its str.
    pub fn into_str(self) -> &'static str {
        self.as_str()
    }
}

impl PartialEq for TypeContainer {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for TypeContainer {}

impl Hash for TypeContainer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_str().hash(state)
    }
}

impl FromStr for TypeContainer {
    type Err = InvalidTypeContainer;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "@set" => Ok(Self::Set),
            other => Err(InvalidTypeContainer(other.to_owned())),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for TypeContainer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TypeContainer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TypeContainer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON-LD type container")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse().map_err(|e| E::custom(e))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}
