# jsonld-context-processing

[![Crate](https://img.shields.io/crates/v/jsonld-context-processing.svg?style=flat-square)](https://crates.io/crates/jsonld-context-processing)
[![Docs](https://img.shields.io/docsrs/jsonld-context-processing?style=flat-square)](https://docs.rs/jsonld-context-processing)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-context-processing?style=flat-square)](https://crates.io/crates/jsonld-context-processing)
[![License](https://img.shields.io/crates/l/jsonld-context-processing.svg?style=flat-square)](#license)

The [JSON-LD context processing algorithm](https://www.w3.org/TR/json-ld11-api/#context-processing-algorithms). It turns a `@context` as written in a document into an active context: the term definitions, base IRI, vocabulary mapping, default language and default base direction that expansion and compaction consult. Remote contexts are fetched through a loader, and `@import` is resolved by merging the imported definition underneath the importing one.

Processed contexts are memoized, so a document that repeats the same context pays for it once.

Part of [`jsonld`](https://crates.io/crates/jsonld), a [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust, which is what you usually want.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `bytewise-iri` | yes | byte-wise IRI comparison, hashing and ordering in `iri-rs` |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
