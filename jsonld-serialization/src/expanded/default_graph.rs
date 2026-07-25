use std::hash::Hash;

use jsonld_core::{ExpandedDocument, Indexed, Object};
use ld_core::{CowRdfTerm, LinkedDataResource, OwnedRdfTerm};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

use crate::Error;

use super::{node::SerializeNode, value::literal_to_value};

pub struct SerializeDefaultGraph<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: &'a mut ExpandedDocument<V::Iri, V::BlankId>,
}

impl<'a, I, V: Vocabulary> SerializeDefaultGraph<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I, result: &'a mut ExpandedDocument<V::Iri, V::BlankId>) -> Self {
        Self {
            vocabulary,
            interpretation,
            result,
        }
    }
}

impl<'a, I: Interpretation, V: Vocabulary> ld_core::GraphVisitor<I> for SerializeDefaultGraph<'a, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = ();
    type Error = Error;

    fn subject<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let id = match value.lexical_representation(self.interpretation).map(CowRdfTerm::into_owned) {
            Some(OwnedRdfTerm::Literal(lit)) => {
                let value = literal_to_value(self.vocabulary, lit);
                self.result.insert(Indexed::new(Object::Value(value), None));
                return Ok(());
            }
            Some(OwnedRdfTerm::Iri(iri)) => Some(jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(self.vocabulary.insert_owned(iri)))),
            Some(OwnedRdfTerm::BlankId(b)) => Some(jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(self.vocabulary.insert_owned_blank_id(b)))),
            _ => None,
        };

        let serializer = SerializeNode::new(self.vocabulary, self.interpretation, id);

        let node = value.visit_subject(serializer)?;
        self.result.insert(Indexed::new(Object::node(node), None));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
