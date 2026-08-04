# jsonld-compaction

[![Crate](https://img.shields.io/crates/v/jsonld-compaction.svg?style=flat-square)](https://crates.io/crates/jsonld-compaction)
[![Docs](https://img.shields.io/docsrs/jsonld-compaction?style=flat-square)](https://docs.rs/jsonld-compaction)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-compaction?style=flat-square)](https://crates.io/crates/jsonld-compaction)
[![License](https://img.shields.io/crates/l/jsonld-compaction.svg?style=flat-square)](#license)

The [JSON-LD compaction algorithms](https://www.w3.org/TR/json-ld11-api/#compaction-algorithms). Compaction turns an expanded document back into the terse shape a `@context` describes: IRIs become terms or compact IRIs, single-element arrays collapse, and value objects reduce to plain JSON scalars wherever the context makes that lossless.

A whole document can be compacted at once, embedding the context it was compacted against, or a single fragment can be compacted on its own.

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
