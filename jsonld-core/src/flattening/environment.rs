use crate::{Id, ValidId, ValidVocabularyId, VocabularyId};
use rdf_rs::{
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use std::{collections::HashMap, hash::Hash};

pub struct Environment<'n, N: Vocabulary, G> {
    vocabulary: &'n mut N,
    generator: G,
    map: HashMap<N::BlankId, ValidVocabularyId<N>>,
}

impl<'n, N: Vocabulary, G> Environment<'n, N, G> {
    pub fn new(vocabulary: &'n mut N, generator: G) -> Self {
        Self {
            vocabulary,
            generator,
            map: HashMap::new(),
        }
    }
}

impl<'n, V: Vocabulary, G: LocalGenerator> Environment<'n, V, G>
where
    V: VocabularyMut,
    V::Iri: Clone,
    V::BlankId: Clone + Hash + Eq,
{
    pub fn assign(&mut self, blank_id: V::BlankId) -> ValidId<V::Iri, V::BlankId> {
        use std::collections::hash_map::Entry;
        match self.map.entry(blank_id) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let id = crate::id::generator_next_id(self.vocabulary, &mut self.generator);
                entry.insert(id.clone());
                id
            }
        }
    }

    pub fn assign_node_id(&mut self, r: Option<&VocabularyId<V>>) -> Id<V::Iri, V::BlankId> {
        match r {
            Some(Id::Valid(ValidId::Blank(id))) => self.assign(id.clone()).into(),
            Some(r) => r.clone(),
            None => self.next().into(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> ValidId<V::Iri, V::BlankId> {
        crate::id::generator_next_id(self.vocabulary, &mut self.generator)
    }
}
