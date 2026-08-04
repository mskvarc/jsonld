use jsonld_core::ProcessingMode;

pub use jsonld_context_processing::algorithm::Action;

/// Expansion options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    /// JSON-LD version the algorithm conforms to.
    ///
    /// Defaults to `json-ld-1.1`. Under `json-ld-1.0` some 1.1 features are
    /// ignored, such as `@included` and `@direction` entries, and others are
    /// rejected, such as a list object among the items of another list object.
    /// The mode also applies to the processing of the contexts met on the way.
    pub processing_mode: ProcessingMode,

    /// What to do with the keys that cannot be expanded into a proper IRI.
    ///
    /// Defaults to [`Policy::default()`], the behaviour the specification
    /// prescribes.
    pub policy: Policy,

    /// Whether the entries of every JSON object are processed in
    /// lexicographic order of their keys.
    ///
    /// Ordering costs a sort per object and only matters when the expanded
    /// output has to be reproducible entry by entry, as in the JSON-LD test
    /// suite. `false` by default, which leaves entries in document order.
    pub ordered: bool,
}

impl Options {
    /// Returns a copy of these options with entry ordering switched off.
    #[must_use]
    pub fn unordered(self) -> Self {
        Self { ordered: false, ..self }
    }
}

impl From<Options> for jsonld_context_processing::Options {
    fn from(options: Options) -> jsonld_context_processing::Options {
        // Only the processing mode carries over. The other context-processing
        // options keep their default value here; the expansion steps that need
        // another value set it on the result, as with `with_override` for a
        // property-scoped context.
        jsonld_context_processing::Options {
            processing_mode: options.processing_mode,
            ..Default::default()
        }
    }
}

/// Key expansion policy.
///
/// By default the expansion algorithm drops the keys that are not defined in
/// the active context, unless:
///   - the active context defines a vocabulary mapping (`@vocab`); or
///   - the key contains a `:` character, in which case it is kept as it is,
///     even though it may not be a well-formed IRI.
///
/// Silently dropping data is not always what an application wants: a stricter
/// setting turns each of those situations into an error instead. Set the
/// wanted policy through the [`Options::policy`] field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// What to do with a key that expands to a malformed IRI.
    ///
    /// [`Action::Keep`] (the default) keeps it in the expanded document,
    /// [`Action::Drop`] drops the entry, and [`Action::Reject`] aborts
    /// expansion with [`Error::KeyExpansionFailed`] — or, for a value of
    /// `@type`, [`Error::InvalidTypeValue`].
    ///
    /// [`Error::KeyExpansionFailed`]: crate::Error::KeyExpansionFailed
    /// [`Error::InvalidTypeValue`]: crate::Error::InvalidTypeValue
    pub invalid: Action,

    /// What to do with a key that only expands because the active context
    /// defines a vocabulary mapping (`@vocab`).
    ///
    /// [`Action::Keep`] (the default) expands it against the vocabulary
    /// mapping, [`Action::Drop`] drops the entry, and [`Action::Reject`]
    /// aborts expansion with [`Error::ForbiddenVocab`].
    ///
    /// [`Error::ForbiddenVocab`]: crate::Error::ForbiddenVocab
    pub vocab: Action,

    /// Whether a key that neither the active context nor a vocabulary mapping
    /// can expand may be dropped.
    ///
    /// `true` by default. Set it to `false` to abort expansion with
    /// [`Error::KeyExpansionFailed`] instead of losing the entry.
    ///
    /// [`Error::KeyExpansionFailed`]: crate::Error::KeyExpansionFailed
    pub allow_undefined: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            invalid: Action::Keep,
            vocab: Action::Keep,
            allow_undefined: true,
        }
    }
}
