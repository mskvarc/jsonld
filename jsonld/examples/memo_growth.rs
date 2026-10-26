#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! Live heap held by one shared processed context after expanding documents
//! whose attribute names are never repeated.
//!
//! Prints one `names live_bytes` line per checkpoint. Uses only API present in
//! every version, so the same file measures either side of an A/B comparison.
//!
//! usage: `cargo run --release --example memo_growth -- [total-names]`
use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    sync::atomic::{AtomicUsize, Ordering},
};

use jsonld::{
    NoLoader,
    expansion::Expand,
    syntax::{Parse, TryFromJson, Value, context::Context as SyntaxContext},
};
use tokio::runtime::Builder;

static LIVE: AtomicUsize = AtomicUsize::new(0);

/// System allocator that tracks live bytes.
struct Counting;

// SAFETY: every call forwards unchanged to `System`; the counter is advisory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(new_size, Ordering::Relaxed);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const NAMES_PER_DOCUMENT: u64 = 50;

fn main() {
    let total: u64 = env::args().nth(1).map_or(1_000_000, |arg| arg.parse().expect("total-names"));
    let runtime = Builder::new_current_thread().build().expect("runtime");
    let document = Value::parse_str(r#"{"@context":{"@vocab":"https://uri.etsi.org/ngsi-ld/default-context/","id":"@id","type":"@type"}}"#)
        .expect("context parse")
        .0;
    let context = document.as_object().and_then(|object| object.get_unique("@context").ok().flatten()).expect("@context");
    let context = SyntaxContext::try_from_json(context).expect("context");
    let processed = runtime
        .block_on(async {
            use jsonld::context_processing::Process;
            context.process(jsonld::rdfx::vocabulary::no_vocabulary_mut(), &NoLoader, None).await
        })
        .expect("process")
        .into_processed();

    let mut checkpoint = 1_000;
    println!("names live_bytes");
    println!("0 {}", LIVE.load(Ordering::Relaxed));
    for first in (0..total).step_by(NAMES_PER_DOCUMENT as usize) {
        let members: String = (first..first + NAMES_PER_DOCUMENT)
            .map(|n| format!(r#","attribute{n}":{{"type":"Property","value":{n}}}"#))
            .collect();
        let document = Value::parse_str(&format!(r#"{{"id":"urn:x:C{first}","type":"Churn"{members}}}"#)).expect("parse").0;
        let expanded = runtime.block_on(async {
            document
                .expand_full(
                    jsonld::rdfx::vocabulary::no_vocabulary_mut(),
                    processed.clone(),
                    None,
                    &NoLoader,
                    jsonld::expansion::Options::default(),
                    (),
                )
                .await
                .expect("expand")
        });
        drop(expanded);
        drop(document);
        if first + NAMES_PER_DOCUMENT >= checkpoint {
            println!("{} {}", first + NAMES_PER_DOCUMENT, LIVE.load(Ordering::Relaxed));
            checkpoint = (checkpoint * 4).min(total.max(checkpoint));
        }
    }
}
