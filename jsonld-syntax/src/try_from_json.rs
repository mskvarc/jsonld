use crate::{Container, ContainerKind, Direction, LenientLangTagBuf, Nullable, context::InvalidContext};
use iri_rs::IriRefBuf;

/// Values that can be built from a JSON value.
pub trait TryFromJson: Sized {
    /// Error raised when the JSON value does not describe a valid `Self`.
    type Error;

    /// Builds a value of this type from the given JSON value.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON value does not describe a valid value of this type.
    fn try_from_json(value: &jstrict::Value) -> Result<Self, Self::Error>;
}

impl crate::TryFromJson for bool {
    type Error = crate::Unexpected;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, Self::Error> {
        match value {
            jstrict::Value::Boolean(b) => Ok(*b),
            unexpected => Err(crate::Unexpected(unexpected.kind(), &[jstrict::Kind::Boolean])),
        }
    }
}

impl TryFromJson for IriRefBuf {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => match IriRefBuf::new(s.as_str().to_owned()) {
                Ok(iri_ref) => Ok(iri_ref),
                Err(e) => Err(InvalidContext::InvalidIriRef(e.0)),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for LenientLangTagBuf {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => {
                let (lang, _) = LenientLangTagBuf::new(s.as_str().to_owned());
                Ok(lang)
            }
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for Direction {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => match Direction::try_from(s.as_str()) {
                Ok(d) => Ok(d),
                Err(_) => Err(InvalidContext::InvalidDirection),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl<T: TryFromJson> TryFromJson for Nullable<T> {
    type Error = T::Error;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, Self::Error> {
        match value {
            jstrict::Value::Null => Ok(Self::Null),
            some => T::try_from_json(some).map(Self::Some),
        }
    }
}

impl TryFromJson for Container {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::Array(a) => {
                let mut container = Vec::new();

                for item in a {
                    container.push(ContainerKind::try_from_json(item)?);
                }

                Ok(Self::Many(container))
            }
            other => ContainerKind::try_from_json(other).map(Into::into),
        }
    }
}

impl TryFromJson for ContainerKind {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => match ContainerKind::try_from(s.as_str()) {
                Ok(t) => Ok(t),
                Err(_) => Err(InvalidContext::InvalidTermDefinition),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}
