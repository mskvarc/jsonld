use super::{
    Context,
    ContextEntry,
    Definition,
    TermDefinition,
    definition,
    term_definition::{self, InvalidNest},
};
use crate::{Container, ErrorCode, Keyword, Nullable, TryFromJson};
use iri_rs::IriRefBuf;

#[derive(Debug, Clone, thiserror::Error)]
/// Error raised when a JSON value is not a valid context.
pub enum InvalidContext {
    #[error("Invalid IRI reference: {0}")]
    /// A context entry was a string that is not a valid IRI reference. Holds
    /// the rejected string.
    InvalidIriRef(String),

    #[error("Unexpected {0}")]
    /// A value had the wrong JSON kind. Holds the kind that was found and the
    /// kinds that would have been accepted.
    Unexpected(jstrict::Kind, &'static [jstrict::Kind]),

    #[error("Invalid `@direction`")]
    /// A `@direction` entry was a string other than `"ltr"` or `"rtl"`.
    InvalidDirection,

    #[error("Duplicate key")]
    /// The same key appeared twice in a context definition or in a term
    /// definition.
    DuplicateKey,

    #[error("Invalid term definition")]
    /// A term definition, or a structured entry of a context definition such as
    /// `@type` or `@version`, held an entry or a value the grammar does not
    /// allow there.
    InvalidTermDefinition,

    #[error("Invalid `@nest` value `{0}`")]
    /// A `@nest` entry held a keyword other than `@nest`. Holds the rejected
    /// string.
    InvalidNestValue(String),

    #[error("Context too deeply nested")]
    /// Scoped-context nesting exceeds [`MAX_CONTEXT_DEPTH`].
    TooDeep,
}

/// Maximum nesting depth of scoped contexts (`@context` inside a term
/// definition) accepted when converting JSON into a [`Context`].
///
/// The conversion recurses on the native stack, so a crafted deeply-nested
/// context could otherwise overflow it (an abort, not UB). Genuine contexts
/// nest a handful of levels at most.
pub const MAX_CONTEXT_DEPTH: usize = 128;

impl InvalidContext {
    /// Returns the JSON-LD error code this error reports as.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidIriRef(_) => ErrorCode::InvalidIriMapping,
            Self::Unexpected(_, _) => ErrorCode::InvalidContextEntry,
            Self::InvalidDirection => ErrorCode::InvalidBaseDirection,
            Self::DuplicateKey => ErrorCode::DuplicateKey,
            Self::InvalidTermDefinition => ErrorCode::InvalidTermDefinition,
            Self::InvalidNestValue(_) => ErrorCode::InvalidNestValue,
            Self::TooDeep => ErrorCode::ContextOverflow,
        }
    }
}

impl From<crate::Unexpected> for InvalidContext {
    fn from(crate::Unexpected(u, e): crate::Unexpected) -> Self {
        Self::Unexpected(u, e)
    }
}

impl TryFromJson for TermDefinition {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        term_definition_try_from_json(value, 0)
    }
}

/// Stores an entry value into an `Option` field, failing with
/// [`InvalidContext::DuplicateKey`] when the field was already set — the same
/// policy applied to duplicate term bindings.
macro_rules! set_unique {
    ($field:expr, $value:expr) => {
        if $field.replace($value).is_some() {
            return Err(InvalidContext::DuplicateKey);
        }
    };
}

fn term_definition_try_from_json(value: &jstrict::Value, depth: usize) -> Result<TermDefinition, InvalidContext> {
    match value {
        jstrict::Value::String(s) => Ok(TermDefinition::Simple(term_definition::Simple(s.as_str().to_owned()))),
        jstrict::Value::Object(o) => {
            let mut def = term_definition::Expanded::new();

            for jstrict::object::Entry { key, value } in o {
                match Keyword::try_from(key.as_str()) {
                    Ok(Keyword::Id) => set_unique!(def.id, Nullable::try_from_json(value)?),
                    Ok(Keyword::Type) => set_unique!(def.type_, Nullable::try_from_json(value)?),
                    Ok(Keyword::Context) => set_unique!(def.context, Box::new(context_try_from_json(value, depth + 1)?)),
                    Ok(Keyword::Reverse) => set_unique!(def.reverse, definition::Key::try_from_json(value)?),
                    Ok(Keyword::Index) => set_unique!(def.index, term_definition::Index::try_from_json(value)?),
                    Ok(Keyword::Language) => set_unique!(def.language, Nullable::try_from_json(value)?),
                    Ok(Keyword::Direction) => set_unique!(def.direction, Nullable::try_from_json(value)?),
                    Ok(Keyword::Container) => {
                        let container = match value {
                            jstrict::Value::Null => Nullable::Null,
                            other => {
                                let container = Container::try_from_json(other)?;
                                Nullable::Some(container)
                            }
                        };

                        set_unique!(def.container, container)
                    }
                    Ok(Keyword::Nest) => set_unique!(def.nest, term_definition::Nest::try_from_json(value)?),
                    Ok(Keyword::Prefix) => set_unique!(def.prefix, bool::try_from_json(value)?),
                    Ok(Keyword::Propagate) => set_unique!(def.propagate, bool::try_from_json(value)?),
                    Ok(Keyword::Protected) => set_unique!(def.protected, bool::try_from_json(value)?),
                    _ => return Err(InvalidContext::InvalidTermDefinition),
                }
            }

            Ok(TermDefinition::Expanded(Box::new(def)))
        }
        unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String, jstrict::Kind::Object])),
    }
}

impl TryFromJson for term_definition::Type {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => Ok(Self::from(s.as_str().to_owned())),
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for definition::TypeContainer {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => match Keyword::try_from(s.as_str()) {
                Ok(Keyword::Set) => Ok(Self::Set),
                _ => Err(InvalidContext::InvalidTermDefinition),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for definition::Type {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::Object(o) => {
                let mut container = None;
                let mut protected = None;

                for jstrict::object::Entry { key, value } in o {
                    match Keyword::try_from(key.as_str()) {
                        Ok(Keyword::Container) => {
                            if container.replace(definition::TypeContainer::try_from_json(value)?).is_some() {
                                return Err(InvalidContext::DuplicateKey);
                            }
                        }
                        Ok(Keyword::Protected) => {
                            if protected.replace(bool::try_from_json(value)?).is_some() {
                                return Err(InvalidContext::DuplicateKey);
                            }
                        }
                        _ => return Err(InvalidContext::InvalidTermDefinition),
                    }
                }

                match container {
                    Some(container) => Ok(Self { container, protected }),
                    None => Err(InvalidContext::InvalidTermDefinition),
                }
            }
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::Object])),
        }
    }
}

impl TryFromJson for definition::Version {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::Number(n) => match n.as_str() {
                "1.1" => Ok(Self::V1_1),
                _ => Err(InvalidContext::InvalidTermDefinition),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::Number])),
        }
    }
}

impl TryFromJson for definition::Vocab {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => Ok(Self::from(s.as_str().to_owned())),
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for term_definition::Id {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => Ok(Self::from(s.as_str().to_owned())),
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for definition::Key {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, Self::Error> {
        match value {
            jstrict::Value::String(s) => Ok(Self::from(s.as_str().to_owned())),
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for term_definition::Index {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => Ok(Self::from(s.as_str().to_owned())),
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for term_definition::Nest {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        match value {
            jstrict::Value::String(s) => match Self::try_from(s.as_str().to_owned()) {
                Ok(nest) => Ok(nest),
                Err(InvalidNest(s)) => Err(InvalidContext::InvalidNestValue(s)),
            },
            unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String])),
        }
    }
}

impl TryFromJson for Context {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        context_try_from_json(value, 0)
    }
}

fn context_try_from_json(value: &jstrict::Value, depth: usize) -> Result<Context, InvalidContext> {
    if depth >= MAX_CONTEXT_DEPTH {
        return Err(InvalidContext::TooDeep);
    }

    match value {
        jstrict::Value::Array(a) => {
            let mut many = Vec::with_capacity(a.len());

            for item in a {
                many.push(context_entry_try_from_json(item, depth)?)
            }

            Ok(Context::Many(many))
        }
        context => Ok(Context::One(context_entry_try_from_json(context, depth)?)),
    }
}

impl TryFromJson for ContextEntry {
    type Error = InvalidContext;

    fn try_from_json(value: &jstrict::Value) -> Result<Self, InvalidContext> {
        context_entry_try_from_json(value, 0)
    }
}

fn context_entry_try_from_json(value: &jstrict::Value, depth: usize) -> Result<ContextEntry, InvalidContext> {
    match value {
        jstrict::Value::Null => Ok(ContextEntry::Null),
        jstrict::Value::String(s) => match IriRefBuf::new(s.as_str().to_owned()) {
            Ok(iri_ref) => Ok(ContextEntry::IriRef(iri_ref)),
            Err(e) => Err(InvalidContext::InvalidIriRef(e.0)),
        },
        jstrict::Value::Object(o) => {
            let mut def = Definition::new();

            for jstrict::object::Entry { key, value } in o {
                match Keyword::try_from(key.as_str()) {
                    Ok(Keyword::Base) => set_unique!(def.base, Nullable::try_from_json(value)?),
                    Ok(Keyword::Import) => set_unique!(def.import, IriRefBuf::try_from_json(value)?),
                    Ok(Keyword::Language) => set_unique!(def.language, Nullable::try_from_json(value)?),
                    Ok(Keyword::Direction) => set_unique!(def.direction, Nullable::try_from_json(value)?),
                    Ok(Keyword::Propagate) => set_unique!(def.propagate, bool::try_from_json(value)?),
                    Ok(Keyword::Protected) => set_unique!(def.protected, bool::try_from_json(value)?),
                    Ok(Keyword::Type) => set_unique!(def.type_, definition::Type::try_from_json(value)?),
                    Ok(Keyword::Version) => set_unique!(def.version, definition::Version::try_from_json(value)?),
                    Ok(Keyword::Vocab) => set_unique!(def.vocab, Nullable::try_from_json(value)?),
                    _ => {
                        let term_def = match value {
                            jstrict::Value::Null => Nullable::Null,
                            other => Nullable::Some(term_definition_try_from_json(other, depth)?),
                        };

                        if def.bindings.insert_with(key.as_str().to_owned().into(), term_def).is_some() {
                            return Err(InvalidContext::DuplicateKey);
                        }
                    }
                }
            }

            Ok(ContextEntry::Definition(def))
        }
        unexpected => Err(InvalidContext::Unexpected(
            unexpected.kind(),
            &[jstrict::Kind::Null, jstrict::Kind::String, jstrict::Kind::Object],
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::Parse;

    fn nested_json(levels: usize) -> jstrict::Value {
        let mut json = String::new();
        for _ in 0..levels {
            json.push_str("{\"a\":{\"@context\":");
        }
        json.push_str("{}");
        for _ in 0..levels {
            json.push_str("}}");
        }

        jstrict::Value::parse_str(&json).unwrap().0
    }

    /// A crafted deeply-nested `@context` must fail with an error instead of
    /// exhausting the native stack during conversion.
    #[test]
    fn deeply_nested_context_conversion_fails_gracefully() {
        let json = nested_json(MAX_CONTEXT_DEPTH + 1);
        assert!(matches!(Context::try_from_json(&json), Err(InvalidContext::TooDeep)));
    }

    #[test]
    fn reasonable_nesting_converts() {
        let json = nested_json(MAX_CONTEXT_DEPTH - 1);
        assert!(Context::try_from_json(&json).is_ok());
    }

    /// Duplicate keyword entries fail like duplicate term bindings do,
    /// instead of silently keeping the last value.
    #[test]
    fn duplicate_keyword_entries_are_rejected() {
        let json = jstrict::Value::parse_str(r#"{"@vocab": "http://a/", "@vocab": "http://b/"}"#).unwrap().0;
        assert!(matches!(Context::try_from_json(&json), Err(InvalidContext::DuplicateKey)));

        let json = jstrict::Value::parse_str(r#"{"@id": "http://a/x", "@id": "http://b/x"}"#).unwrap().0;
        assert!(matches!(TermDefinition::try_from_json(&json), Err(InvalidContext::DuplicateKey)));
    }
}
