use crate::{ActiveProperty, Error, Expanded, Loader, Options, WarningHandler, expand_element};
use jsonld_context_processing::ProcessingCache;
use jsonld_core::{Context, Environment, Object, ProcessingMode, context::TermDefinitionRef, object};
use jsonld_syntax::ContainerKind;
use jstrict::Array;
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Expands a JSON array, concatenating the expansion of each of its items.
///
/// The result is a list object rather than an array when the active property
/// has a `@list` container mapping.
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
) -> Result<Expanded<N::Iri, N::BlankId>, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
    L: Loader,
    W: WarningHandler<N>,
{
    // Initialize an empty array, result.
    let mut result = Vec::new();

    // If the container mapping of `active_property` includes `@list`, the
    // expanded items are wrapped in a single map with a `@list` entry once the
    // loop below is done.
    let mut is_list = false;
    if let Some(definition) = active_property_definition {
        is_list = definition.container().contains(ContainerKind::List);
    }

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
        // JSON-LD 1.0 forbids wrapping list objects in another list; 1.1 lifted
        // the restriction (`expand#ter24`).
        if options.processing_mode == ProcessingMode::JsonLd1_0 && result.iter().any(|item| item.is_list()) {
            return Err(Error::ListOfLists);
        }

        return Ok(Expanded::Object(Object::List(object::List::new(result)).into()));
    }

    // Return result.
    Ok(Expanded::Array(result))
}
