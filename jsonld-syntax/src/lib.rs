//! This library provide functions to parse JSON-LD contexts
//! and print JSON-LD documents.
mod compact_iri;
mod compare;
/// Container mappings.
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
