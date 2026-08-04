//! IR → `TokenStream` lowering, parametric over `V: JsonValue`.

use crate::{
    attrs::{parse_container, parse_field},
    ir::{Coerce, ContainerKind, FieldIr},
    iri::expand_curie,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

/// Generates the `Expandable` implementation for a derive input.
pub fn generate(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let container = parse_container(&input.attrs)?;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(syn::Error::new_spanned(name, "Expandable only supports structs with named fields"));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(name, "Expandable only supports structs"));
        }
    };

    let crate_path = container.crate_path.clone().unwrap_or_else(|| quote!(::jsonld_expandable_core));

    let mut id_stmt: Option<TokenStream> = None;
    let mut type_stmt: Option<TokenStream> = None;
    let mut prop_stmts: Vec<TokenStream> = Vec::new();

    let mut dynamic_type_field: Option<syn::Ident> = None;

    // First pass: discover dynamic-type field.
    for field in fields {
        let f = parse_field(&field.attrs)?;
        if f.is_type_value {
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| syn::Error::new_spanned(field, "type_value requires named field"))?;
            if dynamic_type_field.is_some() {
                return Err(syn::Error::new_spanned(field, "only one field may carry `type_value`"));
            }
            if container.fragment {
                return Err(syn::Error::new_spanned(
                    field,
                    "a `type_value` field conflicts with the container-level `fragment` attribute",
                ));
            }
            if container.type_iri.is_some() {
                return Err(syn::Error::new_spanned(
                    field,
                    "a `type_value` field conflicts with the container-level `type = \"...\"` attribute",
                ));
            }
            dynamic_type_field = Some(ident);
        }
    }

    // Container-level @type emission.
    if !container.fragment {
        if let Some(field_ident) = &dynamic_type_field {
            type_stmt = Some(quote! {
                __entries.push((
                    ::std::string::ToString::to_string("@type"),
                    #crate_path::ExpandableTypeValue::to_type_array::<V>(&self.#field_ident),
                ));
            });
        } else if let Some(iri) = &container.type_iri {
            let expanded = expand_curie(iri, &container.prefixes);
            type_stmt = Some(quote! {
                __entries.push((
                    ::std::string::ToString::to_string("@type"),
                    <V as #crate_path::JsonValue>::array(::std::iter::once(
                        <V as #crate_path::JsonValue>::string(#expanded),
                    )),
                ));
            });
        } else {
            return Err(syn::Error::new_spanned(
                name,
                "Expandable requires `#[jsonld(type = \"...\")]`, a `type_value` field, \
                 or `#[jsonld(fragment)]`",
            ));
        }
    }

    // Second pass: emit property statements.
    let mut seen_property_iris: Vec<String> = Vec::new();
    for field in fields {
        let f = parse_field(&field.attrs)?;
        if f.skip || f.is_type_value {
            continue;
        }
        let field_ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(field, "Expandable requires named fields"))?;

        let is_option = is_option_type(&field.ty);

        if f.is_id {
            if id_stmt.is_some() {
                return Err(syn::Error::new_spanned(field, "only one field may carry `id`"));
            }
            let push = quote! {
                __entries.push((
                    ::std::string::ToString::to_string("@id"),
                    <V as #crate_path::JsonValue>::string(::core::convert::AsRef::<str>::as_ref(__val)),
                ));
            };
            id_stmt = Some(if is_option {
                quote! {
                    if let ::core::option::Option::Some(__val) = &self.#field_ident {
                        #push
                    }
                }
            } else {
                quote! {
                    {
                        let __val = &self.#field_ident;
                        #push
                    }
                }
            });
            continue;
        }

        if f.flatten {
            let frag_stmt = quote! {
                let __frag: V = #crate_path::Expandable::expand::<V>(__src);
                if let ::core::option::Option::Some(__items) =
                    <V as #crate_path::JsonValue>::into_object_entries(__frag)
                {
                    for (__k, __v) in __items {
                        if __k != "@id" && __k != "@type" {
                            __entries.push((__k, __v));
                        }
                    }
                }
            };
            prop_stmts.push(if is_option {
                quote! {
                    if let ::core::option::Option::Some(__src) = &self.#field_ident {
                        #frag_stmt
                    }
                }
            } else {
                quote! {
                    {
                        let __src = &self.#field_ident;
                        #frag_stmt
                    }
                }
            });
            continue;
        }

        if f.flatten_map {
            let map_stmt = quote! {
                for (__k, __v) in __src.iter() {
                    __entries.push((
                        ::std::string::ToString::to_string(::core::convert::AsRef::<str>::as_ref(__k)),
                        #crate_path::Expandable::expand::<V>(__v),
                    ));
                }
            };
            prop_stmts.push(if is_option {
                quote! {
                    if let ::core::option::Option::Some(__src) = &self.#field_ident {
                        #map_stmt
                    }
                }
            } else {
                quote! {
                    {
                        let __src = &self.#field_ident;
                        #map_stmt
                    }
                }
            });
            continue;
        }

        let property_iri = match &f.property {
            Some(iri) => expand_curie(iri, &container.prefixes),
            None => {
                return Err(syn::Error::new_spanned(
                    field,
                    "field needs `#[jsonld(property = \"...\")]`, `id`, `skip`, \
                     `flatten`, or `flatten_map`",
                ));
            }
        };

        if seen_property_iris.contains(&property_iri) {
            return Err(syn::Error::new_spanned(
                field,
                format!("duplicate property IRI `{property_iri}`: another field already expands to it"),
            ));
        }
        seen_property_iris.push(property_iri.clone());

        let value_expr = build_field_expr(&field_ident, &f, is_option, &crate_path, &container.prefixes)?;

        prop_stmts.push(if is_option {
            quote! {
                if let ::core::option::Option::Some(__val) = &self.#field_ident {
                    __entries.push((::std::string::ToString::to_string(#property_iri), #value_expr));
                }
            }
        } else {
            quote! {
                __entries.push((::std::string::ToString::to_string(#property_iri), #value_expr));
            }
        });
    }

    let id_emit = id_stmt.unwrap_or_default();
    let type_emit = type_stmt.unwrap_or_default();

    let body = quote! {
        impl #impl_generics #crate_path::Expandable for #name #ty_generics #where_clause {
            fn expand<V: #crate_path::JsonValue>(&self) -> V {
                let mut __entries: ::std::vec::Vec<(::std::string::String, V)> =
                    ::std::vec::Vec::new();
                #id_emit
                #type_emit
                #(#prop_stmts)*
                <V as #crate_path::JsonValue>::object(__entries)
            }
        }
    };

    if container.debug {
        return Err(syn::Error::new_spanned(name, body.to_string()));
    }
    Ok(body)
}

fn build_field_expr(
    field_ident: &syn::Ident,
    f: &FieldIr,
    is_option: bool,
    crate_path: &TokenStream,
    prefixes: &[(String, String)],
) -> syn::Result<TokenStream> {
    // Source expression: `__val` when wrapped in Option, else `&self.field`.
    let src: TokenStream = if is_option {
        quote!(__val)
    } else {
        let id = field_ident;
        quote!(&self.#id)
    };

    // `passthrough` (legacy `custom`): emit the field's own Expandable
    // output verbatim — the user supplies the wrapping shape.
    if f.passthrough {
        return Ok(quote! {
            #crate_path::Expandable::expand::<V>(#src)
        });
    }

    let in_list = matches!(f.container, Some(ContainerKind::List));

    // Container = "list" + coerce / nested combinations: wrap each item
    // in its coerced shape, then enclose the whole sequence in
    // `[{"@list": [...]}]`. Source must be iterable (Vec, IndexMap-like, or
    // any type implementing `iter()` over items).
    if in_list {
        if f.nested {
            return Ok(quote! {
                <V as #crate_path::JsonValue>::array(::std::iter::once(
                    <V as #crate_path::JsonValue>::object(::std::iter::once(
                        (::std::string::ToString::to_string("@list"),
                         <V as #crate_path::JsonValue>::array(
                             (#src).iter().map(|__item|
                                 #crate_path::Expandable::expand::<V>(__item))
                         ))
                    ))
                ))
            });
        }
        if let Some(coerce) = &f.coerce {
            let item_shape = item_shape_for_coerce(coerce, crate_path, prefixes);
            return Ok(quote! {
                <V as #crate_path::JsonValue>::array(::std::iter::once(
                    <V as #crate_path::JsonValue>::object(::std::iter::once(
                        (::std::string::ToString::to_string("@list"),
                         <V as #crate_path::JsonValue>::array(
                             (#src).iter().map(|__item| #item_shape)
                         ))
                    ))
                ))
            });
        }
        // Plain list: pass the value through `ToJsonValue` (Vec, etc.).
        return Ok(quote! {
            <V as #crate_path::JsonValue>::array(::std::iter::once(
                <V as #crate_path::JsonValue>::object(::std::iter::once(
                    (::std::string::ToString::to_string("@list"),
                     <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src))
                ))
            ))
        });
    }

    if f.nested {
        if f.is_vec {
            return Ok(quote! {
                <V as #crate_path::JsonValue>::array(
                    (#src).iter().map(|__item| #crate_path::Expandable::expand::<V>(__item))
                )
            });
        }
        return Ok(quote! {
            <V as #crate_path::JsonValue>::array(::std::iter::once(
                #crate_path::Expandable::expand::<V>(#src),
            ))
        });
    }

    if let Some(coerce) = &f.coerce {
        return Ok(match coerce {
            Coerce::Id | Coerce::Vocab => {
                let key = if matches!(coerce, Coerce::Id) { "@id" } else { "@vocab" };
                if f.is_vec {
                    quote! {
                        <V as #crate_path::JsonValue>::array(
                            (#src).iter().map(|__item| {
                                <V as #crate_path::JsonValue>::object(::std::iter::once(
                                    (::std::string::ToString::to_string(#key),
                                     <V as #crate_path::JsonValue>::string(
                                         ::core::convert::AsRef::<str>::as_ref(__item))),
                                ))
                            })
                        )
                    }
                } else {
                    quote! {
                        <V as #crate_path::JsonValue>::array(::std::iter::once(
                            <V as #crate_path::JsonValue>::object(::std::iter::once(
                                (::std::string::ToString::to_string(#key),
                                 <V as #crate_path::JsonValue>::string(
                                     ::core::convert::AsRef::<str>::as_ref(#src)))
                            ))
                        ))
                    }
                }
            }
            Coerce::Json => {
                quote! {
                    <V as #crate_path::JsonValue>::array(::std::iter::once(
                        <V as #crate_path::JsonValue>::object([
                            (::std::string::ToString::to_string("@value"),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src)),
                            (::std::string::ToString::to_string("@type"),
                             <V as #crate_path::JsonValue>::string("@json")),
                        ])
                    ))
                }
            }
            Coerce::Datatype(d) => {
                let dlit = expand_curie(d, prefixes);
                quote! {
                    <V as #crate_path::JsonValue>::array(::std::iter::once(
                        <V as #crate_path::JsonValue>::object([
                            (::std::string::ToString::to_string("@value"),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src)),
                            (::std::string::ToString::to_string("@type"),
                             <V as #crate_path::JsonValue>::string(#dlit)),
                        ])
                    ))
                }
            }
        });
    }

    if let Some(c) = &f.container {
        return Ok(match c {
            ContainerKind::List => {
                // already handled in the in_list branch above; treat as
                // a defensive fallback equivalent to the plain-list shape
                quote! {
                    <V as #crate_path::JsonValue>::array(::std::iter::once(
                        <V as #crate_path::JsonValue>::object(::std::iter::once(
                            (::std::string::ToString::to_string("@list"),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src))
                        ))
                    ))
                }
            }
            ContainerKind::Set => quote! {
                <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src)
            },
            ContainerKind::Language => quote! {
                #crate_path::ExpandableLanguageMap::to_expanded_language_map::<V>(#src)
            },
            ContainerKind::Index => quote! {
                <V as #crate_path::JsonValue>::array(
                    (#src).iter().map(|(__k, __v)| {
                        <V as #crate_path::JsonValue>::object([
                            (::std::string::ToString::to_string("@index"),
                             <V as #crate_path::JsonValue>::string(
                                 ::core::convert::AsRef::<str>::as_ref(__k))),
                            (::std::string::ToString::to_string("@value"),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(__v)),
                        ])
                    })
                )
            },
            ContainerKind::Id | ContainerKind::Type | ContainerKind::Graph => {
                return Err(syn::Error::new_spanned(
                    field_ident,
                    "container = \"id\" / \"type\" / \"graph\" not yet implemented",
                ));
            }
        });
    }

    // Default: literal value wrapped in `[{"@value": v}]`.
    Ok(quote! {
        <V as #crate_path::JsonValue>::array(::std::iter::once(
            <V as #crate_path::JsonValue>::object(::std::iter::once(
                (::std::string::ToString::to_string("@value"),
                 <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src))
            ))
        ))
    })
}

/// Produce the per-item token stream that wraps a single `__item`
/// in the JSON-LD shape implied by the field's coercion.
fn item_shape_for_coerce(coerce: &Coerce, crate_path: &TokenStream, prefixes: &[(String, String)]) -> TokenStream {
    match coerce {
        Coerce::Id | Coerce::Vocab => {
            let key = if matches!(coerce, Coerce::Id) { "@id" } else { "@vocab" };
            quote! {
                <V as #crate_path::JsonValue>::object(::std::iter::once(
                    (::std::string::ToString::to_string(#key),
                     <V as #crate_path::JsonValue>::string(
                         ::core::convert::AsRef::<str>::as_ref(__item))),
                ))
            }
        }
        Coerce::Json => quote! {
            <V as #crate_path::JsonValue>::object([
                (::std::string::ToString::to_string("@value"),
                 <_ as #crate_path::ToJsonValue<V>>::to_json_value(__item)),
                (::std::string::ToString::to_string("@type"),
                 <V as #crate_path::JsonValue>::string("@json")),
            ])
        },
        Coerce::Datatype(d) => {
            let dlit = expand_curie(d, prefixes);
            quote! {
                <V as #crate_path::JsonValue>::object([
                    (::std::string::ToString::to_string("@value"),
                     <_ as #crate_path::ToJsonValue<V>>::to_json_value(__item)),
                    (::std::string::ToString::to_string("@type"),
                     <V as #crate_path::JsonValue>::string(#dlit)),
                ])
            }
        }
    }
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            if seg.ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    return args.args.iter().any(|a| matches!(a, GenericArgument::Type(_)));
                }
            }
        }
    }
    false
}
