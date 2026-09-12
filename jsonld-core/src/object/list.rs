use super::{Any, InvalidExpandedJson, MappedEq};
use crate::{HashMap, Id, IndexedObject, Relabel, TryFromJson, ValidId};
use contextual::WithContext;
use derive_where::derive_where;
use jsonld_syntax::{IntoJson, IntoJsonWithContext};
use rdfx::{
    LocalGenerator,
    vocabulary::{Vocabulary, VocabularyMut},
};
use std::hash::Hash;

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Hash)]
#[derive_where(PartialEq; T: Eq + Hash, B: Eq + Hash)]
/// List object.
pub struct List<T, B> {
    entry: Vec<IndexedObject<T, B>>,
}

impl<T: Eq + Hash, B: Eq + Hash> Eq for List<T, B> {}

impl<T, B> List<T, B> {
    /// Creates a new list object.
    #[must_use]
    pub fn new(objects: Vec<IndexedObject<T, B>>) -> Self {
        Self { entry: objects }
    }

    /// Returns the number of objects in the list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entry.len()
    }

    /// Checks whether the list holds no objects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entry.is_empty()
    }

    /// Returns the objects of the `@list` entry, in list order.
    ///
    /// Alias for [`as_slice`](Self::as_slice).
    #[must_use]
    pub fn entry(&self) -> &[IndexedObject<T, B>] {
        &self.entry
    }

    /// Mutably borrows the objects of the `@list` entry.
    pub fn entry_mut(&mut self) -> &mut Vec<IndexedObject<T, B>> {
        &mut self.entry
    }

    /// Returns the objects of the list as a slice, in list order.
    #[must_use]
    pub fn as_slice(&self) -> &[IndexedObject<T, B>] {
        self.entry.as_slice()
    }

    /// Returns the objects of the list as a mutable slice, in list order.
    pub fn as_mut_slice(&mut self) -> &mut [IndexedObject<T, B>] {
        self.entry.as_mut_slice()
    }

    /// Consumes the list, returning the objects of its `@list` entry.
    #[must_use]
    pub fn into_entry(self) -> Vec<IndexedObject<T, B>> {
        self.entry
    }

    /// Appends an object to the end of the list.
    pub fn push(&mut self, object: IndexedObject<T, B>) {
        self.entry.push(object);
    }

    /// Removes the last object of the list and returns it.
    pub fn pop(&mut self) -> Option<IndexedObject<T, B>> {
        self.entry.pop()
    }

    /// Returns an iterator over the objects of the list, in list order.
    pub fn iter(&self) -> core::slice::Iter<'_, IndexedObject<T, B>> {
        self.entry.iter()
    }

    /// Returns a mutable iterator over the objects of the list, in list
    /// order.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, IndexedObject<T, B>> {
        self.entry.iter_mut()
    }

    /// Puts every literal of the list into canonical form, using the given
    /// `buffer` to render numbers.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer) {
        for object in self {
            object.canonicalize_with(buffer);
        }
    }

    /// Puts every literal of the list into canonical form.
    pub fn canonicalize(&mut self) {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer);
    }

    /// Rewrites every IRI and identifier of the list (recursively) with the
    /// given functions.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> List<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        self.map_ids_with(&mut map_iri, &mut map_id)
    }

    pub(crate) fn map_ids_with<U, C>(self, map_iri: &mut impl FnMut(T) -> U, map_id: &mut impl FnMut(Id<T, B>) -> Id<U, C>) -> List<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        List::new(
            self.entry
                .into_iter()
                .map(|indexed_object| indexed_object.map_inner(|object| object.map_ids_with(map_iri, map_id)))
                .collect(),
        )
    }
}

impl<T, B> Relabel<T, B> for List<T, B> {
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
        for object in self {
            object.relabel_with(vocabulary, generator, relabeling)?;
        }
        Ok(())
    }
}

impl<T: Eq + Hash, B: Eq + Hash> List<T, B> {
    pub(crate) fn try_from_json_object_in(
        vocabulary: &mut impl VocabularyMut<Iri = T, BlankId = B>,
        object: jstrict::Object,
        list_entry: jstrict::object::Entry,
    ) -> Result<Self, InvalidExpandedJson> {
        let list = Vec::try_from_json_in(vocabulary, list_entry.value)?;

        match object.into_iter().next() {
            Some(_) => Err(InvalidExpandedJson::UnexpectedEntry),
            None => Ok(Self::new(list)),
        }
    }
}

impl<T, B> Any<T, B> for List<T, B> {
    fn as_ref(&self) -> super::Ref<'_, T, B> {
        super::Ref::List(self)
    }
}

impl<T: Eq + Hash, B: Eq + Hash> MappedEq for List<T, B> {
    type BlankId = B;

    fn mapped_eq<'a, 'b, F: Clone + Fn(&'a B) -> &'b B>(&'a self, other: &Self, f: F) -> bool
    where
        B: 'a + 'b,
    {
        self.entry.mapped_eq(&other.entry, f)
    }
}

impl<'a, T, B> IntoIterator for &'a List<T, B> {
    type Item = &'a IndexedObject<T, B>;
    type IntoIter = core::slice::Iter<'a, IndexedObject<T, B>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, B> IntoIterator for &'a mut List<T, B> {
    type Item = &'a mut IndexedObject<T, B>;
    type IntoIter = core::slice::IterMut<'a, IndexedObject<T, B>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, B> IntoIterator for List<T, B> {
    type Item = IndexedObject<T, B>;
    type IntoIter = std::vec::IntoIter<IndexedObject<T, B>>;

    fn into_iter(self) -> Self::IntoIter {
        self.entry.into_iter()
    }
}

/// List object fragment.
pub enum FragmentRef<'a, T, B> {
    /// "@list" entry.
    Entry(&'a [IndexedObject<T, B>]),
}

impl<'a, T, B> FragmentRef<'a, T, B> {
    /// Returns the sub-fragments of this fragment: the objects of the list.
    #[must_use]
    pub fn sub_fragments(&self) -> SubFragments<'a, T, B> {
        match self {
            Self::Entry(e) => SubFragments(e.iter()),
        }
    }
}

/// Iterator over the sub-fragments of a list fragment.
pub struct SubFragments<'a, T, B>(core::slice::Iter<'a, IndexedObject<T, B>>);

impl<'a, T, B> Iterator for SubFragments<'a, T, B> {
    type Item = super::FragmentRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(super::FragmentRef::IndexedObject)
    }
}

impl<T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoJsonWithContext<N> for List<T, B> {
    fn into_json_with(self, vocabulary: &N) -> jstrict::Value {
        let mut obj = jstrict::Object::new();

        obj.insert("@list".into(), self.entry.into_with(vocabulary).into_json());

        obj.into()
    }
}
