use std::hash::{BuildHasher, Hash};

#[derive(Debug, Default, Clone, Copy)]
/// Hasher builder giving the same hashes across runs, so multiset
/// iteration order is reproducible.
pub struct DeterministicHasherBuilder;

impl BuildHasher for DeterministicHasherBuilder {
    type Hasher = foldhash::fast::FoldHasher<'static>;

    fn build_hasher(&self) -> Self::Hasher {
        foldhash::fast::FixedState::default().build_hasher()
    }
}

use jsonld_syntax::IntoJsonWithContext;

/// Multi-set of values.
#[derive(Debug, Clone)]
pub struct Multiset<T, S = DeterministicHasherBuilder> {
    data: Vec<T>,
    hasher: S,
}

impl<T, S: Default> Default for Multiset<T, S> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            hasher: S::default(),
        }
    }
}

impl<T, S> Multiset<T, S> {
    /// Creates a new `Multiset`.
    pub fn new() -> Self
    where
        S: Default,
    {
        Self::default()
    }

    /// Returns this `Multiset` with its capacity set.
    pub fn with_capacity(cap: usize) -> Self
    where
        S: Default,
    {
        Self {
            data: Vec::with_capacity(cap),
            hasher: S::default(),
        }
    }

    /// Returns the number of entries of this `Multiset`.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Checks whether this `Multiset` is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Checks whether this `Multiset` contains.
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.data.contains(value)
    }

    /// Returns an iterator over the entries of this `Multiset`.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.data.iter()
    }

    /// Returns a mutable iterator over the entries of this `Multiset`.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, T> {
        self.data.iter_mut()
    }

    /// Borrows this `Multiset` as slice, if it is one.
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    // pub fn into_stripped(self) -> Multiset<locspan::Stripped<T>, S> {
    // 	Multiset { data: unsafe { core::mem::transmute(self.data) }, hasher: self.hasher }
    // }
}

impl<T: Hash, S: BuildHasher> Multiset<T, S> {
    /// Builds a multiset holding a single value.
    pub fn singleton(value: T) -> Self
    where
        S: Default,
    {
        let mut result = Self::new();
        result.insert(value);
        result
    }

    /// Inserts an entry into this `Multiset`, returning the entry it replaced.
    pub fn insert(&mut self, value: T) {
        self.data.push(value);
    }

    /// Checks whether this `Multiset` insert unique.
    pub fn insert_unique(&mut self, value: T) -> bool
    where
        T: PartialEq,
    {
        if self.contains(&value) {
            false
        } else {
            self.insert(value);
            true
        }
    }
}

impl<'a, T, S> IntoIterator for &'a Multiset<T, S> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, S> IntoIterator for &'a mut Multiset<T, S> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, S> IntoIterator for Multiset<T, S> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<T: Hash, S: Default + BuildHasher> FromIterator<T> for Multiset<T, S> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut result = Self::new();

        for item in iter {
            result.insert(item)
        }

        result
    }
}

impl<T: Hash, S: BuildHasher> Extend<T> for Multiset<T, S> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.insert(item)
        }
    }
}

impl<T: PartialEq<U>, U, S, P> PartialEq<Multiset<U, P>> for Multiset<T, S> {
    fn eq(&self, other: &Multiset<U, P>) -> bool {
        compare_unordered(&self.data, &other.data)
    }
}

pub(crate) fn compare_unordered<T: PartialEq<U>, U>(a: &[T], b: &[U]) -> bool {
    if a.len() == b.len() {
        let mut free_indexes = Vec::new();
        free_indexes.resize(a.len(), true);

        for item in a {
            match free_indexes.iter_mut().enumerate().find(|(i, free)| **free && item == &b[*i]) {
                Some((_, free)) => *free = false,
                None => return false,
            }
        }

        true
    } else {
        false
    }
}

pub(crate) fn compare_unordered_opt<T: PartialEq<U>, U>(a: Option<&[T]>, b: Option<&[U]>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => compare_unordered(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl<T: Eq, S> Eq for Multiset<T, S> {}

impl<T: Hash, S: BuildHasher> Hash for Multiset<T, S> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut hash = 0u64;

        for item in self {
            hash = hash.wrapping_add(self.hasher.hash_one(item));
        }

        state.write_u64(hash)
    }
}

impl<T: IntoJsonWithContext<N>, S, N> IntoJsonWithContext<N> for Multiset<T, S> {
    fn into_json_with(self, vocabulary: &N) -> jstrict::Value {
        jstrict::Value::Array(self.into_iter().map(|item| item.into_json_with(vocabulary)).collect())
    }
}
