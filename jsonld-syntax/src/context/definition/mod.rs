use super::{TermDefinition, term_definition};
use crate::{Direction, Keyword, LenientLangTagBuf, Nullable, hash::IndexMap};
use educe::Educe;
use iri_rs::IriRefBuf;

mod import;
mod key;
mod reference;
mod type_;
mod version;
mod vocab;

pub use import::*;
pub use key::*;
pub use reference::*;
pub use type_::*;
pub use version::*;
pub use vocab::*;

/// Context definition: the object form of a context entry.
///
/// Holds the keyword entries a context may set (`@base`, `@vocab`, …) and the
/// term definitions it binds.
#[derive(PartialEq, Eq, Clone, Educe, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[educe(Default)]
pub struct Definition {
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@base", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@base` entry, setting the base IRI against which relative IRIs are resolved.
    pub base: Option<Nullable<IriRefBuf>>,

    #[cfg_attr(feature = "serde", serde(rename = "@import", default, skip_serializing_if = "Option::is_none"))]
    /// The `@import` entry, referencing a context to merge into this one.
    pub import: Option<IriRefBuf>,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@language", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@language` entry, tagging string values with a language.
    pub language: Option<Nullable<LenientLangTagBuf>>,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@direction", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@direction` entry, setting the base direction of string values.
    pub direction: Option<Nullable<Direction>>,

    #[cfg_attr(feature = "serde", serde(rename = "@propagate", default, skip_serializing_if = "Option::is_none"))]
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    pub propagate: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@protected", default, skip_serializing_if = "Option::is_none"))]
    /// The `@protected` entry, forbidding redefinition of the terms defined here.
    pub protected: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@type", default, skip_serializing_if = "Option::is_none"))]
    /// The `@type` entry, setting how the node types of the described documents
    /// are laid out and whether that setting is protected.
    pub type_: Option<Type>,

    #[cfg_attr(feature = "serde", serde(rename = "@version", default, skip_serializing_if = "Option::is_none"))]
    /// The `@version` entry, declaring the processing mode.
    pub version: Option<Version>,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@vocab", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@vocab` entry, setting the vocabulary against which terms expand.
    pub vocab: Option<Nullable<Vocab>>,

    #[cfg_attr(feature = "serde", serde(flatten))]
    /// Term definitions of this context.
    pub bindings: Bindings,
}

impl Definition {
    /// Creates an empty context definition, with no entry set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the value of the entry with the given key, if that entry is set.
    ///
    /// The key may name a keyword entry or a term binding. Keywords that a
    /// context definition cannot hold, such as `@id`, always yield `None`.
    pub fn get(&self, key: &KeyOrKeyword) -> Option<EntryValueRef<'_>> {
        match key {
            KeyOrKeyword::Keyword(k) => match k {
                Keyword::Base => self.base.as_ref().map(Nullable::as_ref).map(EntryValueRef::Base),
                Keyword::Import => self.import.as_ref().map(EntryValueRef::Import),
                Keyword::Language => self.language.as_ref().map(Nullable::as_ref).map(EntryValueRef::Language),
                Keyword::Direction => self.direction.map(EntryValueRef::Direction),
                Keyword::Propagate => self.propagate.map(EntryValueRef::Propagate),
                Keyword::Protected => self.protected.map(EntryValueRef::Protected),
                Keyword::Type => self.type_.map(EntryValueRef::Type),
                Keyword::Version => self.version.map(EntryValueRef::Version),
                Keyword::Vocab => self.vocab.as_ref().map(Nullable::as_ref).map(EntryValueRef::Vocab),
                _ => None,
            },
            KeyOrKeyword::Key(k) => self.bindings.get(k).map(EntryValueRef::Definition),
        }
    }

    /// Returns the term definition bound to the given term, if any.
    #[must_use]
    pub fn get_binding(&self, key: &Key) -> Option<Nullable<&TermDefinition>> {
        self.bindings.get(key)
    }
}

/// Term definitions of a context, keyed by term and kept in insertion order.
#[derive(PartialEq, Eq, Clone, Educe, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[educe(Default)]
pub struct Bindings(IndexMap<Key, Nullable<TermDefinition>>);

/// Iterator over the term definitions of a context, in insertion order.
pub struct BindingsIter<'a>(indexmap::map::Iter<'a, Key, Nullable<TermDefinition>>);

impl<'a> Iterator for BindingsIter<'a> {
    type Item = (&'a Key, Nullable<&'a TermDefinition>);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, d)| (k, d.as_ref()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for BindingsIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(k, d)| (k, d.as_ref()))
    }
}

impl ExactSizeIterator for BindingsIter<'_> {}

impl Bindings {
    /// Binds `key` to `def`, returning the definition it replaced, if any.
    pub fn insert(&mut self, key: Key, def: Nullable<TermDefinition>) -> Option<Nullable<TermDefinition>> {
        self.0.insert(key, def)
    }
}

impl Bindings {
    /// Creates an empty set of bindings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of bound terms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks whether no term is bound.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the definition bound to the given term, if any.
    ///
    /// The result is `Some(Nullable::Null)` for a term the context explicitly
    /// unsets by binding it to `null`.
    pub fn get(&self, key: &Key) -> Option<Nullable<&TermDefinition>> {
        self.0.get(key).map(Nullable::as_ref)
    }

    /// Returns the `i`th binding in insertion order, if there is one.
    #[must_use]
    pub fn get_entry(&self, i: usize) -> Option<(&Key, Nullable<&TermDefinition>)> {
        self.0.get_index(i).map(|(key, value)| (key, value.as_ref()))
    }

    /// Returns an iterator over the bindings, in insertion order.
    #[must_use]
    pub fn iter(&self) -> BindingsIter<'_> {
        BindingsIter(self.0.iter())
    }

    /// Binds `key` to `def`, returning the definition it replaced, if any.
    ///
    /// Same as [`insert`](Self::insert).
    pub fn insert_with(&mut self, key: Key, def: Nullable<TermDefinition>) -> Option<Nullable<TermDefinition>> {
        self.0.insert(key, def)
    }
}

impl IntoIterator for Bindings {
    type Item = (Key, Nullable<TermDefinition>);
    type IntoIter = indexmap::map::IntoIter<Key, Nullable<TermDefinition>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(Key, Nullable<TermDefinition>)> for Bindings {
    fn from_iter<T: IntoIterator<Item = (Key, Nullable<TermDefinition>)>>(iter: T) -> Self {
        let mut result = Self::new();

        for (key, binding) in iter {
            result.0.insert(key, binding);
        }

        result
    }
}

/// Fragment of a context definition.
pub enum FragmentRef<'a> {
    /// An entry of the definition, key and value together.
    Entry(EntryRef<'a>),

    /// The key of an entry of the definition.
    Key(EntryKeyRef<'a>),

    /// The value of an entry of the definition.
    Value(EntryValueRef<'a>),

    /// A fragment of one of the term definitions.
    TermDefinitionFragment(term_definition::FragmentRef<'a>),
}

impl<'a> FragmentRef<'a> {
    /// Checks whether this fragment is an entry key.
    #[must_use]
    pub fn is_key(&self) -> bool {
        match self {
            Self::Key(_) => true,
            Self::TermDefinitionFragment(f) => f.is_key(),
            _ => false,
        }
    }

    /// Checks whether this fragment is an entry, key and value together.
    #[must_use]
    pub fn is_entry(&self) -> bool {
        match self {
            Self::Entry(_) => true,
            Self::TermDefinitionFragment(f) => f.is_entry(),
            _ => false,
        }
    }

    /// Checks whether this fragment is a JSON array.
    #[must_use]
    pub fn is_array(&self) -> bool {
        match self {
            Self::TermDefinitionFragment(i) => i.is_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment is a JSON object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        match self {
            Self::Value(v) => v.is_object(),
            Self::TermDefinitionFragment(v) => v.is_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    #[must_use]
    pub fn sub_items(&self) -> SubItems<'a> {
        match self {
            Self::Entry(e) => SubItems::Entry(Some(e.key()), Some(Box::new(e.value()))),
            Self::Key(_) => SubItems::None,
            Self::Value(v) => SubItems::Value(v.sub_items()),
            Self::TermDefinitionFragment(f) => SubItems::TermDefinitionFragment(f.sub_fragments()),
        }
    }
}

/// Iterator over the fragments held by a context entry value.
pub enum EntryValueSubItems<'a> {
    /// Nothing to iterate over: the value holds no fragment of its own.
    None,
    /// The entries of an expanded term definition.
    TermDefinitionFragment(Box<term_definition::Entries<'a>>),
}

impl<'a> Iterator for EntryValueSubItems<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::TermDefinitionFragment(d) => d.next().map(|e| FragmentRef::TermDefinitionFragment(term_definition::FragmentRef::Entry(e))),
        }
    }
}

/// Iterator over the fragments held by a context definition.
pub enum SubItems<'a> {
    /// Nothing to iterate over.
    None,
    /// The key and value of an entry, yielded in that order.
    Entry(Option<EntryKeyRef<'a>>, Option<Box<EntryValueRef<'a>>>),
    /// The fragments held by an entry value.
    Value(EntryValueSubItems<'a>),
    /// The fragments of a term definition.
    TermDefinitionFragment(term_definition::SubFragments<'a>),
}

impl<'a> Iterator for SubItems<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Entry(k, v) => k.take().map(FragmentRef::Key).or_else(|| v.take().map(|v| FragmentRef::Value(*v))),
            Self::Value(d) => d.next(),
            Self::TermDefinitionFragment(d) => d.next().map(FragmentRef::TermDefinitionFragment),
        }
    }
}

#[cfg(all(test, feature = "serde"))]
// Test code may panic on failure; that is the point of a test.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::Definition;

    #[test]
    fn deserialize_null_vocab() {
        let definition: Definition = jstrict::from_value(jstrict::json!({
            "@vocab": null
        }))
        .unwrap();
        assert_eq!(definition.vocab, Some(crate::Nullable::Null));
    }

    #[test]
    fn deserialize_no_vocab() {
        let definition: Definition = jstrict::from_value(jstrict::json!({})).unwrap();
        assert_eq!(definition.vocab, None);
    }
}
