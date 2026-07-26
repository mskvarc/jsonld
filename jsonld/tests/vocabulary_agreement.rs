#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! Expansion against an interning vocabulary must agree with expansion against
//! none.
//!
//! Each test expands the same document twice — once against `NoVocabulary`,
//! where an identifier *is* the IRI and interning cannot go wrong, and once
//! against an `IndexVocabulary` — then resolves the second result back to IRIs
//! and requires the two to match.
//!
//! These began as the regression tests for a removed `parallel` feature, which
//! expanded wide arrays by handing each item its own forked vocabulary and
//! corrupted the output when the forks were not merged back. That feature is
//! gone, but the invariant it violated is the one any future concurrent or
//! forking design must clear, and nothing else in the suite pins it: the W3C
//! conformance tests never compare the two vocabulary types against each other.
//! The document is deliberately built so that every item contributes IRIs and
//! blank node identifiers no other item contributes, since shared terms would
//! let colliding identifiers agree by luck.

use iri_rs::{IriBuf, iri};
use jsonld::{
    BlankIdBuf,
    ExpandedDocument,
    Id,
    JsonLdProcessor,
    NoLoader,
    RemoteDocument,
    ValidId,
    rdfx::vocabulary::{BlankIdVocabulary, IndexVocabulary, IriVocabulary, IriVocabularyMut},
    syntax::{Parse, Value},
};

/// Number of array items. Kept above 32 because that was the lower bound of
/// the removed feature's window, so the fixture still matches the shape any
/// future batching design would target.
const ITEMS: usize = 64;

const DOC_URL: &str = "https://example.org/vocabulary-agreement.jsonld";

/// A `@graph` of `ITEMS` node objects, each contributing IRIs and blank node
/// identifiers that no other item contributes.
///
/// Distinctness is the point: if every item interned the same terms, each
/// task's indices would coincide with the parent's by luck and the bug would
/// not show. Nested objects keep the items "heavy" enough to pass the
/// `array_has_heavy_items` probe.
fn heavy_graph() -> String {
    let context = r#"{"@vocab":"https://ex.org/vocab/","author":{"@id":"https://ex.org/vocab/author","@type":"@id"},"knows":{"@id":"https://ex.org/vocab/knows","@type":"@id"}}"#;
    let mut doc = format!(r#"{{"@context":{context},"@graph":["#);
    for index in 0..ITEMS {
        if index > 0 {
            doc.push(',');
        }
        doc.push_str(&format!(
            r#"{{"@id":"https://ex.org/node/{index}","@type":"https://ex.org/type/T{index}","title":"Node {index}","author":"https://ex.org/person/{index}","knows":"_:peer{index}","meta":{{"@id":"_:meta{index}","tag":"https://ex.org/tag/{index}"}}}}"#
        ));
    }
    doc.push_str("]}");
    doc
}

fn parse(doc: &str) -> Value {
    Value::parse_str(doc).expect("document parses").0
}

/// Expands against `NoVocabulary`, where identifiers are the terms themselves.
async fn expand_without_vocabulary(doc: &str) -> ExpandedDocument<IriBuf, BlankIdBuf> {
    let url = IriBuf::new(DOC_URL.to_owned()).expect("valid document URL");
    let remote = RemoteDocument::new(Some(url), None, parse(doc));
    remote.expand(&NoLoader).await.expect("expansion without a vocabulary")
}

/// Expands against an interning vocabulary, then resolves every identifier in
/// the result back to the term it denotes *in the parent vocabulary*.
///
/// Resolution is where an unmerged fork is caught: an index the fork minted is
/// not present in the parent, so the lookup returns `None`.
async fn expand_with_vocabulary(doc: &str) -> ExpandedDocument<IriBuf, BlankIdBuf> {
    let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
    let url = vocabulary.insert(iri!("https://example.org/vocabulary-agreement.jsonld"));
    let remote = RemoteDocument::new(Some(url), None, parse(doc));
    let expanded = remote.expand_with(&mut vocabulary, &NoLoader).await.expect("expansion with a vocabulary");

    expanded.map_ids(
        |index| {
            let iri = vocabulary
                .iri(&index)
                .unwrap_or_else(|| panic!("IRI index {index:?} in the output does not resolve in the parent vocabulary"));
            IriBuf::from(iri)
        },
        |id| match id {
            Id::Valid(ValidId::Iri(index)) => {
                let iri = vocabulary
                    .iri(&index)
                    .unwrap_or_else(|| panic!("IRI index {index:?} in the output does not resolve in the parent vocabulary"));
                Id::Valid(ValidId::Iri(IriBuf::from(iri)))
            }
            Id::Valid(ValidId::Blank(index)) => {
                let blank_id = vocabulary
                    .blank_id(&index)
                    .unwrap_or_else(|| panic!("blank node index {index:?} in the output does not resolve in the parent vocabulary"));
                Id::Valid(ValidId::Blank(blank_id.to_owned()))
            }
            Id::Invalid(reference) => Id::Invalid(reference),
        },
    )
}

#[tokio::test]
async fn every_identifier_in_a_wide_array_resolves_in_the_parent_vocabulary() {
    let doc = heavy_graph();
    // Resolution happens inside `expand_with_vocabulary`, which panics with the
    // offending index if the parent cannot resolve it.
    let expanded = expand_with_vocabulary(&doc).await;
    assert_eq!(expanded.len(), ITEMS, "every node of the graph should survive expansion");
}

#[tokio::test]
async fn wide_array_expansion_agrees_with_and_without_an_interning_vocabulary() {
    let doc = heavy_graph();
    let without = expand_without_vocabulary(&doc).await;
    let with = expand_with_vocabulary(&doc).await;
    assert_eq!(with, without, "interning must not change what expansion produces");
}

#[tokio::test]
async fn distinct_terms_in_a_wide_array_keep_distinct_identifiers() {
    let doc = heavy_graph();
    let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
    let url = vocabulary.insert(iri!("https://example.org/vocabulary-agreement.jsonld"));
    let remote = RemoteDocument::new(Some(url), None, parse(&doc));
    let expanded = remote.expand_with(&mut vocabulary, &NoLoader).await.expect("expansion with a vocabulary");

    // Two tasks handing the same index to different IRIs is the precise failure
    // the merge exists to prevent, and it collapses the node set: distinct
    // subjects become one. Counting the distinct subject identifiers catches it
    // even if every index happens to resolve.
    let subjects: std::collections::HashSet<_> = expanded.iter().filter_map(|object| object.id().cloned()).collect();
    assert_eq!(subjects.len(), ITEMS, "each node object must keep its own identifier");
}
