use crate::{Error, Options, compact_iri, iri::keyword_alias};
use jsonld_context_processing::{Options as ProcessingOptions, Process};
use jsonld_core::{Container, ContainerKind, Context, ContextRef, Id, Loader, Term, Type, Value, object};
use jsonld_syntax::Keyword;
use rdfx::vocabulary::VocabularyMut;
use std::hash::Hash;

/// Compacts a value object, following the [value compaction algorithm][1].
///
/// Reduces the value object to a bare JSON scalar whenever the active property's
/// type, language and direction mappings already say everything the value object
/// spells out; otherwise rebuilds it as an object with `@value` and whichever of
/// `@type`, `@language`, `@direction` and `@index` still carry information.
///
/// `index` is the `@index` the value was reached through. It is dropped when the
/// active property's container mapping includes `@index`, because it has already
/// become the enclosing map's key.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#value-compaction
pub async fn compact_indexed_value_with<N, L>(
    vocabulary: &mut N,
    value: &Value<N::Iri>,
    index: Option<&str>,
    active_context: &Context<N::Iri, N::BlankId>,
    active_property: Option<&str>,
    loader: &L,
    options: Options,
) -> Result<jstrict::Value, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    L: Loader,
{
    // If the term definition for active property in active context has a local context:
    let mut active_context = ContextRef::Borrowed(active_context);
    if let Some(active_property) = active_property
        && let Some(active_property_definition) = active_context.get(active_property)
        && let Some(local_context) = active_property_definition.context()
    {
        active_context = ContextRef::owned(
            local_context
                .process_with(
                    vocabulary,
                    active_context.as_ref(),
                    loader,
                    active_property_definition.base_url().cloned(),
                    ProcessingOptions::from(options).with_override(),
                )
                .await?
                .into_processed(),
        )
    }

    // If element has an @value or @id entry and the result of using the Value Compaction algorithm,
    // passing active context, active property, and element as value is a scalar,
    // or the term definition for active property has a type mapping of @json,
    // return that result.

    // Here starts the Value Compaction Algorithm.

    // Initialize result to a copy of value.
    let mut result = jstrict::Object::default();

    // If the active context has a null inverse context,
    // set inverse context in active context to the result of calling the
    // Inverse Context Creation algorithm using active context.
    //
    // Initialize inverse context to the value of inverse context in active context.
    //
    // These two steps have no counterpart here: `Context::inverse` builds the
    // inverse context on first access and caches it on the context, so it is
    // never observably null and nothing has to bind it to a local.

    let active_property_definition = match active_property {
        Some(active_property) => active_context.get(active_property),
        None => None,
    };

    // Initialize language to the language mapping for active property in active context,
    // if any, otherwise to the default language of active context.
    let language = match active_property_definition {
        Some(def) => match def.language() {
            Some(lang) => lang.as_ref().map(|l| l.as_lenient_lang_tag_ref()).option(),
            None => active_context.default_language(),
        },
        None => active_context.default_language(),
    };

    // Initialize direction to the direction mapping for active property in active context,
    // if any, otherwise to the default base direction of active context.
    let direction = match active_property_definition {
        Some(def) => match def.direction() {
            Some(dir) => dir.option(),
            None => active_context.default_base_direction(),
        },
        None => active_context.default_base_direction(),
    };

    // If value has an @id entry and has no other entries other than @index:
    //
    // Unreachable here: this function only ever receives a value object, which
    // by construction has an `@value` entry and no `@id`. The `@id`-only case is
    // handled where node objects are compacted, in `node.rs`.

    // Otherwise, if value has an @type entry whose value matches the type mapping of
    // active property, set result to the value associated with the @value entry of value.
    let type_mapping: Option<Type<N::Iri>> = match active_property_definition {
        Some(def) => def.typ().cloned(),
        None => None,
    };

    let container_mapping = match active_property_definition {
        Some(def) => def.container(),
        None => Container::None,
    };

    let remove_index = (index.is_some() && container_mapping.contains(ContainerKind::Index)) || index.is_none();

    match value {
        Value::Literal(lit, ty) => {
            use object::value::Literal;
            if ty.clone().map(Type::Iri) == type_mapping && remove_index {
                match lit {
                    Literal::Null => return Ok(jstrict::Value::Null),
                    Literal::Boolean(b) => return Ok(jstrict::Value::Boolean(*b)),
                    Literal::Number(n) => return Ok(jstrict::Value::Number(n.clone())),
                    Literal::String(s) => {
                        if ty.is_some() || (language.is_none() && direction.is_none()) {
                            return Ok(jstrict::Value::String(s.as_str().into()));
                        } else {
                            let compact_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Value);
                            result.insert(compact_key.into(), jstrict::Value::String(s.as_str().into()));
                        }
                    }
                }
            } else {
                let value_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Value);
                match lit {
                    Literal::Null => {
                        result.insert(value_key.into(), jstrict::Value::Null);
                    }
                    Literal::Boolean(b) => {
                        result.insert(value_key.into(), jstrict::Value::Boolean(*b));
                    }
                    Literal::Number(n) => {
                        result.insert(value_key.into(), jstrict::Value::Number(n.clone()));
                    }
                    Literal::String(s) => {
                        result.insert(value_key.into(), jstrict::Value::String(s.as_str().into()));
                    }
                }

                if let Some(ty) = ty {
                    let type_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Type);
                    let compact_ty = compact_iri(vocabulary, active_context.as_ref(), &Term::Id(Id::iri(ty.clone())), true, false, options)?;
                    result.insert(
                        type_key.into(),
                        match compact_ty {
                            Some(s) => jstrict::Value::String((&*s).into()),
                            None => jstrict::Value::Null,
                        },
                    );
                }
            }
        }
        Value::LangString(ls) => {
            let ls_language = ls.language();
            let ls_direction = ls.direction();

            // The string can be written bare only if the active property's
            // language and direction mappings already imply the ones carried by
            // the value.
            if remove_index && (ls_language.is_none() || language == ls_language) && (ls_direction.is_none() || direction == ls_direction) {
                return Ok(jstrict::Value::String(ls.as_str().into()));
            } else {
                let value_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Value);
                result.insert(value_key.into(), jstrict::Value::String(ls.as_str().into()));

                if let Some(language) = ls.language() {
                    let lang_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Language);
                    result.insert(lang_key.into(), jstrict::Value::String(language.as_str().into()));
                }

                if let Some(direction) = ls.direction() {
                    let dir_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Direction);
                    result.insert(dir_key.into(), jstrict::Value::String(direction.as_str().into()));
                }
            }
        }
        Value::Json(value) => {
            if type_mapping == Some(Type::Json) && remove_index {
                return Ok(value.clone());
            } else {
                let value_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Value);
                result.insert(value_key.into(), value.clone());

                let type_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Type);
                let json_alias = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Json);
                result.insert(type_key.into(), jstrict::Value::String(json_alias.into()));
            }
        }
    }

    if !remove_index && let Some(index) = index {
        let index_key = keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Index);
        result.insert(index_key.into(), jstrict::Value::String(index.into()));
    }

    Ok(jstrict::Value::Object(result))
}
