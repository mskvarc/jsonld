use rdf_rs::{LocalGenerator, vocabulary::VocabularyMut};

use crate::{IdentifyAll, IndexedNode, Relabel, ValidId};
use std::{collections::HashSet, hash::Hash};

/// Result of the document flattening algorithm.
///
/// It is just an alias for a set of (indexed) nodes.
pub type FlattenedDocument<T, B> = Vec<IndexedNode<T, B>>;

impl<T, B> IdentifyAll<T, B> for FlattenedDocument<T, B> {
    #[inline(always)]
    fn identify_all_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut V,
        generator: &mut G,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        for node in self {
            node.identify_all_with(vocabulary, generator)?
        }
        Ok(())
    }
}

impl<T, B> Relabel<T, B> for FlattenedDocument<T, B> {
    fn relabel_with<N: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut N,
        generator: &mut G,
        relabeling: &mut hashbrown::HashMap<B, ValidId<T, B>>,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
    {
        for node in self {
            node.relabel_with(vocabulary, generator, relabeling)?
        }
        Ok(())
    }
}

pub type UnorderedFlattenedDocument<T, B> = HashSet<IndexedNode<T, B>>;
