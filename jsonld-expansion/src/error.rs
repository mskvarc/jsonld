use jsonld_context_processing::algorithm::RejectVocab;
use jsonld_syntax::ErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid context: {0}")]
    ContextSyntax(#[from] jsonld_syntax::context::InvalidContext),

    #[error("Context processing failed: {0}")]
    ContextProcessing(jsonld_context_processing::Error),

    #[error("Invalid `@index` value")]
    InvalidIndexValue,

    #[error("Invalid set or list object")]
    InvalidSetOrListObject,

    #[error("Invalid `@reverse` property map")]
    InvalidReversePropertyMap,

    #[error("Invalid `@type` value")]
    InvalidTypeValue,

    #[error("Key `{0}` expansion failed")]
    KeyExpansionFailed(String),

    #[error("Invalid `@reverse` property value")]
    InvalidReversePropertyValue,

    #[error("Invalid `@language` map value")]
    InvalidLanguageMapValue,

    #[error("Colliding keywords")]
    CollidingKeywords,

    #[error("Invalid `@id` value")]
    InvalidIdValue,

    #[error("Invalid `@included` value")]
    InvalidIncludedValue,

    #[error("Invalid `@reverse` value")]
    InvalidReverseValue,

    #[error("Invalid `@nest` value")]
    InvalidNestValue,

    #[error("Duplicate key `{0}`")]
    DuplicateKey(jstrict::object::Key),

    #[error(transparent)]
    Literal(crate::LiteralExpansionError),

    #[error(transparent)]
    Value(crate::InvalidValue),

    #[error("Forbidden use of `@vocab`")]
    ForbiddenVocab,
}

impl From<RejectVocab> for Error {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

impl Error {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::ContextSyntax(e) => e.code(),
            Self::ContextProcessing(e) => e.code(),
            Self::InvalidIndexValue => ErrorCode::InvalidIndexValue,
            Self::InvalidSetOrListObject => ErrorCode::InvalidSetOrListObject,
            Self::InvalidReversePropertyMap => ErrorCode::InvalidReversePropertyMap,
            Self::InvalidTypeValue => ErrorCode::InvalidTypeValue,
            Self::KeyExpansionFailed(_) => ErrorCode::KeyExpansionFailed,
            Self::InvalidReversePropertyValue => ErrorCode::InvalidReversePropertyValue,
            Self::InvalidLanguageMapValue => ErrorCode::InvalidLanguageMapValue,
            Self::CollidingKeywords => ErrorCode::CollidingKeywords,
            Self::InvalidIdValue => ErrorCode::InvalidIdValue,
            Self::InvalidIncludedValue => ErrorCode::InvalidIncludedValue,
            Self::InvalidReverseValue => ErrorCode::InvalidReverseValue,
            Self::InvalidNestValue => ErrorCode::InvalidNestValue,
            Self::DuplicateKey(_) => ErrorCode::DuplicateKey,
            Self::Literal(e) => e.code(),
            Self::Value(e) => e.code(),
            Self::ForbiddenVocab => ErrorCode::InvalidVocabMapping,
        }
    }
}

impl Error {
    pub fn duplicate_key_ref(jstrict::object::Duplicate(a, _b): jstrict::object::Duplicate<&jstrict::object::Entry>) -> Self {
        Self::DuplicateKey(a.key.clone())
    }
}

impl From<jsonld_context_processing::Error> for Error {
    fn from(e: jsonld_context_processing::Error) -> Self {
        Self::ContextProcessing(e)
    }
}

impl From<crate::LiteralExpansionError> for Error {
    fn from(e: crate::LiteralExpansionError) -> Self {
        Self::Literal(e)
    }
}

impl From<crate::InvalidValue> for Error {
    fn from(e: crate::InvalidValue) -> Self {
        Self::Value(e)
    }
}
