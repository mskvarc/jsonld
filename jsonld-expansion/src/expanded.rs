use jsonld_core::{Id, IndexedObject};
use std::hash::Hash;

/// Result of expanding a fragment of a JSON-LD document.
///
/// Expansion may leave nothing behind, produce a single object, or turn one
/// element into several — which is why it is not simply an
/// [`IndexedObject`].
pub enum Expanded<T, B> {
    /// The fragment expanded to nothing, and is dropped.
    Null,
    /// The fragment expanded to a single object.
    Object(IndexedObject<T, B>),
    /// The fragment expanded to any number of objects.
    Array(Vec<IndexedObject<T, B>>),
}

impl<T, B> Expanded<T, B> {
    /// Returns the number of objects the fragment expanded to.
    pub fn len(&self) -> usize {
        match self {
            Expanded::Null => 0,
            Expanded::Object(_) => 1,
            Expanded::Array(ary) => ary.len(),
        }
    }

    /// Checks whether the fragment expanded to no object at all, either
    /// because it expanded to nothing or to an empty array.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks whether the fragment expanded to nothing, as opposed to an empty
    /// array of objects.
    pub fn is_null(&self) -> bool {
        matches!(self, Expanded::Null)
    }

    /// Checks whether the fragment expanded to a single list object (a `@list`
    /// entry).
    pub fn is_list(&self) -> bool {
        match self {
            Expanded::Object(o) => o.is_list(),
            _ => false,
        }
    }

    /// Returns an iterator over the expanded objects.
    pub fn iter(&self) -> Iter<'_, T, B> {
        match self {
            Expanded::Null => Iter::Null,
            Expanded::Object(o) => Iter::Object(Some(o)),
            Expanded::Array(ary) => Iter::Array(ary.iter()),
        }
    }

    /// Rewrites every IRI and identifier of this expansion result
    /// (recursively) with the given functions.
    ///
    /// `map_iri` translates the datatype IRI of the literal values, `map_id`
    /// the node identifiers, node types and property identifiers. Use it to
    /// move a result produced against one vocabulary into the identifier space
    /// of another.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> Expanded<U, C>
    where
        U: Eq + Hash,
        C: Eq + Hash,
    {
        match self {
            Expanded::Null => Expanded::Null,
            Expanded::Object(object) => Expanded::Object(object.map_inner(|o| o.map_ids(&mut map_iri, &mut map_id))),
            Expanded::Array(array) => Expanded::Array(
                array
                    .into_iter()
                    .map(|object| object.map_inner(|o| o.map_ids(&mut map_iri, &mut map_id)))
                    .collect(),
            ),
        }
    }
}

impl<T, B> IntoIterator for Expanded<T, B> {
    type Item = IndexedObject<T, B>;
    type IntoIter = IntoIter<T, B>;

    fn into_iter(self) -> IntoIter<T, B> {
        match self {
            Expanded::Null => IntoIter::Null,
            Expanded::Object(o) => IntoIter::Object(Some(o)),
            Expanded::Array(ary) => IntoIter::Array(ary.into_iter()),
        }
    }
}

impl<'a, T, B> IntoIterator for &'a Expanded<T, B> {
    type Item = &'a IndexedObject<T, B>;
    type IntoIter = Iter<'a, T, B>;

    fn into_iter(self) -> Iter<'a, T, B> {
        self.iter()
    }
}

/// Iterator over the objects of an expansion result.
pub enum Iter<'a, T, B> {
    /// Iterator over an expansion result that produced nothing.
    Null,
    /// Iterator over a single expanded object, until it has been yielded.
    Object(Option<&'a IndexedObject<T, B>>),
    /// Iterator over the objects of an expanded array.
    Array(std::slice::Iter<'a, IndexedObject<T, B>>),
}

impl<'a, T, B> Iterator for Iter<'a, T, B> {
    type Item = &'a IndexedObject<T, B>;

    fn next(&mut self) -> Option<&'a IndexedObject<T, B>> {
        match self {
            Iter::Null => None,
            Iter::Object(o) => {
                let mut result = None;
                std::mem::swap(o, &mut result);
                result
            }
            Iter::Array(it) => it.next(),
        }
    }
}

/// Owning iterator over the objects of an expansion result.
pub enum IntoIter<T, B> {
    /// Iterator over an expansion result that produced nothing.
    Null,
    /// Iterator over a single expanded object, until it has been yielded.
    Object(Option<IndexedObject<T, B>>),
    /// Iterator over the objects of an expanded array.
    Array(std::vec::IntoIter<IndexedObject<T, B>>),
}

impl<T, B> Iterator for IntoIter<T, B> {
    type Item = IndexedObject<T, B>;

    fn next(&mut self) -> Option<IndexedObject<T, B>> {
        match self {
            IntoIter::Null => None,
            IntoIter::Object(o) => {
                let mut result = None;
                std::mem::swap(o, &mut result);
                result
            }
            IntoIter::Array(it) => it.next(),
        }
    }
}

impl<T, B> From<IndexedObject<T, B>> for Expanded<T, B> {
    fn from(obj: IndexedObject<T, B>) -> Expanded<T, B> {
        Expanded::Object(obj)
    }
}

impl<T, B> From<Vec<IndexedObject<T, B>>> for Expanded<T, B> {
    fn from(list: Vec<IndexedObject<T, B>>) -> Expanded<T, B> {
        Expanded::Array(list)
    }
}
