mod list;
mod node;
mod value;

use ld_core::{LinkedData, LinkedDataGraph, LinkedDataPredicateObjects, LinkedDataResource, LinkedDataSubject};
use rdfx::Interpretation;

use crate::Object;

impl<T, B, I: Interpretation> LinkedDataResource<I> for Object<T, B>
where
    T: LinkedDataResource<I>,
    B: LinkedDataResource<I>,
{
    fn interpretation(&self, interpretation: &mut I) -> ld_core::ResourceInterpretation<'_, I> {
        match self {
            Self::Node(node) => node.interpretation(interpretation),
            Self::List(list) => list.interpretation(interpretation),
            Self::Value(value) => value.interpretation(interpretation),
        }
    }
}

impl<T, B, I: Interpretation> LinkedDataSubject<I> for Object<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_subject<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        match self {
            Self::Node(node) => node.visit_subject(visitor),
            Self::List(list) => list.visit_subject(visitor),
            Self::Value(value) => value.visit_subject(visitor),
        }
    }
}

impl<T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Object<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_objects<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::PredicateObjectsVisitor<I>,
    {
        match self {
            Self::Node(node) => node.visit_objects(visitor),
            Self::List(list) => list.visit_objects(visitor),
            Self::Value(value) => value.visit_objects(visitor),
        }
    }
}

impl<T, B, I: Interpretation> LinkedDataGraph<I> for Object<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_graph<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::GraphVisitor<I>,
    {
        match self {
            Self::Node(node) => node.visit_graph(visitor),
            Self::List(list) => list.visit_graph(visitor),
            Self::Value(value) => value.visit_graph(visitor),
        }
    }
}

impl<T, B, I: Interpretation> LinkedData<I> for Object<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::Visitor<I>,
    {
        match self {
            Self::Node(node) => node.visit(visitor),
            Self::List(list) => list.visit(visitor),
            Self::Value(value) => value.visit(visitor),
        }
    }
}
