#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A remote context named more than once is processed as the Context Processing
//! algorithm says: a repeat at the top level is processed again, a repeat met
//! while validating a scoped context is skipped, and a document is dereferenced
//! once however often it is named ([JSON-LD 1.1 API §4.1.2, steps 5.2.2 to
//! 5.2.4][1]).
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
use iri_rs::{Iri, IriBuf, iri};
use jsonld::{Expand, LoadError, Loader, RemoteDocument, syntax::Parse};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

/// A map loader that counts how often it is asked for a document.
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

const A: Iri<&str> = iri!("http://example.com/a.jsonld");
const B: Iri<&str> = iri!("http://example.com/b.jsonld");

const A_DOCUMENT: &str = r#"{"@context": {"t": "http://example.com/a#t"}}"#;
const B_DOCUMENT: &str = r#"{"@context": {"t": "http://example.com/b#t"}}"#;

/// The properties of every node of the expanded `input`.
async fn expanded_properties(input: &str, loader: &CountingLoader) -> Vec<String> {
    let (json, _) = jsonld::syntax::Value::parse_str(input).unwrap();
    let expanded = json.expand(loader).await.unwrap();
    expanded
        .iter()
        .flat_map(|object| object.as_node().into_iter())
        .flat_map(|node| node.properties().iter().map(|(property, _)| property.as_str().to_owned()))
        .collect()
}

/// Step 5.2.2 skips a context already in `remote contexts` only when `validate
/// scoped context` is false, which it is not for a document's own `@context`:
/// `[A, B, A]` processes A a second time, so A's definition of `t` wins.
#[tokio::test]
async fn a_context_repeated_at_the_top_level_is_processed_again() {
    let loader = CountingLoader::new(&[(A, A_DOCUMENT), (B, B_DOCUMENT)]);
    let input = format!(r#"{{"@context": ["{A}", "{B}", "{A}"], "@id": "http://example.com/n", "t": "v"}}"#);

    let properties = expanded_properties(&input, &loader).await;

    assert_eq!(properties, vec!["http://example.com/a#t".to_owned()]);
}

/// Step 5.2.4: a context "previously dereferenced" is not dereferenced again,
/// so processing A twice fetches it once.
#[tokio::test]
async fn a_context_repeated_at_the_top_level_is_dereferenced_once() {
    let loader = CountingLoader::new(&[(A, A_DOCUMENT), (B, B_DOCUMENT)]);
    let input = format!(r#"{{"@context": ["{A}", "{B}", "{A}"], "@id": "http://example.com/n", "t": "v"}}"#);

    expanded_properties(&input, &loader).await;

    assert_eq!(loader.loads(), 2);
}

/// The order of a repeat still decides: `[B, A, B]` ends with B's definition.
#[tokio::test]
async fn the_last_occurrence_of_a_repeated_context_decides() {
    let loader = CountingLoader::new(&[(A, A_DOCUMENT), (B, B_DOCUMENT)]);
    let input = format!(r#"{{"@context": ["{B}", "{A}", "{B}"], "@id": "http://example.com/n", "t": "v"}}"#);

    let properties = expanded_properties(&input, &loader).await;

    assert_eq!(properties, vec!["http://example.com/b#t".to_owned()]);
}

/// Create Term Definition validates a scoped context with `validate scoped
/// context` false, so a scoped context naming the document it is defined in is
/// skipped there instead of recursing, and applying it later still works.
#[tokio::test]
async fn a_scoped_context_naming_its_own_document_terminates() {
    const SELF_SCOPED: Iri<&str> = iri!("http://example.com/self.jsonld");
    let document = format!(r#"{{"@context": {{"t": {{"@id": "http://example.com/s#t", "@context": "{SELF_SCOPED}"}}}}}}"#);
    let loader = CountingLoader::new(&[(SELF_SCOPED, &document)]);
    let input = format!(r#"{{"@context": "{SELF_SCOPED}", "@id": "http://example.com/n", "t": {{"@id": "http://example.com/m", "t": "v"}}}}"#);

    let properties = expanded_properties(&input, &loader).await;

    assert_eq!(properties, vec!["http://example.com/s#t".to_owned()]);
}

/// A document whose own `@context` names itself is not a scoped context, so
/// step 5.2.2 never skips it: the chain grows until step 5.2.3's processor
/// limit ends it with a context overflow rather than looping forever.
#[tokio::test]
async fn a_top_level_context_including_itself_overflows() {
    const SELF_INCLUDING: Iri<&str> = iri!("http://example.com/loop.jsonld");
    let document = format!(r#"{{"@context": ["{SELF_INCLUDING}", {{"t": "http://example.com/l#t"}}]}}"#);
    let loader = CountingLoader::new(&[(SELF_INCLUDING, &document)]);
    let input = format!(r#"{{"@context": "{SELF_INCLUDING}", "@id": "http://example.com/n", "t": "v"}}"#);

    let (json, _) = jsonld::syntax::Value::parse_str(&input).unwrap();
    let result = json.expand(&loader).await;

    assert!(result.is_err(), "a self-including top-level context must end in a context overflow");
    assert_eq!(loader.loads(), 1, "the repeated document is dereferenced once however deep the chain grows");
}
