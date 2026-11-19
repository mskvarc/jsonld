#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A context cannot define a keyword, and ignores a term that only looks like
//! one. Step 5 of the [create term definition algorithm][1] refuses a keyword
//! with a keyword redefinition error and skips a keyword-like term with a
//! warning. Both the synchronous fast path and the loader-driven path behave
//! the same way.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#create-term-definition
use jsonld::{
    IriBuf,
    RemoteDocument,
    context_processing::{Error, Options, Process, Warning},
    rdfx::vocabulary::no_vocabulary_mut,
    syntax::{ErrorCode, Parse, TryFromJson, Value, context::Context},
    warning::WarningBuf,
};
use std::collections::HashMap;

/// A remote context that defines nothing, named first so processing leaves the
/// synchronous fast path.
const EMPTY_REMOTE: &str = "http://example.com/empty.jsonld";

type MapLoader = HashMap<IriBuf, RemoteDocument>;

fn loader() -> MapLoader {
    let url = IriBuf::new(EMPTY_REMOTE.to_owned()).unwrap();
    let (json, _) = Value::parse_str(r#"{"@context": {}}"#).unwrap();
    [(url.clone(), RemoteDocument::new(Some(url), None, json))].into_iter().collect()
}

/// The context written as `local`, first on the fast path and then behind a
/// remote context that forces the loader-driven path.
fn both_paths(local: &str) -> [String; 2] {
    [local.to_owned(), format!(r#"["{EMPTY_REMOTE}", {local}]"#)]
}

/// Processes the context written as `json`, returning whether `term` ended up
/// defined and the warnings raised, or the error processing ended with.
fn process(json: &str, term: &str) -> Result<(bool, Vec<Warning>), Error<<MapLoader as jsonld::Loader>::Error>> {
    let (value, _) = Value::parse_str(json).unwrap();
    let context = Context::try_from_json(&value).unwrap();
    let loader = loader();
    let mut warnings = WarningBuf::new();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let defined = runtime.block_on(async {
        context
            .process_full(
                no_vocabulary_mut(),
                &jsonld::Context::new(None),
                &loader,
                None,
                Options::default(),
                &mut warnings,
            )
            .await
            .map(|processed| processed.get(term).is_some())
    })?;
    Ok((defined, warnings.0))
}

#[test]
fn a_context_cannot_define_a_keyword() {
    for keyword in ["@id", "@context", "@graph", "@reverse", "@none"] {
        for json in both_paths(&format!(r#"{{"{keyword}": "http://example.com/x"}}"#)) {
            let error = process(&json, keyword).expect_err(&json);
            let Error::TermDefinition { term, error: cause } = &error else {
                panic!("expected a located term definition error for {json}, got {}", error.code());
            };
            assert_eq!(term.as_str(), keyword, "{json}");
            assert_eq!(cause.code(), ErrorCode::KeywordRedefinition, "{json}");
        }
    }
}

#[test]
fn a_context_cannot_define_a_keyword_with_an_expanded_definition() {
    for json in both_paths(r#"{"@id": {"@id": "http://example.com/x"}}"#) {
        assert_eq!(process(&json, "@id").expect_err(&json).code(), ErrorCode::KeywordRedefinition, "{json}");
    }
}

#[test]
fn a_keyword_like_term_is_ignored_with_a_warning() {
    for json in both_paths(r#"{"@ignoreMe": "http://example.com/x", "kept": "http://example.com/kept"}"#) {
        let (defined, warnings) = process(&json, "@ignoreMe").expect(&json);
        assert!(!defined, "{json}");
        assert_eq!(warnings, [Warning::KeywordLikeTerm("@ignoreMe".to_owned())], "{json}");
        assert!(process(&json, "kept").unwrap().0, "{json}");
    }
}
