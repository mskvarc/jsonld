#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Step 5.2.3 of the Context Processing algorithm ends an over-long chain of
//! remote contexts with a `context overflow` error ([JSON-LD 1.1 API
//! §4.1.2][1]). That error has to be reached on an ordinary thread stack: a
//! reference cycle reprocessed at the top level (step 5.2.2) walks the whole
//! chain up to the limit, and a library must report the spec-defined error
//! rather than abort the process on a stack overflow on the way there.
//!
//! Every case runs on a freshly spawned thread with a deliberately small stack,
//! in whatever profile the test binary was built in, so the bound holds for an
//! embedder whose own frames already use part of the thread's stack.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
use iri_rs::IriBuf;
use jsonld::{
    Expand,
    RemoteDocument,
    syntax::{ErrorCode, Parse},
};
use std::{collections::HashMap, thread};

/// The stack a Tokio worker thread gets by default.
const TOKIO_WORKER_STACK: usize = 2 * 1024 * 1024;

/// A stack far smaller than any default, standing in for an embedder that has
/// already used most of its thread's stack when it starts processing.
const NEARLY_EXHAUSTED_STACK: usize = 256 * 1024;

/// The processor-defined limit on the remote context chain.
const LIMIT: usize = jsonld::context_processing::MAX_REMOTE_CONTEXTS;

/// Expands `input` against the remote `documents` on a new thread whose stack
/// is `stack` bytes, returning the number of expanded top-level objects or the
/// code of the error expansion ended with.
fn expand_on_stack(input: String, documents: Vec<(String, String)>, stack: usize) -> Result<usize, ErrorCode> {
    thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let loader: HashMap<IriBuf, RemoteDocument> = documents
                .into_iter()
                .map(|(url, body)| {
                    let (json, _) = jsonld::syntax::Value::parse_str(&body).unwrap();
                    let url = IriBuf::new(url).unwrap();
                    (url.clone(), RemoteDocument::new(Some(url), None, json))
                })
                .collect();
            let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let (json, _) = jsonld::syntax::Value::parse_str(&input).unwrap();
            runtime.block_on(async { json.expand(&loader).await.map(|expanded| expanded.len()).map_err(|error| error.code()) })
        })
        .unwrap()
        .join()
        .unwrap()
}

/// A document naming `url` as its only `@context`, with one term.
fn node_under(url: &str) -> String {
    format!(r#"{{"@context": "{url}", "@id": "http://example.com/node", "t": "v"}}"#)
}

/// The two-document cycle an NGSI-LD broker was brought down with: `a` names a
/// terms document and `b`, and `b` names `a` again.
fn two_document_cycle() -> Vec<(String, String)> {
    vec![
        (
            "http://example.com/terms.jsonld".to_owned(),
            r#"{"@context": {"t": "http://example.com/terms#t", "u": "http://example.com/terms#u", "w": {"@id": "http://example.com/terms#w", "@type": "@id"}}}"#
                .to_owned(),
        ),
        (
            "http://example.com/cycle-a.jsonld".to_owned(),
            r#"{"@context": ["terms.jsonld", "cycle-b.jsonld"]}"#.to_owned(),
        ),
        ("http://example.com/cycle-b.jsonld".to_owned(), r#"{"@context": "cycle-a.jsonld"}"#.to_owned()),
    ]
}

/// A chain of `length` distinct remote contexts, each naming the next, the last
/// defining the term `t`.
fn chain(length: usize) -> Vec<(String, String)> {
    (0..length)
        .map(|index| {
            let url = format!("http://example.com/chain-{index}.jsonld");
            let body = if index + 1 == length {
                r#"{"@context": {"t": "http://example.com/chain#t"}}"#.to_owned()
            } else {
                format!(
                    r#"{{"@context": ["chain-{}.jsonld", {{"c{index}": "http://example.com/chain#c{index}"}}]}}"#,
                    index + 1
                )
            };
            (url, body)
        })
        .collect()
}

#[test]
fn a_two_document_reference_cycle_ends_in_a_context_overflow_on_a_tokio_worker_stack() {
    let result = expand_on_stack(node_under("http://example.com/cycle-a.jsonld"), two_document_cycle(), TOKIO_WORKER_STACK);

    assert_eq!(result, Err(ErrorCode::ContextOverflow));
}

#[test]
fn a_two_document_reference_cycle_ends_in_a_context_overflow_on_a_nearly_exhausted_stack() {
    let result = expand_on_stack(node_under("http://example.com/cycle-a.jsonld"), two_document_cycle(), NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Err(ErrorCode::ContextOverflow));
}

#[test]
fn a_context_including_itself_ends_in_a_context_overflow_on_a_nearly_exhausted_stack() {
    let documents = vec![(
        "http://example.com/loop.jsonld".to_owned(),
        r#"{"@context": ["loop.jsonld", {"t": "http://example.com/loop#t"}]}"#.to_owned(),
    )];

    let result = expand_on_stack(node_under("http://example.com/loop.jsonld"), documents, NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Err(ErrorCode::ContextOverflow));
}

/// Step 5.6.4 refuses an imported context that itself has an `@import`, so two
/// documents importing each other end in an `invalid context entry`, never in a
/// recursion.
#[test]
fn two_documents_importing_each_other_end_in_an_invalid_context_entry() {
    let documents = vec![
        (
            "http://example.com/import-a.jsonld".to_owned(),
            r#"{"@context": {"@import": "import-b.jsonld", "t": "http://example.com/import#t"}}"#.to_owned(),
        ),
        (
            "http://example.com/import-b.jsonld".to_owned(),
            r#"{"@context": {"@import": "import-a.jsonld"}}"#.to_owned(),
        ),
    ];

    let result = expand_on_stack(node_under("http://example.com/import-a.jsonld"), documents, NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Err(ErrorCode::InvalidContextEntry));
}

/// A document whose scoped context imports a document whose own scoped context
/// names the first document again: Create Term Definition validates each scoped
/// context with `validate scoped context` false, so the repeat is skipped (step
/// 5.2.2) and processing ends normally.
#[test]
fn an_import_whose_scoped_context_names_the_importing_document_terminates() {
    let documents = vec![
        (
            "http://example.com/scoped-a.jsonld".to_owned(),
            r#"{"@context": {"@import": "scoped-b.jsonld", "t": "http://example.com/scoped#t"}}"#.to_owned(),
        ),
        (
            "http://example.com/scoped-b.jsonld".to_owned(),
            r#"{"@context": {"s": {"@id": "http://example.com/scoped#s", "@context": "scoped-a.jsonld"}}}"#.to_owned(),
        ),
    ];

    let result = expand_on_stack(node_under("http://example.com/scoped-a.jsonld"), documents, NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Ok(1));
}

/// A chain exactly as long as the limit is legal and is processed in full, even
/// on a nearly exhausted stack.
#[test]
fn a_chain_exactly_at_the_limit_is_processed_on_a_nearly_exhausted_stack() {
    let result = expand_on_stack(node_under("http://example.com/chain-0.jsonld"), chain(LIMIT), NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Ok(1));
}

#[test]
fn a_chain_exactly_at_the_limit_is_processed_on_a_tokio_worker_stack() {
    let result = expand_on_stack(node_under("http://example.com/chain-0.jsonld"), chain(LIMIT), TOKIO_WORKER_STACK);

    assert_eq!(result, Ok(1));
}

/// An inline `@context` whose terms nest scoped contexts `depth` levels deep,
/// so no loader is needed and the synchronous fast path processes it.
fn nested_scoped_context(depth: usize) -> String {
    let mut context = r#"{"t": "http://example.com/nested#t"}"#.to_owned();
    for level in (0..depth).rev() {
        context = format!(r#"{{"s{level}": {{"@id": "http://example.com/nested#s{level}", "@context": {context}}}}}"#);
    }
    format!(r#"{{"@context": {context}, "@id": "http://example.com/node", "t": "v"}}"#)
}

/// A legal nesting of scoped contexts is processed on a nearly exhausted stack
/// by the synchronous fast path.
#[test]
fn nested_scoped_contexts_are_processed_on_a_nearly_exhausted_stack() {
    let result = expand_on_stack(nested_scoped_context(24), Vec::new(), NEARLY_EXHAUSTED_STACK);

    assert!(result.is_ok(), "{result:?}");
}

/// Scoped contexts nested past the synchronous fast path's depth bound end in a
/// context overflow, not a stack overflow.
#[test]
fn scoped_contexts_nested_past_the_depth_bound_end_in_a_context_overflow() {
    let result = expand_on_stack(nested_scoped_context(200), Vec::new(), NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Err(ErrorCode::ContextOverflow));
}

/// One context past the limit is a context overflow (step 5.2.3).
#[test]
fn a_chain_one_past_the_limit_ends_in_a_context_overflow() {
    let result = expand_on_stack(node_under("http://example.com/chain-0.jsonld"), chain(LIMIT + 1), NEARLY_EXHAUSTED_STACK);

    assert_eq!(result, Err(ErrorCode::ContextOverflow));
}
