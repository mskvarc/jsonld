# jsonld-vocab

[![Crate](https://img.shields.io/crates/v/jsonld-vocab.svg?style=flat-square)](https://crates.io/crates/jsonld-vocab)
[![Docs](https://img.shields.io/docsrs/jsonld-vocab?style=flat-square)](https://docs.rs/jsonld-vocab)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-vocab?style=flat-square)](https://crates.io/crates/jsonld-vocab)
[![License](https://img.shields.io/crates/l/jsonld-vocab.svg?style=flat-square)](#license)

A macro that reads local JSON-LD context files at compile time and generates typed IRI constants from them, together with lookup tables in both directions. Terms are partitioned into `classes` and `properties` by their casing, so a context can define both `Property` and `property` without colliding on one Rust name.

```rust
mod vocab {
    jsonld_vocab::generate! {
        contexts: ["contexts/core.jsonld"]
    }
}

let iri: iri_rs::Iri<&'static str> = vocab::expanded::properties::CREATED_AT;
```

Nothing is fetched and nothing is parsed at runtime: the constants are `iri_rs::Iri<&'static str>` values built by `iri-rs`'s own `iri!` macro, which is why the calling crate needs `iri-rs` with its `static` feature as a direct dependency.

The casing convention is the one NGSI-LD and most RDF vocabularies follow rather than anything the specification mandates. A context that names things some other way still gets a constant per term, but the module names stop describing their contents. See [the macro's documentation](https://docs.rs/jsonld-vocab) for that and for the naming collisions to watch out for.

Part of [`jsonld`](https://crates.io/crates/jsonld), which re-exports this macro as `jsonld::vocab!` behind its `vocab` feature.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
