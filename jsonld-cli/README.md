# jsonld-cli

[![Crate](https://img.shields.io/crates/v/jsonld-cli.svg?style=flat-square)](https://crates.io/crates/jsonld-cli)
[![Docs](https://img.shields.io/docsrs/jsonld-cli?style=flat-square)](https://docs.rs/jsonld-cli)
[![MSRV](https://img.shields.io/crates/msrv/jsonld-cli?style=flat-square)](https://crates.io/crates/jsonld-cli)
[![License](https://img.shields.io/crates/l/jsonld-cli.svg?style=flat-square)](#license)

Command line interface to [`jsonld`](https://crates.io/crates/jsonld): expand, compact and flatten JSON-LD documents, or fetch one over HTTP.

```sh
cargo install jsonld-cli

jsonld-cli expand document.jsonld
jsonld-cli compact context.jsonld document.jsonld
jsonld-cli flatten document.jsonld
cat document.jsonld | jsonld-cli expand --base-url https://example.org/
```

Documents are read from a path, an IRI or standard input. Expansion can relabel blank nodes, put the result in canonical form, and report terms it had to guess at through `@vocab` or leave undefined. Remote contexts are fetched as needed, so a document that references one needs network access.

> **Status: experimental.** Version numbers move fast and there is no compatibility promise before 1.0. The [workspace README](https://github.com/mskvarc/jsonld#readme) says what that means in practice.

## Feature flags

| Flag | Default | Enables |
| --- | :---: | --- |
| `ahash`, `gxhash` | | a different hasher for the internal collections |

## License

Dual-licensed, same as upstream. Pick whichever fits:

- [Apache-2.0](https://github.com/mskvarc/jsonld/blob/master/LICENSE-APACHE.md)
- [MIT](https://github.com/mskvarc/jsonld/blob/master/LICENSE-MIT.md)
