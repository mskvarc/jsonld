#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! An invalid `@context` is reported where it is invalid: a term definition the
//! [create term definition algorithm][1] refuses names its term, and a remote
//! document no context can be extracted from names its URL. Both the
//! synchronous fast path and the loader-driven path report the same error.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#create-term-definition
use jsonld::{
    ContextLoadError,
    ExtractContextError,
    IriBuf,
    RemoteContextReference,
    RemoteDocument,
    context_processing::{Error, Process},
    rdfx::vocabulary::no_vocabulary_mut,
    syntax::{
        ErrorCode,
        Parse,
        TryFromJson,
        Value,
        context::{Context, InvalidContext, definition::KeyOrKeyword},
    },
};
use std::collections::HashMap;

/// A remote context that defines nothing, named first so processing leaves the
/// synchronous fast path.
const EMPTY_REMOTE: &str = "http://example.com/empty.jsonld";

/// A remote document whose `@context` defines a term as a number.
const INVALID_REMOTE: &str = "http://example.com/invalid.jsonld";

type MapLoader = HashMap<IriBuf, RemoteDocument>;

fn loader() -> MapLoader {
    [(EMPTY_REMOTE, r#"{"@context": {}}"#), (INVALID_REMOTE, r#"{"@context": {"speed": 5}}"#)]
        .into_iter()
        .map(|(url, body)| {
            let url = IriBuf::new(url.to_owned()).unwrap();
            let (json, _) = Value::parse_str(body).unwrap();
            (url.clone(), RemoteDocument::new(Some(url), None, json))
        })
        .collect()
}

/// The error processing the context written as `json` ends with.
fn processing_error(json: &str) -> Error<<MapLoader as jsonld::Loader>::Error> {
    let (value, _) = Value::parse_str(json).unwrap();
    let context = Context::try_from_json(&value).unwrap();
    let loader = loader();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime
        .block_on(async { context.process(no_vocabulary_mut(), &loader, None).await.map(|_| ()) })
        .expect_err("the context does not process")
}

/// The term an error is located in, and the code of its cause.
fn located<E>(error: &Error<E>) -> (&KeyOrKeyword, ErrorCode) {
    let Error::TermDefinition { term, error: cause } = error else {
        panic!("expected a located term definition error, got {}", error.code());
    };
    assert!(!matches!(**cause, Error::TermDefinition { .. }), "a located error is never located twice");
    (term, cause.code())
}

#[test]
fn a_cyclic_iri_mapping_names_the_term_reached_again() {
    let error = processing_error(r#"{"a": "b:x", "b": "a:y"}"#);
    let (term, cause) = located(&error);
    assert_eq!(term.as_str(), "a");
    assert_eq!(cause, ErrorCode::CyclicIriMapping);
    assert_eq!(error.code(), ErrorCode::CyclicIriMapping);
}

#[test]
fn the_loader_driven_path_locates_a_cyclic_iri_mapping_as_the_fast_path_does() {
    let fast = processing_error(r#"{"a": "b:x", "b": "a:y"}"#);
    let loaded = processing_error(&format!(r#"["{EMPTY_REMOTE}", {{"a": "b:x", "b": "a:y"}}]"#));
    assert_eq!(located(&loaded).0, located(&fast).0);
    assert_eq!(located(&loaded).1, located(&fast).1);
}

#[test]
fn a_term_depending_on_an_invalid_term_names_the_invalid_one() {
    let error = processing_error(r#"{"speed": "ex:speed", "ex": {"@id": "http://example.com/", "@reverse": "http://example.com/r"}}"#);
    let (term, cause) = located(&error);
    assert_eq!(term.as_str(), "ex");
    assert_eq!(cause, ErrorCode::InvalidReverseProperty);
}

#[test]
fn a_remote_context_that_cannot_be_extracted_names_its_url() {
    let error = processing_error(&format!(r#""{INVALID_REMOTE}""#));
    let Error::ContextExtractionFailed { target, error } = error else {
        panic!("expected an extraction failure, got {error}");
    };
    assert_eq!(target.as_str(), INVALID_REMOTE);
    let ExtractContextError::Syntax(invalid) = error else {
        panic!("expected a syntax failure, got {error}");
    };
    assert!(matches!(&invalid, InvalidContext::Entry { key, .. } if key.as_str() == "speed"), "{invalid}");
}

#[test]
fn loading_a_context_reference_names_the_url_it_cannot_extract_from() {
    let reference = RemoteContextReference::Iri(IriBuf::new(INVALID_REMOTE.to_owned()).unwrap());
    let loader = loader();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let error = runtime
        .block_on(reference.load_context_with(no_vocabulary_mut(), &loader))
        .expect_err("the document's context is invalid");
    let ContextLoadError::ContextExtractionFailed { target, .. } = error else {
        panic!("expected an extraction failure, got {error}");
    };
    assert_eq!(target.as_str(), INVALID_REMOTE);
}
