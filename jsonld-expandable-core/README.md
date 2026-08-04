# jsonld-expandable-core

[![Crate](https://img.shields.io/crates/v/jsonld-expandable-core.svg?style=flat-square)](https://crates.io/crates/jsonld-expandable-core)
[![Docs](https://img.shields.io/docsrs/jsonld-expandable-core?style=flat-square)](https://docs.rs/jsonld-expandable-core)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-expandable-core?style=flat-square)](https://crates.io/crates/jsonld-expandable-core)
[![License](https://img.shields.io/crates/l/jsonld-expandable-core.svg?style=flat-square)](#license)

Runtime support for the [`Expandable`](https://crates.io/crates/jsonld-expandable) derive: the `Expandable` trait itself, the `JsonValue` backend trait that lets generated code target any JSON value type, and the `ToJsonValue` conversions for leaf field values. Language maps and dynamic `@type` arrays have their own traits, implemented here for the ordinary map shapes.

Three backends ship behind features: `serde_json`, `sonic-rs` and `jstrict`. A fourth is whatever you implement `JsonValue` for.

Application code rarely names this crate. Enable an `expandable*` feature on [`jsonld`](https://crates.io/crates/jsonld) instead, which re-exports both this crate and the derive.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `serde-json`, `sonic-rs`, `jstrict` | | `JsonValue` for that crate's value type |
| `chrono` | | `chrono` temporal types as field values |
| `codegen` | | the attribute parser and code generator, which only the derive crate needs |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
