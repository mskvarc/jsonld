#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! Benchmarks for one processed context shared by every request, the way an
//! NGSI-LD broker caches its core context.
//!
//! Cloning a processed context shares its memo caches, so these scenarios put
//! the compact-IRI and term-resolution memos under the access patterns a broker
//! produces:
//!
//! - `shared_warm_compact` — single-threaded compaction of one entity against
//!   an already-warm core context: the uncontended memo hit path.
//! - `shared_contended_compact` / `shared_contended_expand` — the same shared
//!   context used by several threads at once, each compacting (or expanding)
//!   in a loop. Throughput is reported in documents per second.
//! - `cold_memo_compact` — type-scoped contexts, which compaction processes
//!   afresh for every node object, so every document creates and fills new
//!   memos.
//!
//! The shared context is the published NGSI-LD core context, v1.9.
use std::{
    collections::BTreeMap,
    hint::black_box,
    sync::Barrier,
    thread,
    time::{Duration, Instant},
};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use iri_rs::iri;
use jsonld::{
    BlankIdBuf,
    ExpandedDocument,
    IriBuf,
    NoLoader,
    RemoteDocument,
    compaction::Compact,
    context_processing::{Process, ProcessedOwned},
    expansion::Expand,
    syntax::{Parse, TryFromJson, Value, context::Context as SyntaxContext},
};
use tokio::runtime::{Builder, Runtime};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod common;

use common::{ngsi_ld_entity, ngsi_ld_type_scoped_graph, parse_remote_doc, parse_syntax_context, pre_expand, pre_process_context};

/// NGSI-LD core context v1.9, as published by ETSI.
const NGSI_LD_CORE_CONTEXT: &str = include_str!("ngsi-ld-core-context-v1.9.jsonld");

/// URL the core context is published at.
const NGSI_LD_CORE_CONTEXT_URL: &str = "https://uri.etsi.org/ngsi-ld/v1/ngsi-ld-core-context-v1.9.jsonld";

/// A current-thread runtime with no I/O or time driver. Every future here runs
/// against [`NoLoader`] and completes on its first poll.
fn bench_runtime() -> Runtime {
    Builder::new_current_thread().build().expect("runtime")
}

struct Shared {
    expanded: ExpandedDocument<IriBuf, BlankIdBuf>,
    entity: Value,
    processed: ProcessedOwned<IriBuf, BlankIdBuf>,
}

/// Processes the core context the way a broker does once at start-up.
///
/// The result is paired with a reference to the context's URL rather than its
/// body, so compacted documents carry `"@context": "<core context URL>"` as a
/// broker's responses do, instead of 9 KB of inlined term definitions.
///
/// The core context's `ngsildproof` term carries a scoped context that imports
/// the W3C data-integrity context, and processing validates scoped contexts,
/// so that import has to resolve. An empty stand-in is enough: no entity here
/// uses `ngsildproof`, and a scoped context is only applied where its term is.
async fn process_core_context() -> ProcessedOwned<IriBuf, BlankIdBuf> {
    let document = Value::parse_str(NGSI_LD_CORE_CONTEXT).expect("core context parse").0;
    let context = document
        .as_object()
        .and_then(|object| object.get_unique("@context").ok().flatten())
        .expect("core context @context");
    let context = SyntaxContext::try_from_json(context).expect("core context try_from_json");
    let data_integrity: IriBuf = iri!("https://w3id.org/security/data-integrity/v2").into();
    let stand_in = Value::parse_str(r#"{"@context":{}}"#).expect("stand-in parse").0;
    let loader = BTreeMap::from([(data_integrity.clone(), RemoteDocument::new(Some(data_integrity), None, stand_in))]);
    let processed = context
        .process(jsonld::rdfx::vocabulary::no_vocabulary_mut(), &loader, None)
        .await
        .expect("core context process");
    let reference = Value::parse_str(&format!("\"{NGSI_LD_CORE_CONTEXT_URL}\"")).expect("core context URL parse").0;
    let reference = SyntaxContext::try_from_json(&reference).expect("core context URL try_from_json");
    ProcessedOwned::new(reference, processed.into_processed())
}

fn prepare_shared(runtime: &Runtime) -> Shared {
    let processed = runtime.block_on(process_core_context());
    let entity = Value::parse_str(&ngsi_ld_entity(7)).expect("entity parse").0;
    let expanded = runtime.block_on(async {
        entity
            .expand_full(
                jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                processed.processed().with_private_caches(),
                None,
                &NoLoader,
                jsonld::expansion::Options::default(),
                (),
            )
            .await
            .expect("entity expand")
    });
    let shared = Shared { expanded, entity, processed };
    // Warm both memos, as a broker's would be after its first requests.
    compact_once(runtime, &shared);
    expand_once(runtime, &shared);
    shared
}

fn compact_once(runtime: &Runtime, shared: &Shared) {
    runtime.block_on(async {
        let out = shared
            .expanded
            .compact_full(
                jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                shared.processed.as_ref(),
                &NoLoader,
                jsonld::compaction::Options::default(),
            )
            .await
            .expect("compact");
        black_box(out);
    });
}

fn expand_once(runtime: &Runtime, shared: &Shared) {
    runtime.block_on(async {
        // Cloning the processed context shares its memos, which is exactly how
        // a broker hands its cached core context to each request.
        let out = shared
            .entity
            .expand_full(
                jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                shared.processed.processed().clone(),
                None,
                &NoLoader,
                jsonld::expansion::Options::default(),
                (),
            )
            .await
            .expect("expand");
        black_box(out);
    });
}

/// Thread counts for the contention scenarios: 1, 2, 4, 8 and the machine's
/// available parallelism.
fn thread_counts() -> Vec<usize> {
    let available = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut counts = vec![1, 2, 4, 8, available];
    counts.sort_unstable();
    counts.dedup();
    counts
}

/// Runs `iters` calls of `work` on each of `threads` threads, all released
/// together, and returns the wall time until the last one finishes.
///
/// Each thread builds its runtime before the start barrier, so neither runtime
/// construction nor thread spawning is timed.
fn run_concurrently(shared: &Shared, threads: usize, iters: u64, work: fn(&Runtime, &Shared)) -> Duration {
    let barrier = Barrier::new(threads + 1);
    thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let runtime = bench_runtime();
                    barrier.wait();
                    for _ in 0..iters {
                        work(&runtime, shared);
                    }
                })
            })
            .collect();
        barrier.wait();
        let start = Instant::now();
        for handle in handles {
            handle.join().expect("worker");
        }
        start.elapsed()
    })
}

fn run_warm_compact(c: &mut Criterion) {
    let runtime = bench_runtime();
    let shared = prepare_shared(&runtime);

    let mut group = c.benchmark_group("shared_warm_compact");
    group.bench_function("ngsi_ld_entity", |b| b.iter(|| compact_once(&runtime, &shared)));
    group.finish();
}

fn run_contended(c: &mut Criterion, name: &str, work: fn(&Runtime, &Shared)) {
    let shared = prepare_shared(&bench_runtime());

    let mut group = c.benchmark_group(name);
    group.sample_size(30);
    for threads in thread_counts() {
        // One iteration is one document per thread, so elements per second is
        // documents per second across all threads.
        group.throughput(Throughput::Elements(threads as u64));
        group.bench_with_input(BenchmarkId::from_parameter(threads), &threads, |b, &threads| {
            b.iter_custom(|iters| run_concurrently(&shared, threads, iters, work));
        });
    }
    group.finish();
}

fn run_contended_compact(c: &mut Criterion) {
    run_contended(c, "shared_contended_compact", compact_once);
}

fn run_contended_expand(c: &mut Criterion) {
    run_contended(c, "shared_contended_expand", expand_once);
}

fn run_cold_memo_compact(c: &mut Criterion) {
    let runtime = bench_runtime();
    let scenario = ngsi_ld_type_scoped_graph(30);
    let expanded = runtime.block_on(pre_expand(&parse_remote_doc(&scenario.doc)));
    let processed = runtime.block_on(pre_process_context(parse_syntax_context(&scenario.context)));

    let mut group = c.benchmark_group("cold_memo_compact");
    group.bench_function(scenario.name, |b| {
        b.iter(|| {
            runtime.block_on(async {
                let out = expanded
                    .compact_full(
                        jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                        processed.as_ref(),
                        &NoLoader,
                        jsonld::compaction::Options::default(),
                    )
                    .await
                    .expect("compact");
                black_box(out);
            });
        });
    });
    group.finish();
}

criterion_group!(benches, run_warm_compact, run_contended_compact, run_contended_expand, run_cold_memo_compact);
criterion_main!(benches);
