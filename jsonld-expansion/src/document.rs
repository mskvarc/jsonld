use super::expand_element;
use crate::{ActiveProperty, Error, Expanded, Loader, Options, WarningHandler};
use jsonld_core::{Context, Environment, ExpandedDocument, IndexedObject, Object};
use jstrict::Value;
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Expands a whole JSON-LD document: expands its root element, then applies
/// the specification's post-processing of the top-level result.
///
/// This is the entry point behind the [`Expand`](crate::Expand) trait, whose
/// [`expand`](crate::Expand::expand) and
/// [`expand_with`](crate::Expand::expand_with) methods build the environment
/// and the initial context for you.
pub(crate) async fn expand<'a, N, L, W>(
    env: Environment<'a, N, L, W>,
    document: &'a Value,
    active_context: Context<N::Iri, N::BlankId>,
    base_url: Option<&'a N::Iri>,
    options: Options,
) -> Result<ExpandedDocument<N::Iri, N::BlankId>, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // When the expansion result is a single object that is an unnamed graph
    // (a node object whose only entry is `@graph`), the document is the
    // content of that graph. Otherwise the object stands alone, unless it is
    // free-floating and gets dropped.
    fn single<T: Eq + Hash, B: Eq + Hash>(obj: IndexedObject<T, B>) -> ExpandedDocument<T, B> {
        match obj.into_unnamed_graph() {
            Ok(graph) => ExpandedDocument::from(graph),
            Err(obj) => {
                let mut result = ExpandedDocument::new();
                if filter_top_level_item(&obj) {
                    result.insert(obj);
                }
                result
            }
        }
    }

    let expanded = expand_element(env, &active_context, ActiveProperty::None, document, base_url, options, false, None).await?;
    match expanded {
        Expanded::Null => Ok(ExpandedDocument::new()),
        Expanded::Object(obj) => Ok(single(obj)),
        Expanded::Array(ary) => match <[_; 1]>::try_from(ary) {
            Ok([obj]) => Ok(single(obj)),
            Err(ary) => Ok(ary.into_iter().filter(filter_top_level_item).collect()),
        },
    }
}

/// Checks whether an expanded object may stay at the top level of a document
/// or of a `@graph` entry.
///
/// A value object in such a position describes no subject: the specification
/// calls it free-floating and drops it.
pub(crate) fn filter_top_level_item<T, B>(item: &IndexedObject<T, B>) -> bool {
    !matches!(item.inner(), Object::Value(_))
}
