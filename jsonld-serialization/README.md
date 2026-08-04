# jsonld-serialization

[![Crate](https://img.shields.io/crates/v/jsonld-serialization.svg?style=flat-square)](https://crates.io/crates/jsonld-serialization)
[![Docs](https://img.shields.io/docsrs/jsonld-serialization?style=flat-square)](https://docs.rs/jsonld-serialization)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-serialization?style=flat-square)](https://crates.io/crates/jsonld-serialization)
[![License](https://img.shields.io/crates/l/jsonld-serialization.svg?style=flat-square)](#license)

Serializes an RDF dataset into JSON-LD, which is the inverse of the `to_rdf` direction implemented in [`jsonld-core`](https://crates.io/crates/jsonld-core). Any type implementing `LinkedData`, including types that derive it, becomes an expanded JSON-LD document.

Blank nodes are resolved through the interpretation the dataset was built against, so a resource seen twice yields one node object rather than two.

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
