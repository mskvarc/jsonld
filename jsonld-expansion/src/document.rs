use super::expand_element;
use crate::{ActiveProperty, Error, Expanded, Loader, Options, WarningHandler};
use jsonld_core::{Context, Environment, ExpandedDocument, IndexedObject, Object};
use jstrict::Value;
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Expand the given JSON-LD document.
///
/// Note that you probably do not want to use this function directly,
/// but instead use the [`Document::expand`](crate::Document::expand) method on
/// a `Value` instance.
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
    let expanded = expand_element(env, &active_context, ActiveProperty::None, document, base_url, options, false, None).await?;

    // A single expanded object gets its `@graph` unwrapped; anything else is
    // filtered and collected as-is.
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

    match expanded {
        Expanded::Null => Ok(ExpandedDocument::new()),
        Expanded::Object(obj) => Ok(single(obj)),
        Expanded::Array(ary) => match <[_; 1]>::try_from(ary) {
            Ok([obj]) => Ok(single(obj)),
            Err(ary) => Ok(ary.into_iter().filter(filter_top_level_item).collect()),
        },
    }
}

pub(crate) fn filter_top_level_item<T, B>(item: &IndexedObject<T, B>) -> bool {
    // Remove dangling values.
    !matches!(item.inner(), Object::Value(_))
}
