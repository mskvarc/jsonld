//! Expansion of a document given as JSON text.
use crate::{Context, Expand, ExpandedDocument, Loader, Options, WarningHandler, error::Error};
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Error of [`expand_slice`].
#[derive(Debug, thiserror::Error)]
pub enum SliceExpansionError<E> {
    /// The bytes are not one well-formed JSON text (RFC 8259).
    #[error("invalid JSON: {0}")]
    Syntax(#[from] jstrict::parse::Error),
    /// The JSON text is not a document the Expansion algorithm accepts.
    #[error(transparent)]
    Expansion(Error<E>),
}

/// Expands the JSON-LD document whose JSON text is `bytes`.
///
/// The text is parsed with `jstrict`'s non-recursive parser, expanded
/// exactly as [`Expand::expand_full`] expands the parsed value, and the
/// parsed tree is dropped before this returns, so no JSON tree outlives the
/// call. `context` is the initial active context: an empty one built with
/// `Context::new(base_url)` for a document that carries its own `@context`,
/// or an active context the caller processed once and reuses across
/// documents. Parsing, expansion and the drop of the parsed tree are all
/// bounded by memory, not by the caller's native stack.
///
/// # Errors
///
/// [`SliceExpansionError::Syntax`] when `bytes` is not JSON, and
/// [`SliceExpansionError::Expansion`] for every failure `expand_full` reports.
pub async fn expand_slice<N, L, W>(
    bytes: &[u8],
    vocabulary: &mut N,
    context: Context<N::Iri, N::BlankId>,
    base_url: Option<&N::Iri>,
    loader: &L,
    options: Options,
    warnings: W,
) -> Result<ExpandedDocument<N::Iri, N::BlankId>, SliceExpansionError<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    let document = jstrict::parse::parse_slice_value(bytes)?;
    document
        .expand_full(vocabulary, context, base_url, loader, options, warnings)
        .await
        .map_err(SliceExpansionError::Expansion)
}
