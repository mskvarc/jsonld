# jsonld

[![Crate](https://img.shields.io/crates/v/jsonld.svg?style=flat-square)](https://crates.io/crates/jsonld)
[![Docs](https://img.shields.io/docsrs/jsonld?style=flat-square)](https://docs.rs/jsonld)
[![MSRV](https://img.shields.io/crates/msrv/jsonld?style=flat-square)](https://crates.io/crates/jsonld)
[![License](https://img.shields.io/crates/l/jsonld.svg?style=flat-square)](#license)

A [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust: context processing, expansion, compaction, flattening and RDF serialization, plus a derive macro that expands your own types straight into JSON-LD.

> **Status: experimental.** The W3C JSON-LD API test suite passes for the four algorithms implemented here: 1,152 of its 1,156 tests, with the other four skipped, each linked to the upstream discussion behind it (see [Conformance](#conformance)). `unwrap`, `expect`, `panic`, `todo` and friends are denied workspace-wide. Even so, this is a young fork, published mainly so a downstream project can depend on it. The API surface still moves, the additions (`Expandable`, `vocab!`) have no upstream track record, and there is no compatibility promise before 1.0. Treat it as usable and tested, but not settled. See [Stability](#stability).

```rust
use iri_rs::{iri, IriBuf};
use jsonld::{JsonLdProcessor, RemoteDocument, syntax::{Parse, Value}};

let input = RemoteDocument::new(
    Some(IriBuf::from(iri!("https://example.com/sample.jsonld"))),
    Some("application/ld+json".parse()?),
    Value::parse_str(r#"{
        "@context": { "name": "https://schema.org/name" },
        "@id": "https://example.com/alice",
        "name": "Alice"
    }"#)?.0,
);

let expanded = input.expand(&jsonld::NoLoader).await?;
```

The entry point is the `JsonLdProcessor` trait, which carries every transformation algorithm. Documents arrive as `RemoteDocument` (JSON already in hand) or `RemoteDocumentReference` (a URL for a loader to dereference). The [crate documentation](https://docs.rs/jsonld) is the full guide, with worked examples for expansion, compaction, flattening, JSON interop, the derive macro and the feature flags. This README covers what the fork is and why it exists.

[`jsonld` and JSON-LD on every request](https://andromeda.skvarc.com/docs/libraries/jsonld/), on the Andromeda documentation site, explains what the processor is tuned for, how it is tested and when the upstream `json-ld` is the better choice.

---

## Why this fork

Fork of [`json-ld`](https://crates.io/crates/json-ld) by [Timothée Haudebourg](https://github.com/timothee-haudebourg/json-ld). The algorithms and their spec conformance are upstream's, and the W3C JSON-LD API test suite still governs this repo across the four manifests whose algorithms are implemented (expansion, compaction, flattening, and RDF serialization). The fork changes four things: what you can do with your own Rust types, the dependency stack, the performance profile, and the JSON types you can hand in and get back.

*Changed performance profile* is meant literally and not as a synonym for *faster*: the work was aimed at one workload, and what makes that one faster can make others slower. See [Performance](#performance).

### The motivating workload: an NGSI-LD context broker

This fork was cut while building an [NGSI-LD](https://ngsi-ld.org/) context broker, and that workload shaped every addition. A broker is not a document-conversion tool that runs JSON-LD once. It is a server whose entire request path *is* JSON-LD, and the traffic has a specific, repetitive shape:

- **The data is Rust structs, not JSON.** The broker models the NGSI-LD information model as typed Rust values throughout its core. Serving one of them through the normal path means turning a struct into compact JSON, running the expansion algorithm over it, and arriving at expanded JSON, which builds and tears down a whole JSON document the broker never wanted. [`Expandable`](#expandable) removes both intermediate steps: a type renders directly into its expanded form.
- **The same handful of `@context`s, on every single request.** NGSI-LD traffic overwhelmingly references the core context plus a few domain contexts. Reprocessing them per request is the dominant cost, hence the [context memoization and the synchronous fast path](#performance).
- **Term IRIs are needed as constants.** Broker internals compare and construct terms from the core context constantly. [`vocab!`](#vocab) turns those context files into compile-time constants, so a mistyped term is a compile error rather than an entity that silently fails to match a query.
- **The HTTP layer speaks `serde_json` or `sonic-rs`; the algorithms need neither.** The algorithms run over a JSON value that keeps every entry as written, a repeated key included, so the processor sees exactly what the document holds. `serde_json::Map` keeps one value per key, and yet a web server is built on it. So the [conversions live at the boundary](#json-interop), explicit and documented where they lose information, rather than one value type being imposed everywhere.

The NGSI-LD information model left fingerprints on the derive vocabulary too: `flatten_map` for attribute maps keyed by term, `fragment` and `nested` for the Property/Relationship value objects, `container = "language"` for LanguageProperty, `coerce = "@vocab"` for VocabProperty, and CURIE prefixes that tolerate hyphenated names (`prefix(ngsi-ld = "...")`).

None of this is NGSI-LD-specific in its API. The crate has no NGSI-LD types in it and knows nothing about the standard; it is a general JSON-LD library whose ergonomics were chosen by a demanding consumer.

### Working with your own types

Two additions have no upstream counterpart.

#### `Expandable`

`#[derive(Expandable)]` generates expanded JSON-LD for a Rust type, skipping the intermediate JSON value and the expansion algorithm entirely:

```rust
use jsonld::Expandable;

#[derive(Expandable)]
#[jsonld(type = "https://example.com/Parent")]
pub struct Parent {
    #[jsonld(id)]
    pub id: String,

    #[jsonld(property = "https://example.com/name")]
    pub name: String,

    #[jsonld(flatten_map)]
    pub subs: BTreeMap<String, Sub>,
}

let expanded: serde_json::Value = parent.expand();
```

Container attributes cover `type`, `fragment` and `prefix(...)`; field attributes cover `id`, `type_value`, `property`, `coerce` (`@id`, `@vocab`, `@json` or a datatype IRI), `container` (`list`, `set`, `language`, `index`), `nested`, `vec`, `flatten`, `flatten_map`, `passthrough` and `skip`. The full reference is on the [`Expandable` derive](https://docs.rs/jsonld-expandable).

The derive is backend-agnostic: `expand()` is generic over the `JsonValue` trait, so one type renders into `serde_json::Value`, `sonic_rs::Value` or `jstrict::Value` depending on which backend feature is enabled, and `expandable-chrono` lets `chrono` temporal types take part without a newtype wrapper. Generic structs are not supported yet.

#### `vocab!`

`jsonld-vocab` reads your context files at compile time and generates typed constants from them:

```rust
mod vocab {
    jsonld::vocab! {
        contexts: ["contexts/core.jsonld"]
    }
}

// `Iri<&'static str>`, resolved at compile time.
let iri = vocab::expanded::properties::CREATED_AT;
```

Terms are partitioned into `classes` (TitleCase) and `properties` (lowerCase) submodules, so a context declaring both `Property` and `property` does not collide. Paths resolve against `CARGO_MANIFEST_DIR`, and the mappings between compact and expanded names are emitted alongside the constants in both directions, so a term resolves to its IRI without a runtime context lookup. Wrap each invocation in its own module, since the macro emits fixed names.

That partition is a naming convention, not a rule from the JSON-LD specification, and it is the convention NGSI-LD follows: a term whose first character is uppercase becomes a class, everything else becomes a property. A context that names things some other way still gets a constant for every term and complete lookup tables, but the two module names stop describing what is inside them, and a context whose terms have no cased characters at all, CJK names for instance, puts everything under `properties`.

Casing is more than cosmetic in one place. A constant name is the term in SHOUTY_SNAKE_CASE, so two terms that differ only in how they mark word boundaries collapse onto the same name and the macro refuses to generate: `createdAt` and `created_at` both want to be `CREATED_AT`, which fails with `property term naming collision: createdAt and created_at both map to CREATED_AT`. Splitting classes from properties keeps `Property` and `property` apart, but it cannot separate two terms from the same side of the split. A context that mixes camelCase and snake_case for the same concept therefore cannot be used with `vocab!`, and one that is internally consistent about either style is fine.

The constants are `iri-rs` types built by that crate's `iri!` macro, which resolves `iri-rs` against your crate's manifest as it expands, so `vocab!` needs `iri-rs = "4"` among your own dependencies. The `vocab` feature turns on the `static` feature that `iri!` lives behind.

### A different dependency stack

Upstream builds on `rdf-types`, `xsd-types`, `iref`, `static-iref` and `json-syntax`. This fork retargets every one of them:

| Upstream | Here | Why |
| --- | --- | --- |
| `rdf-types` | [`rdfx`](https://crates.io/crates/rdfx) | RDF 1.2 model: triple terms, directional language strings |
| `json-syntax` | [`jstrict`](https://crates.io/crates/jstrict) | strict JSON parsing and canonicalization |
| `iref` / `static-iref` | [`iri-rs`](https://crates.io/crates/iri-rs) | allocation-conscious IRI parsing, compile-time literals |
| `xsd-types` | [`xsd-rs`](https://crates.io/crates/xsd-rs) | SIMD-backed `base64Binary` / `hexBinary` |
| `linked-data` | [`ld-core`](https://crates.io/crates/ld-core) | follows the same stack |

The crate names moved with the dependencies: `json-ld` became `jsonld`, and every member crate followed.

The workspace is Rust 2024 with MSRV 1.96, uses `mediatype` for content negotiation instead of hand-rolled parsing, and carries a reworked error surface.

### Performance

The performance work concentrates on context processing and compaction, the parts of JSON-LD that dominate a broker-shaped workload. It is a re-tuning for that shape of traffic, not a general speedup, and some of it is a trade rather than a win. Memoization spends memory and adds bookkeeping that only pays off when the same contexts and terms recur, which is the defining feature of broker traffic and not of a batch job converting a large corpus of one-off documents. Expect regressions on workloads that look unlike NGSI-LD, and measure before assuming this fork is the faster choice for yours.

What was done:

- **Context memoization.** Expanding a document processes each distinct `@context` in it once, however many nodes repeat it, and a `ProcessingCache` passed to `process_with_cache` carries processed contexts from one document to the next. A synchronous fast path skips the async machinery entirely when a context needs no remote loads.
- **Inverse contexts.** The inverse context is rebuilt lazily instead of on every mutation, and its construction is cheaper.
- **Inline term keys.** A term key of up to 23 bytes, nearly every term of a real context, is stored inline, so creating, cloning and dropping one allocates nothing and touches no shared counter; a longer key is reference-counted. There is no process-wide interner: a key is freed with the context that holds it, so contexts from untrusted input do not grow memory for the life of the process.
- **Compaction.** Compact IRI results are memoized on the active context, up to a fixed number of entries, the prefix list is built once per context instead of on each iteration, and sort comparators no longer materialise strings through the vocabulary.
- **Hashing.** SipHash gave way to hashbrown's default hasher, with `ahash` and `gxhash` as opt-in alternatives.
- **Allocation.** Contexts are borrowed rather than cloned on hot paths, and clones were stripped out of expansion and compaction.

Criterion benchmarks for expansion and compaction live in `jsonld/benches`:

```sh
cargo bench -p jsonld
```

These are a development tool, not evidence. They were written to answer narrow questions while making the changes above, of the form "did this patch move this one workload", and each measures a fixed document against a fixed context. Numbers from a micro-benchmark on a synthetic input are not something to draw conclusions from, so no fork-versus-upstream figures are published here and none should be inferred from the benchmarks themselves.

Credit and history are preserved; see [Attribution](#attribution).

## JSON interop

Documents are represented as `jstrict::Value`, whose objects keep their entries in the order they were written, a repeated key included. JSON-LD 1.1 requires the keys of an object to be unique (section 9), and keeping a repeated key is what lets the processor see one: a context that defines a term twice is refused with `duplicate key`, and a node object that names a property twice keeps the values of both. Two features bridge to the common alternatives without manual unpacking:

| Feature | In | Out |
| --- | --- | --- |
| `serde-json` | `RemoteDocument::from_serde_json` | `into_serde_json` on the compacted/flattened `Value` and on `ExpandedDocument` |
| `sonic-rs` | `RemoteDocument::from_sonic_rs` | `into_sonic_rs`, same places |

`sonic_rs::Object` keeps entry order and repeated keys, so reading from `sonic-rs` loses nothing, and writing to it keeps the order but leaves a repeated key once, with its last value. `serde_json::Map` holds one value per key, the last one read, and sorts its keys unless `serde_json`'s `preserve_order` feature is on, so a conversion in either direction drops repeated keys and, by default, the entry order.

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
| `jsonld-cli` | Command line interface: `fetch`, `expand`, `compact`, `flatten` |

`jsonld-testing`, the W3C suite harness, is workspace-internal and not published.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `ahash` | | `ahash` instead of the default hasher |
| `gxhash` | | `gxhash` instead of the default hasher; needs AES intrinsics and fails to build without them |
| `serde` | | `Serialize` / `Deserialize` for syntax and core types |
| `serde-json` | | `serde_json` interop |
| `sonic-rs` | | `sonic-rs` interop |
| `reqwest` | | HTTP document loader; needs a `tokio` runtime |
| `expandable` | | the `Expandable` derive (no JSON backend on its own) |
| `expandable-serde-json`, `-sonic-rs`, `-jstrict` | | output backend for `Expandable` |
| `expandable-chrono` | | `chrono` temporal types in `Expandable` types |
| `vocab` | | the `vocab!` macro |

IRIs are compared as strings, as JSON-LD and RDF require: [IRI Expansion](https://www.w3.org/TR/json-ld11-api/#iri-expansion) performs "neither Syntax-Based Normalization nor Scheme-Based Normalization", the node map is keyed by the `@id` string, and RDF 1.1 Concepts §3.2 says "Further normalization MUST NOT be performed when comparing IRIs for equality". That is what `==`, `Hash` and `Ord` on `iri-rs` 4 types do (RFC 3987 §5.3.1 simple string comparison), so `http://example.org/~a` and `http://example.org/%7Ea` are two nodes, each keeping its spelling. There is no flag that changes this. A caller whose own protocol asks for normalized comparison, such as NGSI-LD, uses `iri-rs`'s `equivalent()` or `normalized()` at its own boundary.

## Conformance

The W3C JSON-LD API test suite is a git submodule. A fresh clone needs it before the conformance tests will run:

```sh
git submodule update --init
cargo test -p jsonld
```

The submodule is pinned to [`92f0770`](https://github.com/w3c/json-ld-api/commit/92f07705a0c0ac27aa9bc6fe1322dcc9fad0114d) (2026-07-01), so a fresh clone reproduces the same corpus.

Four of the seven manifests are in scope. Every test that runs passes; four are skipped, three against open upstream issues and `#t0038` because the working group replaced the rule it tests (its JSON-LD 1.1 twin `#ta038` runs and passes):

| Manifest | In manifest | Run | Ignored |
| --- | --: | --: | --- |
| `expand` | 385 | 385 | none |
| `compact` | 246 | 244 | `#tp004`, `#t0038` |
| `flatten` | 58 | 58 | none |
| `toRdf` | 467 | 465 | `#te122`, `#tli12` |

Ignored tests are declared with `#[ignore_test(..., see = ...)]` in `jsonld/tests/`, each linking the upstream discussion that justifies it.

Tests carrying `specVersion: json-ld-1.0` run in `ProcessingMode::JsonLd1_0` rather than being skipped, so the 1.0 code path is covered too.

A negative test, one that expects an error, checks that processing fails, but not yet that it fails with the error code the manifest names. That covers 233 of the tests above.

The remaining three manifests have no suite here, because the algorithms they exercise are not implemented: `fromRdf` (54 tests, RDF-to-JSON-LD deserialization), `remote-doc` (18 tests, HTTP content negotiation), and `html` (50 tests, HTML `script` extraction).

## Stability

What conformance does *not* cover, and what "experimental" means in practice:

- **The API will break.** Names, error types and trait shapes are still being shaped by the consumer that motivated the fork. Pin an exact version.
- **The additions are new.** `Expandable` and `vocab!` are covered by their own unit and UI tests, not by any external suite, and their attribute vocabularies will grow.
- **Three algorithms are absent.** `fromRdf`, HTML script extraction and HTTP content negotiation. If you need RDF-to-JSON-LD deserialization, this is not the crate.
- **Performance is tuned, not uniformly better.** The work targeted a context-broker workload, so expect regressions on traffic that looks unlike it, and measure your own corpus before assuming this fork is the faster choice. See [Performance](#performance).
- **Untrusted input deserves care.** The default hasher is fast but not resistant to hash flooding, so reach for `ahash` if that matters, and a `ProcessingCache` never evicts, so a hostile stream of distinct contexts through one grows memory until it is cleared.

## MSRV

Rust 1.96 (edition 2024).

## Attribution

Original crates: [`json-ld`](https://crates.io/crates/json-ld) and its members by [Timothée Haudebourg](https://github.com/timothee-haudebourg/json-ld). Upstream commits are preserved in this repo's history under their original authorship, and the bulk of this repository remains their work. This fork is a dependency-stack swap, a layer of performance work, and the `Expandable` and `jsonld-vocab` additions on top of their implementation.

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
