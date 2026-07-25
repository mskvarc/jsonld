use crate::{CompactIri, CompactIriBuf, Container, ContainerKind, Direction, Keyword, LenientLangTag, LenientLangTagBuf, Nullable, container, context};
use educe::Educe;
use iri_rs::{Iri, IriBuf};
use rdfx::{BlankId, BlankIdBuf};

mod id;
mod index;
mod nest;
mod type_;

pub use id::*;
pub use index::*;
pub use nest::*;
pub use type_::*;

/// Term definition.
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum TermDefinition {
    /// A term mapped directly to an IRI, a compact IRI or a blank node.
    Simple(Simple),
    /// A term defined by an object of entries.
    Expanded(Box<Expanded>),
}

impl TermDefinition {
    /// Checks whether this `TermDefinition` is expanded.
    pub fn is_expanded(&self) -> bool {
        matches!(self, Self::Expanded(_))
    }

    /// Checks whether this `TermDefinition` is object.
    pub fn is_object(&self) -> bool {
        self.is_expanded()
    }

    /// Borrows this `TermDefinition` as expanded, if it is one.
    pub fn as_expanded(&self) -> ExpandedRef<'_> {
        match self {
            Self::Simple(term) => ExpandedRef {
                id: Some(Nullable::Some(term.as_str().into())),
                ..Default::default()
            },
            Self::Expanded(e) => e.as_expanded_ref(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
/// Simple term definition: the string a term maps to.
pub struct Simple(pub(crate) String);

impl Simple {
    /// Borrows this `Simple` as IRI, if it is one.
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Borrows this `Simple` as compact IRI, if it is one.
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(&self.0).ok()
    }

    /// Borrows this `Simple` as blank id, if it is one.
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(&self.0).ok()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes this `Simple`, returning its string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<IriBuf> for Simple {
    fn from(value: IriBuf) -> Self {
        Self(value.into_inner())
    }
}

impl From<CompactIriBuf> for Simple {
    fn from(value: CompactIriBuf) -> Self {
        Self(value.into_string())
    }
}

impl From<BlankIdBuf> for Simple {
    fn from(value: BlankIdBuf) -> Self {
        Self(value.to_string())
    }
}

/// Expanded term definition.
#[derive(PartialEq, Eq, Clone, Educe, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[educe(Default)]
pub struct Expanded {
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@id", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    pub id: Option<Nullable<Id>>,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@type", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@type` entry, giving the type of the node or the values.
    pub type_: Option<Nullable<Type>>,

    #[cfg_attr(feature = "serde", serde(rename = "@context", default, skip_serializing_if = "Option::is_none"))]
    /// The `@context` entry, holding a context local to this definition.
    pub context: Option<Box<context::Context>>,

    #[cfg_attr(feature = "serde", serde(rename = "@reverse", default, skip_serializing_if = "Option::is_none"))]
    /// The `@reverse` entry, mapping the term to a reverse property.
    pub reverse: Option<context::definition::Key>,

    #[cfg_attr(feature = "serde", serde(rename = "@index", default, skip_serializing_if = "Option::is_none"))]
    /// The `@index` entry, indexing the value within its container.
    pub index: Option<Index>,

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

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@container", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@container` entry, declaring how values of the term are laid out.
    pub container: Option<Nullable<Container>>,

    #[cfg_attr(feature = "serde", serde(rename = "@nest", default, skip_serializing_if = "Option::is_none"))]
    /// The `@nest` entry, gathering properties under a nesting term.
    pub nest: Option<Nest>,

    #[cfg_attr(feature = "serde", serde(rename = "@prefix", default, skip_serializing_if = "Option::is_none"))]
    /// The `@prefix` entry, allowing the term to expand compact IRIs.
    pub prefix: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@propagate", default, skip_serializing_if = "Option::is_none"))]
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    pub propagate: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@protected", default, skip_serializing_if = "Option::is_none"))]
    /// The `@protected` entry, forbidding redefinition of the term.
    pub protected: Option<bool>,
}

impl Expanded {
    /// Creates a new `Expanded`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks whether this `Expanded` is null.
    pub fn is_null(&self) -> bool {
        matches!(&self.id, None | Some(Nullable::Null))
            && self.type_.is_none()
            && self.context.is_none()
            && self.reverse.is_none()
            && self.index.is_none()
            && self.language.is_none()
            && self.direction.is_none()
            && self.container.is_none()
            && self.nest.is_none()
            && self.prefix.is_none()
            && self.propagate.is_none()
            && self.protected.is_none()
    }

    /// Checks whether this `Expanded` is simple definition.
    pub fn is_simple_definition(&self) -> bool {
        matches!(&self.id, Some(Nullable::Some(_)))
            && self.type_.is_none()
            && self.context.is_none()
            && self.reverse.is_none()
            && self.index.is_none()
            && self.language.is_none()
            && self.direction.is_none()
            && self.container.is_none()
            && self.nest.is_none()
            && self.prefix.is_none()
            && self.propagate.is_none()
            && self.protected.is_none()
    }

    /// Reduces this definition to its simple form when the expanded form
    /// carries nothing but an `@id`.
    pub fn simplify(self) -> Nullable<TermDefinition> {
        if self.is_null() {
            return Nullable::Null;
        }
        match self {
            Self {
                id: Some(Nullable::Some(id)),
                type_: None,
                context: None,
                reverse: None,
                index: None,
                language: None,
                direction: None,
                container: None,
                nest: None,
                prefix: None,
                propagate: None,
                protected: None,
            } => Nullable::Some(TermDefinition::Simple(Simple(id.into_string()))),
            other => Nullable::Some(TermDefinition::Expanded(Box::new(other))),
        }
    }

    /// Returns an iterator over the entries of this `Expanded`.
    pub fn iter(&self) -> Entries<'_> {
        Entries {
            id: self.id.as_ref().map(Nullable::as_ref),
            type_: self.type_.as_ref().map(Nullable::as_ref),
            context: self.context.as_deref(),
            reverse: self.reverse.as_ref(),
            index: self.index.as_ref(),
            language: self.language.as_ref().map(Nullable::as_ref),
            direction: self.direction,
            container: self.container.as_ref().map(Nullable::as_ref),
            nest: self.nest.as_ref(),
            prefix: self.prefix,
            propagate: self.propagate,
            protected: self.protected,
        }
    }

    /// Borrows this `Expanded` as expanded ref, if it is one.
    pub fn as_expanded_ref(&self) -> ExpandedRef<'_> {
        ExpandedRef {
            id: self.id.as_ref().map(|i| i.as_ref().map(|id| id.as_id_ref())),
            type_: self.type_.as_ref().map(Nullable::as_ref),
            context: self.context.as_deref(),
            reverse: self.reverse.as_ref(),
            index: self.index.as_ref(),
            language: self.language.as_ref().map(|n| n.as_ref().map(LenientLangTagBuf::as_lenient_lang_tag_ref)),
            direction: self.direction,
            container: self.container.as_ref().map(Nullable::as_ref),
            nest: self.nest.as_ref(),
            prefix: self.prefix,
            propagate: self.propagate,
            protected: self.protected,
        }
    }
}

/// Expanded term definition.
#[derive(Debug, Educe)]
#[educe(Default)]
pub struct ExpandedRef<'a> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    pub id: Option<Nullable<IdRef<'a>>>,
    /// The `@type` entry, giving the type of the node or the values.
    pub type_: Option<Nullable<&'a Type>>,
    /// The `@context` entry, holding a context local to this definition.
    pub context: Option<&'a context::Context>,
    /// The `@reverse` entry, mapping the term to a reverse property.
    pub reverse: Option<&'a context::definition::Key>,
    /// The `@index` entry, indexing the value within its container.
    pub index: Option<&'a Index>,
    /// The `@language` entry, tagging string values with a language.
    pub language: Option<Nullable<&'a LenientLangTag>>,
    /// The `@direction` entry, setting the base direction of string values.
    pub direction: Option<Nullable<Direction>>,
    /// The `@container` entry, declaring how values of the term are laid out.
    pub container: Option<Nullable<&'a Container>>,
    /// The `@nest` entry, gathering properties under a nesting term.
    pub nest: Option<&'a Nest>,
    /// The `@prefix` entry, allowing the term to expand compact IRIs.
    pub prefix: Option<bool>,
    /// The `@propagate` entry, controlling whether the context survives into
    /// node objects.
    pub propagate: Option<bool>,
    /// The `@protected` entry, forbidding redefinition of the term.
    pub protected: Option<bool>,
}

impl<'a> From<Nullable<&'a TermDefinition>> for ExpandedRef<'a> {
    fn from(value: Nullable<&'a TermDefinition>) -> Self {
        match value {
            Nullable::Some(d) => d.as_expanded(),
            Nullable::Null => Self {
                id: Some(Nullable::Null),
                ..Default::default()
            },
        }
    }
}

/// Term definition entries.
pub struct Entries<'a> {
    id: Option<Nullable<&'a Id>>,
    type_: Option<Nullable<&'a Type>>,
    context: Option<&'a context::Context>,
    reverse: Option<&'a context::definition::Key>,
    index: Option<&'a Index>,
    language: Option<Nullable<&'a LenientLangTagBuf>>,
    direction: Option<Nullable<Direction>>,
    container: Option<Nullable<&'a Container>>,
    nest: Option<&'a Nest>,
    prefix: Option<bool>,
    propagate: Option<bool>,
    protected: Option<bool>,
}

/// Entry of an expanded term definition, key and value together.
pub enum EntryRef<'a> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id(Nullable<&'a Id>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(Nullable<&'a Type>),
    /// The `@context` entry, holding a context local to this definition.
    Context(&'a context::Context),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a context::definition::Key),
    /// The `@index` entry, indexing the value within its container.
    Index(&'a Index),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@container` entry, declaring how values of the term are laid out.
    Container(Nullable<&'a Container>),
    /// The `@nest` entry, gathering properties under a nesting term.
    Nest(&'a Nest),
    /// The `@prefix` entry, allowing the term to expand compact IRIs.
    Prefix(bool),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
}

impl<'a> EntryRef<'a> {
    /// Consumes this `EntryRef`, returning its key.
    pub fn into_key(self) -> EntryKey {
        match self {
            Self::Id(_) => EntryKey::Id,
            Self::Type(_) => EntryKey::Type,
            Self::Context(_) => EntryKey::Context,
            Self::Reverse(_) => EntryKey::Reverse,
            Self::Index(_) => EntryKey::Index,
            Self::Language(_) => EntryKey::Language,
            Self::Direction(_) => EntryKey::Direction,
            Self::Container(_) => EntryKey::Container,
            Self::Nest(_) => EntryKey::Nest,
            Self::Prefix(_) => EntryKey::Prefix,
            Self::Propagate(_) => EntryKey::Propagate,
            Self::Protected(_) => EntryKey::Protected,
        }
    }

    /// Returns the key of this `EntryRef`.
    pub fn key(&self) -> EntryKey {
        match self {
            Self::Id(_) => EntryKey::Id,
            Self::Type(_) => EntryKey::Type,
            Self::Context(_) => EntryKey::Context,
            Self::Reverse(_) => EntryKey::Reverse,
            Self::Index(_) => EntryKey::Index,
            Self::Language(_) => EntryKey::Language,
            Self::Direction(_) => EntryKey::Direction,
            Self::Container(_) => EntryKey::Container,
            Self::Nest(_) => EntryKey::Nest,
            Self::Prefix(_) => EntryKey::Prefix,
            Self::Propagate(_) => EntryKey::Propagate,
            Self::Protected(_) => EntryKey::Protected,
        }
    }

    /// Consumes this `EntryRef`, returning its value.
    pub fn into_value(self) -> EntryValueRef<'a> {
        self.value()
    }

    /// Returns the value of this `EntryRef`.
    pub fn value(&self) -> EntryValueRef<'a> {
        match self {
            Self::Id(e) => EntryValueRef::Id(*e),
            Self::Type(e) => EntryValueRef::Type(*e),
            Self::Context(e) => EntryValueRef::Context(e),
            Self::Reverse(e) => EntryValueRef::Reverse(e),
            Self::Index(e) => EntryValueRef::Index(e),
            Self::Language(e) => EntryValueRef::Language(*e),
            Self::Direction(e) => EntryValueRef::Direction(*e),
            Self::Container(e) => EntryValueRef::Container(*e),
            Self::Nest(e) => EntryValueRef::Nest(e),
            Self::Prefix(e) => EntryValueRef::Prefix(*e),
            Self::Propagate(e) => EntryValueRef::Propagate(*e),
            Self::Protected(e) => EntryValueRef::Protected(*e),
        }
    }

    /// Consumes this `EntryRef`, returning its key value.
    pub fn into_key_value(self) -> (EntryKey, EntryValueRef<'a>) {
        self.key_value()
    }

    /// Returns the key value of this `EntryRef`.
    pub fn key_value(&self) -> (EntryKey, EntryValueRef<'a>) {
        match self {
            Self::Id(e) => (EntryKey::Id, EntryValueRef::Id(*e)),
            Self::Type(e) => (EntryKey::Type, EntryValueRef::Type(*e)),
            Self::Context(e) => (EntryKey::Context, EntryValueRef::Context(e)),
            Self::Reverse(e) => (EntryKey::Reverse, EntryValueRef::Reverse(e)),
            Self::Index(e) => (EntryKey::Index, EntryValueRef::Index(e)),
            Self::Language(e) => (EntryKey::Language, EntryValueRef::Language(*e)),
            Self::Direction(e) => (EntryKey::Direction, EntryValueRef::Direction(*e)),
            Self::Container(e) => (EntryKey::Container, EntryValueRef::Container(*e)),
            Self::Nest(e) => (EntryKey::Nest, EntryValueRef::Nest(e)),
            Self::Prefix(e) => (EntryKey::Prefix, EntryValueRef::Prefix(*e)),
            Self::Propagate(e) => (EntryKey::Propagate, EntryValueRef::Propagate(*e)),
            Self::Protected(e) => (EntryKey::Protected, EntryValueRef::Protected(*e)),
        }
    }
}

/// Key of an expanded term definition entry.
pub enum EntryKey {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id,
    /// The `@type` entry, giving the type of the node or the values.
    Type,
    /// The `@context` entry, holding a context local to this definition.
    Context,
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse,
    /// The `@index` entry, indexing the value within its container.
    Index,
    /// The `@language` entry, tagging string values with a language.
    Language,
    /// The `@direction` entry, setting the base direction of string values.
    Direction,
    /// The `@container` entry, declaring how values of the term are laid out.
    Container,
    /// The `@nest` entry, gathering properties under a nesting term.
    Nest,
    /// The `@prefix` entry, allowing the term to expand compact IRIs.
    Prefix,
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate,
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected,
}

impl EntryKey {
    /// Returns the keyword of this `EntryKey`.
    pub fn keyword(&self) -> Keyword {
        match self {
            Self::Id => Keyword::Id,
            Self::Type => Keyword::Type,
            Self::Context => Keyword::Context,
            Self::Reverse => Keyword::Reverse,
            Self::Index => Keyword::Index,
            Self::Language => Keyword::Language,
            Self::Direction => Keyword::Direction,
            Self::Container => Keyword::Container,
            Self::Nest => Keyword::Nest,
            Self::Prefix => Keyword::Prefix,
            Self::Propagate => Keyword::Propagate,
            Self::Protected => Keyword::Protected,
        }
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        self.keyword().into_str()
    }
}

/// Value of an expanded term definition entry.
pub enum EntryValueRef<'a> {
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id(Nullable<&'a Id>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(Nullable<&'a Type>),
    /// The `@context` entry, holding a context local to this definition.
    Context(&'a context::Context),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a context::definition::Key),
    /// The `@index` entry, indexing the value within its container.
    Index(&'a Index),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@container` entry, declaring how values of the term are laid out.
    Container(Nullable<&'a Container>),
    /// The `@nest` entry, gathering properties under a nesting term.
    Nest(&'a Nest),
    /// The `@prefix` entry, allowing the term to expand compact IRIs.
    Prefix(bool),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
}

impl<'a> EntryValueRef<'a> {
    /// Checks whether this `EntryValueRef` is object.
    pub fn is_object(&self) -> bool {
        match self {
            Self::Context(c) => c.is_object(),
            _ => false,
        }
    }

    /// Checks whether this `EntryValueRef` is array.
    pub fn is_array(&self) -> bool {
        match self {
            Self::Container(Nullable::Some(c)) => c.is_array(),
            _ => false,
        }
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = EntryRef<'a>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = 0;

        if self.id.is_some() {
            len += 1
        }

        if self.type_.is_some() {
            len += 1
        }

        if self.context.is_some() {
            len += 1
        }

        if self.reverse.is_some() {
            len += 1
        }

        if self.index.is_some() {
            len += 1
        }

        if self.language.is_some() {
            len += 1
        }

        if self.direction.is_some() {
            len += 1
        }

        if self.container.is_some() {
            len += 1
        }

        if self.nest.is_some() {
            len += 1
        }

        if self.prefix.is_some() {
            len += 1
        }

        if self.propagate.is_some() {
            len += 1
        }

        if self.protected.is_some() {
            len += 1
        }

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self.id.take() {
            Some(value) => Some(EntryRef::Id(value)),
            None => match self.type_.take() {
                Some(value) => Some(EntryRef::Type(value)),
                None => match self.context.take() {
                    Some(value) => Some(EntryRef::Context(value)),
                    None => match self.reverse.take() {
                        Some(value) => Some(EntryRef::Reverse(value)),
                        None => match self.index.take() {
                            Some(value) => Some(EntryRef::Index(value)),
                            None => match self.language.take() {
                                Some(value) => Some(EntryRef::Language(value)),
                                None => match self.direction.take() {
                                    Some(value) => Some(EntryRef::Direction(value)),
                                    None => match self.container.take() {
                                        Some(value) => Some(EntryRef::Container(value)),
                                        None => match self.nest.take() {
                                            Some(value) => Some(EntryRef::Nest(value)),
                                            None => match self.prefix.take() {
                                                Some(value) => Some(EntryRef::Prefix(value)),
                                                None => match self.propagate.take() {
                                                    Some(value) => Some(EntryRef::Propagate(value)),
                                                    None => self.protected.take().map(EntryRef::Protected),
                                                },
                                            },
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

/// Term definition fragment.
pub enum FragmentRef<'a> {
    /// Term definition entry.
    Entry(EntryRef<'a>),

    /// Term definition entry key.
    Key(EntryKey),

    /// Term definition entry value.
    Value(EntryValueRef<'a>),

    /// Container value fragment.
    ContainerFragment(&'a ContainerKind),
}

impl<'a> FragmentRef<'a> {
    /// Checks whether this `FragmentRef` is key.
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Checks whether this `FragmentRef` is entry.
    pub fn is_entry(&self) -> bool {
        matches!(self, Self::Entry(_))
    }

    /// Checks whether this `FragmentRef` is array.
    pub fn is_array(&self) -> bool {
        match self {
            Self::Value(v) => v.is_array(),
            _ => false,
        }
    }

    /// Checks whether this `FragmentRef` is object.
    pub fn is_object(&self) -> bool {
        match self {
            Self::Value(v) => v.is_object(),
            _ => false,
        }
    }

    /// Returns the sub fragments of this `FragmentRef`.
    pub fn sub_fragments(&self) -> SubFragments<'a> {
        match self {
            Self::Value(EntryValueRef::Container(Nullable::Some(c))) => SubFragments::Container(c.sub_fragments()),
            _ => SubFragments::None,
        }
    }
}

/// Iterator over the fragments held by a term definition entry.
pub enum SubFragments<'a> {
    /// No value.
    None,
    /// The values of a `@container` entry.
    Container(container::SubValues<'a>),
}

impl<'a> Iterator for SubFragments<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Container(c) => c.next().map(FragmentRef::ContainerFragment),
        }
    }
}
