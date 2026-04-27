use std::hash::Hash;

use jsonld_core::{object::Graph, Indexed};
use ld_core::LinkedDataResource;
use rdf_rs::{
	interpretation::{ReverseInterpretation, ReverseLocalInterpretation},
	vocabulary::Vocabulary,
	Interpretation,
};

use crate::Error;

use super::object::serialize_object_with;

pub struct SerializeGraph<'a, I, V: Vocabulary> {
	vocabulary: &'a mut V,
	interpretation: &'a mut I,
	result: Graph<V::Iri, V::BlankId>,
}

impl<'a, I, V: Vocabulary> SerializeGraph<'a, I, V> {
	pub fn new(vocabulary: &'a mut V, interpretation: &'a mut I) -> Self {
		Self {
			vocabulary,
			interpretation,
			result: Graph::new(),
		}
	}
}

impl<'a, I: Interpretation, V: Vocabulary> ld_core::GraphVisitor<I>
	for SerializeGraph<'a, I, V>
where
	V: rdf_rs::vocabulary::VocabularyMut,
	V::Iri: Clone + Eq + Hash,
	V::BlankId: Clone + Eq + Hash,
	I: ReverseInterpretation + ReverseLocalInterpretation,
{
	type Ok = Graph<V::Iri, V::BlankId>;
	type Error = Error;

	fn subject<T>(&mut self, value: &T) -> Result<(), Self::Error>
	where
		T: ?Sized + LinkedDataResource<I> + ld_core::LinkedDataSubject<I>,
	{
		let object = serialize_object_with(self.vocabulary, self.interpretation, value)?;
		self.result.insert(Indexed::new(object, None));
		Ok(())
	}

	fn end(self) -> Result<Self::Ok, Self::Error> {
		Ok(self.result)
	}
}
