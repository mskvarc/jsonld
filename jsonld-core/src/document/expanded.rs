use crate::{
    HashMap,
    Id,
    IndexSet,
    Indexed,
    IndexedObject,
    Node,
    Object,
    Relabel,
    TryFromJson,
    object::{FragmentRef, InvalidExpandedJson, Traverse},
};
use iri_rs::IriBuf;
use rdfx::{
    BlankIdBuf,
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use smallvec::SmallVec;
use std::{
    collections::HashSet,
    hash::{BuildHasher, Hash, Hasher},
};

/// Bucket key standing in for a full object hash.
///
/// Deduplicating the objects of a document needs a hash, and hashing an object
/// walks its entire subtree — every property, every nested node. That is a
/// steep price for what is almost always a miss.
///
/// This reads only the shallow fields: the kind of object, its `@index`, and,
/// for a node, its `@id`. Equal objects necessarily agree on all three, so
/// they always land in the same bucket, which is the property that matters.
/// Unequal objects sharing a bucket cost nothing but the `Eq` comparison that
/// follows.
struct Discriminant<'a, T, B>(&'a IndexedObject<T, B>);

impl<T: Hash, B: Hash> Hash for Discriminant<'_, T, B> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.index().hash(state);

        match self.0.inner() {
            Object::Value(_) => 0u8.hash(state),
            Object::List(_) => 1u8.hash(state),
            Object::Node(node) => {
                2u8.hash(state);
                node.id.hash(state);
            }
        }
    }
}

/// Result of the document expansion algorithm: a set of (indexed) objects.
///
/// Objects are kept in insertion order, and deduplicated through an index of
/// [`Discriminant`] buckets rather than by hashing each object in full.
#[derive(Debug, Clone)]
pub struct ExpandedDocument<T = IriBuf, B = BlankIdBuf> {
    objects: Vec<IndexedObject<T, B>>,

    /// Positions in `objects`, bucketed by discriminant. One entry inline:
    /// collisions are the exception, not the rule.
    buckets: HashMap<u64, SmallVec<[Entry; 1]>>,
}

/// One object's slot in a bucket.
#[derive(Debug, Clone)]
struct Entry {
    /// Position in `ExpandedDocument::objects`.
    index: usize,

    /// Hash of the whole object, filled in only once this bucket holds more
    /// than one entry.
    ///
    /// A bucket with a single occupant never needs it — the discriminant
    /// already separated everything else — and computing it walks the object's
    /// whole subtree. Documents whose objects carry distinct `@id`s therefore
    /// never hash an object in full. Where the discriminant cannot separate
    /// them (top-level nodes with no `@id`, say), this restores what a plain
    /// hash set would have done: compare cheap hashes first, and reach for a
    /// deep equality check only when they match.
    full_hash: Option<u64>,
}

impl<T, B> Default for ExpandedDocument<T, B> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            buckets: HashMap::default(),
        }
    }
}

impl<T, B> ExpandedDocument<T, B> {
    #[inline(always)]
    /// Creates a new `ExpandedDocument`.
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    /// Creates a new `ExpandedDocument` with room for `capacity` objects.
    ///
    /// Worth reaching for whenever the object count is known up front: growing
    /// the set re-hashes every object already in it, and hashing an object
    /// walks its whole subtree.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            objects: Vec::with_capacity(capacity),
            buckets: HashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    #[inline(always)]
    /// Returns the number of entries of this `ExpandedDocument`.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    #[inline(always)]
    /// Checks whether this `ExpandedDocument` is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    #[inline(always)]
    /// Returns the objects of this `ExpandedDocument`, in insertion order.
    pub fn objects(&self) -> &[IndexedObject<T, B>] {
        &self.objects
    }

    #[inline(always)]
    /// Consumes this `ExpandedDocument`, returning its objects in insertion
    /// order.
    pub fn into_objects(self) -> Vec<IndexedObject<T, B>> {
        self.objects
    }

    #[inline(always)]
    /// Returns an iterator over the entries of this `ExpandedDocument`.
    pub fn iter(&self) -> std::slice::Iter<'_, IndexedObject<T, B>> {
        self.objects.iter()
    }

    #[inline(always)]
    /// Returns the traverse of this `ExpandedDocument`.
    pub fn traverse(&self) -> Traverse<'_, T, B> {
        Traverse::new(self.iter().map(|o| FragmentRef::IndexedObject(o)))
    }

    #[inline(always)]
    /// Checks whether this `ExpandedDocument` count.
    pub fn count(&self, f: impl FnMut(&FragmentRef<T, B>) -> bool) -> usize {
        self.traverse().filter(f).count()
    }

    /// Give an identifier (`@id`) to every nodes using the given generator to
    /// generate fresh identifiers for anonymous nodes.
    #[inline(always)]
    pub fn identify_all_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut V,
        generator: &mut G,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        for mut object in self.drain() {
            object.identify_all_with(vocabulary, generator)?;
            self.insert(object);
        }
        Ok(())
    }

    /// Give an identifier (`@id`) to every nodes using the given generator to
    /// generate fresh identifiers for anonymous nodes.
    #[inline(always)]
    pub fn identify_all<G: LocalGenerator>(&mut self, generator: &mut G) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Eq + Hash,
        B: Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.identify_all_with(rdfx::vocabulary::no_vocabulary_mut(), generator)
    }

    /// Give an identifier (`@id`) to every nodes and canonicalize every
    /// literals using the given generator to generate fresh identifiers for
    /// anonymous nodes.
    #[inline(always)]
    pub fn relabel_and_canonicalize_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut V,
        generator: &mut G,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
    {
        let mut relabeling = HashMap::default();
        let mut buffer = ryu_js::Buffer::new();
        for mut object in self.drain() {
            object.relabel_with(vocabulary, generator, &mut relabeling)?;
            object.canonicalize_with(&mut buffer);
            self.insert(object);
        }
        Ok(())
    }

    /// Give an identifier (`@id`) to every nodes and canonicalize every
    /// literals using the given generator to generate fresh identifiers for
    /// anonymous nodes.
    #[inline(always)]
    pub fn relabel_and_canonicalize<G: LocalGenerator>(&mut self, generator: &mut G) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.relabel_and_canonicalize_with(rdfx::vocabulary::no_vocabulary_mut(), generator)
    }

    /// Relabels nodes.
    #[inline(always)]
    pub fn relabel_with<V: VocabularyMut<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut V,
        generator: &mut G,
    ) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
    {
        let mut relabeling = HashMap::default();
        for mut object in self.drain() {
            object.relabel_with(vocabulary, generator, &mut relabeling)?;
            self.insert(object);
        }
        Ok(())
    }

    /// Relabels nodes.
    #[inline(always)]
    pub fn relabel<G: LocalGenerator>(&mut self, generator: &mut G) -> Result<(), crate::id::GeneratedIdError>
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.relabel_with(rdfx::vocabulary::no_vocabulary_mut(), generator)
    }

    /// Puts this document literals into canonical form using the given
    /// `buffer`.
    ///
    /// The buffer is used to compute the canonical form of numbers.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer)
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        for mut object in self.drain() {
            object.canonicalize_with(buffer);
            self.insert(object);
        }
    }

    /// Puts this document literals into canonical form.
    pub fn canonicalize(&mut self)
    where
        T: Eq + Hash,
        B: Eq + Hash,
    {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer)
    }

    /// Map the identifiers present in this expanded document (recursively).
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> ExpandedDocument<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        self.objects
            .into_iter()
            .map(|i| i.map_inner(|o| o.map_ids(&mut map_iri, &mut map_id)))
            .collect()
    }

    /// Returns the set of all blank identifiers in the given document.
    pub fn blank_ids(&self) -> HashSet<&B>
    where
        B: Eq + Hash,
    {
        self.traverse().filter_map(|f| f.into_id().and_then(Id::into_blank)).collect()
    }

    /// Returns the main node object of the document, if any.
    ///
    /// The main node is the unique top level (root) node object. If multiple
    /// node objects are on the root, `None` is returned.
    pub fn main_node(&self) -> Option<&Node<T, B>> {
        let mut result = None;

        for object in self {
            if let Object::Node(node) = object.inner() {
                if result.is_some() {
                    return None;
                }

                result = Some(&**node)
            }
        }

        result
    }

    /// Consumes the document and returns its main node object, if any.
    ///
    /// The main node is the unique top level (root) node object. If multiple
    /// node objects are on the root, `None` is returned.
    pub fn into_main_node(self) -> Option<Node<T, B>> {
        let mut result = None;

        for object in self {
            if let Object::Node(node) = object.into_inner() {
                if result.is_some() {
                    return None;
                }

                result = Some(*node)
            }
        }

        result
    }
}

impl<T: Hash + Eq, B: Hash + Eq> ExpandedDocument<T, B> {
    /// Inserts an object, unless an equal one is already present.
    ///
    /// Returns `true` if the object was added.
    pub fn insert(&mut self, object: IndexedObject<T, B>) -> bool {
        // Cloned rather than borrowed so it stays usable while `buckets` is
        // borrowed mutably below. A clone hashes identically to its original,
        // which is what the cached `full_hash` values rely on.
        let hasher = self.buckets.hasher().clone();
        let discriminant = hasher.hash_one(Discriminant(&object));

        // Split the borrow: the bucket lives in `buckets`, the candidates it
        // points at live in `objects`.
        let Self { objects, buckets } = self;
        let bucket = buckets.entry(discriminant).or_default();

        let full_hash = if bucket.is_empty() {
            None
        } else {
            let full_hash = hasher.hash_one(&object);

            for entry in bucket.iter_mut() {
                let candidate = entry.full_hash.get_or_insert_with(|| hasher.hash_one(&objects[entry.index]));

                if *candidate == full_hash && objects[entry.index] == object {
                    return false;
                }
            }

            Some(full_hash)
        };

        bucket.push(Entry {
            index: objects.len(),
            full_hash,
        });
        objects.push(object);
        true
    }

    /// Checks whether an equal object is present.
    pub fn contains(&self, object: &IndexedObject<T, B>) -> bool {
        let discriminant = self.buckets.hasher().hash_one(Discriminant(object));

        match self.buckets.get(&discriminant) {
            Some(bucket) => bucket.iter().any(|entry| self.objects[entry.index] == *object),
            None => false,
        }
    }

    /// Empties the document, handing back its objects.
    ///
    /// Used by the passes that rewrite every object and put it back: rewriting
    /// can make two objects equal, so the index has to be rebuilt rather than
    /// patched.
    fn drain(&mut self) -> std::vec::IntoIter<IndexedObject<T, B>> {
        self.buckets.clear();
        std::mem::take(&mut self.objects).into_iter()
    }
}

impl<T: Eq + Hash, B: Eq + Hash> From<Indexed<Node<T, B>>> for ExpandedDocument<T, B> {
    fn from(value: Indexed<Node<T, B>>) -> Self {
        let mut result = Self::default();

        result.insert(value.map_inner(Object::node));

        result
    }
}

impl<T: Eq + Hash, B: Eq + Hash> TryFromJson<T, B> for ExpandedDocument<T, B> {
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>, value: jstrict::Value) -> Result<Self, InvalidExpandedJson> {
        match value {
            jstrict::Value::Array(items) => {
                let mut result = Self::new();

                for item in items {
                    result.insert(Indexed::try_from_json_in(vocabulary, item)?);
                }

                Ok(result)
            }
            other => Err(InvalidExpandedJson::Unexpected(other.kind(), jstrict::Kind::Array)),
        }
    }
}

impl<T: Eq + Hash, B: Eq + Hash> PartialEq for ExpandedDocument<T, B> {
    /// Comparison between two expanded documents.
    ///
    /// Order-independent, as it was when this was backed by a set: both sides
    /// are deduplicated, so equal lengths plus containment one way is enough.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.objects.iter().all(|object| other.contains(object))
    }
}

impl<T: Eq + Hash, B: Eq + Hash> Eq for ExpandedDocument<T, B> {}

impl<T, B> IntoIterator for ExpandedDocument<T, B> {
    type IntoIter = IntoIter<T, B>;
    type Item = IndexedObject<T, B>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.objects.into_iter())
    }
}

impl<'a, T, B> IntoIterator for &'a ExpandedDocument<T, B> {
    type IntoIter = std::slice::Iter<'a, IndexedObject<T, B>>;
    type Item = &'a IndexedObject<T, B>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
/// Owning iterator over the objects of an expanded document.
pub struct IntoIter<T, B>(std::vec::IntoIter<IndexedObject<T, B>>);

impl<T, B> Iterator for IntoIter<T, B> {
    type Item = IndexedObject<T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<T: Hash + Eq, B: Hash + Eq> FromIterator<IndexedObject<T, B>> for ExpandedDocument<T, B> {
    fn from_iter<I: IntoIterator<Item = IndexedObject<T, B>>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let mut result = Self::with_capacity(iter.size_hint().0);
        result.extend(iter);
        result
    }
}

impl<T: Hash + Eq, B: Hash + Eq> Extend<IndexedObject<T, B>> for ExpandedDocument<T, B> {
    fn extend<I: IntoIterator<Item = IndexedObject<T, B>>>(&mut self, iter: I) {
        for object in iter {
            self.insert(object);
        }
    }
}

impl<T: Hash + Eq, B: Hash + Eq> From<IndexSet<IndexedObject<T, B>>> for ExpandedDocument<T, B> {
    fn from(set: IndexSet<IndexedObject<T, B>>) -> Self {
        set.into_iter().collect()
    }
}

impl<T: Hash + Eq, B: Hash + Eq> From<Vec<IndexedObject<T, B>>> for ExpandedDocument<T, B> {
    fn from(items: Vec<IndexedObject<T, B>>) -> Self {
        items.into_iter().collect()
    }
}

#[cfg(all(test, feature = "serde-json"))]
// Test code may panic on failure; that is the point of a test.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod serde_json_tests {
    use super::*;
    use crate::Id;
    use iri_rs::iri;

    #[test]
    fn expanded_into_serde_json_with_default_vocab() {
        let id = Id::iri(IriBuf::from(iri!("https://example.com/x")));
        let node = Node::<IriBuf, BlankIdBuf>::with_id(id);
        let doc: ExpandedDocument<IriBuf, BlankIdBuf> = Indexed::new(node, None).into();

        let json = doc.into_serde_json();
        let arr = json.as_array().expect("expanded document is an array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("@id").and_then(|v| v.as_str()), Some("https://example.com/x"));
    }
}

#[cfg(feature = "serde-json")]
impl<T, B> ExpandedDocument<T, B> {
    /// Converts the expanded document into a [`serde_json::Value`] using the
    /// given `vocabulary`.
    pub fn into_serde_json_with<N>(self, vocabulary: &N) -> serde_json::Value
    where
        N: Vocabulary<Iri = T, BlankId = B>,
    {
        use jsonld_syntax::IntoJsonWithContext;
        self.objects.into_json_with(vocabulary).into_serde_json()
    }
}

#[cfg(feature = "serde-json")]
impl<T, B> ExpandedDocument<T, B>
where
    (): Vocabulary<Iri = T, BlankId = B>,
{
    /// Converts the expanded document into a [`serde_json::Value`] using the
    /// default no-vocabulary.
    pub fn into_serde_json(self) -> serde_json::Value {
        self.into_serde_json_with(rdfx::vocabulary::no_vocabulary())
    }
}
