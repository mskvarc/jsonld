//! Reading and writing deeply nested expanded documents on a small stack.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use iri_rs::IriBuf;
use jsonld_core::{ExpandedDocument, ExpandedJsonTextError, TryFromJson};
use jsonld_syntax::IntoJsonWithContext;
use jstrict::Print;
use rdfx::{
    BlankIdBuf,
    vocabulary::{no_vocabulary, no_vocabulary_mut},
};
use std::thread;

/// Nesting depth of every deep fixture.
const DEPTH: usize = 100_000;

/// Stack of the thread each deep document is read or written on.
const SMALL_STACK: usize = 256 * 1024;

type Document = ExpandedDocument<IriBuf, BlankIdBuf>;

/// Runs `job` on a thread with a [`SMALL_STACK`] and waits for it.
fn on_small_stack<R: Send + 'static>(job: impl FnOnce() -> R + Send + 'static) -> R {
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(job)
        .expect("the thread starts")
        .join()
        .expect("the thread returns instead of overflowing its stack")
}

/// An expanded document of one node holding `depth` nodes nested through one
/// property, as compact JSON text.
fn deep_expanded_text(depth: usize) -> String {
    format!("[{}{{}}{}]", r#"{"http://example.com/p":["#.repeat(depth), "]}".repeat(depth))
}

#[test]
fn try_from_json_reads_a_100k_deep_expanded_document_on_a_256_kib_thread() {
    let text = deep_expanded_text(DEPTH);
    let read = on_small_stack(move || Document::try_from_json_slice_in(no_vocabulary_mut(), text.as_bytes()).map(|document| document.len()));
    assert_eq!(read.expect("the document is expanded JSON-LD"), 1);
}

#[test]
fn into_json_writes_a_100k_deep_expanded_document_on_a_256_kib_thread() {
    let text = deep_expanded_text(DEPTH);
    let written = on_small_stack(move || {
        let document = Document::try_from_json_slice_in(no_vocabulary_mut(), text.as_bytes()).expect("the document reads");
        document.into_json_with(no_vocabulary()).compact_print().to_string()
    });
    assert!(written == deep_expanded_text(DEPTH), "the written document is the read one");
}

#[test]
fn try_from_json_then_into_json_round_trips_a_deep_document_unchanged() {
    let text = deep_expanded_text(1_000);
    let document = Document::try_from_json_slice_in(no_vocabulary_mut(), text.as_bytes()).expect("the document reads");
    assert_eq!(document.into_json_with(no_vocabulary()).compact_print().to_string(), text);
}

#[test]
fn try_from_json_slice_in_reads_what_parse_then_try_from_json_in_reads() {
    let text = r#"[{"@id":"http://example.com/s","http://example.com/p":[{"@value":"v"}],"@type":["http://example.com/T"]}]"#;
    let parsed = jstrict::parse::parse_str_value(text).unwrap();

    let from_slice = Document::try_from_json_slice_in(no_vocabulary_mut(), text.as_bytes()).unwrap();
    let from_value = Document::try_from_json_in(no_vocabulary_mut(), parsed).unwrap();

    assert_eq!(from_slice, from_value);
}

#[test]
fn try_from_json_slice_in_reports_malformed_text_as_a_syntax_error() {
    let outcome = Document::try_from_json_slice_in(no_vocabulary_mut(), b"[{\"@id\": ");
    assert!(matches!(outcome, Err(ExpandedJsonTextError::Syntax(_))), "{outcome:?}");

    let outcome = Document::try_from_json_slice_in(no_vocabulary_mut(), b"{\"not\": \"an array\"}");
    assert!(matches!(outcome, Err(ExpandedJsonTextError::Invalid(_))), "{outcome:?}");
}

#[test]
fn expanded_document_into_json_writes_the_objects_in_insertion_order() {
    let text = r#"[{"@id":"http://example.com/b"},{"@id":"http://example.com/a"}]"#;
    let document = Document::try_from_json_slice_in(no_vocabulary_mut(), text.as_bytes()).unwrap();
    assert_eq!(document.into_json_with(no_vocabulary()).compact_print().to_string(), text);
}
