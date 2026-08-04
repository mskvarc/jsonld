//! Core types shared by the JSON-LD processing algorithms.
//!
//! This crate defines the data model the algorithm crates operate on — the
//! processed [`Context`], the [`Object`] tree an expanded document is made
//! of, [`ExpandedDocument`] and [`FlattenedDocument`], and the [`Loader`]
//! trait used to dereference remote contexts — together with the conversions
//! to RDF ([`rdf`]) and the pretty-printer ([`mod@print`]).
//!
//! Identifiers are generic over the vocabulary that interns them: `T` is an
//! IRI handle and `B` a blank node identifier handle, defaulting to owned
//! [`iri_rs::IriBuf`] and [`rdfx::BlankIdBuf`] values.
//!
//! Most users should depend on the `jsonld` umbrella crate rather than on
//! this one directly; it re-exports everything here alongside the algorithms.
// On docs.rs, label every feature-gated item with the feature that unlocks it.
// `doc(auto_cfg)` is still nightly-gated, and `docsrs` is set by docs.rs itself
// (see `rustdoc-args` in Cargo.toml), so stable builds are unaffected.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]
pub use jsonld_syntax::{Direction, LenientLangTag, LenientLangTagBuf, Nullable};

mod container;
pub mod context;
mod deserialization;
mod document;
pub mod flattening;
mod hash;
/// Node identifiers, valid or not.
pub mod id;
mod indexed;
mod lang_string;
/// Document loaders.
pub mod loader;
mod mode;
pub mod object;
/// Pretty-printing of JSON-LD documents.
pub mod print;
/// RDF quads of an expanded document.
pub mod quad;
pub mod rdf;
mod serialization;
pub use serialization::SerializationError;
mod term;
mod ty;
pub mod utils;
/// Warnings raised by the algorithms.
pub mod warning;

pub use container::{Container, ContainerKind};
pub use context::{Context, ContextRef};
pub use document::*;
pub use flattening::Flatten;
pub use hash::{DefaultBuildHasher, HashMap, HashSet, IndexMap, IndexSet};
pub use id::*;
pub use indexed::*;
pub use lang_string::*;
pub use loader::*;
pub use mode::*;
pub use object::{IndexedNode, IndexedObject, Node, Nodes, Object, Objects, TryFromJson, Value};
pub use print::Print;
pub use quad::LdQuads;
pub use rdf::RdfQuads;
pub use term::*;
pub use ty::*;

/// Vocabulary, loader and warning handler an algorithm runs against.
pub struct Environment<'a, N, L, W> {
    /// Vocabulary interning IRIs and blank node identifiers.
    pub vocabulary: &'a mut N,
    /// Loader used to dereference remote documents and contexts.
    pub loader: &'a L,
    /// Handler collecting the warnings raised along the way.
    pub warnings: &'a mut W,
}
