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
