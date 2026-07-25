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
mod term;
mod ty;
pub mod utils;
/// Warnings raised by the algorithms.
pub mod warning;

pub use container::{Container, ContainerKind};
pub use context::Context;
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

/// Marker trait for vocabularies that may be cloned and shared across
/// concurrent expansion and compaction tasks.
///
/// When `parallel` is off, every type satisfies this trait via a blanket
/// impl — code that bounds on it still compiles in default builds.
///
/// When `parallel` is on, the bound widens to `Send + Sync + Clone`,
/// because the concurrent paths hand each task its own copy of the
/// vocabulary. A `&mut V` therefore does *not* qualify: with this feature
/// enabled the algorithms must be given an owned vocabulary.
#[cfg(not(feature = "parallel"))]
pub trait ParallelSafeVocabulary {}

#[cfg(not(feature = "parallel"))]
impl<T: ?Sized> ParallelSafeVocabulary for T {}

#[cfg(feature = "parallel")]
/// Vocabularies usable from the parallel algorithms.
///
/// With the `parallel` feature this demands `Send + Sync + Clone`, since
/// each task works on its own clone; without it, every vocabulary
/// qualifies.
pub trait ParallelSafeVocabulary: Send + Sync + Clone {}

#[cfg(feature = "parallel")]
impl<T: Send + Sync + Clone> ParallelSafeVocabulary for T {}
