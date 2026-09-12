//! Nodes, lists and values.
use crate::{HashMap, Id, IndexSet, Indexed, LenientLangTag, Relabel, ValidId};
use contextual::{IntoRefWithContext, WithContext};
use derive_where::derive_where;
use iri_rs::IriBuf;
use jsonld_syntax::{IntoJsonWithContext, Keyword};
use jstrict::Number;
use rdfx::{
    BlankIdBuf,
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use smallvec::SmallVec;
use std::hash::Hash;

/// List objects.
pub mod list;
mod mapped_eq;
/// Node objects.
pub mod node;
mod typ;
/// Value objects.
pub mod value;

pub use list::List;
pub use mapped_eq::MappedEq;
pub use node::{Graph, IndexedNode, Node, Nodes};
pub use typ::{Type, TypeRef};
pub use value::{Literal, Value};

/// Abstract object.
pub trait Any<T, B> {
    /// Borrows the object as a value, node or list reference.
    fn as_ref(&self) -> Ref<'_, T, B>;

    #[inline]
    /// Returns the identifier (`@id`) of the object, if it is a node object
    /// with one.
    fn id(&self) -> Option<&Id<T, B>> {
        match self.as_ref() {
            Ref::Node(n) => n.id.as_ref(),
            _ => None,
        }
    }

    #[inline]
    /// Returns the language tag of this value, if it carries one.
    fn language<'a>(&'a self) -> Option<&'a LenientLangTag>
    where
        T: 'a,
        B: 'a,
    {
        match self.as_ref() {
            Ref::Value(value) => value.language(),
            _ => None,
        }
    }

    #[inline]
    /// Checks whether the object is a value object.
    fn is_value(&self) -> bool {
        matches!(self.as_ref(), Ref::Value(_))
    }

    #[inline]
    /// Checks whether the object is a node object.
    fn is_node(&self) -> bool {
        matches!(self.as_ref(), Ref::Node(_))
    }

    #[inline]
    /// Checks whether the object is a graph object (a node object with a
    /// `@graph` entry).
    fn is_graph(&self) -> bool {
        match self.as_ref() {
            Ref::Node(n) => n.is_graph(),
            _ => false,
        }
    }

    #[inline]
    /// Checks whether the object is a list object.
    fn is_list(&self) -> bool {
        matches!(self.as_ref(), Ref::List(_))
    }
}

/// Object reference.
pub enum Ref<'a, T, B> {
    /// Value object.
    Value(&'a Value<T>),

    /// Node object.
    Node(&'a Node<T, B>),

    /// List object.
    List(&'a List<T, B>),
}

/// Indexed object.
pub type IndexedObject<T, B = ()> = Indexed<Object<T, B>>;

/// Object.
///
/// JSON-LD connects together multiple kinds of data objects.
/// Objects may be nodes, values or lists of objects.
///
/// You can get an `Object` by expanding a JSON-LD document using the
/// expansion algorithm or by converting an already expanded JSON document
/// using [`TryFromJson`].
#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Hash)]
#[derive_where(PartialEq; T: Eq + Hash, B: Eq + Hash)]
pub enum Object<T = IriBuf, B = BlankIdBuf> {
    /// Value object.
    Value(Value<T>),

    /// Node object.
    Node(Box<Node<T, B>>),

    /// List object.
    List(List<T, B>),
}

impl<T: Eq + Hash, B: Eq + Hash> Eq for Object<T, B> {}

impl<T, B> Object<T, B> {
    /// Creates a `null` value object.
    #[inline(always)]
    #[must_use]
    pub fn null() -> Self {
        Self::Value(Value::null())
    }

    /// Creates a new node object from a node.
    #[inline(always)]
    pub fn node(n: Node<T, B>) -> Self {
        Self::Node(Box::new(n))
    }

    /// Identifier of the object, if it is a node object.
    #[inline(always)]
    pub fn id(&self) -> Option<&Id<T, B>> {
        match self {
            Object::Node(n) => n.id.as_ref(),
            _ => None,
        }
    }

    /// Assigns an identifier to every node included in this object using the given `generator`.
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
        match self {
            Object::Node(n) => n.identify_all_with(vocabulary, generator)?,
            Object::List(l) => {
                for object in l {
                    object.identify_all_with(vocabulary, generator)?;
                }
            }
            Object::Value(_) => (),
        }
        Ok(())
    }

    /// Use the given `generator` to assign an identifier to all nodes that
    /// don't have one.
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

    /// Puts every literal of the object into canonical form, using the given
    /// `buffer` to render numbers.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer) {
        match self {
            Self::List(l) => l.canonicalize_with(buffer),
            Self::Node(n) => n.canonicalize_with(buffer),
            Self::Value(v) => v.canonicalize_with(buffer),
        }
    }

    /// Puts every literal of the object into canonical form.
    pub fn canonicalize(&mut self) {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer);
    }

    /// Returns an iterator over the types of the object.
    pub fn types(&self) -> Types<'_, T, B> {
        match self {
            Self::Value(value) => Types::Value(value.typ()),
            Self::Node(node) => Types::Node(node.types().iter()),
            Self::List(_) => Types::List,
        }
    }

    /// Identifier of the object as an IRI.
    ///
    /// If the object is a node identified by an IRI, returns this IRI.
    /// Returns `None` otherwise.
    #[inline(always)]
    pub fn as_iri(&self) -> Option<&T> {
        match self {
            Object::Node(node) => node.as_iri(),
            _ => None,
        }
    }

    /// Tests if the object is a value.
    #[inline(always)]
    pub fn is_value(&self) -> bool {
        matches!(self, Object::Value(_))
    }

    /// Returns this object as a value, if it is one.
    #[inline(always)]
    pub fn as_value(&self) -> Option<&Value<T>> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Returns this object as a mutable value, if it is one.
    #[inline(always)]
    pub fn as_value_mut(&mut self) -> Option<&mut Value<T>> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Converts this object into a value, if it is one.
    #[inline(always)]
    pub fn into_value(self) -> Option<Value<T>> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Tests if the object is a node.
    #[inline(always)]
    pub fn is_node(&self) -> bool {
        matches!(self, Object::Node(_))
    }

    /// Returns this object as a node, if it is one.
    #[inline(always)]
    pub fn as_node(&self) -> Option<&Node<T, B>> {
        match self {
            Self::Node(n) => Some(n),
            _ => None,
        }
    }

    /// Returns this object as a mutable node, if it is one.
    #[inline(always)]
    pub fn as_node_mut(&mut self) -> Option<&mut Node<T, B>> {
        match self {
            Self::Node(n) => Some(n),
            _ => None,
        }
    }

    /// Converts this object into a node, if it is one.
    #[inline(always)]
    pub fn into_node(self) -> Option<Node<T, B>> {
        match self {
            Self::Node(n) => Some(*n),
            _ => None,
        }
    }

    /// Tests if the object is a graph object (a node with a `@graph` field).
    #[inline(always)]
    pub fn is_graph(&self) -> bool {
        match self {
            Object::Node(n) => n.is_graph(),
            _ => false,
        }
    }

    /// Tests if the object is a list.
    #[inline(always)]
    pub fn is_list(&self) -> bool {
        matches!(self, Object::List(_))
    }

    /// Returns this object as a list, if it is one.
    #[inline(always)]
    pub fn as_list(&self) -> Option<&List<T, B>> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    /// Returns this object as a mutable list, if it is one.
    #[inline(always)]
    pub fn as_list_mut(&mut self) -> Option<&mut List<T, B>> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    /// Converts this object into a list, if it is one.
    #[inline(always)]
    pub fn into_list(self) -> Option<List<T, B>> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    /// Returns the object as a string.
    ///
    /// If the object is a value holding a string, returns that string.
    /// If the object is a node with an identifier, returns the identifier as
    /// a string. Returns `None` otherwise.
    #[inline(always)]
    pub fn as_str(&self) -> Option<&str>
    where
        T: AsRef<str>,
        B: AsRef<str>,
    {
        match self {
            Object::Value(value) => value.as_str(),
            Object::Node(node) => node.as_str(),
            Object::List(_) => None,
        }
    }

    /// Returns the object as a boolean, if it is a value holding one.
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Object::Value(value) => value.as_bool(),
            _ => None,
        }
    }

    /// Returns the object as a number, if it is a value holding one.
    #[inline(always)]
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Object::Value(value) => value.as_number(),
            _ => None,
        }
    }

    /// Returns the associated language, if the object is a language-tagged
    /// value.
    #[inline(always)]
    pub fn language(&self) -> Option<&LenientLangTag> {
        match self {
            Object::Value(value) => value.language(),
            _ => None,
        }
    }

    /// Returns an iterator over all fragments of this object, including the
    /// object itself.
    ///
    /// Fragments include:
    ///   - objects
    ///   - key-value pairs,
    ///   - keys
    ///   - values
    pub fn traverse(&self) -> Traverse<'_, T, B> {
        Traverse::new(Some(FragmentRef::Object(self)))
    }

    fn sub_fragments(&self) -> ObjectSubFragments<'_, T, B> {
        match self {
            Self::Value(v) => ObjectSubFragments::Value(v.entries()),
            Self::List(l) => ObjectSubFragments::List(Some(l.entry())),
            Self::Node(n) => ObjectSubFragments::Node(n.entries()),
        }
    }

    /// Equivalence operator.
    ///
    /// Equivalence differs from equality for anonymous objects: list objects
    /// and anonymous node objects stand for implicit, unlabeled blank nodes,
    /// so they are never equivalent to anything.
    pub fn equivalent(&self, other: &Self) -> bool
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        match (self, other) {
            (Self::Value(a), Self::Value(b)) => a == b,
            (Self::Node(a), Self::Node(b)) => a.equivalent(b),
            _ => false,
        }
    }

    /// Returns an iterator over the entries of the JSON representation of
    /// the object.
    pub fn entries(&self) -> Entries<'_, T, B> {
        match self {
            Self::Value(value) => Entries::Value(value.entries()),
            Self::List(list) => Entries::List(Some(list.entry())),
            Self::Node(node) => Entries::Node(node.entries()),
        }
    }

    /// Rewrites every IRI and identifier of the object (recursively) with
    /// the given functions.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> Object<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        self.map_ids_with(&mut map_iri, &mut map_id)
    }

    fn map_ids_with<U, C>(self, map_iri: &mut impl FnMut(T) -> U, map_id: &mut impl FnMut(Id<T, B>) -> Id<U, C>) -> Object<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        match self {
            Self::Value(value) => Object::Value(value.map_ids(map_iri)),
            Self::List(list) => Object::List(list.map_ids_with(map_iri, map_id)),
            Self::Node(node) => Object::Node(Box::new((*node).map_ids_with(map_iri, map_id))),
        }
    }
}

impl<T, B> Relabel<T, B> for Object<T, B> {
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
        match self {
            Self::Node(n) => n.relabel_with(vocabulary, generator, relabeling),
            Self::List(l) => l.relabel_with(vocabulary, generator, relabeling),
            Self::Value(_) => Ok(()),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> Indexed<Object<T, B>> {
    /// Checks whether the two indexed objects are equivalent: same `@index`
    /// and [equivalent](Object::equivalent) inner objects.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.index() == other.index() && self.inner().equivalent(other.inner())
    }
}

impl<T, B> Indexed<Object<T, B>> {
    /// Converts this indexed object into an indexed node, if it is one.
    #[inline(always)]
    pub fn into_indexed_node(self) -> Option<Indexed<Node<T, B>>> {
        let (object, index) = self.into_parts();
        object.into_node().map(|node| Indexed::new(node, index))
    }

    /// Converts this indexed object into an indexed value, if it is one.
    #[inline(always)]
    pub fn into_indexed_value(self) -> Option<Indexed<Value<T>>> {
        let (object, index) = self.into_parts();
        object.into_value().map(|value| Indexed::new(value, index))
    }

    /// Converts this indexed object into an indexed list, if it is one.
    #[inline(always)]
    pub fn into_indexed_list(self) -> Option<Indexed<List<T, B>>> {
        let (object, index) = self.into_parts();
        object.into_list().map(|list| Indexed::new(list, index))
    }

    /// Try to convert this object into an unnamed graph.
    ///
    /// # Errors
    ///
    /// Returns `self` unchanged when the object is not a graph object, or is a named graph.
    pub fn into_unnamed_graph(self) -> Result<Graph<T, B>, Self> {
        let (obj, index) = self.into_parts();
        match obj {
            Object::Node(n) => match n.into_unnamed_graph() {
                Ok(g) => Ok(g),
                Err(n) => Err(Indexed::new(Object::Node(n), index)),
            },
            obj => Err(Indexed::new(obj, index)),
        }
    }

    /// Returns an iterator over the entries of the JSON representation of
    /// the object, including its `@index` entry if any.
    pub fn entries(&self) -> IndexedEntries<'_, T, B> {
        IndexedEntries {
            index: self.index(),
            inner: self.inner().entries(),
        }
    }
}

#[derive_where(Clone)]
/// Iterator over the entries of an object.
pub enum Entries<'a, T, B> {
    /// A value object.
    Value(value::Entries<'a, T>),
    /// A list object.
    List(Option<&'a [IndexedObject<T, B>]>),
    /// A node object.
    Node(node::Entries<'a, T, B>),
}

impl<'a, T, B> Iterator for Entries<'a, T, B> {
    type Item = EntryRef<'a, T, B>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = match self {
            Self::Value(v) => v.len(),
            Self::List(l) => usize::from(l.is_some()),
            Self::Node(n) => n.len(),
        };

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Value(v) => v.next().map(EntryRef::Value),
            Self::List(l) => l.take().map(EntryRef::List),
            Self::Node(n) => n.next().map(EntryRef::Node),
        }
    }
}

impl<T, B> ExactSizeIterator for Entries<'_, T, B> {}

#[derive_where(Clone)]
/// Iterator over the entries of an indexed object, `@index` included.
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
            .or_else(|| self.inner.next().map(IndexedEntryRef::Object))
    }
}

impl<T, B> ExactSizeIterator for IndexedEntries<'_, T, B> {}

// `bound(false)`: every variant payload is a shared reference, a `Copy` value
// type, or another unconditionally-`Copy` borrow type, so the impls hold for any
// `T`/`B`. A plain `derive` would instead add a bound per field
// type, making the impl conditional and breaking the `&self` methods that consume
// `self` by copy.
#[derive(PartialEq, Eq)]
#[derive_where(Clone, Copy)]
/// Key of an object entry.
pub enum EntryKeyRef<'a, T, B> {
    /// A value object.
    Value(value::EntryKey),
    /// A list object.
    List,
    /// A node object.
    Node(node::EntryKeyRef<'a, T, B>),
}

impl<'a, T, B> EntryKeyRef<'a, T, B> {
    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Value(e) => Some(e.into_keyword()),
            Self::List => Some(Keyword::List),
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
            Self::Value(e) => e.into_str(),
            Self::List => "@list",
            Self::Node(e) => e.into_str(),
        }
    }

    /// Returns this value as a string slice.
    #[must_use]
    pub fn as_str(self) -> &'a str
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
            EntryKeyRef::Value(e) => e.into_str(),
            EntryKeyRef::List => "@list",
            EntryKeyRef::Node(e) => e.into_with(vocabulary).into_str(),
        }
    }
}

#[derive_where(Clone, Copy)]
/// Value of an object entry.
pub enum EntryValueRef<'a, T, B> {
    /// A value object.
    Value(value::EntryRef<'a, T>),
    /// A list object.
    List(&'a [IndexedObject<T, B>]),
    /// A node object.
    Node(node::EntryValueRef<'a, T, B>),
}

#[derive_where(Clone, Copy)]
/// Entry of an object, key and value together.
pub enum EntryRef<'a, T, B> {
    /// A value object.
    Value(value::EntryRef<'a, T>),
    /// A list object.
    List(&'a [IndexedObject<T, B>]),
    /// A node object.
    Node(node::EntryRef<'a, T, B>),
}

impl<'a, T, B> EntryRef<'a, T, B> {
    /// Consumes this `EntryRef`, returning its key.
    #[must_use]
    pub fn into_key(self) -> EntryKeyRef<'a, T, B> {
        match self {
            Self::Value(e) => EntryKeyRef::Value(e.key()),
            Self::List(_) => EntryKeyRef::List,
            Self::Node(e) => EntryKeyRef::Node(e.key()),
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
            Self::Value(v) => EntryValueRef::Value(v),
            Self::List(v) => EntryValueRef::List(v),
            Self::Node(e) => EntryValueRef::Node(e.value()),
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
            Self::Value(e) => (EntryKeyRef::Value(e.key()), EntryValueRef::Value(e)),
            Self::List(e) => (EntryKeyRef::List, EntryValueRef::List(e)),
            Self::Node(e) => {
                let (k, v) = e.into_key_value();
                (EntryKeyRef::Node(k), EntryValueRef::Node(v))
            }
        }
    }

    /// Returns the key and value of this entry.
    #[must_use]
    pub fn as_key_value(&self) -> (EntryKeyRef<'a, T, B>, EntryValueRef<'a, T, B>) {
        self.into_key_value()
    }
}

#[derive(PartialEq, Eq)]
#[derive_where(Clone, Copy)]
/// Key of an indexed object entry.
pub enum IndexedEntryKeyRef<'a, T, B> {
    /// The `@index` entry.
    Index,
    /// The key of one of the object's own entries.
    Object(EntryKeyRef<'a, T, B>),
}

impl<'a, T, B> IndexedEntryKeyRef<'a, T, B> {
    /// Returns the JSON-LD keyword this key stands for, if any.
    #[must_use]
    pub fn into_keyword(self) -> Option<Keyword> {
        match self {
            Self::Index => Some(Keyword::Index),
            Self::Object(e) => e.into_keyword(),
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
            Self::Object(e) => e.into_str(),
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
            IndexedEntryKeyRef::Object(e) => e.into_with(vocabulary).into_str(),
        }
    }
}

#[derive_where(Clone, Copy)]
/// Value of an indexed object entry.
pub enum IndexedEntryValueRef<'a, T, B> {
    /// The value of the `@index` entry.
    Index(&'a str),
    /// The value of one of the object's own entries.
    Object(EntryValueRef<'a, T, B>),
}

#[derive_where(Clone, Copy)]
/// Entry of an indexed object, key and value together.
pub enum IndexedEntryRef<'a, T, B> {
    /// The `@index` entry and its value.
    Index(&'a str),
    /// One of the object's own entries.
    Object(EntryRef<'a, T, B>),
}

impl<'a, T, B> IndexedEntryRef<'a, T, B> {
    /// Consumes this `IndexedEntryRef`, returning its key.
    #[must_use]
    pub fn into_key(self) -> IndexedEntryKeyRef<'a, T, B> {
        match self {
            Self::Index(_) => IndexedEntryKeyRef::Index,
            Self::Object(e) => IndexedEntryKeyRef::Object(e.key()),
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
            Self::Object(e) => IndexedEntryValueRef::Object(e.value()),
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
            Self::Object(e) => {
                let (k, v) = e.into_key_value();
                (IndexedEntryKeyRef::Object(k), IndexedEntryValueRef::Object(v))
            }
        }
    }

    /// Returns the key and value of this entry.
    #[must_use]
    pub fn as_key_value(&self) -> (IndexedEntryKeyRef<'a, T, B>, IndexedEntryValueRef<'a, T, B>) {
        self.into_key_value()
    }
}

/// Try to convert from a JSON value directly into an expanded JSON-LD document
/// without going through the expansion algorithm.
///
/// The input JSON value must be in expanded JSON-LD form.
pub trait TryFromJson<T, B>: Sized {
    /// Builds this fragment from a JSON value, interning terms in the given
    /// vocabulary.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON does not describe a valid expanded object.
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, value: jstrict::Value) -> Result<Self, InvalidExpandedJson>;
}

/// Try to convert from a JSON object directly into an expanded JSON-LD object
/// without going through the expansion algorithm.
///
/// The input JSON object must be in expanded JSON-LD form.
pub trait TryFromJsonObject<T, B>: Sized {
    /// Builds this fragment from a JSON object, interning terms in the given
    /// vocabulary.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON object does not describe a valid expanded object.
    fn try_from_json_object_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, object: jstrict::Object) -> Result<Self, InvalidExpandedJson>;
}

impl<T, B, V: TryFromJson<T, B>> TryFromJson<T, B> for Vec<V> {
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, value: jstrict::Value) -> Result<Self, InvalidExpandedJson> {
        match value {
            jstrict::Value::Array(items) => {
                let mut result = Vec::new();

                for item in items {
                    result.push(V::try_from_json_in(vocabulary, item)?);
                }

                Ok(result)
            }
            _ => Err(InvalidExpandedJson::InvalidList),
        }
    }
}

impl<T, B, V: Eq + Hash + TryFromJson<T, B>> TryFromJson<T, B> for IndexSet<V> {
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, value: jstrict::Value) -> Result<Self, InvalidExpandedJson> {
        match value {
            jstrict::Value::Array(items) => {
                let mut result = IndexSet::default();

                for item in items {
                    result.insert(V::try_from_json_in(vocabulary, item)?);
                }

                Ok(result)
            }
            _ => Err(InvalidExpandedJson::InvalidList),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> TryFromJson<T, B> for Object<T, B> {
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, value: jstrict::Value) -> Result<Self, InvalidExpandedJson> {
        match value {
            jstrict::Value::Object(object) => Self::try_from_json_object_in(vocabulary, object),
            _ => Err(InvalidExpandedJson::InvalidObject),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> TryFromJsonObject<T, B> for Object<T, B> {
    fn try_from_json_object_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, mut object: jstrict::Object) -> Result<Self, InvalidExpandedJson> {
        match object.remove_unique("@context").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(_) => Err(InvalidExpandedJson::NotExpanded),
            None => {
                if let Some(value_entry) = object.remove_unique("@value").map_err(InvalidExpandedJson::duplicate_key)? {
                    Ok(Self::Value(Value::try_from_json_object_in(vocabulary, object, value_entry)?))
                } else if let Some(list_entry) = object.remove_unique("@list").map_err(InvalidExpandedJson::duplicate_key)? {
                    Ok(Self::List(List::try_from_json_object_in(vocabulary, object, list_entry)?))
                } else {
                    let node = Node::try_from_json_object_in(vocabulary, object)?;
                    Ok(Self::node(node))
                }
            }
        }
    }
}

/// Invalid expanded JSON object error.
///
/// This can be raised when trying to directly convert a JSON value into an
/// expanded JSON-LD object without using the expansion algorithm.
#[derive(Debug, thiserror::Error)]
pub enum InvalidExpandedJson {
    /// The value is not a JSON object.
    #[error("invalid object")]
    InvalidObject,
    /// The `@list` entry does not hold an array.
    #[error("invalid `@list` value")]
    InvalidList,
    /// The `@index` entry does not hold a string.
    #[error("invalid `@index` value")]
    InvalidIndex,
    /// The `@id` entry does not hold a valid identifier.
    #[error("invalid `@id` value")]
    InvalidId,
    /// The `@type` entry of a value object is invalid.
    #[error("invalid `@type` value")]
    InvalidValueType,
    /// The `@value` entry does not hold a literal.
    #[error("invalid literal")]
    InvalidLiteral,
    /// The `@language` entry does not hold a language tag.
    #[error("invalid `@language` value")]
    InvalidLanguage,
    /// The `@direction` entry does not hold `ltr` or `rtl`.
    #[error("invalid `@direction` value")]
    InvalidDirection,
    /// The value is not a well-formed language-tagged string.
    #[error("invalid language-tagged string")]
    InvalidLangString,
    /// The document is not in expanded form.
    #[error("not an expanded document")]
    NotExpanded,
    /// The object carries an entry that is not allowed here.
    #[error("unexpected entry")]
    UnexpectedEntry,
    /// The object carries the same key twice.
    #[error("duplicate key `{0}`")]
    DuplicateKey(jstrict::object::Key),
    /// A value of the wrong JSON kind was found.
    #[error("unexpected {0}, expected {1}")]
    Unexpected(jstrict::Kind, jstrict::Kind),
}

impl InvalidExpandedJson {
    /// Builds a duplicate-key error from the duplicate `jstrict` reports.
    #[must_use]
    pub fn duplicate_key(jstrict::object::Duplicate(a, _): jstrict::object::Duplicate<jstrict::object::Entry>) -> Self {
        InvalidExpandedJson::DuplicateKey(a.key)
    }
}

impl<T, B> Any<T, B> for Object<T, B> {
    #[inline(always)]
    fn as_ref(&self) -> Ref<'_, T, B> {
        match self {
            Object::Value(value) => Ref::Value(value),
            Object::Node(node) => Ref::Node(node),
            Object::List(list) => Ref::List(list),
        }
    }
}

impl<T, B> From<Value<T>> for Object<T, B> {
    #[inline(always)]
    fn from(value: Value<T>) -> Self {
        Self::Value(value)
    }
}

impl<T, B> From<Node<T, B>> for Object<T, B> {
    #[inline(always)]
    fn from(node: Node<T, B>) -> Self {
        Self::node(node)
    }
}

/// Iterator through the types of an object.
pub enum Types<'a, T, B> {
    /// A value object.
    Value(Option<value::TypeRef<'a, T>>),
    /// A node object.
    Node(std::slice::Iter<'a, Id<T, B>>),
    /// A list object.
    List,
}

impl<'a, T, B> Iterator for Types<'a, T, B> {
    type Item = TypeRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Value(ty) => ty.take().map(TypeRef::from_value_type),
            Self::Node(tys) => tys.next().map(TypeRef::from_reference),
            Self::List => None,
        }
    }
}

/// Iterator through indexed objects.
pub struct Objects<'a, T, B>(Option<std::slice::Iter<'a, IndexedObject<T, B>>>);

impl<'a, T, B> Objects<'a, T, B> {
    #[inline(always)]
    pub(crate) fn new(inner: Option<std::slice::Iter<'a, IndexedObject<T, B>>>) -> Self {
        Self(inner)
    }
}

impl<'a, T, B> Iterator for Objects<'a, T, B> {
    type Item = &'a IndexedObject<T, B>;

    #[inline(always)]
    fn next(&mut self) -> Option<&'a IndexedObject<T, B>> {
        match &mut self.0 {
            None => None,
            Some(it) => it.next(),
        }
    }
}

/// Object fragment.
pub enum FragmentRef<'a, T, B> {
    /// "@index" entry.
    IndexEntry(&'a str),

    /// "@index" entry key.
    IndexKey,

    /// "@index" entry value.
    IndexValue(&'a str),

    /// Object.
    Object(&'a Object<T, B>),

    /// Indexed object.
    IndexedObject(&'a Indexed<Object<T, B>>),

    /// Node object.
    Node(&'a Node<T, B>),

    /// Indexed node object.
    IndexedNode(&'a IndexedNode<T, B>),

    /// Indexed node list.
    IndexedNodeList(&'a [IndexedNode<T, B>]),

    /// Value object fragment.
    ValueFragment(value::FragmentRef<'a, T>),

    /// List object fragment.
    ListFragment(list::FragmentRef<'a, T, B>),

    /// Node object fragment.
    NodeFragment(node::FragmentRef<'a, T, B>),
}

impl<'a, T, B> FragmentRef<'a, T, B> {
    /// Returns the object this fragment stands for, if it is one.
    #[must_use]
    pub fn into_ref(self) -> Option<Ref<'a, T, B>> {
        match self {
            Self::Object(o) => Some(o.as_ref()),
            Self::IndexedObject(o) => Some(o.inner().as_ref()),
            Self::Node(n) => Some(n.as_ref()),
            Self::IndexedNode(n) => Some(n.inner().as_ref()),
            _ => None,
        }
    }

    /// Returns the identifier this fragment stands for, if it is one.
    pub fn into_id(self) -> Option<Id<&'a T, &'a B>> {
        match self {
            Self::ValueFragment(i) => i.into_iri().map(Id::iri),
            Self::NodeFragment(i) => i.into_id().map(Into::into),
            _ => None,
        }
    }

    /// Returns the identifier this fragment stands for, if it is one.
    pub fn as_id(&self) -> Option<Id<&'a T, &'a B>> {
        match self {
            Self::ValueFragment(i) => i.as_iri().map(Id::iri),
            Self::NodeFragment(i) => i.as_id().map(Into::into),
            _ => None,
        }
    }

    /// Checks whether this fragment renders as a JSON array.
    #[must_use]
    pub fn is_json_array(&self) -> bool {
        match self {
            Self::IndexedNodeList(_) => true,
            Self::ValueFragment(i) => i.is_json_array(),
            Self::NodeFragment(n) => n.is_json_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment renders as a JSON object.
    #[must_use]
    pub fn is_json_object(&self) -> bool {
        match self {
            Self::Object(_) | Self::IndexedObject(_) | Self::Node(_) | Self::IndexedNode(_) => true,
            Self::ValueFragment(i) => i.is_json_object(),
            Self::NodeFragment(i) => i.is_json_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    #[must_use]
    pub fn sub_fragments(&self) -> SubFragments<'a, T, B> {
        match self {
            Self::IndexEntry(v) => SubFragments::IndexEntry(Some(()), Some(v)),
            Self::Object(o) => SubFragments::Object(None, o.sub_fragments()),
            Self::IndexedObject(o) => SubFragments::Object(o.index(), o.sub_fragments()),
            Self::Node(n) => SubFragments::Object(None, ObjectSubFragments::Node(n.entries())),
            Self::IndexedNode(n) => SubFragments::Object(n.index(), ObjectSubFragments::Node(n.inner().entries())),
            Self::IndexedNodeList(l) => SubFragments::IndexedNodeList(l.iter()),
            Self::ValueFragment(i) => SubFragments::Value(i.sub_fragments()),
            Self::NodeFragment(i) => SubFragments::Node(i.sub_fragments()),
            Self::ListFragment(l) => SubFragments::List(l.sub_fragments()),
            Self::IndexKey | Self::IndexValue(_) => SubFragments::None,
        }
    }
}

/// Iterator over the fragments directly held by an object.
pub enum ObjectSubFragments<'a, T, B> {
    /// A list object.
    List(Option<&'a [IndexedObject<T, B>]>),
    /// A value object.
    Value(value::Entries<'a, T>),
    /// A node object.
    Node(node::Entries<'a, T, B>),
}

impl<'a, T, B> Iterator for ObjectSubFragments<'a, T, B> {
    type Item = FragmentRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::List(l) => l.take().map(|e| FragmentRef::ListFragment(list::FragmentRef::Entry(e))),
            Self::Value(e) => e.next_back().map(|e| FragmentRef::ValueFragment(value::FragmentRef::Entry(e))),
            Self::Node(e) => e.next().map(|e| FragmentRef::NodeFragment(node::FragmentRef::Entry(e))),
        }
    }
}

/// Iterator over the fragments directly held by a document fragment.
pub enum SubFragments<'a, T, B> {
    /// No value.
    None,
    /// The `@index` entry of an indexed fragment.
    IndexEntry(Option<()>, Option<&'a str>),
    /// A JSON object.
    Object(Option<&'a str>, ObjectSubFragments<'a, T, B>),
    /// A value object.
    Value(value::SubFragments<'a, T>),
    /// A node object.
    Node(node::SubFragments<'a, T, B>),
    /// A list object.
    List(list::SubFragments<'a, T, B>),
    /// A list of indexed node objects.
    IndexedNodeList(std::slice::Iter<'a, IndexedNode<T, B>>),
}

impl<'a, T, B> Iterator for SubFragments<'a, T, B> {
    type Item = FragmentRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::IndexEntry(k, v) => k.take().map(|()| FragmentRef::IndexKey).or_else(|| v.take().map(FragmentRef::IndexValue)),
            Self::Object(index, i) => match index.take() {
                Some(index) => Some(FragmentRef::IndexEntry(index)),
                None => i.next(),
            },
            Self::Value(i) => i.next().map(FragmentRef::ValueFragment),
            Self::Node(i) => i.next(),
            Self::List(i) => i.next(),
            Self::IndexedNodeList(i) => i.next().map(FragmentRef::IndexedNode),
        }
    }
}

/// Depth-first iterator over a fragment and everything it contains.
pub struct Traverse<'a, T, B> {
    stack: SmallVec<[FragmentRef<'a, T, B>; 8]>,
}

impl<'a, T, B> Traverse<'a, T, B> {
    pub(crate) fn new(items: impl IntoIterator<Item = FragmentRef<'a, T, B>>) -> Self {
        let stack = items.into_iter().collect();
        Self { stack }
    }
}

impl<'a, T, B> Iterator for Traverse<'a, T, B> {
    type Item = FragmentRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some(item) => {
                self.stack.extend(item.sub_fragments());
                Some(item)
            }
            None => None,
        }
    }
}

impl<T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoJsonWithContext<N> for Object<T, B> {
    fn into_json_with(self, vocabulary: &N) -> jstrict::Value {
        match self {
            Self::Value(v) => v.into_json_with(vocabulary),
            Self::Node(n) => n.into_json_with(vocabulary),
            Self::List(l) => l.into_json_with(vocabulary),
        }
    }
}
