#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! Benchmarks for context processing on its own, from JSON text to a processed
//! context.
//!
//! - `context_processing/ngsi_ld_core_v1_9` — the published NGSI-LD core
//!   context, v1.9: about two hundred term definitions, one of them with a
//!   scoped context. A broker does this once at start-up, but every tenant or
//!   inline context it accepts goes through the same path.
//! - `context_processing/inline_user` — a small user context of the kind a
//!   client sends inline with a request: plain terms, a prefix, compact IRIs and
//!   expanded definitions with type coercion.
//!
//! Every iteration parses the JSON text, so term keys are created from scratch
//! each time, as they are for a context received over the wire.
use std::{collections::BTreeMap, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};
use iri_rs::iri;
use jsonld::{
    IriBuf,
    NoLoader,
    RemoteDocument,
    context_processing::Process,
    syntax::{Parse, TryFromJson, Value, context::Context as SyntaxContext},
};
use tokio::runtime::{Builder, Runtime};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// NGSI-LD core context v1.9, as published by ETSI.
const NGSI_LD_CORE_CONTEXT: &str = include_str!("ngsi-ld-core-context-v1.9.jsonld");

/// A user context sent inline with a request.
const INLINE_USER_CONTEXT: &str = r#"{
    "ex": "https://example.org/",
    "Vehicle": "ex:Vehicle",
    "Car": "https://example.org/Vehicle",
    "speed": "https://example.org/speed",
    "velocity": { "@id": "ex:speed" },
    "brandName": "ex:brandName",
    "isParked": { "@id": "ex:isParked", "@type": "@id" },
    "color": { "@id": "ex:color", "@type": "@vocab" },
    "red": "ex:red",
    "observedOn": { "@id": "ex:observedOn", "@type": "http://www.w3.org/2001/XMLSchema#dateTime" },
    "wheels": { "@id": "ex:wheels", "@container": "@list" },
    "tags": { "@id": "ex:tags", "@container": "@set" }
}"#;

/// A current-thread runtime with no I/O or time driver. Every loader here
/// answers from memory, so each future completes on its first poll.
fn bench_runtime() -> Runtime {
    Builder::new_current_thread().build().expect("runtime")
}

/// Parses `text`, a JSON-LD document with an `@context` member, into its
/// context.
fn parse_context(text: &str) -> SyntaxContext {
    let document = Value::parse_str(text).expect("context parse").0;
    let context = document
        .as_object()
        .and_then(|object| object.get_unique("@context").ok().flatten())
        .expect("@context member");
    SyntaxContext::try_from_json(context).expect("context try_from_json")
}

/// Serves the one remote context the core context names.
///
/// The core context's `ngsildproof` term carries a scoped context that imports
/// the W3C data-integrity context, and processing validates scoped contexts,
/// so that import has to resolve. An empty stand-in is enough to measure the
/// core context's own definitions.
fn core_context_loader() -> BTreeMap<IriBuf, RemoteDocument> {
    let data_integrity: IriBuf = iri!("https://w3id.org/security/data-integrity/v2").into();
    let stand_in = Value::parse_str(r#"{"@context":{}}"#).expect("stand-in parse").0;
    BTreeMap::from([(data_integrity.clone(), RemoteDocument::new(Some(data_integrity), None, stand_in))])
}

fn run_core_context(c: &mut Criterion) {
    let runtime = bench_runtime();
    let loader = core_context_loader();

    let mut group = c.benchmark_group("context_processing");
    group.bench_function("ngsi_ld_core_v1_9", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let context = parse_context(NGSI_LD_CORE_CONTEXT);
                let processed = context
                    .process(jsonld::rdfx::vocabulary::no_vocabulary_mut(), &loader, None)
                    .await
                    .expect("core context process");
                black_box(processed);
            });
        });
    });
    group.finish();
}

fn run_inline_user_context(c: &mut Criterion) {
    let runtime = bench_runtime();
    let text = format!(r#"{{"@context":{INLINE_USER_CONTEXT}}}"#);

    let mut group = c.benchmark_group("context_processing");
    group.bench_function("inline_user", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let context = parse_context(&text);
                let processed = context
                    .process(jsonld::rdfx::vocabulary::no_vocabulary_mut(), &NoLoader, None)
                    .await
                    .expect("user context process");
                black_box(processed);
            });
        });
    });
    group.finish();
}

criterion_group!(benches, run_core_context, run_inline_user_context);
criterion_main!(benches);
