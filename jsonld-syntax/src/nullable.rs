use std::fmt;

/// Error returned by [`Nullable::try_unwrap`] when the value is `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("nullable value was null")]
pub struct NullError;

/// Value that can be `null`.
///
/// `Option` is used throughout this crate for entries that may or may not be
/// present. A JSON-LD entry can also be present with the explicit value
/// `null`, which is meaningful: it unsets whatever the surrounding context
/// inherited. Hence this separate type, so that `Option<Nullable<T>>` can tell
/// an absent entry from an entry set to `null`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Nullable<T> {
    /// The `null` value.
    Null,

    /// Any other value.
    Some(T),
}

impl<T> Nullable<T> {
    /// Checks if the value is `null`.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        matches!(self, Nullable::Null)
    }

    /// Checks if the value is not `null`.
    #[inline(always)]
    pub fn is_some(&self) -> bool {
        matches!(self, Nullable::Some(_))
    }

    /// Returns the inner value, or [`NullError`] if `null`.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is null.
    #[inline(always)]
    pub fn try_unwrap(self) -> Result<T, NullError> {
        match self {
            Nullable::Some(t) => Ok(t),
            Nullable::Null => Err(NullError),
        }
    }

    /// Converts `&Nullable<T>` into `Nullable<&T>`.
    #[inline(always)]
    pub fn as_ref(&self) -> Nullable<&T> {
        match self {
            Nullable::Null => Nullable::Null,
            Nullable::Some(t) => Nullable::Some(t),
        }
    }

    /// Converts `&Nullable<T>` into `Nullable<&T::Target>` by dereferencing
    /// the inner value.
    pub fn as_deref(&self) -> Nullable<&T::Target>
    where
        T: std::ops::Deref,
    {
        match self {
            Self::Null => Nullable::Null,
            Self::Some(t) => Nullable::Some(t),
        }
    }

    /// Converts this value into an `Option`, mapping `null` to `None`.
    #[inline(always)]
    pub fn option(self) -> Option<T> {
        match self {
            Nullable::Null => None,
            Nullable::Some(t) => Some(t),
        }
    }

    /// Maps the inner value with the given function, keeping `null` as `null`.
    #[inline(always)]
    pub fn map<F, U>(self, f: F) -> Nullable<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Nullable::Null => Nullable::Null,
            Nullable::Some(t) => Nullable::Some(f(t)),
        }
    }

    /// Converts the contained value, keeping null as null.
    pub fn cast<U>(self) -> Nullable<U>
    where
        T: Into<U>,
    {
        match self {
            Self::Null => Nullable::Null,
            Self::Some(t) => Nullable::Some(t.into()),
        }
    }

    /// Returns the contained value, or `default` if it is null.
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Null => default,
            Self::Some(t) => t,
        }
    }

    /// Returns the contained value, or the default one if it is null.
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        match self {
            Self::Null => T::default(),
            Self::Some(t) => t,
        }
    }
}

impl<T> From<T> for Nullable<T> {
    fn from(value: T) -> Self {
        Self::Some(value)
    }
}

impl<T> From<Option<T>> for Nullable<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(t) => Self::Some(t),
            None => Self::Null,
        }
    }
}

impl<T: Clone> Nullable<&T> {
    /// Clones the referenced inner value.
    #[inline(always)]
    #[must_use]
    pub fn cloned(&self) -> Nullable<T> {
        match self {
            Nullable::Null => Nullable::Null,
            Nullable::Some(t) => Nullable::Some((*t).clone()),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Nullable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(v) => v.fmt(f),
        }
    }
}

impl<T: contextual::DisplayWithContext<V>, V> contextual::DisplayWithContext<V> for Nullable<T> {
    fn fmt_with(&self, vocabulary: &V, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Some(v) => v.fmt_with(vocabulary, f),
        }
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Nullable<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Some(t) => serializer.serialize_some(t),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Nullable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Option::<T>::deserialize(deserializer)?.into())
    }
}

#[cfg(feature = "serde")]
impl<T> Nullable<T> {
    /// Deserializes a `Nullable<T>` and wraps it in `Some`.
    ///
    /// Meant as the `deserialize_with` of an `Option<Nullable<T>>` field that
    /// also carries `#[serde(default)]`: an absent entry then yields `None`,
    /// while an entry present with the value `null` yields
    /// `Some(Nullable::Null)`. Serde's own handling of `Option` would collapse
    /// both cases to `None`.
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error when the value is neither absent, null, nor a valid `T`.
    pub fn optional<'de, D>(deserializer: D) -> Result<Option<Self>, D::Error>
    where
        T: serde::Deserialize<'de>,
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        Ok(Some(Option::<T>::deserialize(deserializer)?.into()))
    }
}
