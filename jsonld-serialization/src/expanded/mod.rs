use jsonld_core::{ExpandedDocument, Indexed, Node, Object};
use ld_core::{CowRdfTerm, OwnedRdfTerm};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};
use std::hash::Hash;

use crate::Error;

mod default_graph;
mod graph;
mod list;
mod node;
mod object;
mod property;
mod value;

use default_graph::SerializeDefaultGraph;
use graph::SerializeGraph;

pub use node::serialize_node_with;
pub use object::serialize_object_with;

pub struct SerializeExpandedDocument<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: ExpandedDocument<V::Iri, V::BlankId>,
}

impl<'a, I, V: Vocabulary> SerializeExpandedDocument<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            result: ExpandedDocument::new(),
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::Visitor<I> for SerializeExpandedDocument<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = ExpandedDocument<V::Iri, V::BlankId>;
    type Error = Error;

    fn default_graph<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ld_core::LinkedDataGraph<I>,
    {
        let serializer = SerializeDefaultGraph::new(self.vocabulary, self.interpretation, &mut self.result);

        value.visit_graph(serializer)
    }

    fn named_graph<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ld_core::LinkedDataResource<I> + ld_core::LinkedDataGraph<I>,
    {
        let mut node = match value.lexical_representation(self.interpretation).map(CowRdfTerm::into_owned) {
            Some(OwnedRdfTerm::Literal(_)) => return Err(Error::InvalidGraph),
            Some(OwnedRdfTerm::Iri(iri)) => Node::with_id(jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(self.vocabulary.insert_owned(iri)))),
            Some(OwnedRdfTerm::BlankId(b)) => Node::with_id(jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(self.vocabulary.insert_owned_blank_id(b)))),
            None => Node::new(),
        };

        let serializer = SerializeGraph::new(self.vocabulary, self.interpretation);

        let graph = value.visit_graph(serializer)?;

        node.graph = Some(graph);
        self.result.insert(Indexed::new(Object::node(node), None));

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}
