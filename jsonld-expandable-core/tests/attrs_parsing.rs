//! Unit tests for the `#[jsonld(...)]` attribute parser: which spellings
//! lower to which IR, and which combinations are rejected.

#![allow(clippy::unwrap_used)]

use jsonld_expandable_core::{
    attrs::{parse_container, parse_field},
    ir::{Coerce, ContainerKind},
};

fn parse_field_attrs(src: &str) -> jsonld_expandable_core::ir::FieldIr {
    let item: syn::ItemStruct = syn::parse_str(&format!("struct S {{ {src} pub x: String, }}")).unwrap();
    let field = item.fields.iter().next().unwrap();
    parse_field(&field.attrs).unwrap()
}

fn parse_container_attrs(src: &str) -> jsonld_expandable_core::ir::ContainerIr {
    let item: syn::ItemStruct = syn::parse_str(&format!("{src} struct S {{ pub x: String, }}")).unwrap();
    parse_container(&item.attrs).unwrap()
}

#[test]
fn container_type_iri() {
    let c = parse_container_attrs("#[jsonld(type = \"https://example.com/T\")]");
    assert_eq!(c.type_iri.as_deref(), Some("https://example.com/T"));
}

#[test]
fn container_prefix_table() {
    let c = parse_container_attrs("#[jsonld(type = \"https://example.com/T\", prefix(ngsi = \"https://uri.etsi.org/ngsi-ld/\"))]");
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
fn coerce_id_plus_vec_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", coerce = \"@id\", vec)]");
    assert_eq!(f.coerce, Some(Coerce::Id));
    assert!(f.is_vec);
}

#[test]
fn coerce_id_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", coerce = \"@id\")]");
    assert_eq!(f.coerce, Some(Coerce::Id));
}

#[test]
fn container_list_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", container = \"list\")]");
    assert_eq!(f.container, Some(ContainerKind::List));
}

#[test]
fn container_language_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", container = \"language\")]");
    assert_eq!(f.container, Some(ContainerKind::Language));
}

#[test]
fn flatten_parses() {
    let f = parse_field_attrs("#[jsonld(flatten)]");
    assert!(f.flatten);
    assert!(!f.flatten_map);
}

#[test]
fn flatten_map_parses() {
    let f = parse_field_attrs("#[jsonld(flatten_map)]");
    assert!(f.flatten_map);
    assert!(!f.flatten);
    assert!(f.container.is_none());
}

#[test]
fn flatten_with_property_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(flatten, property = \"https://e.com/p\")] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn flatten_and_flatten_map_mutually_exclusive() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(flatten, flatten_map)] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn passthrough_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", passthrough)]");
    assert!(f.passthrough);
    assert!(f.coerce.is_none());
    assert!(f.container.is_none());
    assert!(!f.nested);
}

#[test]
fn passthrough_with_coerce_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(property = \"https://e.com/p\", passthrough, coerce = \"@id\")] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn list_plus_coerce_id_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", container = \"list\", coerce = \"@id\")]");
    assert_eq!(f.container, Some(ContainerKind::List));
    assert_eq!(f.coerce, Some(Coerce::Id));
}

#[test]
fn nested_plus_vec_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", nested, vec)]");
    assert!(f.nested);
    assert!(f.is_vec);
}

#[test]
fn unknown_attr_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(bogus)] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn vocab_polymorphic_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(property = \"https://e.com/p\", vocab_polymorphic)] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn hyphenated_prefix_names_parse() {
    let c = parse_container_attrs("#[jsonld(type = \"https://example.com/T\", prefix(ngsi-ld = \"https://uri.etsi.org/ngsi-ld/\"))]");
    assert_eq!(c.prefixes, vec![("ngsi-ld".into(), "https://uri.etsi.org/ngsi-ld/".into())]);
}

#[test]
fn string_literal_prefix_names_parse() {
    let c = parse_container_attrs("#[jsonld(type = \"https://example.com/T\", prefix(\"ngsi-ld\" = \"https://uri.etsi.org/ngsi-ld/\"))]");
    assert_eq!(c.prefixes, vec![("ngsi-ld".into(), "https://uri.etsi.org/ngsi-ld/".into())]);
}

#[test]
fn keyword_coercion_typo_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(property = \"https://e.com/p\", coerce = \"@idd\")] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    let error = parse_field(&field.attrs).unwrap_err();
    assert!(error.to_string().contains("unknown keyword coercion"), "{error}");
}

#[test]
fn non_iri_datatype_coercion_errors() {
    let item: syn::ItemStruct = syn::parse_str("struct S { #[jsonld(property = \"https://e.com/p\", coerce = \"dateTime\")] pub x: String, }").unwrap();
    let field = item.fields.iter().next().unwrap();
    assert!(parse_field(&field.attrs).is_err());
}

#[test]
fn curie_datatype_coercion_parses() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", coerce = \"xsd:dateTime\")]");
    assert_eq!(f.coerce, Some(Coerce::Datatype("xsd:dateTime".into())));
}

#[test]
fn vec_with_vocab_coercion_is_allowed() {
    let f = parse_field_attrs("#[jsonld(property = \"https://e.com/p\", coerce = \"@vocab\", vec)]");
    assert_eq!(f.coerce, Some(Coerce::Vocab));
    assert!(f.is_vec);
}

#[test]
fn fragment_with_container_type_errors() {
    let item: syn::ItemStruct = syn::parse_str("#[jsonld(fragment, type = \"https://e.com/T\")] struct S { pub x: String, }").unwrap();
    assert!(parse_container(&item.attrs).is_err());
}

#[test]
fn removed_type_field_attribute_is_rejected() {
    let item: syn::ItemStruct = syn::parse_str("#[jsonld(fragment, type_field)] struct S { pub x: String }").unwrap();
    assert!(parse_container(&item.attrs).is_err());
}

fn generate(src: &str) -> syn::Result<String> {
    let input: syn::DeriveInput = syn::parse_str(src).unwrap();
    // The derive resolves this from the consuming crate's manifest; these tests
    // exercise codegen directly, so they pin the direct-dependant path.
    let runtime = quote::quote!(::jsonld_expandable_core);
    jsonld_expandable_core::codegen::generate(&input, &runtime).map(|ts| ts.to_string())
}

#[test]
fn coerce_datatype_curies_expand_through_prefix_table() {
    let generated = generate(
        r#"
        #[jsonld(type = "https://e.com/T", prefix(xsd = "http://www.w3.org/2001/XMLSchema#"))]
        struct S {
            #[jsonld(property = "https://e.com/p", coerce = "xsd:dateTime")]
            pub x: String,
        }
    "#,
    )
    .unwrap();
    assert!(generated.contains("http://www.w3.org/2001/XMLSchema#dateTime"), "{generated}");
    assert!(!generated.contains("xsd:dateTime"), "{generated}");
}

#[test]
fn duplicate_property_iris_error() {
    let error = generate(
        r#"
        #[jsonld(type = "https://e.com/T")]
        struct S {
            #[jsonld(property = "https://e.com/p")]
            pub x: String,
            #[jsonld(property = "https://e.com/p")]
            pub y: String,
        }
    "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("duplicate property IRI"), "{error}");
}

#[test]
fn duplicate_id_fields_error() {
    let error = generate(
        r#"
        #[jsonld(type = "https://e.com/T")]
        struct S {
            #[jsonld(id)]
            pub a: String,
            #[jsonld(id)]
            pub b: String,
        }
    "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("only one field may carry `id`"), "{error}");
}

#[test]
fn optional_id_fields_are_supported() {
    let generated = generate(
        r#"
        #[jsonld(type = "https://e.com/T")]
        struct S {
            #[jsonld(id)]
            pub id: Option<String>,
        }
    "#,
    )
    .unwrap();
    assert!(generated.contains("if let :: core :: option :: Option :: Some"), "{generated}");
}

#[test]
fn type_value_field_with_container_type_errors() {
    let error = generate(
        r#"
        #[jsonld(type = "https://e.com/T")]
        struct S {
            #[jsonld(type_value)]
            pub ty: Vec<String>,
        }
    "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("conflicts with the container-level `type"), "{error}");
}
