//! This crate implements JSON-LD serialization (from RDF dataset to JSON-LD)
//! through the [`linked_data`](https://github.com/spruceid/linked-data-rs)
//! crate.
//! The input value can be an RDF dataset, or any type implementing
//! [`ld_core::LinkedData`].
use std::hash::Hash;

use jsonld_core::{ExpandedDocument, Node, Object};

use ld_core::{LinkedData, LinkedDataResource, LinkedDataSubject};
use rdf_rs::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

mod expanded;

use expanded::SerializeExpandedDocument;

pub use expanded::{serialize_node_with, serialize_object_with};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid graph label")]
    InvalidGraph,

    #[error("invalid predicate")]
    InvalidPredicate,

    #[error("invalid node object")]
    InvalidNode,

    #[error("reverse properties on lists are not supported")]
    ListReverseProperty,

    #[error("included nodes on lists are not supported")]
    ListInclude,
}

type DefaultInterpretation = rdf_rs::generator::LocalGeneratorInterpretation<rdf_rs::generator::Blank>;

fn default_interpretation() -> DefaultInterpretation {
    rdf_rs::generator::LocalGeneratorInterpretation::new(rdf_rs::generator::Blank::new())
}

/// Serialize the given Linked-Data value into a JSON-LD document.
pub fn serialize(value: &impl LinkedData<DefaultInterpretation>) -> Result<ExpandedDocument, Error> {
    serialize_with(&mut (), &mut default_interpretation(), value)
}

/// Serialize the given Linked-Data value into a JSON-LD document using a
/// custom vocabulary and interpretation.
pub fn serialize_with<V, I>(vocabulary: &mut V, interpretation: &mut I, value: &impl LinkedData<I>) -> Result<ExpandedDocument<V::Iri, V::BlankId>, Error>
where
    V: Vocabulary + rdf_rs::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: Interpretation + ReverseInterpretation + ReverseLocalInterpretation,
{
    let serializer = SerializeExpandedDocument::new(vocabulary, interpretation);

    value.visit(serializer)
}

/// Serialize the given Linked-Data value into a JSON-LD object.
pub fn serialize_object(value: &(impl LinkedDataSubject<DefaultInterpretation> + LinkedDataResource<DefaultInterpretation>)) -> Result<Object, Error> {
    serialize_object_with(&mut (), &mut default_interpretation(), value)
}

/// Serialize the given Linked-Data value into a JSON-LD node object.
pub fn serialize_node(value: &(impl LinkedDataSubject<DefaultInterpretation> + LinkedDataResource<DefaultInterpretation>)) -> Result<Node, Error> {
    serialize_node_with(&mut (), &mut default_interpretation(), value)
}
