use jsonld_context_processing::algorithm::RejectVocab;
use jsonld_syntax::ErrorCode;

#[derive(Debug, thiserror::Error)]
/// Error raised while expanding a document.
pub enum Error<E = std::convert::Infallible> {
    #[error("Invalid context: {0}")]
    /// Invalid context: the given value.
    ContextSyntax(#[from] jsonld_syntax::context::InvalidContext),

    #[error("Context processing failed: {0}")]
    /// Context processing failed: the given value.
    ContextProcessing(jsonld_context_processing::Error<E>),

    #[error("Invalid `@index` value")]
    /// Invalid `@index` value.
    InvalidIndexValue,

    #[error("Invalid set or list object")]
    /// Invalid set or list object.
    InvalidSetOrListObject,

    #[error("Invalid `@reverse` property map")]
    /// Invalid `@reverse` property map.
    InvalidReversePropertyMap,

    #[error("Invalid `@type` value")]
    /// Invalid `@type` value.
    InvalidTypeValue,

    #[error("Key `{0}` expansion failed")]
    /// Key `the given value` expansion failed.
    KeyExpansionFailed(String),

    #[error("Invalid `@reverse` property value")]
    /// Invalid `@reverse` property value.
    InvalidReversePropertyValue,

    #[error("Invalid `@language` map value")]
    /// Invalid `@language` map value.
    InvalidLanguageMapValue,

    #[error("Colliding keywords")]
    /// Colliding keywords.
    CollidingKeywords,

    #[error("Invalid `@id` value")]
    /// Invalid `@id` value.
    InvalidIdValue,

    #[error("Invalid `@included` value")]
    /// Invalid `@included` value.
    InvalidIncludedValue,

    #[error("Invalid `@reverse` value")]
    /// Invalid `@reverse` value.
    InvalidReverseValue,

    #[error("Invalid `@nest` value")]
    /// Invalid `@nest` value.
    InvalidNestValue,

    #[error("Duplicate key `{0}`")]
    /// Duplicate key `the given value`.
    DuplicateKey(jstrict::object::Key),

    #[error(transparent)]
    /// An RDF literal.
    Literal(crate::LiteralExpansionError),

    #[error(transparent)]
    /// A value object.
    Value(crate::InvalidValue),

    #[error("Forbidden use of `@vocab`")]
    /// Forbidden use of `@vocab`.
    ForbiddenVocab,

    #[error("List of lists")]
    /// A list object was found among the items of another list object.
    ///
    /// JSON-LD 1.0 only: 1.1 allows lists of lists.
    ListOfLists,
}

impl<E> From<RejectVocab> for Error<E> {
    fn from(_value: RejectVocab) -> Self {
        Self::ForbiddenVocab
    }
}

impl<E> Error<E> {
    /// Returns the code of this `Error`.
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
            Self::ListOfLists => ErrorCode::ListOfLists,
        }
    }
}

impl<E> Error<E> {
    /// Builds a duplicate-key error from the duplicate `jstrict` reports.
    pub fn duplicate_key_ref(jstrict::object::Duplicate(a, _b): jstrict::object::Duplicate<&jstrict::object::Entry>) -> Self {
        Self::DuplicateKey(a.key.clone())
    }
}

impl<E> From<jsonld_context_processing::Error<E>> for Error<E> {
    fn from(e: jsonld_context_processing::Error<E>) -> Self {
        Self::ContextProcessing(e)
    }
}

impl<E> From<crate::LiteralExpansionError> for Error<E> {
    fn from(e: crate::LiteralExpansionError) -> Self {
        Self::Literal(e)
    }
}

impl<E> From<crate::InvalidValue> for Error<E> {
    fn from(e: crate::InvalidValue) -> Self {
        Self::Value(e)
    }
}

impl<E> From<crate::literal::NotALiteral> for Error<E> {
    fn from(e: crate::literal::NotALiteral) -> Self {
        Self::Literal(crate::LiteralExpansionError::NotALiteral(e))
    }
}
