use jsonld_context_processing::algorithm::RejectVocab;
use jsonld_syntax::ErrorCode;

#[derive(Debug, thiserror::Error)]
/// Error raised while expanding a document.
pub enum Error<E = std::convert::Infallible> {
    #[error("Invalid context: {0}")]
    /// The value of an `@context` entry is not a valid JSON-LD context.
    ContextSyntax(#[from] jsonld_syntax::context::InvalidContext),

    #[error("Context processing failed: {0}")]
    /// Processing a local, scoped or remote `@context` failed. This includes
    /// the failures of the document loader fetching a remote context.
    ContextProcessing(jsonld_context_processing::Error<E>),

    #[error("Invalid `@index` value")]
    /// An `@index` entry has a value that is not a string.
    InvalidIndexValue,

    #[error("Invalid set or list object")]
    /// A `@set` or `@list` object has an entry other than `@set`/`@list` and
    /// `@index`.
    InvalidSetOrListObject,

    #[error("Invalid `@reverse` property map")]
    /// A key of a `@reverse` map expands to a keyword, or a keyword entry was
    /// found while `@reverse` is the active property.
    InvalidReversePropertyMap,

    #[error("Invalid `@type` value")]
    /// An `@type` entry is not a string or an array of strings, or one of its
    /// values does not expand to an IRI or blank node identifier.
    InvalidTypeValue,

    #[error("Key `{0}` expansion failed")]
    /// The given key could not be expanded into an IRI, and the [`Policy`] in
    /// use rejects such keys instead of dropping them.
    ///
    /// [`Policy`]: crate::Policy
    KeyExpansionFailed(String),

    #[error("Invalid `@reverse` property value")]
    /// A reverse property has a value that is not a node object, which cannot
    /// be the subject of the reversed relation.
    InvalidReversePropertyValue,

    #[error("Invalid `@language` map value")]
    /// A language map (`@container: @language`) has a value that is neither a
    /// string, `null`, nor an array of those.
    InvalidLanguageMapValue,

    #[error("Colliding keywords")]
    /// Two distinct keys of the same node object expand to the same keyword.
    ///
    /// In JSON-LD 1.1 this is tolerated for `@included` and `@type`, whose
    /// values are merged instead.
    CollidingKeywords,

    #[error("Invalid `@id` value")]
    /// An `@id` entry has a value that is not a string.
    InvalidIdValue,

    #[error("Invalid `@included` value")]
    /// An `@included` entry contains something else than node objects.
    InvalidIncludedValue,

    #[error("Invalid `@reverse` value")]
    /// A `@reverse` entry has a value that is not a map.
    InvalidReverseValue,

    #[error("Invalid `@nest` value")]
    /// An `@nest` entry has a value that is not a map (or an array of maps),
    /// or one of those maps has an entry expanding to `@value`.
    InvalidNestValue,

    #[error("Duplicate key `{0}`")]
    /// The given key appears twice in the same JSON object.
    DuplicateKey(jstrict::object::Key),

    #[error(transparent)]
    /// Expanding a scalar into a literal failed. See
    /// [`LiteralExpansionError`](crate::LiteralExpansionError).
    Literal(crate::LiteralExpansionError),

    #[error(transparent)]
    /// A value object (a map with an `@value` entry) is invalid. See
    /// [`InvalidValue`](crate::InvalidValue).
    Value(crate::InvalidValue),

    #[error("Forbidden use of `@vocab`")]
    /// A key could only be expanded through the vocabulary mapping (`@vocab`),
    /// which the [`Policy`](crate::Policy) in use forbids.
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
    /// Returns the JSON-LD error code this error is reported under, as named by
    /// the [JSON-LD API specification][spec].
    ///
    /// [spec]: https://www.w3.org/TR/json-ld11-api/#jsonlderror
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
    /// Builds an [`Error::DuplicateKey`] from the pair of colliding entries
    /// `jstrict` reports, keeping the key of the first one.
    #[must_use]
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
