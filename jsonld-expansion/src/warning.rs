use contextual::DisplayWithContext;
use jsonld_context_processing::algorithm::MalformedIri;
use langtag::InvalidLangTag;
use rdfx::vocabulary::BlankIdVocabulary;
use std::fmt;

#[derive(Debug)]
/// Warning raised while expanding a document.
pub enum Warning<B> {
    /// A term expanded to a malformed IRI.
    MalformedIri(String),
    /// An entry key expanded to the empty term.
    EmptyTerm,
    /// A blank node identifier was used as a property.
    BlankNodeIdProperty(B),
    /// A language tag is not well-formed.
    MalformedLanguageTag(String, InvalidLangTag<String>),
    /// A warning raised while processing a scoped or local `@context`.
    ContextProcessing(jsonld_context_processing::Warning),
}

impl<B> Warning<B> {
    /// Rewrites the blank node identifier this warning carries, if any.
    ///
    /// A warning raised by a task that ran against a forked vocabulary holds an
    /// identifier minted by that fork. It has to go through the fork's remap
    /// before the warning reaches a handler that resolves it against the parent
    /// vocabulary, or the handler prints the wrong blank node — or none at all.
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
/// Scoped and local `@context`s are processed in the middle of expansion.
/// This adapter keeps their warnings flowing to the handler given to
/// [`Expand::expand_full`](crate::Expand::expand_full) instead of leaking to
/// stderr through the default [`Print`](jsonld_core::warning::Print) handler.
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
