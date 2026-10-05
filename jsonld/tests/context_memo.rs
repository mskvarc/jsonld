#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Expanding a document processes each distinct `@context` once, however many
//! of its nodes carry it.
use iri_rs::{Iri, IriBuf, iri};
use jsonld::{Expand, LoadError, Loader, RemoteDocument, syntax::Parse};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

/// A map loader that counts how often a context is fetched, which is once per
/// processing of a local context naming it.
struct CountingLoader {
    documents: HashMap<IriBuf, RemoteDocument>,
    loads: AtomicUsize,
}

impl CountingLoader {
    fn new(contexts: &[(Iri<&str>, &str)]) -> Self {
        let documents = contexts
            .iter()
            .map(|(url, body)| {
                let (json, _) = jsonld::syntax::Value::parse_str(body).unwrap();
                let url = IriBuf::new(url.as_str().to_owned()).unwrap();
                (url.clone(), RemoteDocument::new(Some(url), None, json))
            })
            .collect();
        Self {
            documents,
            loads: AtomicUsize::new(0),
        }
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::Relaxed)
    }
}

impl Loader for CountingLoader {
    type Error = <HashMap<IriBuf, RemoteDocument> as Loader>::Error;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<Self::Error>> {
        self.loads.fetch_add(1, Ordering::Relaxed);
        #[expect(
            clippy::disallowed_methods,
            reason = "a loader delegating to the loader it wraps already runs inside its caller's guarded poll"
        )]
        self.documents.load(url).await
    }
}

const SCHEMA: Iri<&str> = iri!("http://example.com/schema.jsonld");
const OTHER: Iri<&str> = iri!("http://example.com/other.jsonld");

fn node(index: usize, context: Iri<&str>) -> String {
    format!(r#"{{"@context": "{context}", "@id": "http://example.com/node/{index}", "name": "n{index}"}}"#)
}

async fn expand(input: &str, loader: &CountingLoader) -> jsonld::ExpandedDocument {
    let (json, _) = jsonld::syntax::Value::parse_str(input).unwrap();
    json.expand(loader).await.unwrap()
}

#[tokio::test]
async fn nodes_sharing_one_context_process_it_once() {
    let loader = CountingLoader::new(&[(SCHEMA, r#"{"@context": {"name": "http://schema.org/name"}}"#)]);
    let input = format!("[{}]", (0..50).map(|index| node(index, SCHEMA)).collect::<Vec<_>>().join(","));

    let expanded = expand(&input, &loader).await;

    assert_eq!(expanded.len(), 50);
    assert_eq!(loader.loads(), 1, "the shared context must be processed once per document");
}

#[tokio::test]
async fn each_distinct_context_is_processed_once() {
    let loader = CountingLoader::new(&[
        (SCHEMA, r#"{"@context": {"name": "http://schema.org/name"}}"#),
        (OTHER, r#"{"@context": {"name": "http://example.com/vocab#name"}}"#),
    ]);
    let input = format!(
        "[{}]",
        (0..20)
            .map(|index| node(index, if index % 2 == 0 { SCHEMA } else { OTHER }))
            .collect::<Vec<_>>()
            .join(",")
    );

    let expanded = expand(&input, &loader).await;

    assert_eq!(expanded.len(), 20);
    assert_eq!(loader.loads(), 2);
    let names: Vec<&str> = expanded
        .iter()
        .flat_map(|object| object.as_node().into_iter())
        .flat_map(|node| node.properties().iter().map(|(property, _)| property.as_str()))
        .collect();
    assert_eq!(names.iter().filter(|name| **name == "http://schema.org/name").count(), 10);
    assert_eq!(names.iter().filter(|name| **name == "http://example.com/vocab#name").count(), 10);
}

#[tokio::test]
async fn a_second_document_processes_its_context_again() {
    let loader = CountingLoader::new(&[(SCHEMA, r#"{"@context": {"name": "http://schema.org/name"}}"#)]);
    let input = format!("[{}]", node(0, SCHEMA));

    expand(&input, &loader).await;
    expand(&input, &loader).await;

    assert_eq!(loader.loads(), 2, "the memo is scoped to one document and its loader");
}
