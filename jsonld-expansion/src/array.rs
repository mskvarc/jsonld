use crate::{ActiveProperty, Error, Expanded, Loader, Options, WarningHandler, expand_element};
use jstrict::Array;
use jsonld_context_processing::ProcessingCache;
use jsonld_core::{Context, Environment, Object, context::TermDefinitionRef, object};
use jsonld_syntax::ContainerKind;
use rdf_rs::vocabulary::VocabularyMut;
use std::hash::Hash;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn expand_array<'a, N, L, W>(
    env: Environment<'a, N, L, W>,
    active_context: &'a Context<N::Iri, N::BlankId>,
    active_property: ActiveProperty<'a>,
    active_property_definition: Option<TermDefinitionRef<'a, N::Iri, N::BlankId>>,
    element: &'a Array,
    base_url: Option<&'a N::Iri>,
    options: Options,
    from_map: bool,
    cache: Option<&'a ProcessingCache<N::Iri, N::BlankId>>,
) -> Result<Expanded<N::Iri, N::BlankId>, Error>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // Initialize an empty array, result.
    let mut is_list = false;
    let mut result = Vec::new();

    // If the container mapping of `active_property` includes `@list`, and
    // `expanded_item` is an array, set `expanded_item` to a new map containing
    // the entry `@list` where the value is the original `expanded_item`.
    if let Some(definition) = active_property_definition {
        is_list = definition.container().contains(ContainerKind::List);
    }

    // For each item in element:
    for item in element.iter() {
        // Initialize `expanded_item` to the result of using this algorithm
        // recursively, passing `active_context`, `active_property`, `item` as element,
        // `base_url`, the `frame_expansion`, `ordered`, and `from_map` flags.
        let e = Box::pin(expand_element(
            Environment {
                vocabulary: env.vocabulary,
                loader: env.loader,
                warnings: env.warnings,
            },
            active_context,
            active_property,
            item,
            base_url,
            options,
            from_map,
            cache,
        ))
        .await?;

        result.extend(e);
    }

    if is_list {
        return Ok(Expanded::Object(Object::List(object::List::new(result)).into()));
    }

    // Return result.
    Ok(Expanded::Array(result))
}
