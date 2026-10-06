#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Expansion from JSON text, and the single-term IRI expansion entry points.
use iri_rs::IriBuf;
use jsonld::{
    Expand,
    NoLoader,
    Term,
    context_processing::{
        Process,
        algorithm::{Action, expand_property_key, expand_type_value},
    },
    expansion::{Context, Options, SliceExpansionError, expand_slice},
    syntax::{Parse, TryFromJson, Value},
};
use rdfx::vocabulary::no_vocabulary_mut;
use std::thread;

/// The stack a Tokio worker thread gets by default.
const TOKIO_WORKER_STACK: usize = 2 * 1024 * 1024;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().build().unwrap()
}

/// The processed form of `context`, over an empty initial context whose
/// base IRI is `http://example.com/base/`.
async fn processed(context: &str) -> Context<IriBuf> {
    let (json, _) = Value::parse_str(context).unwrap();
    let context = jsonld::syntax::context::Context::try_from_json(&json).unwrap();
    let base = IriBuf::new("http://example.com/base/".to_owned()).unwrap();
    let initial = Context::new(Some(base.clone()));
    context
        .process_full(
            no_vocabulary_mut(),
            &initial,
            &NoLoader,
            Some(base),
            jsonld::context_processing::Options::default(),
            (),
        )
        .await
        .unwrap()
        .into_processed()
}

#[test]
fn expand_slice_reports_invalid_json_as_a_syntax_error_with_its_position() {
    let outcome = runtime().block_on(expand_slice(
        b"{\"@id\": \"http://example.com/s\", ",
        no_vocabulary_mut(),
        Context::<IriBuf>::new(None),
        None,
        &NoLoader,
        Options::default(),
        (),
    ));
    match outcome {
        Err(SliceExpansionError::Syntax(error)) => assert_eq!(error.position(), 32),
        other => panic!("a syntax error, got {other:?}"),
    }
}

#[test]
fn expand_slice_of_a_100k_deep_document_on_a_tokio_worker_stack_returns_and_drops() {
    let depth = 100_000;
    let text = format!(
        r#"{{"@context": {{"p": "http://example.com/p"}}, "@id": "http://example.com/s", "p": {}"leaf"{}}}"#,
        r#"{"p": "#.repeat(depth),
        "}".repeat(depth)
    );
    let expanded = thread::Builder::new()
        .stack_size(TOKIO_WORKER_STACK)
        .spawn(move || {
            runtime()
                .block_on(expand_slice(
                    text.as_bytes(),
                    no_vocabulary_mut(),
                    Context::<IriBuf>::new(None),
                    None,
                    &NoLoader,
                    Options::default(),
                    (),
                ))
                .map(|document| document.len())
                .is_ok()
        })
        .unwrap()
        .join()
        .expect("expansion returns instead of overflowing its stack");
    assert!(expanded);
}

#[test]
fn expand_slice_from_a_processed_context_matches_expanding_the_document_carrying_that_context() {
    let runtime = runtime();
    let context = r#"{"p": "http://example.com/p", "T": "http://example.com/T"}"#;
    let body = r#""@id": "http://example.com/s", "@type": "T", "p": "v""#;
    let carrying = format!(r#"{{"@context": {context}, {body}}}"#);
    let bare = format!("{{{body}}}");

    let (carrying, _) = Value::parse_str(&carrying).unwrap();
    let expected = runtime.block_on(carrying.expand(&NoLoader)).unwrap();
    let active = runtime.block_on(processed(context));
    let from_slice = runtime
        .block_on(expand_slice(
            bare.as_bytes(),
            no_vocabulary_mut(),
            active,
            None,
            &NoLoader,
            Options::default(),
            (),
        ))
        .unwrap();

    assert_eq!(from_slice, expected);
}

/// The term the Expansion algorithm writes for the member key `key` of a node
/// object under `context`.
fn key_the_algorithm_writes(context: &str, key: &str) -> Option<String> {
    let document = format!(r#"{{"@context": {context}, "@id": "http://example.com/s", "{key}": "v"}}"#);
    let (document, _) = Value::parse_str(&document).unwrap();
    let expanded = runtime().block_on(document.expand(&NoLoader)).unwrap();
    let node = expanded.into_iter().next()?.into_inner().into_node()?;
    node.properties().iter().next().map(|(property, _)| property.to_string())
}

#[test]
fn expand_property_key_matches_the_key_the_expansion_algorithm_writes() {
    let context = r#"{"@vocab": "http://example.com/vocab#", "ex": "http://example.com/ns#", "name": "http://schema.org/name", "alias": "@id"}"#;
    let active = runtime().block_on(processed(context));

    for key in ["name", "ex:thing", "free", "http://example.com/absolute"] {
        let term = expand_property_key(no_vocabulary_mut(), &active, key, Action::Keep).unwrap().unwrap();
        assert_eq!(Some(term.to_string()), key_the_algorithm_writes(context, key), "key `{key}`");
    }

    let alias = expand_property_key(no_vocabulary_mut(), &active, "alias", Action::Keep).unwrap().unwrap();
    assert!(matches!(*alias, Term::Keyword(jsonld::syntax::Keyword::Id)));

    let blank = expand_property_key(no_vocabulary_mut(), &active, "_:b0", Action::Keep).unwrap().unwrap();
    assert_eq!(blank.to_string(), "_:b0");
}

#[test]
fn expand_property_key_of_an_undefined_term_without_vocab_is_an_invalid_identifier() {
    let active = runtime().block_on(processed(r#"{"name": "http://schema.org/name"}"#));
    let term = expand_property_key(no_vocabulary_mut(), &active, "undefined", Action::Keep).unwrap().unwrap();
    assert!(!matches!(*term, Term::Id(jsonld::Id::Valid(_))), "{term:?}");
}

#[test]
fn expand_property_key_follows_the_vocab_policy() {
    let active = runtime().block_on(processed(r#"{"@vocab": "http://example.com/vocab#"}"#));

    let kept = expand_property_key(no_vocabulary_mut(), &active, "free", Action::Keep).unwrap();
    assert_eq!(kept.map(|term| term.to_string()).as_deref(), Some("http://example.com/vocab#free"));

    let dropped = expand_property_key(no_vocabulary_mut(), &active, "free", Action::Drop).unwrap();
    assert!(dropped.is_none());

    let rejected = expand_property_key(no_vocabulary_mut(), &active, "free", Action::Reject);
    assert!(rejected.is_err());
}

/// The type the Expansion algorithm writes for the `@type` value `value` under
/// `context`.
fn type_the_algorithm_writes(context: &str, value: &str) -> Option<String> {
    let document = format!(r#"{{"@context": {context}, "@id": "http://example.com/s", "@type": "{value}"}}"#);
    let (document, _) = Value::parse_str(&document).unwrap();
    let base = IriBuf::new("http://example.com/base/".to_owned()).unwrap();
    let expanded = runtime()
        .block_on(document.expand_full(
            no_vocabulary_mut(),
            Context::new(Some(base.clone())),
            Some(&base),
            &NoLoader,
            Options::default(),
            (),
        ))
        .unwrap();
    let node = expanded.into_iter().next()?.into_inner().into_node()?;
    node.types().first().map(ToString::to_string)
}

#[test]
fn expand_type_value_matches_the_type_the_expansion_algorithm_writes() {
    let context = r#"{"@vocab": "http://example.com/vocab#", "Vehicle": "http://example.com/Vehicle", "ex": "http://example.com/ns#"}"#;
    let active = runtime().block_on(processed(context));

    for value in ["Vehicle", "ex:Car", "Free", "http://example.com/Absolute"] {
        let term = expand_type_value(no_vocabulary_mut(), &active, value, Action::Keep).unwrap().unwrap();
        assert_eq!(Some(term.to_string()), type_the_algorithm_writes(context, value), "type `{value}`");
    }
}

#[test]
fn expand_type_value_resolves_a_relative_reference_against_the_base_iri_when_no_vocab_applies() {
    let active = runtime().block_on(processed(r#"{"Vehicle": "http://example.com/Vehicle"}"#));
    let term = expand_type_value(no_vocabulary_mut(), &active, "relative", Action::Keep).unwrap().unwrap();
    assert_eq!(term.to_string(), "http://example.com/base/relative");
}

#[test]
fn expand_property_key_reads_a_keyword_as_the_keyword() {
    let active = runtime().block_on(processed(r#"{"name": "http://schema.org/name"}"#));
    let term = expand_property_key(no_vocabulary_mut(), &active, "@type", Action::Keep).unwrap().unwrap();
    assert!(matches!(*term, Term::Keyword(jsonld::syntax::Keyword::Type)));
}
