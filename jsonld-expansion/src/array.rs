use crate::{ActiveProperty, Error, Expanded, Loader, Options, WarningHandler, expand_element};
use jstrict::Array;
use jsonld_context_processing::ProcessingCache;
use jsonld_core::{Context, Environment, Object, ParallelSafeVocabulary, context::TermDefinitionRef, object};
use jsonld_syntax::ContainerKind;
use rdf_rs::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Probe a sample of the array's items to decide whether the parallel branch
/// is worth its overhead. Returns `true` only when at least one of the sampled
/// items is a non-trivial node-like object or nested array — those contain
/// recursive expansion work that amortizes the per-task scheduling cost.
///
/// "Trivial" items are scalars (`Null`/`Boolean`/`Number`/`String`) and
/// `@value`-shaped objects (objects whose only keys are `@value`/`@language`/
/// `@type`/`@direction`/`@index`). These compile down to a flat literal in
/// the expanded form — the work per item is below the FuturesOrdered overhead
/// floor.
#[cfg(feature = "parallel")]
fn array_has_heavy_items(items: &Array) -> bool {
    const SAMPLE: usize = 8;
    let take = items.len().min(SAMPLE);
    items.iter().take(take).any(item_is_heavy)
}

#[cfg(feature = "parallel")]
fn item_is_heavy(v: &jstrict::Value) -> bool {
    match v {
        jstrict::Value::Null | jstrict::Value::Boolean(_) | jstrict::Value::Number(_) | jstrict::Value::String(_) => false,
        jstrict::Value::Array(_) => true,
        jstrict::Value::Object(obj) => !is_value_object_shape(obj),
    }
}

#[cfg(feature = "parallel")]
fn is_value_object_shape(obj: &jstrict::Object) -> bool {
    obj.iter().all(|entry| {
        matches!(
            entry.key.as_str(),
            "@value" | "@language" | "@type" | "@direction" | "@index"
        )
    })
}

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
    N: VocabularyMut + ParallelSafeVocabulary,
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

    // Parallel sibling branch: only fires when the `parallel` feature is
    // enabled and the array is wide enough to amortize the per-task overhead.
    // Output position (`result.extend` order, `is_list` wrapping) is preserved
    // by `FuturesOrdered`, and per-task warnings are merged in iteration order
    // so the observable warning sequence matches the sequential branch.
    #[cfg(feature = "parallel")]
    {
        if (crate::PAR_LO..=crate::PAR_HI).contains(&element.len()) && array_has_heavy_items(element) {
            use futures::stream::{FuturesOrdered, StreamExt};
            use jsonld_core::warning::WarningBuf;

            let env_loader: &'a L = env.loader;
            let mut stream: FuturesOrdered<_> = element
                .iter()
                .map(|item| {
                    let mut vocab: N = (*env.vocabulary).clone();
                    let mut warn_buf: WarningBuf<crate::Warning<N::BlankId>> = WarningBuf::new();
                    Box::pin(async move {
                        let task_env = Environment {
                            vocabulary: &mut vocab,
                            loader: env_loader,
                            warnings: &mut warn_buf,
                        };
                        let r = expand_element(
                            task_env,
                            active_context,
                            active_property,
                            item,
                            base_url,
                            options,
                            from_map,
                            cache,
                        )
                        .await;
                        (r, warn_buf)
                    })
                })
                .collect();

            while let Some((res, buf)) = stream.next().await {
                let e = res?;
                for w in buf.0 {
                    env.warnings.handle(env.vocabulary, w);
                }
                result.extend(e);
            }

            if is_list {
                return Ok(Expanded::Object(Object::List(object::List::new(result)).into()));
            }
            return Ok(Expanded::Array(result));
        }
    }

    // Sequential fallback (always used when feature is off, or when the array
    // is below the threshold).
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
