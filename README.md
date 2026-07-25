# jsonld

[![Crate](https://img.shields.io/crates/v/jsonld.svg?style=flat-square)](https://crates.io/crates/jsonld)
[![Docs](https://img.shields.io/docsrs/jsonld?style=flat-square)](https://docs.rs/jsonld)
[![MSRV](https://img.shields.io/crates/msrv/jsonld?style=flat-square)](https://crates.io/crates/jsonld)
[![License](https://img.shields.io/crates/l/jsonld.svg?style=flat-square)](#license)

A [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust: context processing, expansion, compaction, flattening and RDF serialization, plus a derive macro that expands your own types straight into JSON-LD.

```rust
use iri_rs::iri;
use jsonld::{JsonLdProcessor, RemoteDocument, syntax::{Parse, Value}};

let input = RemoteDocument::new(
    Some(iri!("https://example.com/sample.jsonld").to_owned()),
    Some("application/ld+json".parse().unwrap()),
    Value::parse_str(r#"{
        "@context": { "name": "https://schema.org/name" },
        "@id": "https://example.com/alice",
        "name": "Alice"
    }"#).expect("parse error").0,
);

let expanded = input.expand(&jsonld::NoLoader).await?;
```

The entry point is the `JsonLdProcessor` trait, which carries every transformation algorithm. Documents arrive as `RemoteDocument` (JSON already in hand) or `RemoteDocumentReference` (a URL for a loader to dereference).

---

## Why this fork

Fork of [`json-ld`](https://crates.io/crates/json-ld) by [Timothée Haudebourg](https://github.com/timothee-haudebourg/json-ld). The algorithms and their spec conformance are upstream's — the W3C JSON-LD API test suite still governs this repo. The fork changes three things: the dependency stack, the performance profile, and what you can do with your own Rust types.

### A different dependency stack

Upstream builds on `rdf-types`, `xsd-types`, `iref`, `static-iref` and `json-syntax`. This fork retargets every one of them:

| Upstream | Here | Why |
| --- | --- | --- |
| `rdf-types` | [`rdfx`](https://crates.io/crates/rdfx) | RDF 1.2 model: triple terms, directional language strings |
| `json-syntax` | [`jstrict`](https://crates.io/crates/jstrict) | strict JSON parsing and canonicalization |
| `iref` / `static-iref` | [`iri-rs`](https://crates.io/crates/iri-rs) | allocation-conscious IRI parsing, compile-time literals |
| `xsd-types` | [`xsd-rs`](https://crates.io/crates/xsd-rs) | SIMD-backed `base64Binary` / `hexBinary` |
| `linked-data` | [`ld-core`](https://crates.io/crates/ld-core) | follows the same stack |

The crate names moved with the dependencies — `json-ld` became `jsonld`, and every member crate followed.

The workspace is Rust 2024 with MSRV 1.85, uses `mediatype` for content negotiation instead of hand-rolled parsing, and carries a reworked error surface.

### Performance

The performance work concentrates on context processing and compaction — the parts of JSON-LD that dominate real workloads:

- **Context memoization.** Processed contexts are cached rather than reprocessed per document, and a synchronous fast path skips the async machinery entirely when a context needs no remote loads.
- **Inverse contexts.** The inverse context is rebuilt lazily instead of on every mutation, and its construction is cheaper.
- **Interning.** Term keys and strings are interned, so comparison and hashing work on handles instead of string data.
- **Compaction.** Compact IRI results are memoized per compaction call, the prefix list is cached instead of rebuilt each iteration, and sort comparators no longer materialise strings through the vocabulary.
- **Hashing.** SipHash gave way to hashbrown's default hasher, with `ahash` and `gxhash` as opt-in alternatives.
- **Allocation.** Contexts are borrowed rather than cloned on hot paths, and clones were stripped out of expansion and compaction.
- **Parallel expansion.** An optional `parallel` feature expands a batch of documents concurrently, each task owning its own vocabulary clone.

Criterion benchmarks for expansion and compaction live in `jsonld/benches`, so the claims are measurable rather than asserted:

```sh
cargo bench -p jsonld
```

### Working with your own types

Two additions have no upstream counterpart.

**`Expandable`** derives JSON-LD expansion for a Rust type, skipping the intermediate JSON value entirely:

```rust
use jsonld_expandable::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Parent")]
pub struct Parent {
    #[jsonld(property = "https://example.com/name")]
    pub name: String,

    #[jsonld(flatten_map)]
    pub subs: BTreeMap<String, Sub>,
}

let expanded: serde_json::Value = parent.expand();
```

The attribute vocabulary covers `property`, `id`, `type`, `nested`, `vec`, `list`, `language_map`, `typed_value`, `vocab` / `id_ref`, `flatten_object`, `flatten_map`, `passthrough` and `skip`. Output goes to whichever backend you enable — `serde-json`, `sonic-rs` or `jstrict` — and the `chrono` feature lets temporal types take part without a newtype wrapper.

**`jsonld-vocab`** reads your context files at compile time and generates typed constants from them:

```rust
jsonld_vocab::generate! {
    contexts: ["contexts/core.jsonld"]
}
```

Paths resolve against `CARGO_MANIFEST_DIR`, so a mistyped term becomes a compile error rather than a runtime surprise.

Credit and history preserved — see [Attribution](#attribution).

## Install

```sh
cargo add jsonld
```

## Crates

| Crate | Purpose |
| --- | --- |
| `jsonld` | Umbrella crate: `JsonLdProcessor` and the transformation algorithms |
| `jsonld-core` | Core types: objects, nodes, values, documents, loaders |
| `jsonld-syntax` | Syntax layer: contexts, term definitions, keywords |
| `jsonld-context-processing` | Context processing algorithm |
| `jsonld-expansion` | Expansion algorithm |
| `jsonld-compaction` | Compaction algorithm |
| `jsonld-serialization` | Serialization of expanded documents |
| `jsonld-expandable` / `jsonld-expandable-core` | `Expandable` derive and its runtime |
| `jsonld-vocab` | Compile-time vocabulary generation |
| `jsonld-testing` | Test-suite harness |
| `jsonld-cli` | Command line interface |

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `fast-hash` | yes | hashbrown's default hasher across the workspace |
| `ahash` | | `ahash` as the hasher instead |
| `gxhash` | | `gxhash` as the hasher; wins if both end up enabled |
| `serde` | | `Serialize` / `Deserialize` for syntax and core types |
| `serde-json` | | `serde_json` interop |
| `reqwest` | | HTTP document loader |
| `parallel` | | concurrent batch expansion; needs a `Clone` vocabulary |
| `expandable` | | the `Expandable` derive |
| `expandable-serde-json`, `-sonic-rs`, `-jstrict` | | output backend for `Expandable` |
| `expandable-chrono` | | `chrono` temporal types in `Expandable` types |
| `vocab` | | compile-time vocabulary generation |

## Conformance

The W3C JSON-LD API test suite is a git submodule. A fresh clone needs it before the conformance tests will run:

```sh
git submodule update --init
cargo test -p jsonld
```

## MSRV

Rust 1.85 (edition 2024).

## Attribution

Original crates: [`json-ld`](https://crates.io/crates/json-ld) and its members by [Timothée Haudebourg](https://github.com/timothee-haudebourg/json-ld). Upstream commits are preserved in this repo's history under their original authorship, and the bulk of this repository remains their work. This fork is a dependency-stack swap, a layer of performance work, and the `Expandable` and `jsonld-vocab` additions on top of their implementation.

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT)
