use contextual::DisplayWithContext;
use jsonld_context_processing::algorithm::MalformedIri;
use langtag::InvalidLangTag;
use rdfx::vocabulary::BlankIdVocabulary;
use std::fmt;

#[derive(Debug)]
/// Warning raised while expanding a document.
pub enum Warning<B> {
    /// A term expanded to a string that is not a well-formed IRI. It becomes
    /// an invalid identifier, which the [`Policy`](crate::Policy) in use
    /// decides to keep, drop or reject.
    MalformedIri(String),
    /// A JSON object has an empty key, which no context can define as a term.
    EmptyTerm,
    /// A key expanded to a blank node identifier, making it a property with no
    /// RDF equivalent unless generalized RDF is allowed.
    BlankNodeIdProperty(B),
    /// A language tag is not well-formed per BCP 47. It is kept in the expanded
    /// document as it is; the warning carries the tag and the reason it was
    /// rejected.
    MalformedLanguageTag(String, InvalidLangTag<String>),
    /// A warning raised while processing a scoped or local `@context`.
    ContextProcessing(jsonld_context_processing::Warning),
}

impl<B> Warning<B> {
    /// Rewrites the blank node identifier this warning carries, if any.
    ///
    /// [`Warning::BlankNodeIdProperty`] is the only variant holding one. Use
    /// this when a warning crosses from one vocabulary to another, so that the
    /// handler resolving the identifier for display looks it up in the
    /// vocabulary that minted it rather than printing the wrong blank node.
    pub fn map_blank_id<C>(self, map: impl FnOnce(B) -> C) -> Warning<C> {
        match self {
            Self::MalformedIri(s) => Warning::MalformedIri(s),
            Self::EmptyTerm => Warning::EmptyTerm,
            Self::BlankNodeIdProperty(b) => Warning::BlankNodeIdProperty(map(b)),
            Self::MalformedLanguageTag(t, e) => Warning::MalformedLanguageTag(t, e),
            Self::ContextProcessing(w) => Warning::ContextProcessing(w),
        }
    }
}

impl<B> From<jsonld_context_processing::Warning> for Warning<B> {
    fn from(w: jsonld_context_processing::Warning) -> Self {
        Self::ContextProcessing(w)
    }
}

/// Forwards context-processing warnings into the expansion warning handler,
/// wrapped as [`Warning::ContextProcessing`].
///
/// Scoped and local `@context`s are processed in the middle of expansion, and
/// the context processing algorithm reports warnings of its own type. This
/// adapter re-types them so they all reach the single handler given to
/// [`Expand::expand_full`](crate::Expand::expand_full), rather than the
/// [`Print`](jsonld_core::warning::Print) handler that context processing
/// falls back to on its own.
pub(crate) struct ContextWarnings<'a, W>(pub &'a mut W);

impl<N, W> jsonld_core::warning::Handler<N, jsonld_context_processing::Warning> for ContextWarnings<'_, W>
where
    N: BlankIdVocabulary,
    W: crate::WarningHandler<N>,
{
    fn handle(&mut self, vocabulary: &N, warning: jsonld_context_processing::Warning) {
        self.0.handle(vocabulary, Warning::ContextProcessing(warning));
    }
}

impl<B> From<MalformedIri> for Warning<B> {
    fn from(MalformedIri(s): MalformedIri) -> Self {
        Self::MalformedIri(s)
    }
}

impl<B: fmt::Display> fmt::Display for Warning<B> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MalformedIri(s) => write!(f, "malformed IRI `{s}`"),
            Self::EmptyTerm => write!(f, "empty term"),
            Self::BlankNodeIdProperty(b) => {
                write!(f, "blank node identifier `{b}` used as property")
            }
            Self::MalformedLanguageTag(t, e) => write!(f, "invalid language tag `{t}`: {e}"),
            Self::ContextProcessing(w) => w.fmt(f),
        }
    }
}

impl<B, N: BlankIdVocabulary<BlankId = B>> DisplayWithContext<N> for Warning<B> {
    fn fmt_with(&self, vocabulary: &N, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MalformedIri(s) => write!(f, "malformed IRI `{s}`"),
            Self::EmptyTerm => write!(f, "empty term"),
            Self::BlankNodeIdProperty(b) => match vocabulary.blank_id(b) {
                Some(s) => write!(f, "blank node identifier `{s}` used as property"),
                None => f.write_str("blank node identifier `<unresolved blank>` used as property"),
            },
            Self::MalformedLanguageTag(t, e) => write!(f, "invalid language tag `{t}`: {e}"),
            Self::ContextProcessing(w) => write!(f, "{w}"),
        }
    }
}
