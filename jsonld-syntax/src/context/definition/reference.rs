use super::{BindingsIter, Definition, EntryValueSubItems, Key, Type, Version, Vocab};
use crate::{Direction, LenientLangTagBuf, Nullable, context::TermDefinition};

use iri_rs::IriRefBuf;

impl Definition {
    /// Returns an iterator over the entries of this value.
    pub fn iter(&self) -> Entries<'_> {
        Entries {
            base: self.base.as_ref().map(Nullable::as_ref),
            import: self.import.as_ref(),
            language: self.language.as_ref().map(Nullable::as_ref),
            direction: self.direction,
            propagate: self.propagate,
            protected: self.protected,
            type_: self.type_,
            version: self.version,
            vocab: self.vocab.as_ref().map(Nullable::as_ref),
            bindings: self.bindings.iter(),
        }
    }
}

/// Iterator over the entries of a context definition.
pub struct Entries<'a> {
    base: Option<Nullable<&'a IriRefBuf>>,
    import: Option<&'a IriRefBuf>,
    language: Option<Nullable<&'a LenientLangTagBuf>>,
    direction: Option<Nullable<Direction>>,
    propagate: Option<bool>,
    protected: Option<bool>,
    type_: Option<Type>,
    version: Option<Version>,
    vocab: Option<Nullable<&'a Vocab>>,
    bindings: BindingsIter<'a>,
}

impl<'a> Iterator for Entries<'a> {
    type Item = EntryRef<'a>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = self.bindings.len();

        if self.base.is_some() {
            len += 1
        }

        if self.import.is_some() {
            len += 1
        }

        if self.language.is_some() {
            len += 1
        }

        if self.direction.is_some() {
            len += 1
        }

        if self.propagate.is_some() {
            len += 1
        }

        if self.protected.is_some() {
            len += 1
        }

        if self.type_.is_some() {
            len += 1
        }

        if self.version.is_some() {
            len += 1
        }

        if self.vocab.is_some() {
            len += 1
        }

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self.base.take() {
            Some(value) => Some(EntryRef::Base(value)),
            None => match self.import.take() {
                Some(value) => Some(EntryRef::Import(value)),
                None => match self.language.take() {
                    Some(value) => Some(EntryRef::Language(value)),
                    None => match self.direction.take() {
                        Some(value) => Some(EntryRef::Direction(value)),
                        None => match self.propagate.take() {
                            Some(value) => Some(EntryRef::Propagate(value)),
                            None => match self.protected.take() {
                                Some(value) => Some(EntryRef::Protected(value)),
                                None => match self.type_.take() {
                                    Some(value) => Some(EntryRef::Type(value)),
                                    None => match self.version.take() {
                                        Some(value) => Some(EntryRef::Version(value)),
                                        None => match self.vocab.take() {
                                            Some(value) => Some(EntryRef::Vocab(value)),
                                            None => self.bindings.next().map(|(k, v)| EntryRef::Definition(k, v)),
                                        },
                                    },
                                },
                            },
                        },
                    },
                },
            },
        }
    }
}

impl<'a> ExactSizeIterator for Entries<'a> {}

/// Value of a context definition entry.
pub enum EntryValueRef<'a> {
    /// The `@base` entry, setting the base IRI against which relative IRIs are resolved.
    Base(Nullable<&'a IriRefBuf>),
    /// The `@import` entry, referencing a context to merge into this one.
    Import(&'a IriRefBuf),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
    /// The `@type` entry, giving the type of the node or the values.
    Type(Type),
    /// The `@version` entry, declaring the processing mode.
    Version(Version),
    /// The `@vocab` entry, setting the vocabulary against which terms expand.
    Vocab(Nullable<&'a Vocab>),
    /// A term definition.
    Definition(Nullable<&'a TermDefinition>),
}

impl<'a> EntryValueRef<'a> {
    /// Checks whether this `EntryValueRef` is object.
    pub fn is_object(&self) -> bool {
        match self {
            Self::Type(_) => true,
            Self::Definition(Nullable::Some(d)) => d.is_object(),
            _ => false,
        }
    }

    /// Returns the sub items of this `EntryValueRef`.
    pub fn sub_items(&self) -> EntryValueSubItems<'a> {
        match self {
            Self::Definition(Nullable::Some(TermDefinition::Expanded(e))) => EntryValueSubItems::TermDefinitionFragment(Box::new(e.iter())),
            _ => EntryValueSubItems::None,
        }
    }
}

/// Entry of a context definition, key and value together.
pub enum EntryRef<'a> {
    /// The `@base` entry, setting the base IRI against which relative IRIs are resolved.
    Base(Nullable<&'a IriRefBuf>),
    /// The `@import` entry, referencing a context to merge into this one.
    Import(&'a IriRefBuf),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
    /// The `@type` entry, giving the type of the node or the values.
    Type(Type),
    /// The `@version` entry, declaring the processing mode.
    Version(Version),
    /// The `@vocab` entry, setting the vocabulary against which terms expand.
    Vocab(Nullable<&'a Vocab>),
    /// A term definition.
    Definition(&'a Key, Nullable<&'a TermDefinition>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Key of a context definition entry.
pub enum EntryKeyRef<'a> {
    /// The `@base` entry, setting the base IRI against which relative IRIs are resolved.
    Base,
    /// The `@import` entry, referencing a context to merge into this one.
    Import,
    /// The `@language` entry, tagging string values with a language.
    Language,
    /// The `@direction` entry, setting the base direction of string values.
    Direction,
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate,
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected,
    /// The `@type` entry, giving the type of the node or the values.
    Type,
    /// The `@version` entry, declaring the processing mode.
    Version,
    /// The `@vocab` entry, setting the vocabulary against which terms expand.
    Vocab,
    /// A term definition.
    Definition(&'a Key),
}

impl<'a> EntryKeyRef<'a> {
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'a str {
        match self {
            Self::Base => "@base",
            Self::Import => "@import",
            Self::Language => "@language",
            Self::Direction => "@direction",
            Self::Propagate => "@propagate",
            Self::Protected => "@protected",
            Self::Type => "@type",
            Self::Version => "@version",
            Self::Vocab => "@vocab",
            Self::Definition(d) => d.as_str(),
        }
    }
}

impl<'a> EntryRef<'a> {
    /// Consumes this `EntryKeyRef`, returning its key.
    pub fn into_key(self) -> EntryKeyRef<'a> {
        match self {
            Self::Base(_) => EntryKeyRef::Base,
            Self::Import(_) => EntryKeyRef::Import,
            Self::Language(_) => EntryKeyRef::Language,
            Self::Direction(_) => EntryKeyRef::Direction,
            Self::Propagate(_) => EntryKeyRef::Propagate,
            Self::Protected(_) => EntryKeyRef::Protected,
            Self::Type(_) => EntryKeyRef::Type,
            Self::Version(_) => EntryKeyRef::Version,
            Self::Vocab(_) => EntryKeyRef::Vocab,
            Self::Definition(key, _) => EntryKeyRef::Definition(key),
        }
    }

    /// Returns the key of this `EntryKeyRef`.
    pub fn key(&self) -> EntryKeyRef<'a> {
        match self {
            Self::Base(_) => EntryKeyRef::Base,
            Self::Import(_) => EntryKeyRef::Import,
            Self::Language(_) => EntryKeyRef::Language,
            Self::Direction(_) => EntryKeyRef::Direction,
            Self::Propagate(_) => EntryKeyRef::Propagate,
            Self::Protected(_) => EntryKeyRef::Protected,
            Self::Type(_) => EntryKeyRef::Type,
            Self::Version(_) => EntryKeyRef::Version,
            Self::Vocab(_) => EntryKeyRef::Vocab,
            Self::Definition(key, _) => EntryKeyRef::Definition(key),
        }
    }

    /// Consumes this `EntryKeyRef`, returning its value.
    pub fn into_value(self) -> EntryValueRef<'a> {
        match self {
            Self::Base(v) => EntryValueRef::Base(v),
            Self::Import(v) => EntryValueRef::Import(v),
            Self::Language(v) => EntryValueRef::Language(v),
            Self::Direction(v) => EntryValueRef::Direction(v),
            Self::Propagate(v) => EntryValueRef::Propagate(v),
            Self::Protected(v) => EntryValueRef::Protected(v),
            Self::Type(v) => EntryValueRef::Type(v),
            Self::Version(v) => EntryValueRef::Version(v),
            Self::Vocab(v) => EntryValueRef::Vocab(v),
            Self::Definition(_, b) => EntryValueRef::Definition(b),
        }
    }

    /// Returns the value of this `EntryKeyRef`.
    pub fn value(&self) -> EntryValueRef<'a> {
        match self {
            Self::Base(v) => EntryValueRef::Base(*v),
            Self::Import(v) => EntryValueRef::Import(v),
            Self::Language(v) => EntryValueRef::Language(*v),
            Self::Direction(v) => EntryValueRef::Direction(*v),
            Self::Propagate(v) => EntryValueRef::Propagate(*v),
            Self::Protected(v) => EntryValueRef::Protected(*v),
            Self::Type(v) => EntryValueRef::Type(*v),
            Self::Version(v) => EntryValueRef::Version(*v),
            Self::Vocab(v) => EntryValueRef::Vocab(*v),
            Self::Definition(_, b) => EntryValueRef::Definition(*b),
        }
    }

    /// Consumes this `EntryKeyRef`, returning its key value.
    pub fn into_key_value(self) -> (EntryKeyRef<'a>, EntryValueRef<'a>) {
        self.key_value()
    }

    /// Returns the key value of this `EntryKeyRef`.
    pub fn key_value(&self) -> (EntryKeyRef<'a>, EntryValueRef<'a>) {
        match self {
            Self::Base(v) => (EntryKeyRef::Base, EntryValueRef::Base(*v)),
            Self::Import(v) => (EntryKeyRef::Import, EntryValueRef::Import(v)),
            Self::Language(v) => (EntryKeyRef::Language, EntryValueRef::Language(*v)),
            Self::Direction(v) => (EntryKeyRef::Direction, EntryValueRef::Direction(*v)),
            Self::Propagate(v) => (EntryKeyRef::Propagate, EntryValueRef::Propagate(*v)),
            Self::Protected(v) => (EntryKeyRef::Protected, EntryValueRef::Protected(*v)),
            Self::Type(v) => (EntryKeyRef::Type, EntryValueRef::Type(*v)),
            Self::Version(v) => (EntryKeyRef::Version, EntryValueRef::Version(*v)),
            Self::Vocab(v) => (EntryKeyRef::Vocab, EntryValueRef::Vocab(*v)),
            Self::Definition(key, b) => (EntryKeyRef::Definition(key), EntryValueRef::Definition(*b)),
        }
    }
}
