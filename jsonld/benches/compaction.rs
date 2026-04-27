use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use jsonld::{NoLoader, compaction::Compact};

mod common;

use common::{
	Scenario, corpus, parse_remote_doc, parse_syntax_context, pre_expand, pre_process_context,
};

fn run_compaction(c: &mut Criterion) {
	let prepared: Vec<_> = corpus()
		.into_iter()
		.map(|s: Scenario| {
			let remote = parse_remote_doc(&s.doc);
			let expanded =
				async_std::task::block_on(async { pre_expand(&remote).await });
			let ctx = parse_syntax_context(&s.context);
			let processed =
				async_std::task::block_on(async { pre_process_context(ctx).await });
			(s.name, expanded, processed)
		})
		.collect();

	let mut group = c.benchmark_group("compact");
	group.sample_size(20);
	for (name, expanded, processed) in &prepared {
		group.bench_function(*name, |b| {
			b.iter(|| {
				async_std::task::block_on(async {
					let loader = NoLoader;
					let out = expanded
						.compact_full(
							jsonld::rdf_rs::vocabulary::no_vocabulary_mut(),
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
