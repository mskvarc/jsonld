use ld_core::{LinkedData, LinkedDataGraph, LinkedDataPredicateObjects, LinkedDataResource, LinkedDataSubject, ResourceInterpretation};
use rdf_rs::Interpretation;

use crate::{
    IndexedObject,
    object::List,
    rdf::{RDF_FIRST, RDF_REST},
};

impl<T, B, I: Interpretation> LinkedDataResource<I> for List<T, B> {
    fn interpretation(&self, _interpretation: &mut I) -> ResourceInterpretation<'_, I> {
        ResourceInterpretation::Uninterpreted(None)
    }
}

impl<T, B, I: Interpretation> LinkedDataSubject<I> for List<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_subject<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        Rest(self.as_slice()).visit_subject(visitor)
    }
}

impl<T, B, I: Interpretation> LinkedDataPredicateObjects<I> for List<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::PredicateObjectsVisitor<I>,
    {
        visitor.object(self)?;
        visitor.end()
    }
}

impl<T, B, I: Interpretation> LinkedDataGraph<I> for List<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_graph<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::GraphVisitor<I>,
    {
        visitor.subject(self)?;
        visitor.end()
    }
}

impl<T, B, I: Interpretation> LinkedData<I> for List<T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::Visitor<I>,
    {
        visitor.default_graph(self)?;
        visitor.end()
    }
}

struct Rest<'a, T, B>(&'a [IndexedObject<T, B>]);

impl<'a, T, B, I: Interpretation> LinkedDataResource<I> for Rest<'a, T, B> {
    fn interpretation(&self, _interpretation: &mut I) -> ResourceInterpretation<'_, I> {
        ResourceInterpretation::Uninterpreted(None)
    }
}

impl<'a, T, B, I: Interpretation> LinkedDataSubject<I> for Rest<'a, T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_subject<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        if let Some((first, rest)) = self.0.split_first() {
            visitor.predicate(&RDF_FIRST, first.inner())?;
            visitor.predicate(&RDF_REST, &Rest(rest))?;
        }

        visitor.end()
    }
}

impl<'a, T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Rest<'a, T, B>
where
    T: LinkedDataResource<I> + LinkedDataSubject<I>,
    B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
    fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::PredicateObjectsVisitor<I>,
    {
        visitor.object(self)?;
        visitor.end()
    }
}
