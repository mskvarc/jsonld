use crate::{Id, ValidId, ValidVocabularyId, VocabularyId};
use rdfx::{
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use std::{collections::HashMap, hash::Hash};

/// Vocabulary and blank node generator used while flattening.
pub struct Environment<'n, N: Vocabulary, G> {
    vocabulary: &'n mut N,
    generator: G,
    map: HashMap<N::BlankId, ValidVocabularyId<N>>,
}

impl<'n, N: Vocabulary, G> Environment<'n, N, G> {
    /// Creates a new `Environment`.
    pub fn new(vocabulary: &'n mut N, generator: G) -> Self {
        Self {
            vocabulary,
            generator,
            map: HashMap::default(),
        }
    }
}

impl<'n, V: Vocabulary, G: LocalGenerator> Environment<'n, V, G>
where
    V: VocabularyMut,
    V::Iri: Clone,
    V::BlankId: Clone + Hash + Eq,
{
    /// Returns the identifier this blank node was relabelled to, generating
    /// one on first sight.
    pub fn assign(&mut self, blank_id: V::BlankId) -> Result<ValidId<V::Iri, V::BlankId>, crate::id::GeneratedIdError> {
        use std::collections::hash_map::Entry;
        match self.map.entry(blank_id) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let id = crate::id::generator_next_id(self.vocabulary, &mut self.generator)?;
                entry.insert(id.clone());
                Ok(id)
            }
        }
    }

    /// Returns the identifier of a node, generating a blank one when the node
    /// is anonymous.
    pub fn assign_node_id(&mut self, r: Option<&VocabularyId<V>>) -> Result<Id<V::Iri, V::BlankId>, crate::id::GeneratedIdError> {
        match r {
            Some(Id::Valid(ValidId::Blank(id))) => Ok(self.assign(id.clone())?.into()),
            Some(r) => Ok(r.clone()),
            None => Ok(self.next_id()?.into()),
        }
    }

    /// Generates a fresh blank node identifier.
    pub fn next_id(&mut self) -> Result<ValidId<V::Iri, V::BlankId>, crate::id::GeneratedIdError> {
        crate::id::generator_next_id(self.vocabulary, &mut self.generator)
    }
}
