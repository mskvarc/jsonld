# jsonld-syntax

[![Crate](https://img.shields.io/crates/v/jsonld-syntax.svg?style=flat-square)](https://crates.io/crates/jsonld-syntax)
[![Docs](https://img.shields.io/docsrs/jsonld-syntax?style=flat-square)](https://docs.rs/jsonld-syntax)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-syntax?style=flat-square)](https://crates.io/crates/jsonld-syntax)
[![License](https://img.shields.io/crates/l/jsonld-syntax.svg?style=flat-square)](#license)

Rust types mirroring the JSON-LD grammar of a `@context`: context definitions, term definitions, keywords, container mappings, language tags and text directions. Each one parses from a JSON value, converts back, and pretty-prints.

This crate is only the syntax. Interpreting a context, which means resolving remote references and expanding terms into IRIs, is the job of [`jsonld-context-processing`](https://crates.io/crates/jsonld-context-processing).

Part of [`jsonld`](https://crates.io/crates/jsonld), a [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust. That crate re-exports this one as `jsonld::syntax`.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `bytewise-iri` | yes | byte-wise IRI comparison, hashing and ordering in `iri-rs` |
| `ahash`, `gxhash` | | a different hasher for the internal collections |
| `serde` | | `Serialize` and `Deserialize` for the syntax types |
| `serde-json`, `sonic-rs` | | those crates as the JSON value backend |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
