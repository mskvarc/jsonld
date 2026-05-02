#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use std::{env, hint::black_box, process::ExitCode, time::Instant};

use jsonld::{JsonLdProcessor, NoLoader, compaction::Compact};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[path = "../benches/common.rs"]
mod common;

use common::{Scenario, corpus, parse_remote_doc, parse_syntax_context, pre_expand, pre_process_context};

const DEFAULT_ITERS: usize = 200;

enum Mode {
    Expand,
    Compact,
    Both,
}

fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "expand" => Ok(Mode::Expand),
        "compact" => Ok(Mode::Compact),
        "both" => Ok(Mode::Both),
        other => Err(format!("unknown mode `{other}` (expected expand|compact|both)")),
    }
}

fn usage() {
    eprintln!("usage: profile <expand|compact|both> <scenario|all> [iters]");
    eprintln!("       iters defaults to {DEFAULT_ITERS}");
}

fn select_scenarios(name: &str) -> Result<Vec<Scenario>, String> {
    let all = corpus();
    if name == "all" {
        return Ok(all);
    }
    let picked: Vec<Scenario> = all.into_iter().filter(|s| s.name == name).collect();
    if picked.is_empty() {
        Err(format!("scenario `{name}` not found; use `all` or one of the corpus names"))
    } else {
        Ok(picked)
    }
}

struct Prepared {
    name: &'static str,
    remote: jsonld::RemoteDocument,
    expanded: jsonld::ExpandedDocument<jsonld::IriBuf, jsonld::BlankIdBuf>,
    processed: jsonld::context_processing::ProcessedOwned<jsonld::IriBuf, jsonld::BlankIdBuf>,
}

async fn prepare(scenarios: Vec<Scenario>) -> Vec<Prepared> {
    let mut out = Vec::with_capacity(scenarios.len());
    for s in scenarios {
        let remote = parse_remote_doc(&s.doc);
        let expanded = pre_expand(&remote).await;
        let ctx = parse_syntax_context(&s.context);
        let processed = pre_process_context(ctx).await;
        out.push(Prepared { name: s.name, remote, expanded, processed });
    }
    out
}

async fn run_expand(p: &Prepared, iters: usize) {
    let loader = NoLoader;
    for _ in 0..iters {
        let expanded = p.remote.expand(&loader).await.expect("expand");
        black_box(expanded);
    }
}

async fn run_compact(p: &Prepared, iters: usize) {
    let loader = NoLoader;
    for _ in 0..iters {
        let out = p
            .expanded
            .compact_full(
                jsonld::rdf_rs::vocabulary::no_vocabulary_mut(),
                p.processed.as_ref(),
                &loader,
                jsonld::compaction::Options::default(),
            )
            .await
            .expect("compact");
        black_box(out);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        usage();
        return ExitCode::from(2);
    }
    let mode = match parse_mode(&args[1]) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            usage();
            return ExitCode::from(2);
        }
    };
    let scenarios = match select_scenarios(&args[2]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let iters: usize = match args.get(3) {
        Some(s) => match s.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("iters must be a positive integer");
                return ExitCode::from(2);
            }
        },
        None => DEFAULT_ITERS,
    };

    async_std::task::block_on(async {
        let prepared = prepare(scenarios).await;
        let start = Instant::now();
        for p in &prepared {
            let scenario_start = Instant::now();
            match mode {
                Mode::Expand => run_expand(p, iters).await,
                Mode::Compact => run_compact(p, iters).await,
                Mode::Both => {
                    run_expand(p, iters).await;
                    run_compact(p, iters).await;
                }
            }
            println!("{:<36} {} iters in {:?}", p.name, iters, scenario_start.elapsed());
        }
        println!("total elapsed: {:?}", start.elapsed());
    });

    ExitCode::SUCCESS
}
