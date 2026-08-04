use super::{InvalidExpandedJson, Traverse, TryFromJson, TryFromJsonObject};
use crate::{HashMap, Id, Indexed, IndexedObject, Object, Objects, Relabel, Term, ValidId, object, utils};
use contextual::{IntoRefWithContext, WithContext};
use educe::Educe;
use iri_rs::IriBuf;
use jsonld_syntax::{IntoJson, IntoJsonWithContext, Keyword};
use rdfx::{
    BlankIdBuf,
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use std::{
    convert::TryFrom,
    hash::{Hash, Hasher},
};

/// Multiset used to hold the several values of a property.
pub mod multiset;
/// Properties of a node object.
pub mod properties;
/// Reverse properties of a node object.
pub mod reverse_properties;

pub use multiset::Multiset;
pub use properties::Properties;
pub use reverse_properties::ReverseProperties;

/// Type alias for the `@graph` entry of a node.
///
/// Switched from `IndexSet` to `Vec` because all writes are insertion-only
/// and reads only iterate (no `contains`, no hash-based lookup). The original
/// `IndexSet` paid for `Hash` on each insert — and that hash recursively walked
/// the entire `IndexedObject` tree (Properties → Multiset → `IndexedObject`…),
/// which dominates expansion of nested or repeated-term documents.
///
/// JSON-LD does not require dedup of identical sub-objects in `@graph` /
/// `@included`, so simple `Vec::push` is correct (and the conformance suite
/// agrees).
pub type Graph<T, B> = Vec<IndexedObject<T, B>>;

/// Node objects included alongside another through `@included`.
pub type Included<T, B> = Vec<IndexedNode<T, B>>;

/// Node object carrying an optional `@index`.
pub type IndexedNode<T = IriBuf, B = BlankIdBuf> = Indexed<Node<T, B>>;

/// Node object.
///
/// A node object represents zero or more properties of a node in the graph serialized by a JSON-LD document.
/// A node is defined by its identifier (`@id` field), types, properties and reverse properties.
/// In addition, a node may represent a graph (`@graph` field) and include
/// other nodes (`@included` field).
#[derive(Debug, Clone)]
pub struct Node<T = IriBuf, B = BlankIdBuf> {
    /// Identifier.
    ///
    /// This is the `@id` field.
    pub id: Option<Id<T, B>>,

    /// Types.
    ///
    /// This is the `@type` field.
    pub types: Option<Vec<Id<T, B>>>,

    /// Associated graph.
    ///
    /// This is the `@graph` field.
    pub graph: Option<Graph<T, B>>,

    /// Included nodes.
    ///
    /// This is the `@included` field.
    pub included: Option<Included<T, B>>,

    /// Properties.
    ///
    /// Any non-keyword field.
    pub properties: Properties<T, B>,

    /// Reverse properties.
    ///
    /// This is the `@reverse` field.
    pub reverse_properties: Option<ReverseProperties<T, B>>,
}

impl<T, B> Default for Node<T, B> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<T, B> Node<T, B> {
    /// Creates a new empty node.
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            types: None,
            graph: None,
            included: None,
            properties: Properties::new(),
            reverse_properties: None,
        }
    }

    /// Creates a new empty node with the given id.
    #[inline(always)]
    pub fn with_id(id: Id<T, B>) -> Self {
        Self {
            id: Some(id),
            types: None,
            graph: None,
            included: None,
            properties: Properties::new(),
            reverse_properties: None,
        }
    }

    /// Creates a new graph node.
    pub fn new_graph(id: Id<T, B>, graph: Graph<T, B>) -> Self {
        Self {
            id: Some(id),
            types: None,
            graph: Some(graph),
            included: None,
            properties: Properties::new(),
            reverse_properties: None,
        }
    }

    /// Returns a mutable reference to the reverse properties of the node.
    ///
    /// If no `@reverse` entry is present, one is created.
    #[inline(always)]
    pub fn reverse_properties_mut_or_default(&mut self) -> &mut ReverseProperties<T, B> {
        self.reverse_properties.get_or_insert_with(ReverseProperties::default)
    }

    /// Returns a mutable reference to the included nodes.
    ///
    /// If no `@included` entry is present, one is created.
    #[inline(always)]
    pub fn included_mut_or_default(&mut self) -> &mut Included<T, B> {
        self.included.get_or_insert_with(Included::default)
    }

    /// Assigns an identifier to this node and every other node included in this
    /// one using the given `generator`.
    ///
    /// # Errors
    ///
    /// Returns an error when the generator runs out of identifiers.
    pub fn identify_all_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut V,
        generator: &mut G,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        if self.id.is_none() {
            self.id = Some(crate::id::generator_next_id(vocabulary, generator)?.into());
        }

        if let Some(graph) = self.graph_mut() {
            for o in graph.iter_mut() {
                o.identify_all_with(vocabulary, generator)?;
            }
        }

        if let Some(included) = self.included_mut() {
            for n in included.iter_mut() {
                n.identify_all_with(vocabulary, generator)?;
            }
        }

        for (_, objects) in self.properties_mut() {
            for object in objects {
                object.identify_all_with(vocabulary, generator)?;
            }
        }

        if let Some(reverse_properties) = self.reverse_properties_mut() {
            for (_, nodes) in reverse_properties.iter_mut() {
                for node in nodes {
                    node.identify_all_with(vocabulary, generator)?;
                }
            }
        }

        Ok(())
    }

    /// Assigns an identifier to this node and every other node included in this one using the given `generator`.
    ///
    /// # Errors
    ///
    /// Returns an error when the generator runs out of identifiers.
    pub fn identify_all<G: LocalGenerator>(&mut self, generator: &mut G) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Eq + Hash,
        B: Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.identify_all_with(&mut (), generator)
    }

    /// Puts every literal of the node into canonical form, using the given
    /// `buffer` to render numbers.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer) {
        for (_, objects) in self.properties_mut() {
            for object in objects {
                object.canonicalize_with(buffer);
            }
        }

        if let Some(reverse_properties) = self.reverse_properties_mut() {
            for (_, nodes) in reverse_properties.iter_mut() {
                for node in nodes {
                    node.canonicalize_with(buffer);
                }
            }
        }
    }

    /// Puts every literal of the node into canonical form.
    pub fn canonicalize(&mut self) {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer);
    }

    /// Returns the node's identifier as an IRI, if the node has one and it
    /// is an IRI.
    #[inline(always)]
    pub fn as_iri(&self) -> Option<&T> {
        if let Some(id) = &self.id { id.as_iri() } else { None }
    }

    /// Returns the node's identifier as a string slice.
    ///
    /// Returns `None` if the node has no `@id` field.
    #[inline(always)]
    pub fn as_str(&self) -> Option<&str>
    where
        T: AsRef<str>,
    {
        match self.as_iri() {
            Some(iri) => Some(iri.as_ref()),
            None => None,
        }
    }

    /// Returns the types of the node, or an empty slice if it has no `@type`
    /// entry.
    #[inline(always)]
    pub fn types(&self) -> &[Id<T, B>] {
        match self.types.as_ref() {
            Some(entry) => entry,
            None => &[],
        }
    }

    /// Mutably borrows the types of the node, or an empty slice if it has no
    /// `@type` entry.
    #[inline(always)]
    pub fn types_mut(&mut self) -> &mut [Id<T, B>] {
        match self.types.as_mut() {
            Some(entry) => entry,
            None => &mut [],
        }
    }

    /// Mutably borrows the types of this node, inserting an empty list if it
    /// has none.
    pub fn types_mut_or_default(&mut self) -> &mut Vec<Id<T, B>> {
        self.types.get_or_insert_with(Vec::new)
    }

    /// Mutably borrows the types of this node, inserting `value` if it has
    /// none.
    pub fn types_mut_or_insert(&mut self, value: Vec<Id<T, B>>) -> &mut Vec<Id<T, B>> {
        self.types.get_or_insert(value)
    }

    /// Mutably borrows the types of this node, inserting the result of `f` if
    /// it has none.
    pub fn types_mut_or_insert_with(&mut self, f: impl FnOnce() -> Vec<Id<T, B>>) -> &mut Vec<Id<T, B>> {
        self.types.get_or_insert_with(f)
    }

    /// Checks if the node has the given type.
    #[inline]
    pub fn has_type<U>(&self, ty: &U) -> bool
    where
        Id<T, B>: PartialEq<U>,
    {
        for self_ty in self.types() {
            if self_ty == ty {
                return true;
            }
        }

        false
    }

    /// Tests if the node is empty.
    ///
    /// It is empty if every field other than `@id` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.types.is_none() && self.graph.is_none() && self.included.is_none() && self.properties.is_empty() && self.reverse_properties.is_none()
    }

    /// Tests if the node is a graph object (has a `@graph` field, and
    /// optionally an `@id` field). A node object with a `@graph` entry is
    /// not considered a graph object if it includes any entry other than
    /// `@id`.
    #[inline]
    pub fn is_graph(&self) -> bool {
        self.graph.is_some() && self.types.is_none() && self.included.is_none() && self.properties.is_empty() && self.reverse_properties.is_none()
    }

    /// Tests if the node is a simple graph object (a graph object without `@id` field)
    #[inline(always)]
    pub fn is_simple_graph(&self) -> bool {
        self.id.is_none() && self.is_graph()
    }

    /// Returns the node objects of the `@graph` entry, if the node has one.
    ///
    /// Note that having a `@graph` entry is necessary but not sufficient for
    /// the node to be a graph object; see [`is_graph`](Self::is_graph).
    #[inline(always)]
    pub fn graph(&self) -> Option<&Graph<T, B>> {
        self.graph.as_ref()
    }

    /// Mutably borrows the node objects of the `@graph` entry, if the node
    /// has one.
    #[inline(always)]
    pub fn graph_mut(&mut self) -> Option<&mut Graph<T, B>> {
        self.graph.as_mut()
    }

    /// Returns the node objects of the `@graph` entry, if the node has one.
    ///
    /// Alias for [`graph`](Self::graph).
    #[inline(always)]
    pub fn graph_entry(&self) -> Option<&Graph<T, B>> {
        self.graph.as_ref()
    }

    /// Mutably borrows the node objects of the `@graph` entry, if the node
    /// has one.
    ///
    /// Alias for [`graph_mut`](Self::graph_mut).
    #[inline(always)]
    pub fn graph_entry_mut(&mut self) -> Option<&mut Graph<T, B>> {
        self.graph.as_mut()
    }

    /// Sets the `@graph` entry of the node, or removes it when given `None`.
    #[inline(always)]
    pub fn set_graph_entry(&mut self, graph: Option<Graph<T, B>>) {
        self.graph = graph;
    }

    /// Returns the set of nodes included by this node.
    ///
    /// This corresponds to the `@included` field in the JSON representation.
    #[inline(always)]
    pub fn included_entry(&self) -> Option<&Included<T, B>> {
        self.included.as_ref()
    }

    /// Returns the mutable set of nodes included by this node.
    ///
    /// This corresponds to the `@included` field in the JSON representation.
    #[inline(always)]
    pub fn included_entry_mut(&mut self) -> Option<&mut Included<T, B>> {
        self.included.as_mut()
    }

    /// Returns a reference to the set of `@included` nodes.
    pub fn included(&self) -> Option<&Included<T, B>> {
        self.included.as_ref()
    }

    /// Returns a mutable reference to the set of `@included` nodes.
    pub fn included_mut(&mut self) -> Option<&mut Included<T, B>> {
        self.included.as_mut()
    }

    /// Sets the `@included` entry of the node, or removes it when given
    /// `None`.
    #[inline(always)]
    pub fn set_included(&mut self, included: Option<Included<T, B>>) {
        self.included = included;
    }

    /// Returns a reference to the properties of the node.
    #[inline(always)]
    pub fn properties(&self) -> &Properties<T, B> {
        &self.properties
    }

    /// Returns a mutable reference to the properties of the node.
    #[inline(always)]
    pub fn properties_mut(&mut self) -> &mut Properties<T, B> {
        &mut self.properties
    }

    /// Returns a reference to the reverse properties of the node, if it has
    /// a `@reverse` entry.
    #[inline(always)]
    pub fn reverse_properties(&self) -> Option<&ReverseProperties<T, B>> {
        self.reverse_properties.as_ref()
    }

    /// Returns a reference to the reverse properties of the node, if it has
    /// a `@reverse` entry.
    #[inline(always)]
    pub fn reverse_properties_entry(&self) -> Option<&ReverseProperties<T, B>> {
        self.reverse_properties.as_ref()
    }

    /// Returns a mutable reference to the reverse properties of the node, if
    /// it has a `@reverse` entry.
    #[inline(always)]
    pub fn reverse_properties_mut(&mut self) -> Option<&mut ReverseProperties<T, B>> {
        self.reverse_properties.as_mut()
    }

    /// Sets the reverse properties of this `Node`.
    pub fn set_reverse_properties(&mut self, reverse_properties: Option<ReverseProperties<T, B>>) {
        self.reverse_properties = reverse_properties;
    }

    /// Tests if the node is an unnamed graph object.
    ///
    /// Returns `true` if the only field of the object is a `@graph` field,
    /// `false` otherwise.
    #[inline]
    pub fn is_unnamed_graph(&self) -> bool {
        self.graph.is_some()
            && self.id.is_none()
            && self.types.is_none()
            && self.included.is_none()
            && self.properties.is_empty()
            && self.reverse_properties.is_none()
    }

    /// Returns the node as an unnamed graph, if it is one.
    ///
    /// The unnamed graph is returned as a set of indexed objects.
    /// Fails and returns itself if the node is *not* an unnamed graph.
    ///
    /// # Errors
    ///
    /// Returns `self` unchanged when the object is not a graph object, or is a named graph.
    #[inline(always)]
    pub fn into_unnamed_graph(self: Box<Self>) -> Result<Graph<T, B>, Box<Self>> {
        if self.is_unnamed_graph() {
            // SAFETY: `is_unnamed_graph()` implies `self.graph` is `Some`.
            Ok(unsafe { self.graph.unwrap_unchecked() })
        } else {
            Err(self)
        }
    }

    /// Returns an iterator that visits every fragment of the node: the node
    /// itself and, recursively, everything it contains.
    pub fn traverse(&self) -> Traverse<'_, T, B> {
        Traverse::new(Some(super::FragmentRef::Node(self)))
    }

    #[inline(always)]
    /// Counts the fragments of the node matching the given predicate.
    pub fn count(&self, f: impl FnMut(&super::FragmentRef<T, B>) -> bool) -> usize {
        self.traverse().filter(f).count()
    }

    /// Returns an iterator over the entries of the JSON representation of
    /// the node.
    pub fn entries(&self) -> Entries<'_, T, B> {
        Entries {
            id: self.id.as_ref(),
            type_: self.types.as_deref(),
            graph: self.graph.as_ref(),
            included: self.included.as_ref(),
            reverse: self.reverse_properties.as_ref(),
            properties: self.properties.iter(),
        }
    }

    /// Rewrites every IRI and identifier of the node (recursively) with the
    /// given functions.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> Node<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        self.map_ids_with(&mut map_iri, &mut map_id)
    }

    pub(crate) fn map_ids_with<U, C>(self, map_iri: &mut impl FnMut(T) -> U, map_id: &mut impl FnMut(Id<T, B>) -> Id<U, C>) -> Node<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        Node {
            id: self.id.map(&mut *map_id),
            types: self.types.map(|t| t.into_iter().map(&mut *map_id).collect()),
            graph: self
                .graph
                .map(|g| g.into_iter().map(|o| o.map_inner(|o| o.map_ids_with(map_iri, map_id))).collect()),
            included: self
                .included
                .map(|i| i.into_iter().map(|o| o.map_inner(|o| o.map_ids_with(map_iri, map_id))).collect()),
            properties: self
                .properties
                .into_iter()
                .map(|(id, values)| {
                    (
                        map_id(id),
                        values.into_iter().map(|o| o.map_inner(|o| o.map_ids_with(map_iri, map_id))).collect::<Vec<_>>(),
                    )
                })
                .collect(),
            reverse_properties: self.reverse_properties.map(|r| {
                r.into_iter()
                    .map(|(id, values)| {
                        (
                            map_id(id),
                            values.into_iter().map(|o| o.map_inner(|o| o.map_ids_with(map_iri, map_id))).collect::<Vec<_>>(),
                        )
                    })
                    .collect()
            }),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> Node<T, B> {
    /// Checks if the node object has the given term as key.
    ///
    /// # Example
    /// ```
    /// # use jsonld_syntax::Keyword;
    /// # use jsonld_core::Term;
    /// # let node: jsonld_core::Node = jsonld_core::Node::new();
    ///
    /// // Checks if the JSON object representation of the node has an `@id` key.
    /// if node.has_key(&Term::Keyword(Keyword::Id)) {
    ///   // ...
    /// }
    /// ```
    #[inline(always)]
    pub fn has_key(&self, key: &Term<T, B>) -> bool {
        match key {
            Term::Keyword(Keyword::Id) => self.id.is_some(),
            Term::Keyword(Keyword::Type) => self.types.is_some(),
            Term::Keyword(Keyword::Graph) => self.graph.is_some(),
            Term::Keyword(Keyword::Included) => self.included.is_some(),
            Term::Keyword(Keyword::Reverse) => self.reverse_properties.is_some(),
            Term::Id(prop) => self.properties.contains(prop),
            _ => false,
        }
    }

    /// Returns all the objects associated with the node through the given
    /// property.
    #[inline(always)]
    pub fn get<'a, Q: ?Sized + Hash + indexmap::Equivalent<Id<T, B>>>(&self, prop: &Q) -> Objects<'_, T, B>
    where
        T: 'a,
    {
        self.properties.get(prop)
    }

    /// Returns one of the objects associated with the node through the given
    /// property.
    ///
    /// If multiple objects are attached to the node with this property,
    /// there is no guarantee on which object is returned.
    #[inline(always)]
    pub fn get_any<'a, Q: ?Sized + Hash + indexmap::Equivalent<Id<T, B>>>(&self, prop: &Q) -> Option<&IndexedObject<T, B>>
    where
        T: 'a,
    {
        self.properties.get_any(prop)
    }

    /// Associates the given object with the node through the given property.
    #[inline(always)]
    pub fn insert(&mut self, prop: Id<T, B>, value: IndexedObject<T, B>) {
        self.properties.insert(prop, value);
    }

    /// Associates all the given objects with the node through the given
    /// property.
    ///
    /// If objects are already associated with the property, the new ones are
    /// appended to them. Duplicate objects are not removed.
    #[inline(always)]
    pub fn insert_all<Objects: Iterator<Item = IndexedObject<T, B>>>(&mut self, prop: Id<T, B>, values: Objects) {
        self.properties.insert_all(prop, values);
    }

    /// Mutably borrows the reverse properties, inserting `props` if there are
    /// none.
    pub fn reverse_properties_or_insert(&mut self, props: ReverseProperties<T, B>) -> &mut ReverseProperties<T, B> {
        self.reverse_properties.get_or_insert(props)
    }

    /// Mutably borrows the reverse properties, inserting an empty set if there
    /// are none.
    pub fn reverse_properties_or_default(&mut self) -> &mut ReverseProperties<T, B> {
        self.reverse_properties.get_or_insert_with(ReverseProperties::default)
    }

    /// Mutably borrows the reverse properties, inserting the result of `f` if
    /// there are none.
    pub fn reverse_properties_or_insert_with(&mut self, f: impl FnOnce() -> ReverseProperties<T, B>) -> &mut ReverseProperties<T, B> {
        self.reverse_properties.get_or_insert_with(f)
    }

    /// Equivalence operator.
    ///
    /// Equivalence differs from equality for anonymous objects: an anonymous
    /// node object stands for an implicit, unlabeled blank node, so it is
    /// never equivalent to anything.
    pub fn equivalent(&self, other: &Self) -> bool {
        if self.id.is_some() && other.id.is_some() { self == other } else { false }
    }
}

impl<T, B> Relabel<T, B> for Node<T, B> {
    fn relabel_with<N: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut N,
        generator: &mut G,
        relabeling: &mut HashMap<B, ValidId<T, B>>,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
    {
        self.id = match self.id.take() {
            Some(Id::Valid(ValidId::Blank(b))) => {
                let value = match relabeling.entry(b) {
                    hashbrown::hash_map::Entry::Occupied(o) => o.get().clone(),
                    hashbrown::hash_map::Entry::Vacant(v) => v.insert(crate::id::generator_next_id(vocabulary, generator)?).clone(),
                };
                Some(value.into())
            }
            None => {
                let value = crate::id::generator_next_id(vocabulary, generator)?;
                Some(value.into())
            }
            id => id,
        };

        for ty in self.types_mut() {
            if let Some(b) = ty.as_blank().cloned() {
                let value = match relabeling.entry(b) {
                    hashbrown::hash_map::Entry::Occupied(o) => o.get().clone(),
                    hashbrown::hash_map::Entry::Vacant(v) => v.insert(crate::id::generator_next_id(vocabulary, generator)?).clone(),
                };
                *ty = value.into();
            }
        }

        if let Some(graph) = self.graph_mut() {
            for o in graph.iter_mut() {
                o.relabel_with(vocabulary, generator, relabeling)?;
            }
        }

        if let Some(included) = self.included_mut() {
            for n in included.iter_mut() {
                n.relabel_with(vocabulary, generator, relabeling)?;
            }
        }

        // "If property is a blank node identifier, replace it with a newly
        // generated blank node identifier": property keys are relabeled from
        // the same map as node identifiers (`toRdf#t0118`). Keys are rewritten
        // in place, in insertion order, so the generator sees each blank node
        // in document order.
        if !self.properties.is_empty() {
            let properties = std::mem::take(&mut self.properties);
            for (property, mut objects) in properties {
                let property = match property {
                    Id::Valid(ValidId::Blank(b)) => {
                        let value = match relabeling.entry(b) {
                            hashbrown::hash_map::Entry::Occupied(o) => o.get().clone(),
                            hashbrown::hash_map::Entry::Vacant(v) => v.insert(crate::id::generator_next_id(vocabulary, generator)?).clone(),
                        };
                        value.into()
                    }
                    property => property,
                };

                for object in &mut objects {
                    object.relabel_with(vocabulary, generator, relabeling)?;
                }

                self.properties.set(property, objects);
            }
        }

        if let Some(reverse_properties) = self.reverse_properties_mut() {
            for (_, nodes) in reverse_properties.iter_mut() {
                for node in nodes {
                    node.relabel_with(vocabulary, generator, relabeling)?;
                }
            }
        }

        Ok(())
    }
}

impl<T: Eq + Hash, B: Eq + Hash> PartialEq for Node<T, B> {
    fn eq(&self, other: &Self) -> bool {
        // `@graph` and `@included` are sets of node objects: their order is not
        // significant (only `@list` is ordered). This matches `Hash`, which
        // already hashes both with `hash_set_opt`.
        self.id.eq(&other.id)
            && multiset::compare_unordered_opt(self.types.as_deref(), other.types.as_deref())
            && multiset::compare_unordered_opt(self.graph.as_deref(), other.graph.as_deref())
            && multiset::compare_unordered_opt(self.included.as_deref(), other.included.as_deref())
            && self.properties.eq(&other.properties)
            && self.reverse_properties.eq(&other.reverse_properties)
    }
}

impl<T: Eq + Hash, B: Eq + Hash> Eq for Node<T, B> {}

impl<T, B> Indexed<Node<T, B>> {
    /// Returns an iterator over the entries of the JSON representation of
    /// the node, including its `@index` entry if any.
    pub fn entries(&self) -> IndexedEntries<'_, T, B> {
        IndexedEntries {
            index: self.index(),
            inner: self.inner().entries(),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> Indexed<Node<T, B>> {
    /// Checks whether the two indexed nodes are equivalent: same `@index`
    /// and [equivalent](Node::equivalent) inner nodes.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.index() == other.index() && self.inner().equivalent(other.inner())
    }
}

// `bound(false)`: every variant payload is a shared reference, a slice, or
// another unconditionally-`Copy` borrow type, so the impls hold for any `T`/`B`.
// Educe's automatic bounds would instead propagate a predicate per field type,
// making the impl conditional and breaking the `&self` methods that consume
// `self` by copy.
#[derive(Educe, PartialEq, Eq)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Key of a node object entry.
pub enum EntryKeyRef<'a, T, B> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id,
    /// The `@type` entry, giving the type of the node or the values.
    Type,
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph,
    /// The `@included` entry, holding node objects included alongside this one.
    Included,
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse,
    /// A property of a node object.
    Property(&'a Id<T, B>),
}

impl<'a, T, B> EntryKeyRef<'a, T, B> {
    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Id => Some(Keyword::Id),
            Self::Type => Some(Keyword::Type),
            Self::Graph => Some(Keyword::Graph),
            Self::Included => Some(Keyword::Included),
            Self::Reverse => Some(Keyword::Reverse),
            Self::Property(_) => None,
        }
    }

    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn as_keyword(&self) -> Option<Keyword> {
        self.into_keyword()
    }

    /// Returns the key as a string slice.
    #[must_use]
    pub fn into_str(self) -> &'a str
    where
        T: AsRef<str>,
        B: AsRef<str>,
    {
        match self {
            Self::Id => "@id",
            Self::Type => "@type",
            Self::Graph => "@graph",
            Self::Included => "@included",
            Self::Reverse => "@reverse",
            Self::Property(p) => p.as_str(),
        }
    }

    /// Returns this value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &'a str
    where
        T: AsRef<str>,
        B: AsRef<str>,
    {
        self.into_str()
    }
}

impl<'a, T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoRefWithContext<'a, str, N> for EntryKeyRef<'a, T, B> {
    fn into_ref_with(self, vocabulary: &'a N) -> &'a str {
        match self {
            EntryKeyRef::Id => "@id",
            EntryKeyRef::Type => "@type",
            EntryKeyRef::Graph => "@graph",
            EntryKeyRef::Included => "@included",
            EntryKeyRef::Reverse => "@reverse",
            EntryKeyRef::Property(p) => p.with(vocabulary).as_str(),
        }
    }
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Value of a node object entry.
pub enum EntryValueRef<'a, T, B> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id(&'a Id<T, B>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(&'a [Id<T, B>]),
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph(&'a Graph<T, B>),
    /// The `@included` entry, holding node objects included alongside this one.
    Included(&'a Included<T, B>),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a ReverseProperties<T, B>),
    /// A property of a node object.
    Property(&'a [IndexedObject<T, B>]),
}

impl<'a, T, B> EntryValueRef<'a, T, B> {
    /// Checks whether this entry value renders as a JSON array.
    #[must_use]
    pub fn is_json_array(&self) -> bool {
        matches!(self, Self::Type(_) | Self::Graph(_) | Self::Included(_) | Self::Property(_))
    }

    /// Checks whether this entry value renders as a JSON object.
    #[must_use]
    pub fn is_json_object(&self) -> bool {
        matches!(self, Self::Reverse(_))
    }

    fn sub_fragments(&self) -> SubFragments<'a, T, B> {
        match self {
            Self::Type(l) => SubFragments::Type(l.iter()),
            Self::Graph(g) => SubFragments::Graph(g.iter()),
            Self::Included(i) => SubFragments::Included(i.iter()),
            Self::Reverse(r) => SubFragments::Reverse(r.iter()),
            Self::Property(p) => SubFragments::Property(p.iter()),
            Self::Id(_) => SubFragments::None,
        }
    }
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Entry of a node object, key and value together.
pub enum EntryRef<'a, T, B> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id(&'a Id<T, B>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(&'a [Id<T, B>]),
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph(&'a Graph<T, B>),
    /// The `@included` entry, holding node objects included alongside this one.
    Included(&'a Included<T, B>),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a ReverseProperties<T, B>),
    /// A property of a node object.
    Property(&'a Id<T, B>, &'a [IndexedObject<T, B>]),
}

impl<'a, T, B> EntryRef<'a, T, B> {
    /// Consumes this `EntryRef`, returning its key.
    #[must_use]
    pub fn into_key(self) -> EntryKeyRef<'a, T, B> {
        match self {
            Self::Id(_) => EntryKeyRef::Id,
            Self::Type(_) => EntryKeyRef::Type,
            Self::Graph(_) => EntryKeyRef::Graph,
            Self::Included(_) => EntryKeyRef::Included,
            Self::Reverse(_) => EntryKeyRef::Reverse,
            Self::Property(k, _) => EntryKeyRef::Property(k),
        }
    }

    /// Returns the key of this `EntryRef`.
    #[must_use]
    pub fn key(&self) -> EntryKeyRef<'a, T, B> {
        self.into_key()
    }

    /// Consumes this `EntryRef`, returning its value.
    #[must_use]
    pub fn into_value(self) -> EntryValueRef<'a, T, B> {
        match self {
            Self::Id(v) => EntryValueRef::Id(v),
            Self::Type(v) => EntryValueRef::Type(v),
            Self::Graph(v) => EntryValueRef::Graph(v),
            Self::Included(v) => EntryValueRef::Included(v),
            Self::Reverse(v) => EntryValueRef::Reverse(v),
            Self::Property(_, v) => EntryValueRef::Property(v),
        }
    }

    /// Returns the value of this `EntryRef`.
    #[must_use]
    pub fn value(&self) -> EntryValueRef<'a, T, B> {
        self.into_value()
    }

    /// Consumes this `EntryRef`, returning its key and value.
    #[must_use]
    pub fn into_key_value(self) -> (EntryKeyRef<'a, T, B>, EntryValueRef<'a, T, B>) {
        match self {
            Self::Id(v) => (EntryKeyRef::Id, EntryValueRef::Id(v)),
            Self::Type(v) => (EntryKeyRef::Type, EntryValueRef::Type(v)),
            Self::Graph(v) => (EntryKeyRef::Graph, EntryValueRef::Graph(v)),
            Self::Included(v) => (EntryKeyRef::Included, EntryValueRef::Included(v)),
            Self::Reverse(v) => (EntryKeyRef::Reverse, EntryValueRef::Reverse(v)),
            Self::Property(k, v) => (EntryKeyRef::Property(k), EntryValueRef::Property(v)),
        }
    }

    /// Returns the key and value of this entry.
    #[must_use]
    pub fn as_key_value(&self) -> (EntryKeyRef<'a, T, B>, EntryValueRef<'a, T, B>) {
        match self {
            Self::Id(v) => (EntryKeyRef::Id, EntryValueRef::Id(*v)),
            Self::Type(v) => (EntryKeyRef::Type, EntryValueRef::Type(v)),
            Self::Graph(v) => (EntryKeyRef::Graph, EntryValueRef::Graph(*v)),
            Self::Included(v) => (EntryKeyRef::Included, EntryValueRef::Included(*v)),
            Self::Reverse(v) => (EntryKeyRef::Reverse, EntryValueRef::Reverse(*v)),
            Self::Property(k, v) => (EntryKeyRef::Property(*k), EntryValueRef::Property(v)),
        }
    }
}

#[derive(Educe)]
#[educe(Clone)]
/// Iterator over the entries of a node object.
pub struct Entries<'a, T, B> {
    id: Option<&'a Id<T, B>>,
    type_: Option<&'a [Id<T, B>]>,
    graph: Option<&'a Graph<T, B>>,
    included: Option<&'a Included<T, B>>,
    reverse: Option<&'a ReverseProperties<T, B>>,
    properties: properties::Iter<'a, T, B>,
}

impl<'a, T, B> Iterator for Entries<'a, T, B> {
    type Item = EntryRef<'a, T, B>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = self.properties.len();

        if self.id.is_some() {
            len += 1;
        }

        if self.type_.is_some() {
            len += 1;
        }

        if self.graph.is_some() {
            len += 1;
        }

        if self.included.is_some() {
            len += 1;
        }

        if self.reverse.is_some() {
            len += 1;
        }

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.id.take().map(EntryRef::Id).or_else(|| {
            self.type_.take().map(EntryRef::Type).or_else(|| {
                self.graph.take().map(EntryRef::Graph).or_else(|| {
                    self.included.take().map(EntryRef::Included).or_else(|| {
                        self.reverse
                            .take()
                            .map(EntryRef::Reverse)
                            .or_else(|| self.properties.next().map(|(k, v)| EntryRef::Property(k, v)))
                    })
                })
            })
        })
    }
}

impl<T, B> ExactSizeIterator for Entries<'_, T, B> {}

#[derive(Educe)]
#[educe(Clone)]
/// Iterator over the entries of an indexed node object, `@index` included.
pub struct IndexedEntries<'a, T, B> {
    index: Option<&'a str>,
    inner: Entries<'a, T, B>,
}

impl<'a, T, B> Iterator for IndexedEntries<'a, T, B> {
    type Item = IndexedEntryRef<'a, T, B>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.inner.len() + usize::from(self.index.is_some());
        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.index
            .take()
            .map(IndexedEntryRef::Index)
            .or_else(|| self.inner.next().map(IndexedEntryRef::Node))
    }
}

impl<T, B> ExactSizeIterator for IndexedEntries<'_, T, B> {}

#[derive(Educe, PartialEq, Eq)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Key of an indexed node object entry.
pub enum IndexedEntryKeyRef<'a, T, B> {
    /// The `@index` entry.
    Index,
    /// A node object.
    Node(EntryKeyRef<'a, T, B>),
}

impl<'a, T, B> IndexedEntryKeyRef<'a, T, B> {
    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Index => Some(Keyword::Index),
            Self::Node(e) => e.into_keyword(),
        }
    }

    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn as_keyword(&self) -> Option<Keyword> {
        self.into_keyword()
    }

    /// Returns the key as a string slice.
    #[must_use]
    pub fn into_str(self) -> &'a str
    where
        T: AsRef<str>,
        B: AsRef<str>,
    {
        match self {
            Self::Index => "@index",
            Self::Node(e) => e.into_str(),
        }
    }

    /// Returns this value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &'a str
    where
        T: AsRef<str>,
        B: AsRef<str>,
    {
        self.into_str()
    }
}

impl<'a, T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoRefWithContext<'a, str, N> for IndexedEntryKeyRef<'a, T, B> {
    fn into_ref_with(self, vocabulary: &'a N) -> &'a str {
        match self {
            IndexedEntryKeyRef::Index => "@index",
            IndexedEntryKeyRef::Node(e) => e.into_with(vocabulary).into_str(),
        }
    }
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Value of an indexed node object entry.
pub enum IndexedEntryValueRef<'a, T, B> {
    /// The value of the `@index` entry.
    Index(&'a str),
    /// A node object.
    Node(EntryValueRef<'a, T, B>),
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Entry of an indexed node object, key and value together.
pub enum IndexedEntryRef<'a, T, B> {
    /// The `@index` entry and its value.
    Index(&'a str),
    /// A node object.
    Node(EntryRef<'a, T, B>),
}

impl<'a, T, B> IndexedEntryRef<'a, T, B> {
    /// Consumes this `IndexedEntryRef`, returning its key.
    #[must_use]
    pub fn into_key(self) -> IndexedEntryKeyRef<'a, T, B> {
        match self {
            Self::Index(_) => IndexedEntryKeyRef::Index,
            Self::Node(e) => IndexedEntryKeyRef::Node(e.key()),
        }
    }

    /// Returns the key of this `IndexedEntryRef`.
    #[must_use]
    pub fn key(&self) -> IndexedEntryKeyRef<'a, T, B> {
        self.into_key()
    }

    /// Consumes this `IndexedEntryRef`, returning its value.
    #[must_use]
    pub fn into_value(self) -> IndexedEntryValueRef<'a, T, B> {
        match self {
            Self::Index(v) => IndexedEntryValueRef::Index(v),
            Self::Node(e) => IndexedEntryValueRef::Node(e.value()),
        }
    }

    /// Returns the value of this `IndexedEntryRef`.
    #[must_use]
    pub fn value(&self) -> IndexedEntryValueRef<'a, T, B> {
        self.into_value()
    }

    /// Consumes this `IndexedEntryRef`, returning its key and value.
    #[must_use]
    pub fn into_key_value(self) -> (IndexedEntryKeyRef<'a, T, B>, IndexedEntryValueRef<'a, T, B>) {
        match self {
            Self::Index(v) => (IndexedEntryKeyRef::Index, IndexedEntryValueRef::Index(v)),
            Self::Node(e) => {
                let (k, v) = e.into_key_value();
                (IndexedEntryKeyRef::Node(k), IndexedEntryValueRef::Node(v))
            }
        }
    }

    /// Returns the key and value of this entry.
    #[must_use]
    pub fn as_key_value(&self) -> (IndexedEntryKeyRef<'a, T, B>, IndexedEntryValueRef<'a, T, B>) {
        self.into_key_value()
    }
}

/// Node object fragment reference.
pub enum FragmentRef<'a, T, B> {
    /// Node object entry.
    Entry(EntryRef<'a, T, B>),

    /// Node object entry key.
    Key(EntryKeyRef<'a, T, B>),

    /// Node object entry value.
    Value(EntryValueRef<'a, T, B>),

    /// "@type" entry value fragment.
    TypeFragment(&'a Id<T, B>),
}

impl<'a, T, B> FragmentRef<'a, T, B> {
    /// Returns the identifier this fragment stands for, if it is one.
    #[must_use]
    pub fn into_id(self) -> Option<&'a Id<T, B>> {
        match self {
            Self::Key(EntryKeyRef::Property(id)) => Some(id),
            Self::Value(EntryValueRef::Id(id)) => Some(id),
            Self::TypeFragment(ty) => Some(ty),
            _ => None,
        }
    }

    /// Returns the identifier this fragment stands for, if it is one.
    #[must_use]
    pub fn as_id(&self) -> Option<&'a Id<T, B>> {
        match self {
            Self::Key(EntryKeyRef::Property(id)) => Some(id),
            Self::Value(EntryValueRef::Id(id)) => Some(id),
            Self::TypeFragment(ty) => Some(ty),
            _ => None,
        }
    }

    /// Checks whether this fragment renders as a JSON array.
    #[must_use]
    pub fn is_json_array(&self) -> bool {
        match self {
            Self::Value(v) => v.is_json_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment renders as a JSON object.
    #[must_use]
    pub fn is_json_object(&self) -> bool {
        match self {
            Self::Value(v) => v.is_json_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    #[must_use]
    pub fn sub_fragments(&self) -> SubFragments<'a, T, B> {
        match self {
            Self::Entry(e) => SubFragments::Entry(Some(e.key()), Some(e.value())),
            Self::Value(v) => v.sub_fragments(),
            _ => SubFragments::None,
        }
    }
}

/// Iterator over the fragments directly held by a node object.
pub enum SubFragments<'a, T, B> {
    /// No value.
    None,
    /// An object entry.
    Entry(Option<EntryKeyRef<'a, T, B>>, Option<EntryValueRef<'a, T, B>>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(std::slice::Iter<'a, Id<T, B>>),
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph(std::slice::Iter<'a, IndexedObject<T, B>>),
    /// The `@included` entry, holding node objects included alongside this one.
    Included(std::slice::Iter<'a, IndexedNode<T, B>>),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(reverse_properties::Iter<'a, T, B>),
    /// A property of a node object.
    Property(std::slice::Iter<'a, IndexedObject<T, B>>),
}

impl<'a, T, B> Iterator for SubFragments<'a, T, B> {
    type Item = super::FragmentRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Entry(k, v) => k
                .take()
                .map(|k| super::FragmentRef::NodeFragment(FragmentRef::Key(k)))
                .or_else(|| v.take().map(|v| super::FragmentRef::NodeFragment(FragmentRef::Value(v)))),
            Self::Type(l) => l.next_back().map(|t| super::FragmentRef::NodeFragment(FragmentRef::TypeFragment(t))),
            Self::Graph(g) => g.next().map(|o| super::FragmentRef::IndexedObject(o)),
            Self::Included(i) => i.next().map(|n| super::FragmentRef::IndexedNode(n)),
            Self::Reverse(r) => r.next().map(|(_, n)| super::FragmentRef::IndexedNodeList(n)),
            Self::Property(o) => o.next().map(|o| super::FragmentRef::IndexedObject(o)),
        }
    }
}

impl<T, B> object::Any<T, B> for Node<T, B> {
    #[inline(always)]
    fn as_ref(&self) -> object::Ref<'_, T, B> {
        object::Ref::Node(self)
    }
}

impl<T, B> TryFrom<Object<T, B>> for Node<T, B> {
    type Error = Object<T, B>;

    #[inline(always)]
    fn try_from(obj: Object<T, B>) -> Result<Node<T, B>, Object<T, B>> {
        match obj {
            Object::Node(node) => Ok(*node),
            obj => Err(obj),
        }
    }
}

impl<T: Hash, B: Hash> Hash for Node<T, B> {
    #[inline]
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h);
        utils::hash_set_opt(self.types.as_ref(), h);
        utils::hash_set_opt(self.graph.as_ref(), h);
        utils::hash_set_opt(self.included.as_ref(), h);
        self.properties.hash(h);
        self.reverse_properties.hash(h);
    }
}

/// Iterator through indexed nodes.
pub struct Nodes<'a, T, B>(Option<std::slice::Iter<'a, IndexedNode<T, B>>>);

impl<'a, T, B> Nodes<'a, T, B> {
    #[inline(always)]
    pub(crate) fn new(inner: Option<std::slice::Iter<'a, IndexedNode<T, B>>>) -> Self {
        Self(inner)
    }
}

impl<'a, T, B> Iterator for Nodes<'a, T, B> {
    type Item = &'a IndexedNode<T, B>;

    #[inline(always)]
    fn next(&mut self) -> Option<&'a IndexedNode<T, B>> {
        match &mut self.0 {
            None => None,
            Some(it) => it.next(),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> TryFromJsonObject<T, B> for Node<T, B> {
    fn try_from_json_object_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, mut object: jstrict::Object) -> Result<Self, InvalidExpandedJson> {
        let id = match object.remove_unique("@id").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(entry) => Some(Id::try_from_json_in(vocabulary, entry.value)?),
            None => None,
        };

        let types = match object.remove_unique("@type").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(entry) => Some(Vec::try_from_json_in(vocabulary, entry.value)?),
            None => None,
        };

        let graph = match object.remove_unique("@graph").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(entry) => Some(Vec::try_from_json_in(vocabulary, entry.value)?),
            None => None,
        };

        let included = match object.remove_unique("@included").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(entry) => Some(Vec::try_from_json_in(vocabulary, entry.value)?),
            None => None,
        };

        let reverse_properties = match object.remove_unique("@reverse").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(entry) => Some(ReverseProperties::try_from_json_in(vocabulary, entry.value)?),
            None => None,
        };

        let properties = Properties::try_from_json_object_in(vocabulary, object)?;

        Ok(Self {
            id,
            types,
            graph,
            included,
            properties,
            reverse_properties,
        })
    }
}

impl<T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoJsonWithContext<N> for Node<T, B> {
    fn into_json_with(self, vocabulary: &N) -> jstrict::Value {
        let mut obj = jstrict::Object::new();

        if let Some(id) = self.id {
            obj.insert("@id".into(), id.into_with(vocabulary).into_json());
        }

        if let Some(types) = self.types
            && !types.is_empty()
        {
            let value = types.into_with(vocabulary).into_json();

            obj.insert("@type".into(), value);
        }

        if let Some(graph) = self.graph {
            obj.insert("@graph".into(), graph.into_with(vocabulary).into_json());
        }

        if let Some(included) = self.included {
            obj.insert("@included".into(), included.into_with(vocabulary).into_json());
        }

        if let Some(reverse_properties) = self.reverse_properties {
            obj.insert("@reverse".into(), reverse_properties.into_with(vocabulary).into_json());
        }

        for (prop, objects) in self.properties {
            obj.insert(prop.with(vocabulary).to_string().into(), objects.into_json_with(vocabulary));
        }

        obj.into()
    }
}
