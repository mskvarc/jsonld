//! IR → `TokenStream` lowering, parametric over `V: JsonValue`.

use crate::attrs::{parse_container, parse_field};
use crate::ir::{Coerce, ContainerKind, FieldIr};
use crate::iri::expand_curie;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

pub fn generate(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let container = parse_container(&input.attrs)?;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    name,
                    "Expandable only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "Expandable only supports structs",
            ));
        }
    };

    let crate_path = container
        .crate_path
        .clone()
        .unwrap_or_else(|| quote!(::jsonld_expandable_core));

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
                return Err(syn::Error::new_spanned(
                    field,
                    "only one field may carry `type_value`",
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
                    "@type".to_string(),
                    #crate_path::ExpandableTypeValue::to_type_array::<V>(&self.#field_ident),
                ));
            });
        } else if let Some(iri) = &container.type_iri {
            let expanded = expand_curie(iri, &container.prefixes);
            type_stmt = Some(quote! {
                __entries.push((
                    "@type".to_string(),
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
    for field in fields {
        let f = parse_field(&field.attrs)?;
        if f.skip || f.is_type_value {
            continue;
        }
        let field_ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(field, "Expandable requires named fields"))?;

        if f.is_id {
            id_stmt = Some(quote! {
                __entries.push((
                    "@id".to_string(),
                    <V as #crate_path::JsonValue>::string(::core::convert::AsRef::<str>::as_ref(&self.#field_ident)),
                ));
            });
            continue;
        }

        let property_iri = match &f.property {
            Some(iri) => expand_curie(iri, &container.prefixes),
            None => {
                if f.flatten {
                    String::new() // unused for flatten
                } else {
                    return Err(syn::Error::new_spanned(
                        field,
                        "field needs `#[jsonld(property = \"...\")]`, `id`, `skip`, \
                         or `flatten`",
                    ));
                }
            }
        };

        if f.flatten {
            return Err(syn::Error::new_spanned(
                field,
                "`flatten` / `flatten_object` is reserved but not yet implemented",
            ));
        }

        let is_option = is_option_type(&field.ty);
        let value_expr = build_field_expr(&field_ident, &f, is_option, &crate_path)?;

        prop_stmts.push(if is_option {
            quote! {
                if let ::core::option::Option::Some(__val) = &self.#field_ident {
                    __entries.push((#property_iri.to_string(), #value_expr));
                }
            }
        } else {
            quote! {
                __entries.push((#property_iri.to_string(), #value_expr));
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
) -> syn::Result<TokenStream> {
    // Source expression: `__val` when wrapped in Option, else `&self.field`.
    let src: TokenStream = if is_option {
        quote!(__val)
    } else {
        let id = field_ident;
        quote!(&self.#id)
    };

    // Coerce / nested / container cases.
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
                let key = if matches!(coerce, Coerce::Id) {
                    "@id"
                } else {
                    "@vocab"
                };
                if f.is_vec {
                    quote! {
                        <V as #crate_path::JsonValue>::array(
                            (#src).iter().map(|__item| {
                                <V as #crate_path::JsonValue>::object(::std::iter::once(
                                    (#key.to_string(),
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
                                (#key.to_string(),
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
                            ("@value".to_string(),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src)),
                            ("@type".to_string(),
                             <V as #crate_path::JsonValue>::string("@json")),
                        ])
                    ))
                }
            }
            Coerce::Datatype(d) => {
                let dlit = d.clone();
                quote! {
                    <V as #crate_path::JsonValue>::array(::std::iter::once(
                        <V as #crate_path::JsonValue>::object([
                            ("@value".to_string(),
                             <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src)),
                            ("@type".to_string(),
                             <V as #crate_path::JsonValue>::string(#dlit)),
                        ])
                    ))
                }
            }
        });
    }

    if let Some(c) = &f.container {
        return Ok(match c {
            ContainerKind::List => quote! {
                <V as #crate_path::JsonValue>::array(::std::iter::once(
                    <V as #crate_path::JsonValue>::object(::std::iter::once(
                        ("@list".to_string(),
                         <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src))
                    ))
                ))
            },
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
                            ("@index".to_string(),
                             <V as #crate_path::JsonValue>::string(
                                 ::core::convert::AsRef::<str>::as_ref(__k))),
                            ("@value".to_string(),
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
                ("@value".to_string(),
                 <_ as #crate_path::ToJsonValue<V>>::to_json_value(#src))
            ))
        ))
    })
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

