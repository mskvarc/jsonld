# jsonld-expansion

[![Crate](https://img.shields.io/crates/v/jsonld-expansion.svg?style=flat-square)](https://crates.io/crates/jsonld-expansion)
[![Docs](https://img.shields.io/docsrs/jsonld-expansion?style=flat-square)](https://docs.rs/jsonld-expansion)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-expansion?style=flat-square)](https://crates.io/crates/jsonld-expansion)
[![License](https://img.shields.io/crates/l/jsonld-expansion.svg?style=flat-square)](#license)

The [JSON-LD 1.1 expansion algorithm](https://www.w3.org/TR/json-ld11-api/#expansion-algorithms). Expansion rewrites a document into a regular form: terms and compact IRIs become full IRIs, the defaults a context sets are applied to every value, and the `@context` entries disappear. Almost every other JSON-LD algorithm starts here, because expansion removes the many ways the syntax offers to say the same thing.

Part of [`jsonld`](https://crates.io/crates/jsonld), a [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust. Reach for the umbrella crate unless you specifically want expansion on its own.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `bytewise-iri` | yes | byte-wise IRI comparison, hashing and ordering in `iri-rs` |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
