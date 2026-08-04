//! Flattening algorithm and related types.
use crate::{ExpandedDocument, FlattenedDocument, IndexedNode, IndexedObject, Object, flattened::UnorderedFlattenedDocument};
use contextual::WithContext;
use rdfx::{
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use std::{collections::HashSet, hash::Hash};

mod environment;
mod node_map;

pub use environment::Environment;
pub use node_map::*;

/// Result of flattening a document into an ordered list of nodes.
pub type FlattenResult<I, B> = Result<FlattenedDocument<I, B>, NodeMapError<I, B>>;

/// Result of flattening a document into an unordered set of nodes.
pub type FlattenUnorderedResult<I, B> = Result<UnorderedFlattenedDocument<I, B>, NodeMapError<I, B>>;

/// Documents that can be flattened into node objects.
pub trait Flatten<I, B> {
    /// Flattens this document using the given vocabulary and blank node
    /// generator.
    fn flatten_with<V, G: LocalGenerator>(self, vocabulary: &mut V, generator: G, ordered: bool) -> FlattenResult<I, B>
    where
        V: Vocabulary<Iri = I, BlankId = B> + VocabularyMut;

    /// Flattens this document into an unordered set, using the given
    /// vocabulary and generator.
    fn flatten_unordered_with<V, G: LocalGenerator>(self, vocabulary: &mut V, generator: G) -> FlattenUnorderedResult<I, B>
    where
        V: Vocabulary<Iri = I, BlankId = B> + VocabularyMut;

    /// Flattens this document, sorting the result when `ordered` is set.
    fn flatten<G: LocalGenerator>(self, generator: G, ordered: bool) -> FlattenResult<I, B>
    where
        (): Vocabulary<Iri = I, BlankId = B>,
        Self: Sized,
    {
        self.flatten_with(rdfx::vocabulary::no_vocabulary_mut(), generator, ordered)
    }

    /// Flattens this document into an unordered set.
    fn flatten_unordered<G: LocalGenerator>(self, generator: G) -> FlattenUnorderedResult<I, B>
    where
        (): Vocabulary<Iri = I, BlankId = B>,
        Self: Sized,
    {
        self.flatten_unordered_with(rdfx::vocabulary::no_vocabulary_mut(), generator)
    }
}

impl<I: Clone + Eq + Hash, B: Clone + Eq + Hash> Flatten<I, B> for ExpandedDocument<I, B> {
    fn flatten_with<V, G: LocalGenerator>(self, vocabulary: &mut V, generator: G, ordered: bool) -> FlattenResult<I, B>
    where
        V: Vocabulary<Iri = I, BlankId = B> + VocabularyMut,
    {
        Ok(self.generate_node_map_with(vocabulary, generator)?.flatten_with(vocabulary, ordered))
    }

    fn flatten_unordered_with<V, G: LocalGenerator>(self, vocabulary: &mut V, generator: G) -> FlattenUnorderedResult<I, B>
    where
        V: Vocabulary<Iri = I, BlankId = B> + VocabularyMut,
    {
        Ok(self.generate_node_map_with(vocabulary, generator)?.flatten_unordered())
    }
}

fn filter_graph<T, B>(node: IndexedNode<T, B>) -> Option<IndexedNode<T, B>> {
    if node.index().is_none() && node.is_empty() { None } else { Some(node) }
}

fn filter_sub_graph<T, B>(mut node: IndexedNode<T, B>) -> Option<IndexedObject<T, B>> {
    if node.index().is_none() && node.properties().is_empty() {
        None
    } else {
        node.set_graph_entry(None);
        node.set_included(None);
        node.set_reverse_properties(None);
        Some(node.map_inner(Object::node))
    }
}

impl<T: Clone + Eq + Hash, B: Clone + Eq + Hash> NodeMap<T, B> {
    /// Flattens this node map into a list of nodes, sorted when `ordered` is
    /// set.
    pub fn flatten(self, ordered: bool) -> FlattenedDocument<T, B>
    where
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.flatten_with(&(), ordered)
    }

    /// Flattens this node map into a list of nodes, sorting through the given
    /// vocabulary when `ordered` is set.
    pub fn flatten_with<V>(self, vocabulary: &V, ordered: bool) -> FlattenedDocument<T, B>
    where
        V: Vocabulary<Iri = T, BlankId = B>,
    {
        let (mut default_graph, named_graphs) = self.into_parts();

        let named_graphs: Vec<_> = if ordered {
            let mut decorated: Vec<_> = named_graphs
                .into_iter()
                .map(|entry| (entry.0.with(vocabulary).as_str().to_string(), entry))
                .collect();
            decorated.sort_by(|a, b| a.0.cmp(&b.0));
            decorated.into_iter().map(|(_, entry)| entry).collect()
        } else {
            named_graphs.into_iter().collect()
        };

        for (graph_id, graph) in named_graphs {
            // `declare_node` with no index is infallible (only a conflicting
            // index can fail it), so the skip is unreachable.
            let Ok(entry) = default_graph.declare_node(graph_id, None) else {
                continue;
            };
            let nodes: Vec<_> = if ordered {
                let mut decorated: Vec<_> = graph
                    .into_nodes()
                    .map(|n| {
                        // Every node in a node-map graph has an `id`, so the
                        // empty-key fallback is unreachable.
                        let key = n.id.as_ref().map(|id| id.with(vocabulary).as_str().to_string()).unwrap_or_default();
                        (key, n)
                    })
                    .collect();
                decorated.sort_by(|a, b| a.0.cmp(&b.0));
                decorated.into_iter().map(|(_, n)| n).collect()
            } else {
                graph.into_nodes().collect()
            };
            entry.set_graph_entry(Some(nodes.into_iter().filter_map(filter_sub_graph).collect()));
        }

        let nodes: Vec<_> = if ordered {
            let mut decorated: Vec<_> = default_graph
                .into_nodes()
                .filter_map(filter_graph)
                .map(|n| {
                    // Every node in a node-map graph has an `id`, so the
                    // empty-key fallback is unreachable.
                    let key = n.id.as_ref().map(|id| id.with(vocabulary).as_str().to_string()).unwrap_or_default();
                    (key, n)
                })
                .collect();
            decorated.sort_by(|a, b| a.0.cmp(&b.0));
            decorated.into_iter().map(|(_, n)| n).collect()
        } else {
            default_graph.into_nodes().filter_map(filter_graph).collect()
        };

        nodes
    }

    /// Flattens this node map into an unordered set of nodes.
    pub fn flatten_unordered(self) -> HashSet<IndexedNode<T, B>> {
        let (mut default_graph, named_graphs) = self.into_parts();

        for (graph_id, graph) in named_graphs {
            // `declare_node` with no index is infallible (only a conflicting
            // index can fail it), so the skip is unreachable.
            let Ok(entry) = default_graph.declare_node(graph_id, None) else {
                continue;
            };
            entry.set_graph_entry(Some(graph.into_nodes().filter_map(filter_sub_graph).collect()));
        }

        default_graph.into_nodes().filter_map(filter_graph).collect()
    }
}
