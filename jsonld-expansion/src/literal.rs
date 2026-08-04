use crate::{ActiveProperty, WarningHandler, expand_iri, node_id_of_term};
use jsonld_context_processing::algorithm::{Action, RejectVocab};
use jsonld_core::{Context, Environment, IndexedObject, LangString, Node, Object, Type, Value, object::value::Literal};
use jsonld_syntax::{ErrorCode, LenientLangTag, Nullable};
use jstrict::Number;
use rdfx::vocabulary::VocabularyMut;

/// Scalar taken as it appears in the input document.
pub(crate) enum GivenLiteralValue<'a> {
    Boolean(bool),
    Number(&'a Number),
    String(&'a str),
}

/// Error raised when value expansion is asked to expand something that is not
/// a JSON scalar, such as an array or an object.
#[derive(Debug, thiserror::Error)]
#[error("not a literal value")]
pub struct NotALiteral;

impl<'a> GivenLiteralValue<'a> {
    pub fn new(value: &'a jstrict::Value) -> Result<Self, NotALiteral> {
        match value {
            jstrict::Value::Boolean(b) => Ok(Self::Boolean(*b)),
            jstrict::Value::Number(n) => Ok(Self::Number(n)),
            jstrict::Value::String(s) => Ok(Self::String(s)),
            _ => Err(NotALiteral),
        }
    }

    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Value handed to the Value Expansion algorithm.
pub(crate) enum LiteralValue<'a> {
    /// A scalar read from the input document.
    Given(GivenLiteralValue<'a>),
    /// A string the algorithm produced itself, as when an index map key is
    /// re-expanded as the value of the index property.
    Inferred(jsonld_syntax::String),
}

impl LiteralValue<'_> {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Given(v) => v.as_str(),
            Self::Inferred(s) => Some(s.as_str()),
        }
    }
}

pub(crate) type ExpandedLiteral<T, B> = IndexedObject<T, B>;

/// Error raised while expanding a scalar into a literal or a node reference.
#[derive(Debug, thiserror::Error)]
pub enum LiteralExpansionError {
    /// The active property has a type mapping that is neither `@id`, `@vocab`,
    /// `@none` nor an IRI, so it cannot be used as the datatype of the
    /// literal.
    #[error("Invalid `@type` value")]
    InvalidTypeValue,

    /// The value had to be expanded through the vocabulary mapping (`@vocab`),
    /// which the [`Policy`](crate::Policy) in use forbids.
    #[error("Forbidden use of `@vocab`")]
    ForbiddenVocab,

    /// The active property has an `@id` type mapping, but the value expanded
    /// to nothing, leaving no identifier for the node reference.
    #[error("IRI expansion produced no result")]
    IdExpansionEmpty,

    /// The value to expand is not a JSON scalar.
    #[error(transparent)]
    NotALiteral(#[from] NotALiteral),
}

impl LiteralExpansionError {
    /// Returns the JSON-LD error code this error is reported under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidTypeValue => ErrorCode::InvalidTypeValue,
            Self::ForbiddenVocab => ErrorCode::InvalidVocabMapping,
            Self::IdExpansionEmpty => ErrorCode::InvalidIdValue,
            Self::NotALiteral(_) => ErrorCode::InvalidValueObject,
        }
    }
}

impl From<RejectVocab> for LiteralExpansionError {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

pub(crate) type LiteralExpansionResult<T, B> = Result<ExpandedLiteral<T, B>, LiteralExpansionError>;

/// Expands a scalar into a value object, or into a node reference when the
/// active property maps its values to identifiers.
///
/// Implements the [Value Expansion
/// algorithm](https://www.w3.org/TR/json-ld11-api/#value-expansion): the type,
/// language and direction mappings of the active property, or the defaults of
/// the active context, decide what the scalar becomes.
pub(crate) fn expand_literal<N, L, W>(
    mut env: Environment<N, L, W>,
    vocab_policy: Action,
    active_context: &Context<N::Iri, N::BlankId>,
    active_property: ActiveProperty<'_>,
    value: LiteralValue,
) -> LiteralExpansionResult<N::Iri, N::BlankId>
where
    N: VocabularyMut,
    N::Iri: Clone,
    N::BlankId: Clone,
    W: WarningHandler<N>,
{
    let active_property_definition = active_property.get_from(active_context);
    let active_property_type = if let Some(active_property_definition) = active_property_definition {
        active_property_definition.typ().cloned()
    } else {
        None
    };

    match active_property_type {
        // If the `active_property` has a type mapping in `active_context` that is `@id`, and the
        // `value` is a string, return a new map containing a single entry where the key is `@id` and
        // the value is the result of IRI expanding `value` using `true` for `document_relative` and
        // `false` for vocab.
        Some(Type::Id) if let Some(s) = value.as_str() => {
            let mut node = Node::new();
            let Some(id_term) = expand_iri(&mut env, active_context, Nullable::Some(s.into()), true, None)? else {
                return Err(LiteralExpansionError::IdExpansionEmpty);
            };
            node.id = node_id_of_term(id_term);
            Ok(Object::node(node).into())
        }

        // If `active_property` has a type mapping in active context that is `@vocab`, and the
        // value is a string, return a new map containing a single entry where the key is
        // `@id` and the value is the result of IRI expanding `value` using `true` for
        // document relative.
        Some(Type::Vocab) if let Some(s) = value.as_str() => {
            let mut node = Node::new();

            let ty = expand_iri(&mut env, active_context, Nullable::Some(s.into()), true, Some(vocab_policy))?;

            if let Some(ty) = ty {
                let id = node_id_of_term(ty);
                node.id = id;
            }

            Ok(Object::node(node).into())
        }

        _ => {
            // Otherwise, initialize `result` to a map with an `@value` entry whose value is set to
            // `value`.
            let result: Literal = match value {
                LiteralValue::Given(v) => match v {
                    GivenLiteralValue::Boolean(b) => Literal::Boolean(b),
                    GivenLiteralValue::Number(n) => Literal::Number(jstrict::NumberBuf::from_number(n)),
                    GivenLiteralValue::String(s) => Literal::String(s.into()),
                },
                LiteralValue::Inferred(s) => Literal::String(s),
            };

            // If `active_property` has a type mapping in active context, other than `@id`,
            // `@vocab`, or `@none`, add `@type` to `result` and set its value to the value
            // associated with the type mapping.
            let mut ty = None;
            match active_property_type {
                None | Some(Type::Id | Type::Vocab | Type::None) => {
                    // Otherwise, if value is a string:
                    if let Literal::String(s) = result {
                        // Initialize `language` to the language mapping for
                        // `active_property` in `active_context`, if any, otherwise to the
                        // default language of `active_context`.
                        let language = if let Some(active_property_definition) = active_property_definition {
                            if let Some(language) = active_property_definition.language() {
                                language.cloned().option()
                            } else {
                                active_context.default_language().map(LenientLangTag::to_owned)
                            }
                        } else {
                            active_context.default_language().map(LenientLangTag::to_owned)
                        };

                        // Initialize `direction` to the direction mapping for
                        // `active_property` in `active_context`, if any, otherwise to the
                        // default base direction of `active_context`.
                        let direction = if let Some(active_property_definition) = active_property_definition {
                            if let Some(direction) = active_property_definition.direction() {
                                direction.option()
                            } else {
                                active_context.default_base_direction()
                            }
                        } else {
                            active_context.default_base_direction()
                        };

                        // If `language` is not null, add `@language` to result with the value
                        // `language`.
                        // If `direction` is not null, add `@direction` to result with the
                        // value `direction`.
                        return match LangString::new(s, language, direction) {
                            Ok(lang_str) => Ok(Object::Value(Value::LangString(lang_str)).into()),
                            Err(jsonld_core::InvalidLangString(s)) => Ok(Object::Value(Value::Literal(Literal::String(s), None)).into()),
                        };
                    }
                }

                Some(t) => {
                    if let Ok(t) = t.into_iri() {
                        ty = Some(t);
                    } else {
                        return Err(LiteralExpansionError::InvalidTypeValue);
                    }
                }
            }

            Ok(Object::Value(Value::Literal(result, ty)).into())
        }
    }
}
