use iri_rs::Iri;
use jsonld_core::{Indexed, Node, Object, object::node::Multiset, rdf::RDF_TYPE};
use ld_core::{CowRdfTerm, LinkedDataResource, OwnedRdfTerm};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::{IriVocabulary, Vocabulary},
};
use std::hash::Hash;

use crate::Error;

use super::{
    graph::SerializeGraph,
    property::{SerializeProperty, SerializeReverseProperty},
};

/// Serialize the given Linked-Data value into a JSON-LD node object using a
/// custom vocabulary and interpretation.
pub fn serialize_node_with<I, V, T>(vocabulary: &mut V, interpretation: &mut I, value: &T) -> Result<Node<V::Iri, V::BlankId>, Error>
where
    V: Vocabulary + rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
    T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
{
    let id = match value.lexical_representation(interpretation).map(CowRdfTerm::into_owned) {
        Some(OwnedRdfTerm::Literal(_)) => return Err(Error::InvalidNode),
        Some(OwnedRdfTerm::Iri(iri)) => Some(jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(vocabulary.insert_owned(iri)))),
        Some(OwnedRdfTerm::BlankId(b)) => Some(jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(vocabulary.insert_owned_blank_id(b)))),
        None => None,
    };

    let serializer = SerializeNode::new(vocabulary, interpretation, id);

    value.visit_subject(serializer)
}

pub struct SerializeNode<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    result: Node<V::Iri, V::BlankId>,
}

impl<'a, I, V: Vocabulary> SerializeNode<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I, id: Option<jsonld_core::Id<V::Iri, V::BlankId>>) -> Self {
        let result = match id {
            Some(id) => Node::with_id(id),
            None => Node::new(),
        };

        Self {
            vocabulary,
            interpretation,
            result,
        }
    }
}

impl<'a, I: Interpretation, V: Vocabulary> ld_core::SubjectVisitor<I> for SerializeNode<'a, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Node<V::Iri, V::BlankId>;
    type Error = Error;

    fn predicate<L, T>(&mut self, predicate: &L, value: &T) -> Result<(), Self::Error>
    where
        L: ?Sized + LinkedDataResource<I>,
        T: ?Sized + ld_core::LinkedDataPredicateObjects<I>,
    {
        let prop = match predicate.lexical_representation(self.interpretation).map(CowRdfTerm::into_owned) {
            Some(OwnedRdfTerm::Iri(iri)) => jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(self.vocabulary.insert_owned(iri))),
            Some(OwnedRdfTerm::BlankId(b)) => jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(self.vocabulary.insert_owned_blank_id(b))),
            _ => return Err(Error::InvalidPredicate),
        };

        let serializer = SerializeProperty::new(self.vocabulary, self.interpretation);

        let objects = value.visit_objects(serializer)?;

        if is_iri(self.vocabulary, &prop, RDF_TYPE) {
            let mut non_iri_objects = Multiset::new();

            for obj in objects {
                match into_type_value(obj) {
                    Ok(ty) => self.result.types_mut_or_default().push(ty),
                    Err(obj) => {
                        non_iri_objects.insert(obj);
                    }
                }
            }

            if !non_iri_objects.is_empty() {
                self.result.properties_mut().set(prop, non_iri_objects);
            }
        } else {
            self.result.properties_mut().set(prop, objects);
        }

        Ok(())
    }

    fn reverse_predicate<L, T>(&mut self, predicate: &L, value: &T) -> Result<(), Self::Error>
    where
        L: ?Sized + LinkedDataResource<I>,
        T: ?Sized + ld_core::LinkedDataPredicateObjects<I>,
    {
        let prop = match predicate.lexical_representation(self.interpretation).map(CowRdfTerm::into_owned) {
            Some(OwnedRdfTerm::Iri(iri)) => jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(self.vocabulary.insert_owned(iri))),
            Some(OwnedRdfTerm::BlankId(b)) => jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(self.vocabulary.insert_owned_blank_id(b))),
            _ => return Err(Error::InvalidPredicate),
        };

        let serializer = SerializeReverseProperty::new(self.vocabulary, self.interpretation);

        let objects = value.visit_objects(serializer)?;
        self.result.reverse_properties_mut_or_default().set(prop, objects);

        Ok(())
    }

    fn include<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let node = serialize_node_with(self.vocabulary, self.interpretation, value)?;

        self.result.included_mut_or_default().push(Indexed::none(node));
        Ok(())
    }

    fn graph<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ld_core::LinkedDataGraph<I>,
    {
        let serializer = SerializeGraph::new(self.vocabulary, self.interpretation);

        let graph = value.visit_graph(serializer)?;
        self.result.graph = Some(graph);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.result)
    }
}

pub(crate) fn into_type_value<I, B>(obj: Indexed<Object<I, B>>) -> Result<jsonld_core::Id<I, B>, Indexed<Object<I, B>>> {
    match obj.index() {
        Some(_) => Err(obj),
        None => match obj.into_inner() {
            Object::Node(mut node) => {
                if node.is_empty()
                    && let Some(id) = node.id.take()
                {
                    return Ok(id);
                }
                Err(Indexed::none(Object::Node(node)))
            }
            obj => Err(Indexed::none(obj)),
        },
    }
}

pub(crate) fn is_iri<V, B>(vocabulary: &V, id: &jsonld_core::Id<V::Iri, B>, iri: Iri<&str>) -> bool
where
    V: IriVocabulary,
{
    match id {
        jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(i)) => match vocabulary.iri(i) {
            Some(i) => i == iri,
            None => false,
        },
        _ => false,
    }
}
