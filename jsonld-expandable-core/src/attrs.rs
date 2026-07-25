//! `#[jsonld(...)]` attribute parser.
//!
//! Accepts both the spec-aligned attribute names and the legacy
//! NGSI-flavored names from the original `json-ld-expandable` crate, lowering
//! both to the same [`crate::ir`] form.

use crate::{
    ir::{Coerce, ContainerIr, ContainerKind, FieldIr},
    iri::looks_like_iri,
};
use proc_macro2::Span;
use syn::spanned::Spanned;

/// Parses the `#[jsonld(...)]` attributes carried by a type.
pub fn parse_container(attrs: &[syn::Attribute]) -> syn::Result<ContainerIr> {
    let mut out = ContainerIr::default();

    for attr in attrs {
        if !attr.path().is_ident("jsonld") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let iri = lit.value();
                if !looks_like_iri(&iri) && !is_curie(&iri) {
                    return Err(syn::Error::new(
                        lit.span(),
                        format!(
                            "invalid IRI \"{iri}\": expected a full IRI with a scheme \
                             (e.g. \"https://example.com/Type\") or a CURIE \
                             (e.g. \"ex:Type\") backed by a `prefix(...)` entry"
                        ),
                    ));
                }
                out.type_iri = Some(iri);
            } else if meta.path.is_ident("type_field") {
                // Marker only; the actual ident comes from a field attr.
                // Container holds it after field parsing.
                out.fragment = false; // explicit reset for clarity
            // Nothing more to do here; codegen will discover it.
            } else if meta.path.is_ident("fragment") {
                out.fragment = true;
            } else if meta.path.is_ident("crate") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let path: syn::Path = syn::parse_str(&lit.value())?;
                out.crate_path = Some(quote::quote!(#path));
            } else if meta.path.is_ident("debug") {
                out.debug = true;
            } else if meta.path.is_ident("prefix") {
                meta.parse_nested_meta(|sub| {
                    let name = sub.path.get_ident().ok_or_else(|| sub.error("expected `name = \"iri\"`"))?.to_string();
                    let lit: syn::LitStr = sub.value()?.parse()?;
                    out.prefixes.push((name, lit.value()));
                    Ok(())
                })?;
            } else {
                return Err(meta.error(format!(
                    "unknown jsonld container attribute `{}`; expected one of: \
                     type, type_field, fragment, crate, debug, prefix",
                    meta.path.get_ident().map_or("?".into(), |i| i.to_string())
                )));
            }
            Ok(())
        })?;
    }

    Ok(out)
}

/// Parses the `#[jsonld(...)]` attributes carried by a field.
pub fn parse_field(attrs: &[syn::Attribute]) -> syn::Result<FieldIr> {
    let mut out = FieldIr::default();
    let mut span: Option<Span> = None;
    let mut datatype: Option<String> = None;
    let mut typed_value_seen = false;

    for attr in attrs {
        if !attr.path().is_ident("jsonld") {
            continue;
        }
        span = Some(attr.span());

        attr.parse_nested_meta(|meta| {
            // ----- new spec-aligned forms ------------------------------------
            if meta.path.is_ident("id") {
                out.is_id = true;
            } else if meta.path.is_ident("type_value") {
                out.is_type_value = true;
            } else if meta.path.is_ident("skip") {
                out.skip = true;
            } else if meta.path.is_ident("property") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let iri = lit.value();
                if !looks_like_iri(&iri) && !is_curie(&iri) {
                    return Err(syn::Error::new(
                        lit.span(),
                        format!(
                            "invalid property IRI \"{iri}\": expected a full IRI \
                             with a scheme or a CURIE backed by `prefix(...)`"
                        ),
                    ));
                }
                out.property = Some(iri);
            } else if meta.path.is_ident("coerce") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let v = lit.value();
                out.coerce = Some(match v.as_str() {
                    "@id" => Coerce::Id,
                    "@vocab" => Coerce::Vocab,
                    "@json" => Coerce::Json,
                    other => Coerce::Datatype(other.to_owned()),
                });
            } else if meta.path.is_ident("container") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let v = lit.value();
                out.container = Some(match v.as_str() {
                    "list" => ContainerKind::List,
                    "set" => ContainerKind::Set,
                    "language" => ContainerKind::Language,
                    "index" => ContainerKind::Index,
                    "id" => ContainerKind::Id,
                    "type" => ContainerKind::Type,
                    "graph" => ContainerKind::Graph,
                    other => {
                        return Err(syn::Error::new(lit.span(), format!("unknown container `{other}`")));
                    }
                });
            } else if meta.path.is_ident("flatten") {
                out.flatten = true;
            } else if meta.path.is_ident("nest") || meta.path.is_ident("reverse") {
                return Err(meta.error("`nest` / `reverse` are reserved but not yet implemented"));
            // ----- legacy aliases (additive compatibility) -------------------
            } else if meta.path.is_ident("nested") {
                out.nested = true;
            } else if meta.path.is_ident("vec") {
                out.is_vec = true;
            } else if meta.path.is_ident("custom") || meta.path.is_ident("passthrough") {
                // Field's own Expandable impl produces the final shape;
                // codegen calls expand() and inserts the value verbatim.
                out.passthrough = true;
            } else if meta.path.is_ident("list") {
                out.container = Some(ContainerKind::List);
            } else if meta.path.is_ident("vocab") || meta.path.is_ident("id_ref") {
                out.coerce = Some(Coerce::Id);
            } else if meta.path.is_ident("vocab_vec") {
                out.coerce = Some(Coerce::Id);
                out.is_vec = true;
            } else if meta.path.is_ident("language_map") {
                out.container = Some(ContainerKind::Language);
            } else if meta.path.is_ident("flatten_map") {
                out.flatten_map = true;
            } else if meta.path.is_ident("flatten_object") {
                out.flatten = true;
            } else if meta.path.is_ident("typed_value") {
                typed_value_seen = true;
            } else if meta.path.is_ident("datatype") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                datatype = Some(lit.value());
            } else if meta.path.is_ident("json_value") {
                out.coerce = Some(Coerce::Json);
            } else if meta.path.is_ident("vocab_polymorphic") {
                return Err(meta.error("`vocab_polymorphic` is no longer supported; use a concrete enum"));
            } else {
                return Err(meta.error(format!(
                    "unknown jsonld field attribute `{}`",
                    meta.path.get_ident().map_or("?".into(), |i| i.to_string())
                )));
            }
            Ok(())
        })?;
    }

    if typed_value_seen {
        let dt = datatype.ok_or_else(|| {
            syn::Error::new(
                span.unwrap_or_else(Span::call_site),
                "`typed_value` requires a `datatype = \"...\"` companion attribute",
            )
        })?;
        out.coerce = Some(Coerce::Datatype(dt));
    } else if datatype.is_some() {
        return Err(syn::Error::new(
            span.unwrap_or_else(Span::call_site),
            "`datatype` is only valid alongside `typed_value`",
        ));
    }

    validate_field(&out, span)?;
    Ok(out)
}

fn validate_field(f: &FieldIr, span_hint: Option<Span>) -> syn::Result<()> {
    let span = span_hint.unwrap_or_else(Span::call_site);

    if f.is_id && (f.property.is_some() || f.nested || f.coerce.is_some() || f.container.is_some() || f.flatten || f.flatten_map) {
        return Err(syn::Error::new(
            span,
            "`id` cannot be combined with property, nested, coerce, container, or flatten",
        ));
    }
    if f.skip && (f.is_id || f.nested || f.coerce.is_some() || f.container.is_some() || f.flatten || f.flatten_map || f.property.is_some()) {
        return Err(syn::Error::new(span, "`skip` cannot be combined with other attributes"));
    }
    if f.flatten && f.flatten_map {
        return Err(syn::Error::new(span, "`flatten` / `flatten_object` and `flatten_map` are mutually exclusive"));
    }
    if (f.flatten || f.flatten_map) && f.property.is_some() {
        return Err(syn::Error::new(
            span,
            "`flatten` / `flatten_object` / `flatten_map` cannot be combined with `property`",
        ));
    }
    if (f.flatten || f.flatten_map) && (f.coerce.is_some() || f.container.is_some() || f.nested) {
        return Err(syn::Error::new(
            span,
            "`flatten` / `flatten_map` cannot be combined with coerce, container, or nested",
        ));
    }
    if f.passthrough && (f.coerce.is_some() || f.container.is_some() || f.nested || f.flatten || f.flatten_map || f.is_id) {
        return Err(syn::Error::new(
            span,
            "`custom` / `passthrough` cannot be combined with coerce, container, \
             nested, flatten, flatten_map, or id",
        ));
    }
    if f.is_vec && !f.nested && !matches!(f.coerce, Some(Coerce::Id)) {
        return Err(syn::Error::new(span, "`vec` requires `nested` (or a value coercion that iterates)"));
    }
    if f.nested && f.coerce.is_some() {
        return Err(syn::Error::new(span, "`nested` and `coerce` are mutually exclusive"));
    }
    if matches!(f.container, Some(ContainerKind::Language)) && f.coerce.is_some() {
        return Err(syn::Error::new(span, "`container = \"language\"` and `coerce` are mutually exclusive"));
    }

    Ok(())
}

fn is_curie(s: &str) -> bool {
    if let Some((prefix, _suffix)) = s.split_once(':') {
        // a single-token prefix like "ngsi" without `://` qualifies
        !prefix.is_empty()
            && prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !s.starts_with("http://")
            && !s.starts_with("https://")
            && !s.starts_with("urn:")
    } else {
        false
    }
}
