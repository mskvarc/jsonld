# jsonld-expandable

[![Crate](https://img.shields.io/crates/v/jsonld-expandable.svg?style=flat-square)](https://crates.io/crates/jsonld-expandable)
[![Docs](https://img.shields.io/docsrs/jsonld-expandable?style=flat-square)](https://docs.rs/jsonld-expandable)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-expandable?style=flat-square)](https://crates.io/crates/jsonld-expandable)
[![License](https://img.shields.io/crates/l/jsonld-expandable.svg?style=flat-square)](#license)

The `Expandable` derive macro: it generates expanded JSON-LD straight from your own Rust structs, with no intermediate `serde_json::Value` and no context processing at runtime. Field attributes cover `@id`, `@type`, typed and IRI coercions, list, set, language and index containers, nesting, flattening and more.

```rust
#[derive(Expandable)]
#[jsonld(type = "ex:Sensor", prefix(ex = "https://example.com/"))]
struct Sensor {
    #[jsonld(id)]
    id: String,
    #[jsonld(property = "ex:temperature", coerce = "xsd:double")]
    temperature: f64,
}

let doc: serde_json::Value = sensor.expand();
```

This crate is the proc-macro shell. The traits the generated code calls live in [`jsonld-expandable-core`](https://crates.io/crates/jsonld-expandable-core), and one of the two has to be a dependency of the crate that derives: either the umbrella [`jsonld`](https://crates.io/crates/jsonld) with an `expandable` feature, which re-exports both, or `jsonld-expandable-core` directly. The derive reads your manifest to work out which, and follows a renamed dependency to the name you gave it.

The full attribute reference is in [the derive's documentation](https://docs.rs/jsonld-expandable).

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
