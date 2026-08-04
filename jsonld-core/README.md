# jsonld-core

[![Crate](https://img.shields.io/crates/v/jsonld-core.svg?style=flat-square)](https://crates.io/crates/jsonld-core)
[![Docs](https://img.shields.io/docsrs/jsonld-core?style=flat-square)](https://docs.rs/jsonld-core)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-core?style=flat-square)](https://crates.io/crates/jsonld-core)
[![License](https://img.shields.io/crates/l/jsonld-core.svg?style=flat-square)](#license)

The data model the JSON-LD algorithms operate on: the processed context, the object tree an expanded document is made of, `ExpandedDocument` and `FlattenedDocument`, and the `Loader` trait that dereferences remote contexts. Conversion to RDF and the pretty-printer live here too.

Identifiers are generic over the vocabulary that interns them, so a document can carry owned IRIs or compact handles into an interning vocabulary without the algorithms knowing the difference.

Part of [`jsonld`](https://crates.io/crates/jsonld), a [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) implementation for Rust. That crate re-exports everything here and is what you usually want; depend on this one directly when you need the model and the loaders without the algorithm crates.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `bytewise-iri` | yes | byte-wise IRI comparison, hashing and ordering in `iri-rs` |
| `ahash`, `gxhash` | | a different hasher for the internal collections |
| `serde` | | `Serialize` and `Deserialize` for the core types |
| `serde-json`, `sonic-rs` | | conversion to and from those crates' value types |
| `reqwest` | | the HTTP document loader, which needs a `tokio` runtime |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
