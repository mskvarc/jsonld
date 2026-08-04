use std::hash::Hash;

use jsonld_core::{
    Indexed,
    IndexedObject,
    Object,
    rdf::{RDF_FIRST, RDF_REST},
};
use ld_core::{CowRdfTerm, LinkedDataResource, OwnedRdfTerm};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

use crate::Error;

use super::object::serialize_object_with;

pub struct SerializeList<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    first: Option<Object<V::Iri, V::BlankId>>,
    rest: Vec<IndexedObject<V::Iri, V::BlankId>>,
}

impl<'a, I, V: Vocabulary> SerializeList<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            first: None,
            rest: Vec::new(),
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::SubjectVisitor<I> for SerializeList<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Vec<IndexedObject<V::Iri, V::BlankId>>;
    type Error = Error;

    fn predicate<L, T>(&mut self, predicate: &L, value: &T) -> Result<(), Self::Error>
    where
        L: ?Sized + LinkedDataResource<I>,
        T: ?Sized + ld_core::LinkedDataPredicateObjects<I>,
    {
        let repr = predicate
            .interpretation(self.interpretation)
            .into_lexical_representation(self.interpretation)
            .map(CowRdfTerm::into_owned);

        match repr {
            Some(OwnedRdfTerm::Iri(iri)) => {
                if iri.as_ref() == RDF_FIRST {
                    let serializer = SerializeListFirst::new(self.vocabulary, self.interpretation);
                    self.first = value.visit_objects(serializer)?;
                } else if iri.as_ref() == RDF_REST {
                    let serializer = SerializeListRest::new(self.vocabulary, self.interpretation);
                    self.rest = value.visit_objects(serializer)?;
                }

                Ok(())
            }
            Some(OwnedRdfTerm::BlankId(_)) => Ok(()),
            _ => Err(Error::InvalidPredicate),
        }
    }

    fn reverse_predicate<L, T>(&mut self, _predicate: &L, _subjects: &T) -> Result<(), Self::Error>
    where
        L: ?Sized + LinkedDataResource<I>,
        T: ?Sized + ld_core::LinkedDataPredicateObjects<I>,
    {
        Err(Error::ListReverseProperty)
    }

    fn graph<T>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ld_core::LinkedDataGraph<I>,
    {
        Ok(())
    }

    fn include<T>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        Err(Error::ListInclude)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let first = self.first.unwrap_or_else(Object::null);
        let mut result = self.rest;
        result.push(Indexed::none(first));
        Ok(result)
    }
}

pub struct SerializeListFirst<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: Option<Object<V::Iri, V::BlankId>>,
}

impl<'a, I, V: Vocabulary> SerializeListFirst<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            result: None,
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::PredicateObjectsVisitor<I> for SerializeListFirst<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Option<Object<V::Iri, V::BlankId>>;
    type Error = Error;

    fn object<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        self.result = Some(serialize_object_with(self.vocabulary, self.interpretation, value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}

pub struct SerializeListRest<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: Vec<IndexedObject<V::Iri, V::BlankId>>,
}

impl<'a, I, V: Vocabulary> SerializeListRest<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            result: Vec::new(),
        }
    }
}

impl<I: Interpretation, V: Vocabulary> ld_core::PredicateObjectsVisitor<I> for SerializeListRest<'_, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Vec<IndexedObject<V::Iri, V::BlankId>>;
    type Error = Error;

    fn object<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let serializer = SerializeList::new(self.vocabulary, self.interpretation);
        self.result = value.visit_subject(serializer)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}
