use ld_core::{
	LinkedData, LinkedDataGraph, LinkedDataPredicateObjects, LinkedDataResource, LinkedDataSubject,
	ResourceInterpretation,
};
use rdf_rs::Interpretation;

use crate::{IndexedNode, IndexedObject, Node, rdf::RDF_TYPE};

impl<T, B, I: Interpretation> LinkedDataResource<I> for Node<T, B>
where
	T: LinkedDataResource<I>,
	B: LinkedDataResource<I>,
{
	fn interpretation(&self, interpretation: &mut I) -> ResourceInterpretation<'_, I> {
		match &self.id {
			Some(crate::Id::Valid(id)) => id.interpretation(interpretation),
			_ => ResourceInterpretation::Uninterpreted(None),
		}
	}
}

impl<T, B, I: Interpretation> LinkedDataSubject<I> for Node<T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_subject<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::SubjectVisitor<I>,
	{
		if !self.types().is_empty() {
			visitor.predicate(&RDF_TYPE, &Types(self.types()))?;
		}

		for (property, objects) in self.properties() {
			if let crate::Id::Valid(id) = property {
				visitor.predicate(id, &Objects(objects))?;
			}
		}

		if let Some(reverse_properties) = self.reverse_properties() {
			for (property, nodes) in reverse_properties {
				if let crate::Id::Valid(id) = property {
					visitor.reverse_predicate(id, &Nodes(nodes))?;
				}
			}
		}

		if self.is_graph() {
			visitor.graph(self)?;
		}

		if let Some(included) = self.included() {
			for node in included {
				visitor.include(node.inner())?;
			}
		}

		visitor.end()
	}
}

impl<T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Node<T, B>
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

impl<T, B, I: Interpretation> LinkedDataGraph<I> for Node<T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_graph<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::GraphVisitor<I>,
	{
		match self.graph() {
			Some(g) => {
				for object in g.iter() {
					visitor.subject(object.inner())?;
				}
			}
			None => {
				visitor.subject(self)?;
			}
		}

		visitor.end()
	}
}

impl<T, B, I: Interpretation> LinkedData<I> for Node<T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::Visitor<I>,
	{
		if self.is_graph() {
			visitor.named_graph(self)?;
		} else {
			visitor.default_graph(self)?;
		}

		visitor.end()
	}
}

struct Types<'a, T, B>(&'a [crate::Id<T, B>]);

impl<'a, T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Types<'a, T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::PredicateObjectsVisitor<I>,
	{
		for ty in self.0 {
			if let crate::Id::Valid(id) = ty {
				visitor.object(id)?;
			}
		}

		visitor.end()
	}
}

struct Objects<'a, T, B>(&'a [IndexedObject<T, B>]);

impl<'a, T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Objects<'a, T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::PredicateObjectsVisitor<I>,
	{
		for object in self.0 {
			visitor.object(object.inner())?;
		}

		visitor.end()
	}
}

struct Nodes<'a, T, B>(&'a [IndexedNode<T, B>]);

impl<'a, T, B, I: Interpretation> LinkedDataPredicateObjects<I> for Nodes<'a, T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_objects<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::PredicateObjectsVisitor<I>,
	{
		for node in self.0 {
			visitor.object(node.inner())?;
		}

		visitor.end()
	}
}
