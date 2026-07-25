use std::{hash::Hash, str::FromStr};

#[derive(Debug, thiserror::Error)]
#[error("unknown JSON-LD version `{0}`")]
/// Error raised when `@version` holds anything but `1.1`.
pub struct UnknownVersion(pub String);

/// Version number.
///
/// The only allowed value is a number with the value `1.1`.
#[derive(Clone, Copy, PartialOrd, Ord, Debug)]
pub enum Version {
    /// JSON-LD 1.1 processing mode.
    V1_1,
}

impl Version {
    /// Consumes this `Version`, returning its bytes.
    pub fn into_bytes(self) -> &'static [u8] {
        match self {
            Self::V1_1 => b"1.1",
        }
    }

    /// Consumes this `Version`, returning its str.
    pub fn into_str(self) -> &'static str {
        match self {
            Self::V1_1 => "1.1",
        }
    }

    /// Consumes this `Version`, returning its JSON number.
    pub fn into_json_number(self) -> &'static jstrict::Number {
        unsafe { jstrict::Number::new_unchecked(self.into_bytes()) }
    }

    /// Consumes this `Version`, returning its JSON number buf.
    pub fn into_json_number_buf(self) -> jstrict::NumberBuf {
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
        self.into_str().hash(state)
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
