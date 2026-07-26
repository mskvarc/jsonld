#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
#![cfg(feature = "serde-json")]

use iri_rs::{IriBuf, iri};
use jsonld::{JsonLdProcessor, LD_JSON_MEDIA_TYPE, RemoteContextReference, RemoteDocument};

fn input_value() -> serde_json::Value {
    serde_json::json!({
        "@context": {"name": "http://xmlns.com/foaf/0.1/name"},
        "@id": "https://example.com/x",
        "name": "Test"
    })
}

fn expected_id() -> &'static str {
    "https://example.com/x"
}

fn foaf_name() -> &'static str {
    "http://xmlns.com/foaf/0.1/name"
}

#[tokio::test]
async fn expand_serde_json_input() {
    let url = IriBuf::from(iri!("https://example.com/sample.jsonld"));
    let doc = RemoteDocument::from_serde_json(Some(url), Some(LD_JSON_MEDIA_TYPE.into()), input_value());
    let expanded = doc.expand(&jsonld::NoLoader).await.expect("expansion failed");

    assert_eq!(expanded.len(), 1);
    let object = expanded.iter().next().unwrap();
    let id = object.id().expect("id");
    assert_eq!(id.as_iri().unwrap().as_str(), expected_id());
    let node = object.as_node().expect("node");
    let name = node.get_any(&iri!("http://xmlns.com/foaf/0.1/name")).unwrap().as_str().unwrap();
    assert_eq!(name, "Test");
    let _ = foaf_name();
}

#[tokio::test]
async fn expand_via_from_value_with_serde_json() {
    let doc: RemoteDocument<IriBuf> = RemoteDocument::from_value(None, None, input_value());
    let expanded = doc.expand(&jsonld::NoLoader).await.expect("expansion failed");
    assert_eq!(expanded.len(), 1);
    let id = expanded.iter().next().unwrap().id().unwrap();
    assert_eq!(id.as_iri().unwrap().as_str(), expected_id());
}

#[tokio::test]
async fn compact_output_into_serde_json() {
    let doc: RemoteDocument<IriBuf> = RemoteDocument::from_value(None, None, input_value());
    let context = RemoteContextReference::Loaded(RemoteDocument::new(None, None, jsonld_syntax::context::Context::default()));
    let compact = doc.compact(context, &jsonld::NoLoader).await.expect("compaction failed");

    let serde_v: serde_json::Value = compact.clone().into_serde_json();
    let round_trip = jstrict::Value::from_serde_json(serde_v);
    assert!(jsonld_syntax::Compare::compare(&compact, &round_trip));
}

#[tokio::test]
async fn flatten_output_into_serde_json() {
    let doc: RemoteDocument<IriBuf> = RemoteDocument::from_value(None, None, input_value());
    let mut generator = rdfx::generator::Blank::new();
    let flattened = doc.flatten(&mut generator, &jsonld::NoLoader).await.expect("flatten failed");

    let serde_v: serde_json::Value = flattened.clone().into_serde_json();
    let round_trip = jstrict::Value::from_serde_json(serde_v);
    assert!(jsonld_syntax::Compare::compare(&flattened, &round_trip));
}
