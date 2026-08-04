//! Syntax of JSON-LD contexts.
//!
//! This crate provides the Rust types mirroring the JSON-LD grammar of a
//! `@context`: context definitions, term definitions, keywords, container
//! mappings, language tags and text directions. Each of them can be built
//! from a JSON value with [`TryFromJson`], converted back with [`IntoJson`],
//! and pretty-printed with the [`Print`] trait re-exported from `jstrict`.
//!
//! Interpreting a context — resolving remote references, expanding terms into
//! IRIs, compacting IRIs back into terms — is the job of the sibling
//! `jsonld-context-processing`, `jsonld-expansion` and `jsonld-compaction`
//! crates.
// On docs.rs, label every feature-gated item with the feature that unlocks it.
// `doc(auto_cfg)` is still nightly-gated, and `docsrs` is set by docs.rs itself
// (see `rustdoc-args` in Cargo.toml), so stable builds are unaffected.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
mod compact_iri;
mod compare;
/// Container mappings: the values a `@container` entry may take.
pub mod container;
/// Contexts, their definitions and term definitions.
pub mod context;
mod direction;
mod error;
mod expandable;
pub mod hash;
pub mod intern;
mod into_json;
mod keyword;
mod lang;
mod nullable;
mod print_ld;
mod try_from_json;
mod utils;

pub use compact_iri::*;
pub use compare::*;
pub use container::{Container, ContainerKind};
pub use context::{Context, ContextDocument, ContextEntry};
pub use direction::*;
pub use error::*;
pub use expandable::*;
pub use into_json::*;
pub use jstrict::{
    BorrowUnordered,
    Kind,
    Number,
    NumberBuf,
    Object,
    Parse,
    Print,
    String,
    Unordered,
    UnorderedEq,
    UnorderedHash,
    UnorderedPartialEq,
    Value,
    object,
    parse,
    print,
};
pub use keyword::*;
pub use lang::*;
pub use nullable::*;
pub use try_from_json::*;

#[cfg(feature = "serde")]
pub use jstrict::{from_value, to_value};

#[derive(Clone, Copy, Debug)]
/// Error raised when a JSON value has the wrong kind.
pub struct Unexpected(jstrict::Kind, &'static [jstrict::Kind]);

impl Unexpected {
    /// Returns the kind of the value that was found.
    pub fn found(&self) -> jstrict::Kind {
        self.0
    }

    /// Returns the kinds that were expected instead.
    pub fn expected(&self) -> &'static [jstrict::Kind] {
        self.1
    }
}

impl std::fmt::Display for Unexpected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unexpected {}, expected ", self.0)?;
        for (i, kind) in self.1.iter().enumerate() {
            if i > 0 {
                f.write_str(" or ")?;
            }
            write!(f, "{kind}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Unexpected {}
