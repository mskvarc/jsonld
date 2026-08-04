use std::{hash::Hash, str::FromStr};

#[derive(Debug, thiserror::Error)]
#[error("unknown JSON-LD version `{0}`")]
/// Error raised when `@version` holds anything but `1.1`.
pub struct UnknownVersion(pub String);

/// Value of the `@version` entry, selecting the processing mode.
///
/// The only value JSON-LD allows is the number `1.1`, so this enum has a single
/// variant and all its values are equal.
#[derive(Clone, Copy, PartialOrd, Ord, Debug)]
pub enum Version {
    /// JSON-LD 1.1 processing mode.
    V1_1,
}

impl Version {
    /// Returns the decimal spelling of this version as bytes: `b"1.1"`.
    #[must_use]
    pub fn into_bytes(self) -> &'static [u8] {
        match self {
            Self::V1_1 => b"1.1",
        }
    }

    /// Returns the decimal spelling of this version: `"1.1"`.
    #[must_use]
    pub fn into_str(self) -> &'static str {
        match self {
            Self::V1_1 => "1.1",
        }
    }

    /// Returns this version as a borrowed JSON number.
    #[must_use]
    pub fn into_json_number(self) -> &'static jstrict::Number {
        // SAFETY: `into_bytes` only returns `b"1.1"`, a valid JSON number.
        unsafe { jstrict::Number::new_unchecked(self.into_bytes()) }
    }

    /// Returns this version as an owned JSON number.
    #[must_use]
    pub fn into_json_number_buf(self) -> jstrict::NumberBuf {
        // SAFETY: `into_bytes` only returns `b"1.1"`, a valid JSON number.
        unsafe { jstrict::NumberBuf::new_unchecked(self.into_bytes().into()) }
    }
}

impl PartialEq for Version {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for Version {}

impl Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_str().hash(state);
    }
}

impl From<Version> for &jstrict::Number {
    fn from(v: Version) -> Self {
        v.into_json_number()
    }
}

impl From<Version> for jstrict::NumberBuf {
    fn from(v: Version) -> Self {
        v.into_json_number_buf()
    }
}

impl TryFrom<jstrict::NumberBuf> for Version {
    type Error = UnknownVersion;

    fn try_from(value: jstrict::NumberBuf) -> Result<Self, Self::Error> {
        if value.trimmed().as_str() == "1.1" {
            Ok(Self::V1_1)
        } else {
            Err(UnknownVersion(value.to_string()))
        }
    }
}

impl FromStr for Version {
    type Err = UnknownVersion;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "1.1" { Ok(Version::V1_1) } else { Err(UnknownVersion(s.to_owned())) }
    }
}

impl TryFrom<f32> for Version {
    type Error = UnknownVersion;

    // Exact comparison is the intent: `@version` is the literal `1.1` and
    // nothing else, and a JSON `1.1` parses to exactly this float. An epsilon
    // window would accept values the specification does not define.
    #[allow(clippy::float_cmp)]
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value == 1.1 {
            Ok(Version::V1_1)
        } else {
            Err(UnknownVersion(value.to_string()))
        }
    }
}

impl TryFrom<f64> for Version {
    type Error = UnknownVersion;

    #[allow(clippy::float_cmp)]
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value == 1.1 {
            Ok(Version::V1_1)
        } else {
            Err(UnknownVersion(value.to_string()))
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.into_json_number().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        jstrict::NumberBuf::deserialize(deserializer)?.try_into().map_err(serde::de::Error::custom)
    }
}
