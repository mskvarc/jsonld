use ld_core::{LinkedData, LinkedDataGraph, LinkedDataPredicateObjects, LinkedDataResource, LinkedDataSubject, ResourceInterpretation};
use rdfx::Interpretation;

use crate::Value;

// TODO (task 7): port the full lexical-representation logic for `Value` once
// the rdf module is back online (task 8). For now, expose Value as an
// uninterpreted resource with no inline term, so that LinkedData traversal
// still works for surrounding nodes/lists.

impl<T, I: Interpretation> LinkedDataResource<I> for Value<T> {
    fn interpretation(&self, _interpretation: &mut I) -> ResourceInterpretation<'_, I> {
        ResourceInterpretation::Uninterpreted(None)
    }
}

impl<T, I: Interpretation> LinkedDataSubject<I> for Value<T> {
    fn visit_subject<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedDataPredicateObjects<I> for Value<T> {
    fn visit_objects<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::PredicateObjectsVisitor<I>,
    {
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedDataGraph<I> for Value<T> {
    fn visit_graph<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::GraphVisitor<I>,
    {
        visitor.subject(self)?;
        visitor.end()
    }
}

impl<T, I: Interpretation> LinkedData<I> for Value<T> {
    fn visit<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::Visitor<I>,
    {
        visitor.default_graph(self)?;
        visitor.end()
    }
}
