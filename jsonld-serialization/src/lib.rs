//! Serialization of RDF datasets into JSON-LD.
//!
//! Turns an RDF dataset — or any type implementing [`ld_core::LinkedData`],
//! which includes types deriving it — into a [`jsonld_core::ExpandedDocument`].
//! This is the inverse of the `to_rdf` direction implemented in
//! `jsonld-core`.
//!
//! Blank nodes are resolved through the interpretation the dataset was built
//! against, so the same resource seen twice yields one node object rather than
//! two.
use std::hash::Hash;

use jsonld_core::{ExpandedDocument, Node, Object};

use ld_core::{LinkedData, LinkedDataResource, LinkedDataSubject};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

mod expanded;

use expanded::SerializeExpandedDocument;

pub use expanded::{serialize_node_with, serialize_object_with};

#[derive(Debug, thiserror::Error)]
/// Error raised while serializing a document.
pub enum Error {
    #[error("invalid graph label")]
    /// Invalid graph label.
    InvalidGraph,

    #[error("invalid predicate")]
    /// Invalid predicate.
    InvalidPredicate,

    #[error("invalid node object")]
    /// Invalid node object.
    InvalidNode,

    #[error("reverse properties on lists are not supported")]
    /// Reverse properties on lists are not supported.
    ListReverseProperty,

    #[error("included nodes on lists are not supported")]
    /// Included nodes on lists are not supported.
    ListInclude,
}

type DefaultInterpretation = rdfx::generator::LocalGeneratorInterpretation<rdfx::generator::Blank>;

fn default_interpretation() -> DefaultInterpretation {
    rdfx::generator::LocalGeneratorInterpretation::new(rdfx::generator::Blank::new())
}

/// Serialize the given Linked-Data value into a JSON-LD document.
///
/// # Errors
///
/// Returns an error when the value cannot be expressed as a JSON-LD document.
pub fn serialize(value: &impl LinkedData<DefaultInterpretation>) -> Result<ExpandedDocument, Error> {
    serialize_with(&mut (), &mut default_interpretation(), value)
}

/// Serialize the given Linked-Data value into a JSON-LD document using a
/// custom vocabulary and interpretation.
///
/// # Errors
///
/// Returns an error when the value cannot be expressed as a JSON-LD document.
pub fn serialize_with<V, I>(vocabulary: &mut V, interpretation: &mut I, value: &impl LinkedData<I>) -> Result<ExpandedDocument<V::Iri, V::BlankId>, Error>
where
    V: Vocabulary + rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: Interpretation + ReverseInterpretation + ReverseLocalInterpretation,
{
    let serializer = SerializeExpandedDocument::new(vocabulary, interpretation);

    value.visit(serializer)
}

/// Serialize the given Linked-Data value into a JSON-LD object.
///
/// # Errors
///
/// Returns an error when the value cannot be expressed as a JSON-LD object.
pub fn serialize_object(value: &(impl LinkedDataSubject<DefaultInterpretation> + LinkedDataResource<DefaultInterpretation>)) -> Result<Object, Error> {
    serialize_object_with(&mut (), &mut default_interpretation(), value)
}

/// Serialize the given Linked-Data value into a JSON-LD node object.
///
/// # Errors
///
/// Returns an error when the value cannot be expressed as a JSON-LD node object.
pub fn serialize_node(value: &(impl LinkedDataSubject<DefaultInterpretation> + LinkedDataResource<DefaultInterpretation>)) -> Result<Node, Error> {
    serialize_node_with(&mut (), &mut default_interpretation(), value)
}
