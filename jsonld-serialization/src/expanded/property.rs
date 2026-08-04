use std::hash::Hash;

use jsonld_core::{Indexed, IndexedNode, IndexedObject, object::node::Multiset};
use ld_core::LinkedDataResource;
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

use crate::Error;

use super::{object::serialize_object_with, serialize_node_with};

pub struct SerializeProperty<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: Multiset<IndexedObject<V::Iri, V::BlankId>>,
}

impl<'a, I, V: Vocabulary> SerializeProperty<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            result: Multiset::new(),
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::PredicateObjectsVisitor<I> for SerializeProperty<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Multiset<IndexedObject<V::Iri, V::BlankId>>;
    type Error = Error;

    fn object<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let object = serialize_object_with(self.vocabulary, self.interpretation, value)?;
        self.result.insert(Indexed::none(object));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}

pub struct SerializeReverseProperty<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: Multiset<IndexedNode<V::Iri, V::BlankId>>,
}

impl<'a, I, V: Vocabulary> SerializeReverseProperty<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            result: Multiset::new(),
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::PredicateObjectsVisitor<I> for SerializeReverseProperty<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Multiset<IndexedNode<V::Iri, V::BlankId>>;
    type Error = Error;

    fn object<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let object = serialize_node_with(self.vocabulary, self.interpretation, value)?;
        self.result.insert(Indexed::none(object));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}
