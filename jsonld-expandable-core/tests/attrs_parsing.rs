//! Unit tests for the attribute parser. Covers both new spec-aligned
//! spellings and legacy NGSI-flavored aliases.

#![allow(clippy::unwrap_used)]

use jsonld_expandable_core::attrs::{parse_container, parse_field};
use jsonld_expandable_core::ir::{Coerce, ContainerKind};

fn parse_field_attrs(src: &str) -> jsonld_expandable_core::ir::FieldIr {
    let item: syn::ItemStruct = syn::parse_str(&format!(
        "struct S {{ {src} pub x: String, }}"
    ))
    .unwrap();
    let field = item.fields.iter().next().unwrap();
    parse_field(&field.attrs).unwrap()
}

fn parse_container_attrs(src: &str) -> jsonld_expandable_core::ir::ContainerIr {
    let item: syn::ItemStruct = syn::parse_str(&format!(
        "{src} struct S {{ pub x: String, }}"
    ))
    .unwrap();
    parse_container(&item.attrs).unwrap()
}

#[test]
fn container_type_iri() {
    let c = parse_container_attrs("#[jsonld(type = \"https://example.com/T\")]");
    assert_eq!(c.type_iri.as_deref(), Some("https://example.com/T"));
}

#[test]
fn container_prefix_table() {
    let c = parse_container_attrs(
        "#[jsonld(type = \"https://example.com/T\", prefix(ngsi = \"https://uri.etsi.org/ngsi-ld/\"))]",
    );
    assert_eq!(c.prefixes, vec![("ngsi".into(), "https://uri.etsi.org/ngsi-ld/".into())]);
}

#[test]
fn field_id() {
    assert!(parse_field_attrs("#[jsonld(id)]").is_id);
}

#[test]
fn field_skip() {
    assert!(parse_field_attrs("#[jsonld(skip)]").skip);
}

#[test]
fn field_property_only() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\")]");
    assert_eq!(f.property.as_deref(), Some("https://e.com/p"));
    assert!(f.coerce.is_none());
}

#[test]
fn legacy_vocab_lowers_to_coerce_id() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", vocab)]");
    assert_eq!(f.coerce, Some(Coerce::Id));
}

#[test]
fn legacy_vocab_vec_lowers_to_coerce_id_plus_vec() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", vocab_vec)]");
    assert_eq!(f.coerce, Some(Coerce::Id));
    assert!(f.is_vec);
}

#[test]
fn legacy_typed_value_lowers_to_coerce_datatype() {
    let f = parse_field_attrs(
        "#[jsonld(property = \"https://e.com/p\", typed_value, datatype = \"http://www.w3.org/2001/XMLSchema#dateTime\")]",
    );
    assert_eq!(
        f.coerce,
        Some(Coerce::Datatype("http://www.w3.org/2001/XMLSchema#dateTime".into()))
    );
}

#[test]
fn new_coerce_id_equivalent() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", coerce = \"@id\")]");
    assert_eq!(f.coerce, Some(Coerce::Id));
}

#[test]
fn new_container_list_equivalent_to_legacy_list() {
    let a = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", list)]");
    let b = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", container = \"list\")]");
    assert_eq!(a.container, Some(ContainerKind::List));
    assert_eq!(a.container, b.container);
}

#[test]
fn legacy_language_map_lowers_to_container_language() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", language_map)]");
    assert_eq!(f.container, Some(ContainerKind::Language));
}

#[test]
fn legacy_flatten_object_lowers_to_flatten() {
    let f = parse_field_attrs("#[jsonld(flatten_object)]");
    assert!(f.flatten);
    assert!(!f.flatten_map);
}

#[test]
fn legacy_flatten_map_lowers_to_flatten_map() {
    let f = parse_field_attrs("#[jsonld(flatten_map)]");
    assert!(f.flatten_map);
    assert!(!f.flatten);
    assert!(f.container.is_none());
}

#[test]
fn flatten_with_property_errors() {
    let item: syn::ItemStruct = syn::parse_str(
        "struct S { #[jsonld(flatten_object, property = \"https://e.com/p\")] pub x: String, }",
    )
    .unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn flatten_and_flatten_map_mutually_exclusive() {
    let item: syn::ItemStruct =
        syn::parse_str("struct S { #[jsonld(flatten_object, flatten_map)] pub x: String, }")
            .unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn legacy_custom_lowers_to_passthrough() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", custom)]");
    assert!(f.passthrough);
    assert!(f.coerce.is_none());
    assert!(f.container.is_none());
    assert!(!f.nested);
}

#[test]
fn passthrough_with_coerce_errors() {
    let item: syn::ItemStruct = syn::parse_str(
        "struct S { #[jsonld(property = \"https://e.com/p\", custom, vocab)] pub x: String, }",
    )
    .unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn list_plus_coerce_id_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", list, id_ref)]");
    assert_eq!(f.container, Some(ContainerKind::List));
    assert_eq!(f.coerce, Some(Coerce::Id));
}

#[test]
fn legacy_nested_no_op_marker() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", nested, vec)]");
    assert!(f.nested);
    assert!(f.is_vec);
}

#[test]
fn unknown_attr_errors() {
    let item: syn::ItemStruct =
        syn::parse_str("struct S { #[jsonld(bogus)] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn vocab_polymorphic_errors() {
    let item: syn::ItemStruct =
        syn::parse_str("struct S { #[jsonld(property = \"https://e.com/p\", vocab_polymorphic)] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}
