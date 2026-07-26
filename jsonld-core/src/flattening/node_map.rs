use super::Environment;
use crate::{ExpandedDocument, Id, Indexed, IndexedNode, IndexedObject, Node, Object, hash::IndexMap, object};
use educe::Educe;
use rdfx::{
    LocalGenerator,
    vocabulary::{BlankIdVocabulary, IriVocabulary, VocabularyMut},
};
use std::hash::Hash;

/// Conflicting indexes error.
///
/// Raised when a single node is declared with two different indexes.
#[derive(Clone, Debug, thiserror::Error)]
#[error("Index `{defined_index}` conflicts with index `{conflicting_index}`")]
pub struct ConflictingIndexes<T, B> {
    /// Node carrying the conflicting indexes.
    pub node_id: Id<T, B>,
    /// Index already recorded for the node.
    pub defined_index: String,
    /// Index that contradicts the recorded one.
    pub conflicting_index: String,
}

/// Error returned by node-map construction.
#[derive(Debug, thiserror::Error)]
pub enum NodeMapError<T, B> {
    #[error(transparent)]
    /// A node was given two different `@index` values.
    Conflicting(#[from] ConflictingIndexes<T, B>),

    #[error(transparent)]
    /// A blank node identifier could not be generated.
    GeneratedId(#[from] crate::id::GeneratedIdError),
}

/// Default graph of a node map, paired with its named graphs.
///
/// Named graphs are yielded in the order they were first declared, which is
/// document traversal order.
pub type Parts<T, B> = (NodeMapGraph<T, B>, IndexMap<Id<T, B>, NodeMapGraph<T, B>>);

/// Node identifier to node definition map.
///
/// Graphs are stored in declaration order so that flattening is deterministic:
/// [§4.9 of the JSON-LD API][spec] assigns blank node identifiers in document
/// traversal order.
///
/// [spec]: https://www.w3.org/TR/json-ld11-api/#node-map-generation
#[derive(Educe)]
#[educe(Default)]
pub struct NodeMap<T, B> {
    graphs: IndexMap<Id<T, B>, NodeMapGraph<T, B>>,
    default_graph: NodeMapGraph<T, B>,
}

impl<T, B> NodeMap<T, B> {
    /// Creates a new `NodeMap`.
    pub fn new() -> Self {
        Self {
            graphs: IndexMap::default(),
            default_graph: NodeMapGraph::new(),
        }
    }

    /// Consumes this `NodeMap`, returning its parts.
    pub fn into_parts(self) -> Parts<T, B> {
        (self.default_graph, self.graphs)
    }

    /// Returns an iterator over the entries of this `NodeMap`.
    pub fn iter(&self) -> Iter<'_, T, B> {
        Iter {
            default_graph: Some(&self.default_graph),
            graphs: self.graphs.iter(),
        }
    }

    /// Returns the iter named of this `NodeMap`, in graph declaration order.
    pub fn iter_named(&self) -> indexmap::map::Iter<'_, Id<T, B>, NodeMapGraph<T, B>> {
        self.graphs.iter()
    }
}

impl<T: Eq + Hash, B: Eq + Hash> NodeMap<T, B> {
    /// Returns the graph of this `NodeMap`.
    pub fn graph(&self, id: Option<&Id<T, B>>) -> Option<&NodeMapGraph<T, B>> {
        match id {
            Some(id) => self.graphs.get(id),
            None => Some(&self.default_graph),
        }
    }

    /// Mutably borrows the graph of the given name, if it is declared.
    pub fn graph_mut(&mut self, id: Option<&Id<T, B>>) -> Option<&mut NodeMapGraph<T, B>> {
        match id {
            Some(id) => self.graphs.get_mut(id),
            None => Some(&mut self.default_graph),
        }
    }

    /// Declares a named graph, leaving it untouched if it already exists.
    pub fn declare_graph(&mut self, id: Id<T, B>) {
        if let indexmap::map::Entry::Vacant(entry) = self.graphs.entry(id) {
            entry.insert(NodeMapGraph::new());
        }
    }

    /// Merge all the graphs into a single `NodeMapGraph`.
    ///
    /// Graphs are merged into the default graph in declaration order.
    pub fn merge(self) -> NodeMapGraph<T, B>
    where
        T: Clone,
        B: Clone,
    {
        let mut result = self.default_graph;

        for (_, graph) in self.graphs {
            result.merge_with(graph)
        }

        result
    }
}

/// Iterator over the graphs of a node map.
///
/// The default graph comes first, then the named graphs in declaration order.
pub struct Iter<'a, T, B> {
    default_graph: Option<&'a NodeMapGraph<T, B>>,
    graphs: indexmap::map::Iter<'a, Id<T, B>, NodeMapGraph<T, B>>,
}

impl<'a, T, B> Iterator for Iter<'a, T, B> {
    type Item = (Option<&'a Id<T, B>>, &'a NodeMapGraph<T, B>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.default_graph.take() {
            Some(default_graph) => Some((None, default_graph)),
            None => self.graphs.next().map(|(id, graph)| (Some(id), graph)),
        }
    }
}

impl<'a, T, B> IntoIterator for &'a NodeMap<T, B> {
    type Item = (Option<&'a Id<T, B>>, &'a NodeMapGraph<T, B>);
    type IntoIter = Iter<'a, T, B>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Owning iterator over the graphs of a node map.
///
/// The default graph comes first, then the named graphs in declaration order.
pub struct IntoIter<T, B> {
    default_graph: Option<NodeMapGraph<T, B>>,
    graphs: indexmap::map::IntoIter<Id<T, B>, NodeMapGraph<T, B>>,
}

impl<T, B> Iterator for IntoIter<T, B> {
    type Item = (Option<Id<T, B>>, NodeMapGraph<T, B>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.default_graph.take() {
            Some(default_graph) => Some((None, default_graph)),
            None => self.graphs.next().map(|(id, graph)| (Some(id), graph)),
        }
    }
}

impl<T, B> IntoIterator for NodeMap<T, B> {
    type Item = (Option<Id<T, B>>, NodeMapGraph<T, B>);
    type IntoIter = IntoIter<T, B>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            default_graph: Some(self.default_graph),
            graphs: self.graphs.into_iter(),
        }
    }
}

#[derive(Educe)]
#[educe(Default)]
/// Nodes of a single graph within a node map.
///
/// Nodes are stored in declaration order; see [`NodeMap`].
pub struct NodeMapGraph<T, B> {
    nodes: IndexMap<Id<T, B>, IndexedNode<T, B>>,
}

impl<T, B> NodeMapGraph<T, B> {
    /// Creates a new `NodeMapGraph`.
    pub fn new() -> Self {
        Self { nodes: IndexMap::default() }
    }
}

/// Result of declaring a node in a graph.
pub type DeclareNodeResult<'a, T, B> = Result<&'a mut Indexed<Node<T, B>>, ConflictingIndexes<T, B>>;

impl<T: Eq + Hash, B: Eq + Hash> NodeMapGraph<T, B> {
    /// Checks whether this `NodeMapGraph` contains.
    pub fn contains(&self, id: &Id<T, B>) -> bool {
        self.nodes.contains_key(id)
    }

    /// Returns the value bound to the given key, if any.
    pub fn get(&self, id: &Id<T, B>) -> Option<&IndexedNode<T, B>> {
        self.nodes.get(id)
    }

    /// Returns a mutable reference to the value bound to the given key, if any.
    pub fn get_mut(&mut self, id: &Id<T, B>) -> Option<&mut IndexedNode<T, B>> {
        self.nodes.get_mut(id)
    }

    /// Declares a node in this graph, failing if `index` contradicts the index
    /// already recorded for it.
    pub fn declare_node(&mut self, id: Id<T, B>, index: Option<&str>) -> DeclareNodeResult<'_, T, B>
    where
        T: Clone,
        B: Clone,
    {
        if let Some(entry) = self.nodes.get_mut(&id) {
            match (entry.index(), index) {
                (Some(entry_index), Some(index)) if entry_index != index => {
                    return Err(ConflictingIndexes {
                        node_id: id,
                        defined_index: entry_index.to_string(),
                        conflicting_index: index.to_string(),
                    });
                }
                (None, Some(index)) => entry.set_index(Some(index.to_owned())),
                _ => (),
            }
        } else {
            self.nodes
                .insert(id.clone(), Indexed::new(Node::with_id(id.clone()), index.map(ToOwned::to_owned)));
        }

        // SAFETY: just inserted above if not present.
        Ok(unsafe { self.nodes.get_mut(&id).unwrap_unchecked() })
    }

    /// Merge this graph with `other`.
    ///
    /// This calls [`merge_node`](Self::merge_node) with every node of `other`.
    pub fn merge_with(&mut self, other: Self)
    where
        T: Clone,
        B: Clone,
    {
        for (_, node) in other {
            self.merge_node(node)
        }
    }

    /// Merge the given `node` into the graph.
    ///
    /// The `node` must has an identifier, or this function will have no effect.
    /// If there is already a node with the same identifier:
    /// - The index of `node`, if any, overrides the previously existing index.
    /// - The list of `node` types is concatenated after the preexisting types.
    /// - The graph and imported values are overridden.
    /// - Properties and reverse properties are merged.
    pub fn merge_node(&mut self, node: IndexedNode<T, B>)
    where
        T: Clone,
        B: Clone,
    {
        let (node, index) = node.into_parts();

        if let Some(id) = &node.id {
            if let Some(entry) = self.nodes.get_mut(id) {
                if let Some(index) = index {
                    entry.set_index(Some(index))
                }
            } else {
                self.nodes.insert(id.clone(), Indexed::new(Node::with_id(id.clone()), index));
            }

            // SAFETY: just inserted above if not present.
            let flat_node = unsafe { self.nodes.get_mut(id).unwrap_unchecked() };

            if let Some(types) = node.types {
                flat_node.types_mut_or_default().extend(types);
            }

            flat_node.set_graph_entry(node.graph);
            flat_node.set_included(node.included);
            flat_node.properties_mut().extend_unique(node.properties);

            if let Some(props) = node.reverse_properties {
                flat_node.reverse_properties_or_default().extend_unique(props);
            }
        }
    }

    /// Returns the nodes of this `NodeMapGraph`.
    pub fn nodes(&self) -> NodeMapGraphNodes<'_, T, B> {
        self.nodes.values()
    }

    /// Consumes this `NodeMapGraph`, returning its nodes.
    pub fn into_nodes(self) -> IntoNodeMapGraphNodes<T, B> {
        self.nodes.into_values()
    }
}

/// Iterator over the nodes of a graph, in declaration order.
pub type NodeMapGraphNodes<'a, T, B> = indexmap::map::Values<'a, Id<T, B>, IndexedNode<T, B>>;
/// Owning iterator over the nodes of a graph, in declaration order.
pub type IntoNodeMapGraphNodes<T, B> = indexmap::map::IntoValues<Id<T, B>, IndexedNode<T, B>>;

impl<T, B> IntoIterator for NodeMapGraph<T, B> {
    type Item = (Id<T, B>, IndexedNode<T, B>);
    type IntoIter = indexmap::map::IntoIter<Id<T, B>, IndexedNode<T, B>>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_iter()
    }
}

impl<'a, T, B> IntoIterator for &'a NodeMapGraph<T, B> {
    type Item = (&'a Id<T, B>, &'a IndexedNode<T, B>);
    type IntoIter = indexmap::map::Iter<'a, Id<T, B>, IndexedNode<T, B>>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.iter()
    }
}

impl<T: Clone + Eq + Hash, B: Clone + Eq + Hash> ExpandedDocument<T, B> {
    /// Builds the node map of this document using the given vocabulary and
    /// blank node generator.
    pub fn generate_node_map_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &self,
        vocabulary: &mut V,
        generator: G,
    ) -> Result<NodeMap<T, B>, NodeMapError<T, B>> {
        let mut node_map: NodeMap<T, B> = NodeMap::new();
        let mut env: Environment<V, G> = Environment::new(vocabulary, generator);
        for object in self {
            extend_node_map(&mut env, &mut node_map, object, None)?;
        }
        Ok(node_map)
    }
}

/// Result of extending a node map with a document fragment.
pub type ExtendNodeMapResult<V> = Result<
    IndexedObject<<V as IriVocabulary>::Iri, <V as BlankIdVocabulary>::BlankId>,
    NodeMapError<<V as IriVocabulary>::Iri, <V as BlankIdVocabulary>::BlankId>,
>;

/// Extends the `NodeMap` with the given `element` of an expanded JSON-LD document.
fn extend_node_map<N: VocabularyMut, G: LocalGenerator>(
    env: &mut Environment<N, G>,
    node_map: &mut NodeMap<N::Iri, N::BlankId>,
    element: &IndexedObject<N::Iri, N::BlankId>,
    active_graph: Option<&Id<N::Iri, N::BlankId>>,
) -> ExtendNodeMapResult<N>
where
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
{
    match element.inner() {
        Object::Value(value) => {
            let flat_value = value.clone();
            Ok(Indexed::new(Object::Value(flat_value), element.index().map(ToOwned::to_owned)))
        }
        Object::List(list) => {
            let mut flat_list = Vec::new();

            for item in list {
                flat_list.push(extend_node_map(env, node_map, item, active_graph)?);
            }

            Ok(Indexed::new(Object::List(object::List::new(flat_list)), element.index().map(ToOwned::to_owned)))
        }
        Object::Node(node) => {
            let flat_node = extend_node_map_from_node(env, node_map, node, element.index(), active_graph)?;
            Ok(flat_node.map_inner(Object::node))
        }
    }
}

type ExtendNodeMapFromNodeResult<T, B> = Result<Indexed<Node<T, B>>, NodeMapError<T, B>>;

fn extend_node_map_from_node<N: VocabularyMut, G: LocalGenerator>(
    env: &mut Environment<N, G>,
    node_map: &mut NodeMap<N::Iri, N::BlankId>,
    node: &Node<N::Iri, N::BlankId>,
    index: Option<&str>,
    active_graph: Option<&Id<N::Iri, N::BlankId>>,
) -> ExtendNodeMapFromNodeResult<N::Iri, N::BlankId>
where
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + Eq + Hash,
{
    let id = env.assign_node_id(node.id.as_ref())?;

    {
        // SAFETY: `active_graph` is always either `None` (default graph) or a
        // graph id that was previously declared via `node_map.declare_graph`.
        let flat_node = unsafe { node_map.graph_mut(active_graph).unwrap_unchecked() }.declare_node(id.clone(), index)?;

        if let Some(entry) = node.types.as_deref() {
            let types: Result<Vec<_>, _> = entry.iter().map(|ty| env.assign_node_id(Some(ty))).collect();
            flat_node.types = Some(types?);
        }
    }

    if let Some(graph_entry) = node.graph_entry() {
        node_map.declare_graph(id.clone());

        let mut flat_graph: Vec<_> = Vec::new();
        for object in graph_entry.iter() {
            let flat_object = extend_node_map(env, node_map, object, Some(&id))?;
            flat_graph.push(flat_object);
        }

        // SAFETY: `id` was just declared above; `active_graph` is `None` or
        // declared earlier.
        let flat_node = unsafe { node_map.graph_mut(active_graph).unwrap_unchecked().get_mut(&id).unwrap_unchecked() };
        match flat_node.graph_entry_mut() {
            Some(graph) => graph.extend(flat_graph),
            None => flat_node.set_graph_entry(Some(flat_graph)),
        }
    }

    if let Some(included_entry) = node.included_entry() {
        for inode in included_entry {
            extend_node_map_from_node(env, node_map, inode.inner(), inode.index(), active_graph)?;
        }
    }

    for (property, objects) in node.properties() {
        // "If property is a blank node identifier, replace it with a newly
        // generated blank node identifier" — properties share the relabeling
        // map with node identifiers (`flatten#t0038`).
        let property = env.assign_node_id(Some(property))?;

        let mut flat_objects = Vec::new();
        for object in objects {
            let flat_object = extend_node_map(env, node_map, object, active_graph)?;
            flat_objects.push(flat_object);
        }
        // SAFETY: `id` was declared in this graph above.
        unsafe { node_map.graph_mut(active_graph).unwrap_unchecked().get_mut(&id).unwrap_unchecked() }
            .properties_mut()
            .insert_all_unique(property, flat_objects)
    }

    if let Some(reverse_properties) = node.reverse_properties_entry() {
        for (property, nodes) in reverse_properties.iter() {
            for subject in nodes {
                let flat_subject = extend_node_map_from_node(env, node_map, subject.inner(), subject.index(), active_graph)?;

                // SAFETY: every flat node produced by `extend_node_map_from_node`
                // has an `id` set.
                let subject_id = unsafe { flat_subject.id.as_ref().unwrap_unchecked() };

                // SAFETY: subject was just declared in this graph.
                let flat_subject = unsafe { node_map.graph_mut(active_graph).unwrap_unchecked().get_mut(subject_id).unwrap_unchecked() };

                flat_subject
                    .properties_mut()
                    .insert_unique(property.clone(), Indexed::none(Object::node(Node::with_id(id.clone()))))
            }

            // let mut flat_nodes = Vec::new();
            // for node in nodes {
            // 	let flat_node = extend_node_map_from_node(
            // 		env,
            // 		node_map,
            // 		node.inner(),
            // 		node.index(),
            // 		active_graph,
            // 	)?;
            // 	flat_nodes.push(flat_node);
            // }

            // node_map
            // 	.graph_mut(active_graph)
            // 	.unwrap()
            // 	.get_mut(&id)
            // 	.unwrap()
            // 	.reverse_properties_mut()
            // 	.insert_all_unique(property.clone(), flat_nodes)
        }
    }

    Ok(Indexed::new(Node::with_id(id), None))
}
