#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! JSON-LD identifies a node by its `@id` string.
//!
//! IRI Expansion performs "neither Syntax-Based Normalization nor Scheme-Based
//! Normalization", the Node Map Generation algorithm keys nodes by their `@id`,
//! and RDF 1.1 Concepts §3.2 says "Further normalization MUST NOT be performed
//! when comparing IRIs for equality". So spellings that RFC 3986 §6.2.2 would
//! call equivalent (`~` and `%7E`, `HTTP://EXAMPLE.org` and `http://example.org`)
//! name different nodes, and each keeps the spelling the document gave it.
//! Comparing IRIs by their normal form merges them, which these tests catch.

use iri_rs::{IriBuf, iri};
use jsonld::{
    JsonLdProcessor,
    NoLoader,
    RemoteDocument,
    syntax::{Parse, Value},
};
use std::collections::BTreeMap;

const DOCUMENT: &str = r#"{"@graph":[
  {"@id":"http://example.org/~a",   "http://example.org/p":"one"},
  {"@id":"http://example.org/%7Ea", "http://example.org/p":"two"},
  {"@id":"HTTP://EXAMPLE.org/b",    "http://example.org/p":"three"},
  {"@id":"http://example.org/b",    "http://example.org/p":"four"}]}"#;

/// Each node's `@id`, exactly as written, and the value of its one property.
const NODES: [(&str, &str); 4] = [
    ("http://example.org/~a", "one"),
    ("http://example.org/%7Ea", "two"),
    ("HTTP://EXAMPLE.org/b", "three"),
    ("http://example.org/b", "four"),
];

fn document() -> RemoteDocument<IriBuf> {
    RemoteDocument::new(None, None, Value::parse_str(DOCUMENT).expect("document parses").0)
}

fn expected() -> BTreeMap<&'static str, &'static str> {
    NODES.into_iter().collect()
}

#[tokio::test]
async fn expansion_keeps_each_spelling_of_an_equivalent_iri() {
    let expanded = document().expand(&NoLoader).await.expect("expansion");
    let found: BTreeMap<&str, &str> = expanded
        .iter()
        .map(|object| {
            let id = object.id().and_then(|id| id.as_iri()).expect("a node with an IRI @id").as_str();
            let value = object
                .as_node()
                .expect("a node object")
                .get_any(&iri!("http://example.org/p"))
                .and_then(|value| value.as_str())
                .expect("a string value of http://example.org/p");
            (id, value)
        })
        .collect();
    assert_eq!(expanded.len(), NODES.len());
    assert_eq!(found, expected());
}

#[tokio::test]
async fn flattening_keeps_one_node_per_spelling_of_an_equivalent_iri() {
    let mut generator = rdfx::generator::Blank::new();
    let flattened = document().flatten(&mut generator, &NoLoader).await.expect("flattening");
    let nodes = flattened.as_array().expect("a document flattened without a context is an array of nodes");
    let found: BTreeMap<&str, &str> = nodes
        .iter()
        .map(|node| {
            let node = node.as_object().expect("a node object");
            let id = node.get_unique("@id").expect("one @id").and_then(Value::as_str).expect("a string @id");
            let value = node
                .get_unique("http://example.org/p")
                .expect("one http://example.org/p")
                .and_then(Value::as_array)
                .and_then(<[Value]>::first)
                .and_then(Value::as_object)
                .and_then(|value| value.get_unique("@value").expect("one @value"))
                .and_then(Value::as_str)
                .expect("a string value of http://example.org/p");
            (id, value)
        })
        .collect();
    assert_eq!(nodes.len(), NODES.len());
    assert_eq!(found, expected());
}
