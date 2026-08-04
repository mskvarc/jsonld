use crate::{Action, ExpandedEntry, Warning, WarningHandler, expand_iri};
use jsonld_context_processing::algorithm::RejectVocab;
use jsonld_core::{Context, Environment, Id, Indexed, IndexedObject, LangString, Object, ProcessingMode, Term, ValidId, Value, object::value::Literal};
use jsonld_syntax::{Direction, ErrorCode, Keyword, LenientLangTagBuf, Nullable};
use rdfx::vocabulary::VocabularyMut;

/// Error raised while expanding a value object, a map with an `@value` entry.
#[derive(Debug, thiserror::Error)]
pub enum InvalidValue {
    /// The `@language` entry of the value object is not a string.
    #[error("Invalid language tagged string")]
    LanguageTaggedString,

    /// The `@direction` entry of the value object is neither `"ltr"` nor
    /// `"rtl"`.
    #[error("Invalid base `@direction`")]
    BaseDirection,

    /// The `@index` entry of the value object is not a string.
    #[error("Invalid `@index` value")]
    IndexValue,

    /// The `@type` entry of the value object is not a string, or does not
    /// expand to an IRI or to `@json`.
    #[error("Invalid typed value")]
    TypedValue,

    /// The value object has an entry other than `@value`, `@type`,
    /// `@language`, `@direction` and `@index`, or combines `@type` with
    /// `@language` or `@direction`.
    #[error("Invalid value object")]
    ValueObject,

    /// The `@value` entry holds an array or a map where only a scalar or
    /// `null` is allowed. A `@type` of `@json` lifts that restriction.
    #[error("Invalid value object value")]
    ValueObjectValue,

    /// The value object carries `@language` or `@direction` but its `@value`
    /// is not a string, so it cannot be tagged.
    #[error("Invalid language tagged value")]
    LanguageTaggedValue,

    /// The `@type` entry had to be expanded through the vocabulary mapping
    /// (`@vocab`), which the [`Policy`](crate::Policy) in use forbids.
    #[error("Forbidden use of `@vocab`")]
    ForbiddenVocab,
}

impl InvalidValue {
    /// Returns the JSON-LD error code this error is reported under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::LanguageTaggedString => ErrorCode::InvalidLanguageTaggedString,
            Self::BaseDirection => ErrorCode::InvalidBaseDirection,
            Self::IndexValue => ErrorCode::InvalidIndexValue,
            Self::TypedValue => ErrorCode::InvalidTypedValue,
            Self::ValueObject => ErrorCode::InvalidValueObject,
            Self::ValueObjectValue => ErrorCode::InvalidValueObjectValue,
            Self::LanguageTaggedValue => ErrorCode::InvalidLanguageTaggedValue,
            Self::ForbiddenVocab => ErrorCode::InvalidVocabMapping,
        }
    }
}

impl From<RejectVocab> for InvalidValue {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

/// Result of expanding a value object, `None` standing for a value object that
/// is dropped because its `@value` is `null`.
pub type ValueExpansionResult<T, B> = Result<Option<IndexedObject<T, B>>, InvalidValue>;

/// Expands a value object, a map whose entries have already been expanded and
/// one of which is `@value`.
///
/// Implements the value object cases of the [Expansion
/// algorithm](https://www.w3.org/TR/json-ld11-api/#expansion-algorithm): the
/// only entries a value object may have are `@value`, `@type`, `@language`,
/// `@direction` and `@index`, and their combination decides whether the result
/// is a plain literal, a typed literal, a language-tagged string or a JSON
/// literal.
pub(crate) fn expand_value<N, L, W>(
    env: &mut Environment<N, L, W>,
    vocab_policy: Action,
    processing_mode: ProcessingMode,
    input_type: Option<&Term<N::Iri, N::BlankId>>,
    type_scoped_context: &Context<N::Iri, N::BlankId>,
    expanded_entries: Vec<ExpandedEntry<N::Iri, N::BlankId>>,
    value_entry: &jstrict::Value,
) -> ValueExpansionResult<N::Iri, N::BlankId>
where
    N: VocabularyMut,
    N::Iri: Clone + PartialEq,
    N::BlankId: Clone + PartialEq,
    W: WarningHandler<N>,
{
    let mut is_json = input_type.is_some_and(|t| *t == Term::Keyword(Keyword::Json));
    let mut ty = None;
    let mut index = None;
    let mut language = None;
    let mut direction = None;

    for ExpandedEntry(_, expanded_key, value) in expanded_entries {
        match expanded_key.as_ref() {
            // If expanded property is @language:
            Term::Keyword(Keyword::Language) => {
                // If value is not a string, an invalid language-tagged string
                // error has been detected and processing is aborted.
                if let Some(value) = value.as_str() {
                    // Otherwise, set expanded value to value. If value is not
                    // well-formed according to section 2.2.9 of [BCP47],
                    // processors SHOULD issue a warning. That warning is
                    // raised further down, where the tag is turned into a
                    // `LenientLangTagBuf`.

                    if value != "@none" {
                        language = Some(value.to_owned());
                    }
                } else {
                    return Err(InvalidValue::LanguageTaggedString);
                }
            }
            // If expanded property is @direction:
            Term::Keyword(Keyword::Direction) => {
                // If processing mode is json-ld-1.0, continue with the next key
                // from element.
                if processing_mode == ProcessingMode::JsonLd1_0 {
                    continue;
                }

                // If value is neither "ltr" nor "rtl", an invalid base direction
                // error has been detected and processing is aborted.
                if let Some(value) = value.as_str() {
                    if let Ok(value) = Direction::try_from(value) {
                        direction = Some(value);
                    } else {
                        return Err(InvalidValue::BaseDirection);
                    }
                } else {
                    return Err(InvalidValue::BaseDirection);
                }
            }
            // If expanded property is @index:
            Term::Keyword(Keyword::Index) => {
                // If value is not a string, an invalid @index value error has
                // been detected and processing is aborted.
                if let Some(value) = value.as_str() {
                    index = Some(value.to_string());
                } else {
                    return Err(InvalidValue::IndexValue);
                }
            }
            // If expanded property is @type:
            Term::Keyword(Keyword::Type) => {
                if let Some(ty_value) = value.as_str() {
                    let expanded_ty = expand_iri(env, type_scoped_context, Nullable::Some(ty_value.into()), true, Some(vocab_policy))?;

                    match expanded_ty.as_deref() {
                        Some(Term::Keyword(Keyword::Json)) => {
                            is_json = true;
                        }
                        Some(Term::Id(Id::Valid(ValidId::Iri(expanded_ty)))) => {
                            is_json = false;
                            ty = Some(expanded_ty.clone());
                        }
                        _ => return Err(InvalidValue::TypedValue),
                    }
                } else {
                    return Err(InvalidValue::TypedValue);
                }
            }
            // The `@value` entry itself is handled after the loop, from
            // `value_entry`.
            Term::Keyword(Keyword::Value) => (),
            // A value object must not have any other entry.
            _ => {
                return Err(InvalidValue::ValueObject);
            }
        }
    }

    // If input type is @json, set expanded value to value. The specification
    // also raises an invalid value object value error here when the processing
    // mode is json-ld-1.0, which this implementation does not: context
    // processing already rejects a term definition with a `@json` type mapping
    // under 1.0, but an explicit `"@type": "@json"` entry in a value object
    // still goes through.
    if is_json {
        // A value object must not combine a type with `@language` or
        // `@direction`, `@json` included.
        if language.is_some() || direction.is_some() {
            return Err(InvalidValue::ValueObject);
        }
        return Ok(Some(Indexed::new(Object::Value(Value::Json(value_entry.clone())), index)));
    }

    // Otherwise, if value is not a scalar or null, an invalid value object value
    // error has been detected and processing is aborted.
    let result = match value_entry {
        jstrict::Value::Null => Literal::Null,
        jstrict::Value::String(s) => Literal::String(s.clone()),
        jstrict::Value::Number(n) => Literal::Number(n.clone()),
        jstrict::Value::Boolean(b) => Literal::Boolean(*b),
        _ => {
            return Err(InvalidValue::ValueObjectValue);
        }
    };

    // A `@type` of `@json` lets the `@value` entry hold any value, treated as a
    // JSON literal. That case returned above, before the scalar check.

    // Otherwise, if the value of result's @value entry is null, or an empty array,
    // return null
    if matches!(result, Literal::Null) {
        return Ok(None);
    }

    // Otherwise, if the value of result's @value entry is not a string and result
    // contains the entry @language, an invalid language-tagged value error has
    // been detected (only strings can be language-tagged) and processing is
    // aborted.
    if language.is_some() || direction.is_some() {
        // A value object must not have an `@type` entry alongside `@language`
        // or `@direction`.
        if ty.is_some() {
            return Err(InvalidValue::ValueObject);
        }

        if let Literal::String(s) = result {
            let lang = match language {
                Some(language) => {
                    let (language, error) = LenientLangTagBuf::new(language);

                    if let Some(error) = error {
                        env.warnings.handle(env.vocabulary, Warning::MalformedLanguageTag(language.to_string(), error));
                    }

                    Some(language)
                }
                None => None,
            };

            return match LangString::new(s, lang, direction) {
                Ok(result) => Ok(Some(Indexed::new(Object::Value(Value::LangString(result)), index))),
                Err(_) => Err(InvalidValue::LanguageTaggedValue),
            };
        }
        return Err(InvalidValue::LanguageTaggedValue);
    }

    // The specification also drops free-floating value objects here, when the
    // active property is null or `@graph`. This implementation drops them one
    // level up instead, in `filter_top_level_item`, which filters both the top
    // level of the document and the content of every `@graph` entry.

    Ok(Some(Indexed::new(Object::Value(Value::Literal(result, ty)), index)))
}
