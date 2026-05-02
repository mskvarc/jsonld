#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use jsonld::{JsonLdProcessor, NoLoader};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod common;

use common::{Scenario, corpus, parse_remote_doc};

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

criterion_group!(benches, run_expansion);
criterion_main!(benches);
