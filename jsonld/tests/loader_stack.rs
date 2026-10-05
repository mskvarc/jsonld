#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Context Processing algorithm dereferences a remote context through the
//! embedder's `LoadDocumentCallback` (step 5.2.5) and an `@import` (step
//! 5.6.4) ([JSON-LD 1.1 API §4.1.2][1]). The loader is the embedder's code: an
//! HTTP client behind a cache can take more native stack per poll than one
//! level of the algorithm, and the crate cannot bound it.
//!
//! The algorithms keep their recursion off the caller's stack by checking, at
//! every recursion point, that at least the red zone is left. A loader polled
//! without such a check runs on whatever the last check left, or on whatever
//! the caller left when no check ran yet, and a loader needing more than that
//! overflows the thread. Every dereference therefore starts with at least the
//! red zone available, wherever the embedder's stack stood.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
use iri_rs::{Iri, IriBuf};
use jsonld::{
    JsonLdProcessor,
    LoadError,
    Loader,
    RemoteDocument,
    syntax::{Parse, Value, native_stack::RED_ZONE},
};
use std::{
    hint::black_box,
    io,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

/// The native stack the loader's own entry takes between the guard and the
/// body of `load`: the provided `load_with` and the forwarding through `&L`.
/// An unguarded loader falls short of the red zone by up to the 64 KiB the
/// sweep reaches below it.
const LOADER_ENTRY: usize = 16 * 1024;

/// The stack each run starts from, ample for spending it down to any margin.
const THREAD_STACK: usize = 8 * 1024 * 1024;

/// The remote context every document of this test references.
const REMOTE_CONTEXT: &str = r#"{"@context": {"p": "http://example.com/vocab#p"}}"#;

/// A document referencing the remote context at its top level.
const REFERENCING: &str = r#"{"@context": "http://example.com/context.jsonld", "p": "v"}"#;

/// A document importing the remote context.
const IMPORTING: &str = r#"{"@context": {"@version": 1.1, "@import": "http://example.com/context.jsonld"}, "p": "v"}"#;

/// A loader recording the least native stack any of its polls ran on.
struct StackRecordingLoader {
    /// The least stack seen, in bytes.
    least: AtomicUsize,
}

impl Loader for StackRecordingLoader {
    type Error = io::Error;

    async fn load(&self, url: Iri<&str>) -> Result<RemoteDocument<IriBuf>, LoadError<io::Error>> {
        let left = stacker::remaining_stack().expect("the platform reports its stack");
        self.least.fetch_min(left, Ordering::Relaxed);
        let (json, _) = Value::parse_str(REMOTE_CONTEXT).unwrap();
        Ok(RemoteDocument::new(Some(url.into()), None, json))
    }
}

/// Expands `document` on a current-thread runtime and answers the least stack
/// the loader was polled on.
fn least_stack_at_the_loader(document: &str) -> usize {
    let (json, _) = Value::parse_str(document).unwrap();
    let document = RemoteDocument::new(None, None, json);
    let loader = StackRecordingLoader {
        least: AtomicUsize::new(usize::MAX),
    };
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime.block_on(document.expand(&loader)).unwrap();
    loader.least.into_inner()
}

/// Spends the native stack in small frames until about `left` bytes remain,
/// then runs `run`.
fn with_stack_left(left: usize, run: fn() -> usize) -> usize {
    if stacker::remaining_stack().expect("the platform reports its stack") > left {
        let pad = black_box([0_u8; 1024]);
        let least = with_stack_left(left, run);
        black_box(&pad);
        least
    } else {
        run()
    }
}

/// Runs `run` on a fresh thread with about `left` bytes of native stack.
fn on_a_thread_with(left: usize, run: fn() -> usize) -> usize {
    thread::Builder::new()
        .stack_size(THREAD_STACK)
        .spawn(move || with_stack_left(left, run))
        .unwrap()
        .join()
        .unwrap()
}

/// The least stack the loader was polled on, for every caller margin that
/// would leave an unchecked loader from just below to just above the red zone.
///
/// A first run on ample stack measures how much the algorithm takes between
/// the caller and the loader.
fn sweep(run: fn() -> usize) -> Vec<(usize, usize)> {
    let ample = 4 * 1024 * 1024;
    let taken = ample - on_a_thread_with(ample, run);
    (0..=16)
        .map(|step| taken + RED_ZONE - 64 * 1024 + step * 8 * 1024)
        .map(|margin| (margin, on_a_thread_with(margin, run)))
        .collect()
}

/// The runs of `sweep` whose loader was polled on less than the red zone,
/// less its own entry.
fn starved(run: fn() -> usize) -> Vec<(usize, usize)> {
    sweep(run).into_iter().filter(|(_, least)| *least < RED_ZONE - LOADER_ENTRY).collect()
}

#[test]
fn a_remote_context_is_dereferenced_on_at_least_the_red_zone() {
    let starved = starved(|| least_stack_at_the_loader(REFERENCING));

    assert!(starved.is_empty(), "(caller margin, stack at the loader): {starved:?}");
}

#[test]
fn an_imported_context_is_dereferenced_on_at_least_the_red_zone() {
    let starved = starved(|| least_stack_at_the_loader(IMPORTING));

    assert!(starved.is_empty(), "(caller margin, stack at the loader): {starved:?}");
}
