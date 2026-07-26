#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use jsonld::{JsonLdProcessor, NoLoader};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod common;

use common::{Scenario, base_relative_corpus, corpus, fresh_indexed_vocabulary, parse_remote_doc, parse_remote_doc_indexed, vocabulary_corpus};

fn run_expansion(c: &mut Criterion) {
    let scenarios: Vec<(Scenario, _)> = corpus()
        .into_iter()
        .map(|s| {
            let remote = parse_remote_doc(&s.doc);
            (s, remote)
        })
        .collect();

    let mut group = c.benchmark_group("expand");
    group.sample_size(20);
    for (scenario, remote) in &scenarios {
        // Bench id stays as `<scenario>` so `--baseline new` keeps comparing
        // against the saved baseline. Differentiate runs across feature combos
        // by passing `--save-baseline <name>` (or by reading the criterion
        // estimates.json directly from `target/criterion/expand/<scenario>/new/`).
        group.bench_function(scenario.name, |b| {
            b.iter(|| {
                async_std::task::block_on(async {
                    let loader = NoLoader;
                    let expanded = remote.expand(&loader).await.expect("expand");
                    black_box(expanded);
                });
            });
        });
    }
    group.finish();
}

/// Expansion against an interning [`IndexVocabulary`], the configuration the
/// `parallel` feature targets and the one [`run_expansion`] never covers.
///
/// The vocabulary is rebuilt per iteration in the (untimed) setup closure so
/// that interning cost is measured every time rather than only on the first
/// iteration. Compare feature combinations by saving separate baselines, e.g.
/// `--save-baseline vocab_seq` against `--save-baseline vocab_par`.
fn run_expansion_with_vocabulary(c: &mut Criterion) {
    let scenarios: Vec<(Scenario, _)> = vocabulary_corpus()
        .into_iter()
        .map(|s| {
            let remote = parse_remote_doc_indexed(&s.doc, &mut fresh_indexed_vocabulary());
            (s, remote)
        })
        .collect();

    let mut group = c.benchmark_group("expand_vocabulary");
    group.sample_size(20);
    for (scenario, remote) in &scenarios {
        group.bench_function(scenario.name, |b| {
            b.iter_batched(
                fresh_indexed_vocabulary,
                |mut vocabulary| {
                    async_std::task::block_on(async {
                        let loader = NoLoader;
                        let expanded = remote.expand_with(&mut vocabulary, &loader).await.expect("expand");
                        black_box(expanded);
                    });
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Expansion of documents whose identifiers are relative references resolved
/// against `@base` — the only part of the corpus where `iri-rs`'s resolver does
/// meaningful work. Use this to judge whether a change to `iri-rs` moves
/// JSON-LD at all.
fn run_expansion_base_relative(c: &mut Criterion) {
    let scenarios: Vec<(Scenario, _)> = base_relative_corpus()
        .into_iter()
        .map(|s| {
            let remote = parse_remote_doc(&s.doc);
            (s, remote)
        })
        .collect();

    let mut group = c.benchmark_group("expand_base_relative");
    group.sample_size(20);
    for (scenario, remote) in &scenarios {
        group.bench_function(scenario.name, |b| {
            b.iter(|| {
                async_std::task::block_on(async {
                    let loader = NoLoader;
                    let expanded = remote.expand(&loader).await.expect("expand");
                    black_box(expanded);
                });
            });
        });
    }
    group.finish();
}

criterion_group!(benches, run_expansion, run_expansion_with_vocabulary, run_expansion_base_relative);
criterion_main!(benches);
