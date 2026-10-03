#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Expansion and compaction recurse once per nesting level of the document:
//! Expansion algorithm steps 13.7 to 13.9 expand every entry value, and step 5
//! every array item ([JSON-LD 1.1 API §5.1.2][1]); the Compaction algorithm
//! does the same in steps 2 and 12 ([§6.1.2][2]). The `async` implementation
//! boxes each recursive call, which moves the callee's state to the heap but
//! not its polling, so a parent polls its child from inside its own `poll` and
//! the native stack deepens by one set of poll frames per level.
//!
//! A document nested as deeply as an ordinary JSON parser admits has to be
//! processed on an ordinary thread stack, whatever the embedder has already
//! used of it, rather than abort the process on a stack overflow. Every case
//! runs on a freshly spawned thread with a deliberately small stack, in
//! whatever profile the test binary was built in.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#expansion-algorithm
//! [2]: https://www.w3.org/TR/json-ld11-api/#compaction-algorithm
use jsonld::{
    JsonLdProcessor,
    NoLoader,
    RemoteContextReference,
    RemoteDocument,
    syntax::{Parse, context::Context},
};
use std::thread;

/// The stack a Tokio worker thread gets by default.
const TOKIO_WORKER_STACK: usize = 2 * 1024 * 1024;

/// A stack far smaller than any default, standing in for an embedder that has
/// already used most of its thread's stack when it starts processing.
const NEARLY_EXHAUSTED_STACK: usize = 256 * 1024;

/// The nesting `serde_json` admits by default, and so the deepest document a
/// typical embedder hands over.
const SERDE_JSON_RECURSION_LIMIT: usize = 128;

/// A node object whose property `p` holds a one-item array holding the next
/// node object, `depth` levels deep, so every level passes through both the
/// node and the array recursion of each algorithm.
fn nested_document(depth: usize) -> String {
    let mut node = r#"{"p": "leaf"}"#.to_owned();
    for _ in 0..depth {
        node = format!(r#"{{"p": [{node}]}}"#);
    }
    format!(r#"{{"@context": {{"p": "http://example.com/nesting#p"}}, "@id": "http://example.com/node", "p": [{node}]}}"#)
}

/// What one algorithm run on the spawned thread ended with.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The algorithm completed.
    Completed,
    /// The algorithm reported an error.
    Failed,
}

/// Runs `algorithm` over the parsed `input` on a new thread whose stack is
/// `stack` bytes.
fn run_on_stack(input: String, stack: usize, algorithm: fn(&RemoteDocument) -> Outcome) -> Outcome {
    thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let (json, _) = jsonld::syntax::Value::parse_str(&input).unwrap();
            algorithm(&RemoteDocument::new(None, None, json))
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Expands `document` on a current-thread runtime.
fn expand(document: &RemoteDocument) -> Outcome {
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    match runtime.block_on(document.expand(&NoLoader)) {
        Ok(_) => Outcome::Completed,
        Err(_) => Outcome::Failed,
    }
}

/// Compacts `document` against an empty context on a current-thread runtime,
/// which expands it first and then compacts every level back.
fn compact(document: &RemoteDocument) -> Outcome {
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let context = RemoteContextReference::Loaded(RemoteDocument::new(None, None, Context::default()));
    match runtime.block_on(document.compact(context, &NoLoader)) {
        Ok(_) => Outcome::Completed,
        Err(_) => Outcome::Failed,
    }
}

#[test]
fn a_document_nested_to_the_serde_json_limit_expands_on_a_tokio_worker_stack() {
    let outcome = run_on_stack(nested_document(SERDE_JSON_RECURSION_LIMIT), TOKIO_WORKER_STACK, expand);

    assert_eq!(outcome, Outcome::Completed);
}

#[test]
fn a_document_nested_to_the_serde_json_limit_expands_on_a_nearly_exhausted_stack() {
    let outcome = run_on_stack(nested_document(SERDE_JSON_RECURSION_LIMIT), NEARLY_EXHAUSTED_STACK, expand);

    assert_eq!(outcome, Outcome::Completed);
}

#[test]
fn a_document_nested_to_the_serde_json_limit_compacts_on_a_tokio_worker_stack() {
    let outcome = run_on_stack(nested_document(SERDE_JSON_RECURSION_LIMIT), TOKIO_WORKER_STACK, compact);

    assert_eq!(outcome, Outcome::Completed);
}

#[test]
fn a_document_nested_to_the_serde_json_limit_compacts_on_a_nearly_exhausted_stack() {
    let outcome = run_on_stack(nested_document(SERDE_JSON_RECURSION_LIMIT), NEARLY_EXHAUSTED_STACK, compact);

    assert_eq!(outcome, Outcome::Completed);
}

/// A shallow document, nested only as deep as an NGSI-LD Attribute carrying a
/// proof, is the case an embedder already near the end of its stack meets.
#[test]
fn a_shallow_document_expands_and_compacts_on_a_nearly_exhausted_stack() {
    assert_eq!(run_on_stack(nested_document(4), NEARLY_EXHAUSTED_STACK, expand), Outcome::Completed);
    assert_eq!(run_on_stack(nested_document(4), NEARLY_EXHAUSTED_STACK, compact), Outcome::Completed);
}
