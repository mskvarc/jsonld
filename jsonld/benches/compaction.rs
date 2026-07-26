#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use jsonld::{NoLoader, compaction::Compact};
use tokio::runtime::Builder;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod common;

use common::{Scenario, corpus, parse_remote_doc, parse_syntax_context, pre_expand, pre_process_context};

fn run_compaction(c: &mut Criterion) {
    // A current-thread runtime with no I/O or time driver, built once so that
    // runtime construction never lands inside a timed iteration. Every future
    // here runs against `NoLoader` and completes on its first poll.
    let runtime = Builder::new_current_thread().build().expect("runtime");
    let prepared: Vec<_> = corpus()
        .into_iter()
        .map(|s: Scenario| {
            let remote = parse_remote_doc(&s.doc);
            let expanded = runtime.block_on(async { pre_expand(&remote).await });
            let ctx = parse_syntax_context(&s.context);
            let processed = runtime.block_on(async { pre_process_context(ctx).await });
            (s.name, expanded, processed)
        })
        .collect();

    let mut group = c.benchmark_group("compact");
    group.sample_size(20);
    for (name, expanded, processed) in &prepared {
        group.bench_function(*name, |b| {
            b.iter(|| {
                runtime.block_on(async {
                    let loader = NoLoader;
                    let out = expanded
                        .compact_full(
                            jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                            processed.as_ref(),
                            &loader,
                            jsonld::compaction::Options::default(),
                        )
                        .await
                        .expect("compact");
                    black_box(out);
                });
            });
        });
    }
    group.finish();
}

criterion_group!(benches, run_compaction);
criterion_main!(benches);
