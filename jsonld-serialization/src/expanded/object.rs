use std::hash::Hash;

use jsonld_core::{
    Indexed,
    IndexedObject,
    Node,
    Object,
    object::{
        Graph,
        List,
        node::{Included, Multiset, Properties, ReverseProperties},
    },
    rdf::{RDF_FIRST, RDF_REST, RDF_TYPE},
};
use ld_core::{CowRdfTerm, LinkedDataResource, OwnedRdfTerm};
use rdfx::{
    Interpretation,
    interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
    vocabulary::Vocabulary,
};

use crate::Error;

use super::{
    graph::SerializeGraph,
    list::{SerializeListFirst, SerializeListRest},
    node::{SerializeNode, into_type_value, is_iri},
    property::{SerializeProperty, SerializeReverseProperty},
    serialize_node_with,
    value::literal_to_value,
};

/// Serialize the given Linked-Data value into a JSON-LD object using a
/// custom vocabulary and interpretation.
pub fn serialize_object_with<I, V, T>(vocabulary: &mut V, interpretation: &mut I, value: &T) -> Result<Object<V::Iri, V::BlankId>, Error>
where
    V: Vocabulary + rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
    T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
{
    match value.lexical_representation(interpretation).map(CowRdfTerm::into_owned) {
        Some(OwnedRdfTerm::Literal(lit)) => {
            let value = literal_to_value(vocabulary, lit);
            Ok(Object::Value(value))
        }
        Some(OwnedRdfTerm::Iri(iri)) => {
            let id = jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(vocabulary.insert_owned(iri)));
            let serializer = SerializeNode::new(vocabulary, interpretation, Some(id));

            Ok(Object::node(value.visit_subject(serializer)?))
        }
        Some(OwnedRdfTerm::BlankId(b)) => {
            let id = jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(vocabulary.insert_owned_blank_id(b)));
            let serializer = SerializeNode::new(vocabulary, interpretation, Some(id));

            Ok(Object::node(value.visit_subject(serializer)?))
        }
        None => {
            let serializer = SerializeObject::new(vocabulary, interpretation);

            value.visit_subject(serializer)
        }
    }
}

pub struct SerializeObject<'a, I, V: Vocabulary> {
    vocabulary: &'a mut V,
    interpretation: &'a mut I,
    types: Vec<jsonld_core::Id<V::Iri, V::BlankId>>,
    properties: Properties<V::Iri, V::BlankId>,
    reverse_properties: ReverseProperties<V::Iri, V::BlankId>,
    included: Included<V::Iri, V::BlankId>,
    graph: Option<Graph<V::Iri, V::BlankId>>,
    first: Option<Object<V::Iri, V::BlankId>>,
    rest: Option<Vec<IndexedObject<V::Iri, V::BlankId>>>,
}

impl<'a, I, V: Vocabulary> SerializeObject<'a, I, V> {
    pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
        Self {
            vocabulary,
            interpretation,
            types: Vec::new(),
            properties: Properties::new(),
            reverse_properties: ReverseProperties::new(),
            included: Included::default(),
            graph: None,
            first: None,
            rest: None,
        }
    }
}

impl<'a, I: Interpretation, V: Vocabulary> ld_core::SubjectVisitor<I> for SerializeObject<'a, I, V>
where
    V: rdfx::vocabulary::VocabularyMut,
    V::Iri: Clone + Eq + Hash,
    V::BlankId: Clone + Eq + Hash,
    I: ReverseInterpretation + ReverseLocalInterpretation,
{
    type Ok = Object<V::Iri, V::BlankId>;
    type Error = Error;

    fn predicate<L, T>(&mut self, predicate: &L, value: &T) -> Result<(), Self::Error>
    where
        L: ?Sized + LinkedDataResource<I>,
        T: ?Sized + ld_core::LinkedDataPredicateObjects<I>,
    {
        let prop = match predicate.lexical_representation(self.interpretation).map(CowRdfTerm::into_owned) {
            Some(OwnedRdfTerm::Iri(iri)) => {
                if iri.as_ref() == RDF_FIRST {
                    let serializer = SerializeListFirst::new(self.vocabulary, self.interpretation);
                    self.first = value.visit_objects(serializer)?;
                } else if iri.as_ref() == RDF_REST {
                    let serializer = SerializeListRest::new(self.vocabulary, self.interpretation);
                    self.rest = Some(value.visit_objects(serializer)?);
                }

                jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(self.vocabulary.insert_owned(iri)))
            }
            Some(OwnedRdfTerm::BlankId(b)) => jsonld_core::Id::Valid(jsonld_core::ValidId::Blank(self.vocabulary.insert_owned_blank_id(b))),
            _ => return Err(Error::InvalidPredicate),
        };

        let serializer = SerializeProperty::new(self.vocabulary, self.interpretation);

        let objects = value.visit_objects(serializer)?;

        if is_iri(self.vocabulary, &prop, RDF_TYPE) {
            let mut non_iri_objects = Multiset::new();

            for obj in objects {
                match into_type_value(obj) {
                    Ok(ty) => self.types.push(ty),
                    Err(obj) => {
                        non_iri_objects.insert(obj);
                    }
                }
            }

            if !non_iri_objects.is_empty() {
                self.properties.set(prop, non_iri_objects);
            }
        } else {
            self.properties.set(prop, objects);
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
        self.reverse_properties.set(prop, objects);

        Ok(())
    }

    fn include<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
    {
        let node = serialize_node_with(self.vocabulary, self.interpretation, value)?;
        self.included.push(Indexed::none(node));
        Ok(())
    }

    fn graph<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ld_core::LinkedDataGraph<I>,
    {
        let serializer = SerializeGraph::new(self.vocabulary, self.interpretation);
        self.graph = Some(value.visit_graph(serializer)?);
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        if let (Some(first), Some(mut items)) = (self.first.take(), self.rest.take()) {
            if self.types.is_empty() && self.properties.is_empty() && self.graph.is_none() {
                items.push(Indexed::none(first));
                items.reverse();
                return Ok(Object::List(List::new(items)));
            }
            // restore moved values for the fallthrough path
            self.first = Some(first);
            self.rest = Some(items);
        }
        {
            if let Some(item) = self.first {
                let iri = self.vocabulary.insert(RDF_FIRST);
                self.properties
                    .insert(jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(iri)), Indexed::none(item))
            }

            if let Some(rest) = self.rest {
                let iri = self.vocabulary.insert(RDF_REST);
                self.properties.insert(
                    jsonld_core::Id::Valid(jsonld_core::ValidId::Iri(iri)),
                    Indexed::none(Object::List(List::new(rest))),
                )
            }

            let mut node = Node::new();

            if !self.types.is_empty() {
                node.types = Some(self.types)
            }

            *node.properties_mut() = self.properties;

            if !self.reverse_properties.is_empty() {
                node.set_reverse_properties(Some(self.reverse_properties));
            }

            if !self.included.is_empty() {
                node.set_included(Some(self.included));
            }

            node.graph = self.graph;

            Ok(Object::node(node))
        }
    }
}
