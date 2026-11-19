use super::{
    Context,
    ContextEntry,
    Definition,
    TermDefinition,
    definition::{self, KeyOrKeyword, KeyOrKeywordRef},
    term_definition::{self, InvalidNest},
};
use crate::{Container, ErrorCode, Keyword, Nullable, TryFromJson, native_stack::on_sufficient_stack};
use iri_rs::IriRefBuf;

#[derive(Debug, Clone, thiserror::Error)]
/// Error raised when a JSON value is not a valid context.
///
/// An error raised by the value of an entry of a context definition or of an
/// expanded term definition is located in that entry by [`Self::Entry`], so
/// the error names the term or keyword whose value is invalid.
pub enum InvalidContext {
    #[error("Invalid entry `{key}`: {error}")]
    /// An entry of a context definition or of an expanded term definition is
    /// invalid. Holds the entry's key, a term or a keyword, and the error its
    /// value raised, itself located when the value is a term definition or a
    /// scoped context.
    Entry {
        /// The key of the invalid entry.
        key: KeyOrKeyword,
        /// Why the entry is invalid.
        error: Box<InvalidContext>,
    },

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
/// Deeper nesting is refused with [`InvalidContext::TooDeep`]. Genuine contexts
/// nest a handful of levels at most.
pub const MAX_CONTEXT_DEPTH: usize = 128;

/// Converts a scoped context nested in a term definition, on a fresh stack
/// segment when the current one is nearly used.
///
/// The conversion recurses once per nesting level, and an embedder may call it
/// on a thread whose stack it has already partly used, so without this a
/// nesting well inside [`MAX_CONTEXT_DEPTH`] could abort the process on a stack
/// overflow instead of converting or failing with
/// [`InvalidContext::TooDeep`].
fn nested_context_try_from_json(value: &jstrict::Value, depth: usize) -> Result<Context, InvalidContext> {
    on_sufficient_stack(|| context_try_from_json(value, depth))
}

impl InvalidContext {
    /// Returns the JSON-LD error code this error reports as: the code of its
    /// [`Self::cause`].
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Entry { .. } => self.cause().code(),
            Self::InvalidIriRef(_) => ErrorCode::InvalidIriMapping,
            Self::Unexpected(_, _) => ErrorCode::InvalidContextEntry,
            Self::InvalidDirection => ErrorCode::InvalidBaseDirection,
            Self::DuplicateKey => ErrorCode::DuplicateKey,
            Self::InvalidTermDefinition => ErrorCode::InvalidTermDefinition,
            Self::InvalidNestValue(_) => ErrorCode::InvalidNestValue,
            Self::TooDeep => ErrorCode::ContextOverflow,
        }
    }

    /// Returns the error that made the context invalid, past every
    /// [`Self::Entry`] locating it.
    #[must_use]
    pub fn cause(&self) -> &InvalidContext {
        let mut error = self;
        while let Self::Entry { error: located, .. } = error {
            error = located;
        }
        error
    }

    /// Locates this error in the entry `key`.
    fn in_entry(self, key: &str) -> InvalidContext {
        InvalidContext::Entry {
            key: KeyOrKeywordRef::from(key).to_owned(),
            error: Box::new(self),
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
                expanded_entry_try_from_json(&mut def, key.as_str(), value, depth).map_err(|error| error.in_entry(key.as_str()))?;
            }

            Ok(TermDefinition::Expanded(Box::new(def)))
        }
        unexpected => Err(InvalidContext::Unexpected(unexpected.kind(), &[jstrict::Kind::String, jstrict::Kind::Object])),
    }
}

/// Reads the entry `key` of an expanded term definition into `def`.
fn expanded_entry_try_from_json(def: &mut term_definition::Expanded, key: &str, value: &jstrict::Value, depth: usize) -> Result<(), InvalidContext> {
    match Keyword::try_from(key) {
        Ok(Keyword::Id) => set_unique!(def.id, Nullable::try_from_json(value)?),
        Ok(Keyword::Type) => set_unique!(def.type_, Nullable::try_from_json(value)?),
        Ok(Keyword::Context) => set_unique!(def.context, Box::new(nested_context_try_from_json(value, depth + 1)?)),
        Ok(Keyword::Reverse) => set_unique!(def.reverse, definition::Key::try_from_json(value)?),
        Ok(Keyword::Index) => set_unique!(def.index, term_definition::Index::try_from_json(value)?),
        Ok(Keyword::Language) => set_unique!(def.language, Nullable::try_from_json(value)?),
        Ok(Keyword::Direction) => set_unique!(def.direction, Nullable::try_from_json(value)?),
        Ok(Keyword::Container) => {
            let container = match value {
                jstrict::Value::Null => Nullable::Null,
                other => Nullable::Some(Container::try_from_json(other)?),
            };

            set_unique!(def.container, container);
        }
        Ok(Keyword::Nest) => set_unique!(def.nest, term_definition::Nest::try_from_json(value)?),
        Ok(Keyword::Prefix) => set_unique!(def.prefix, bool::try_from_json(value)?),
        Ok(Keyword::Propagate) => set_unique!(def.propagate, bool::try_from_json(value)?),
        Ok(Keyword::Protected) => set_unique!(def.protected, bool::try_from_json(value)?),
        _ => return Err(InvalidContext::InvalidTermDefinition),
    }

    Ok(())
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
                many.push(context_entry_try_from_json(item, depth)?);
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
                definition_entry_try_from_json(&mut def, key.as_str(), value, depth).map_err(|error| error.in_entry(key.as_str()))?;
            }

            Ok(ContextEntry::Definition(def))
        }
        unexpected => Err(InvalidContext::Unexpected(
            unexpected.kind(),
            &[jstrict::Kind::Null, jstrict::Kind::String, jstrict::Kind::Object],
        )),
    }
}

/// Reads the entry `key` of a context definition into `def`: a keyword entry,
/// or the definition of the term `key`.
fn definition_entry_try_from_json(def: &mut Definition, key: &str, value: &jstrict::Value, depth: usize) -> Result<(), InvalidContext> {
    match Keyword::try_from(key) {
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

            if def.bindings.insert_with(definition::Key::from(key), term_def).is_some() {
                return Err(InvalidContext::DuplicateKey);
            }
        }
    }

    Ok(())
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

    /// The conversion error of the context written as `json`.
    fn refusal(json: &str) -> InvalidContext {
        let value = jstrict::Value::parse_str(json).unwrap().0;
        Context::try_from_json(&value).unwrap_err()
    }

    /// The keys an error is located in, outermost first.
    fn location(error: &InvalidContext) -> Vec<&str> {
        let mut keys = Vec::new();
        let mut error = error;
        while let InvalidContext::Entry { key, error: located } = error {
            keys.push(key.as_str());
            error = located;
        }
        keys
    }

    /// A crafted deeply-nested `@context` must fail with an error instead of
    /// exhausting the native stack during conversion.
    #[test]
    fn deeply_nested_context_conversion_fails_gracefully() {
        let json = nested_json(MAX_CONTEXT_DEPTH + 1);
        assert!(matches!(
            Context::try_from_json(&json).map_err(|error| error.cause().clone()),
            Err(InvalidContext::TooDeep)
        ));
    }

    /// A term defined as a number names the term, and the cause keeps the
    /// kinds a term definition may take.
    #[test]
    fn an_invalid_term_definition_is_located_in_its_term() {
        let error = refusal(r#"{"speed": 5}"#);
        assert_eq!(location(&error), ["speed"]);
        assert!(matches!(
            error.cause(),
            InvalidContext::Unexpected(jstrict::Kind::Number, [jstrict::Kind::String, jstrict::Kind::Object])
        ));
        assert_eq!(error.code(), ErrorCode::InvalidContextEntry);
    }

    /// An invalid entry of an expanded term definition is located in the term
    /// and then in the entry.
    #[test]
    fn an_invalid_entry_of_a_term_definition_is_located_in_the_term_and_the_entry() {
        let error = refusal(r#"{"speed": {"@id": 5}}"#);
        assert_eq!(location(&error), ["speed", "@id"]);
        assert!(matches!(
            error.cause(),
            InvalidContext::Unexpected(jstrict::Kind::Number, [jstrict::Kind::String])
        ));

        let error = refusal(r#"{"speed": {"@id": "https://example.org/speed", "@container": "@bogus"}}"#);
        assert_eq!(location(&error), ["speed", "@container"]);
        assert!(matches!(error.cause(), InvalidContext::InvalidTermDefinition));
    }

    /// A keyword entry of a context definition is located in the keyword.
    #[test]
    fn an_invalid_keyword_entry_is_located_in_the_keyword() {
        let error = refusal(r#"{"@type": "https://example.org/t"}"#);
        assert_eq!(location(&error), ["@type"]);
        assert!(matches!(
            error,
            InvalidContext::Entry {
                key: KeyOrKeyword::Keyword(Keyword::Type),
                ..
            }
        ));
        assert!(matches!(
            error.cause(),
            InvalidContext::Unexpected(jstrict::Kind::String, [jstrict::Kind::Object])
        ));
    }

    /// An error inside a scoped context is located through the term that
    /// carries the scoped context.
    #[test]
    fn an_error_in_a_scoped_context_is_located_through_the_scoping_term() {
        let error = refusal(r#"{"speed": {"@id": "https://example.org/speed", "@context": {"gear": true}}}"#);
        assert_eq!(location(&error), ["speed", "@context", "gear"]);
    }

    /// An error of the context itself, not of one of its entries, is located
    /// nowhere.
    #[test]
    fn an_invalid_context_reference_is_not_located() {
        let error = refusal(r#""not a url""#);
        assert!(matches!(error, InvalidContext::InvalidIriRef(_)));
    }

    #[test]
    fn reasonable_nesting_converts() {
        let json = nested_json(MAX_CONTEXT_DEPTH - 1);
        assert!(Context::try_from_json(&json).is_ok());
    }

    /// The deepest accepted nesting converts, and one level more is refused,
    /// on a thread whose stack is far smaller than the conversion of that
    /// nesting needs, so an embedder that already used most of its stack gets
    /// a result rather than an abort.
    #[test]
    fn nesting_up_to_the_bound_converts_on_a_small_stack() {
        let deepest = nested_json(MAX_CONTEXT_DEPTH - 1);
        let too_deep = nested_json(MAX_CONTEXT_DEPTH + 1);

        let (converted, refused) = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || (Context::try_from_json(&deepest).is_ok(), Context::try_from_json(&too_deep)))
            .unwrap()
            .join()
            .unwrap();

        assert!(converted);
        assert!(matches!(refused.map_err(|error| error.cause().clone()), Err(InvalidContext::TooDeep)));
    }

    /// Duplicate keyword entries fail like duplicate term bindings do,
    /// instead of silently keeping the last value, and name the duplicated key.
    #[test]
    fn duplicate_keyword_entries_are_rejected() {
        let error = refusal(r#"{"@vocab": "http://a/", "@vocab": "http://b/"}"#);
        assert_eq!(location(&error), ["@vocab"]);
        assert!(matches!(error.cause(), InvalidContext::DuplicateKey));

        let error = refusal(r#"{"speed": "http://a/speed", "speed": "http://b/speed"}"#);
        assert_eq!(location(&error), ["speed"]);
        assert!(matches!(error.cause(), InvalidContext::DuplicateKey));

        let json = jstrict::Value::parse_str(r#"{"@id": "http://a/x", "@id": "http://b/x"}"#).unwrap().0;
        let error = TermDefinition::try_from_json(&json).unwrap_err();
        assert_eq!(location(&error), ["@id"]);
        assert!(matches!(error.cause(), InvalidContext::DuplicateKey));
    }
}
