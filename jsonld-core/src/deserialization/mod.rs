use ld_core::{LinkedData, LinkedDataGraph, LinkedDataResource, LinkedDataSubject};
use rdf_rs::Interpretation;

use crate::ExpandedDocument;

mod object;

impl<T, B, I: Interpretation> LinkedDataGraph<I> for ExpandedDocument<T, B>
where
	T: LinkedDataResource<I> + LinkedDataSubject<I>,
	B: LinkedDataResource<I> + LinkedDataSubject<I>,
{
	fn visit_graph<S>(&self, mut visitor: S) -> Result<S::Ok, S::Error>
	where
		S: ld_core::GraphVisitor<I>,
	{
		for object in self {
			visitor.subject(object.inner())?;
		}

		visitor.end()
	}
}

impl<T, B, I: Interpretation> LinkedData<I> for ExpandedDocument<T, B>
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
