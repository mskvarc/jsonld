//! JSON-LD core types.
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
