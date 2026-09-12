use crate::{CompactIri, CompactIriBuf, Container, ContainerKind, Direction, Keyword, LenientLangTag, LenientLangTagBuf, Nullable, container, context};
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

/// Definition bound to a term by a context.
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum TermDefinition {
    /// A string mapping the term directly to an IRI, a compact IRI or a blank
    /// node identifier.
    Simple(Simple),
    /// An object, which can set more about the term than the IRI it maps to.
    Expanded(Box<Expanded>),
}

impl TermDefinition {
    /// Checks whether this definition is in expanded form.
    #[must_use]
    pub fn is_expanded(&self) -> bool {
        matches!(self, Self::Expanded(_))
    }

    /// Checks whether this definition is written as a JSON object, which is the
    /// case exactly when it is in expanded form.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.is_expanded()
    }

    /// Returns a borrowed view of this definition in expanded form.
    ///
    /// A simple definition is presented as an expanded one whose only entry is
    /// the `@id` it maps to.
    #[must_use]
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
///
/// Kept unvalidated: the string may be an IRI, a compact IRI, a blank node
/// identifier or another term, and which one it is can only be told once the
/// context is processed.
pub struct Simple(pub(crate) String);

impl Simple {
    /// Parses this definition as an IRI, returning `None` if it is not one.
    #[must_use]
    pub fn as_iri(&self) -> Option<Iri<&str>> {
        Iri::parse(self.0.as_str()).ok()
    }

    /// Parses this definition as a compact IRI, returning `None` if it is not
    /// one.
    #[must_use]
    pub fn as_compact_iri(&self) -> Option<&CompactIri> {
        CompactIri::new(&self.0).ok()
    }

    /// Parses this definition as a blank node identifier, returning `None` if
    /// it is not one.
    #[must_use]
    pub fn as_blank_id(&self) -> Option<&BlankId> {
        BlankId::new(&self.0).ok()
    }

    /// Returns this value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Unwraps the underlying string.
    #[must_use]
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
#[derive(PartialEq, Eq, Clone, Default, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Expanded {
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@id", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@id` entry, giving the IRI the term expands to.
    pub id: Option<Nullable<Id>>,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "@type", default, deserialize_with = "Nullable::optional", skip_serializing_if = "Option::is_none")
    )]
    /// The `@type` entry, giving the type of the values of the term.
    pub type_: Option<Nullable<Type>>,

    #[cfg_attr(feature = "serde", serde(rename = "@context", default, skip_serializing_if = "Option::is_none"))]
    /// The `@context` entry, holding a context scoped to the term.
    pub context: Option<Box<context::Context>>,

    #[cfg_attr(feature = "serde", serde(rename = "@reverse", default, skip_serializing_if = "Option::is_none"))]
    /// The `@reverse` entry, mapping the term to a reverse property.
    pub reverse: Option<context::definition::Key>,

    #[cfg_attr(feature = "serde", serde(rename = "@index", default, skip_serializing_if = "Option::is_none"))]
    /// The `@index` entry, naming the property that carries the index of the
    /// values in an index map.
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
    /// The `@container` entry, declaring the container the values of the term
    /// are held in.
    pub container: Option<Nullable<Container>>,

    #[cfg_attr(feature = "serde", serde(rename = "@nest", default, skip_serializing_if = "Option::is_none"))]
    /// The `@nest` entry, naming the term under which this property is nested.
    pub nest: Option<Nest>,

    #[cfg_attr(feature = "serde", serde(rename = "@prefix", default, skip_serializing_if = "Option::is_none"))]
    /// The `@prefix` entry, allowing the term to be used as the prefix of a
    /// compact IRI.
    pub prefix: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@propagate", default, skip_serializing_if = "Option::is_none"))]
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    pub propagate: Option<bool>,

    #[cfg_attr(feature = "serde", serde(rename = "@protected", default, skip_serializing_if = "Option::is_none"))]
    /// The `@protected` entry, forbidding redefinition of the term.
    pub protected: Option<bool>,
}

impl Expanded {
    /// Creates an expanded term definition with no entry set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks whether this definition is equivalent to `null`, which unsets the
    /// term: no entry is set, except possibly an `@id` explicitly set to `null`.
    #[must_use]
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

    /// Checks whether this definition can be written in simple form: `@id` is
    /// set to a non-null value and no other entry is set.
    #[must_use]
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

    /// Reduces this definition to the shortest equivalent form: `null` when it
    /// unsets the term, a simple string definition when it carries nothing but
    /// an `@id`, and the expanded form otherwise.
    #[must_use]
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

    /// Returns an iterator over the entries this definition sets, in the order
    /// they are declared on this struct.
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

    /// Returns a borrowed view of this definition.
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

/// Borrowed view of a term definition in expanded form.
///
/// Also used to present a simple definition, or `null`, in expanded form; see
/// [`TermDefinition::as_expanded`].
#[derive(Debug, Default)]
pub struct ExpandedRef<'a> {
    /// The `@id` entry, giving the IRI the term expands to.
    pub id: Option<Nullable<IdRef<'a>>>,
    /// The `@type` entry, giving the type of the values of the term.
    pub type_: Option<Nullable<&'a Type>>,
    /// The `@context` entry, holding a context scoped to the term.
    pub context: Option<&'a context::Context>,
    /// The `@reverse` entry, mapping the term to a reverse property.
    pub reverse: Option<&'a context::definition::Key>,
    /// The `@index` entry, naming the property that carries the index of the
    /// values in an index map.
    pub index: Option<&'a Index>,
    /// The `@language` entry, tagging string values with a language.
    pub language: Option<Nullable<&'a LenientLangTag>>,
    /// The `@direction` entry, setting the base direction of string values.
    pub direction: Option<Nullable<Direction>>,
    /// The `@container` entry, declaring the container the values of the term
    /// are held in.
    pub container: Option<Nullable<&'a Container>>,
    /// The `@nest` entry, naming the term under which this property is nested.
    pub nest: Option<&'a Nest>,
    /// The `@prefix` entry, allowing the term to be used as the prefix of a
    /// compact IRI.
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

/// Iterator over the entries of an expanded term definition.
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
    /// The `@id` entry, giving the IRI the term expands to.
    Id(Nullable<&'a Id>),
    /// The `@type` entry, giving the type of the values of the term.
    Type(Nullable<&'a Type>),
    /// The `@context` entry, holding a context scoped to the term.
    Context(&'a context::Context),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a context::definition::Key),
    /// The `@index` entry, naming the property that carries the index of the
    /// values in an index map.
    Index(&'a Index),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@container` entry, declaring the container the values of the term
    /// are held in.
    Container(Nullable<&'a Container>),
    /// The `@nest` entry, naming the term under which this property is nested.
    Nest(&'a Nest),
    /// The `@prefix` entry, allowing the term to be used as the prefix of a
    /// compact IRI.
    Prefix(bool),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
}

impl<'a> EntryRef<'a> {
    /// Returns the key of this entry, taking `self` by value.
    #[must_use]
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

    /// Returns the key of this entry.
    #[must_use]
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

    /// Returns the value of this entry, taking `self` by value.
    #[must_use]
    pub fn into_value(self) -> EntryValueRef<'a> {
        self.value()
    }

    /// Returns the value of this entry.
    #[must_use]
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

    /// Returns the key and value of this entry, taking `self` by value.
    #[must_use]
    pub fn into_key_value(self) -> (EntryKey, EntryValueRef<'a>) {
        self.key_value()
    }

    /// Returns the key and value of this entry.
    #[must_use]
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
    /// The `@id` entry, giving the IRI the term expands to.
    Id,
    /// The `@type` entry, giving the type of the values of the term.
    Type,
    /// The `@context` entry, holding a context scoped to the term.
    Context,
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse,
    /// The `@index` entry, naming the property that carries the index of the
    /// values in an index map.
    Index,
    /// The `@language` entry, tagging string values with a language.
    Language,
    /// The `@direction` entry, setting the base direction of string values.
    Direction,
    /// The `@container` entry, declaring the container the values of the term
    /// are held in.
    Container,
    /// The `@nest` entry, naming the term under which this property is nested.
    Nest,
    /// The `@prefix` entry, allowing the term to be used as the prefix of a
    /// compact IRI.
    Prefix,
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate,
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected,
}

impl EntryKey {
    /// Returns the keyword naming this entry.
    #[must_use]
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

    /// Returns the keyword naming this entry as a string, such as `"@id"`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.keyword().into_str()
    }
}

/// Value of an expanded term definition entry.
pub enum EntryValueRef<'a> {
    /// The `@id` entry, giving the IRI the term expands to.
    Id(Nullable<&'a Id>),
    /// The `@type` entry, giving the type of the values of the term.
    Type(Nullable<&'a Type>),
    /// The `@context` entry, holding a context scoped to the term.
    Context(&'a context::Context),
    /// The `@reverse` entry, mapping the term to a reverse property.
    Reverse(&'a context::definition::Key),
    /// The `@index` entry, naming the property that carries the index of the
    /// values in an index map.
    Index(&'a Index),
    /// The `@language` entry, tagging string values with a language.
    Language(Nullable<&'a LenientLangTagBuf>),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Nullable<Direction>),
    /// The `@container` entry, declaring the container the values of the term
    /// are held in.
    Container(Nullable<&'a Container>),
    /// The `@nest` entry, naming the term under which this property is nested.
    Nest(&'a Nest),
    /// The `@prefix` entry, allowing the term to be used as the prefix of a
    /// compact IRI.
    Prefix(bool),
    /// The `@propagate` entry, controlling whether the context survives into node objects.
    Propagate(bool),
    /// The `@protected` entry, forbidding redefinition of the term.
    Protected(bool),
}

impl EntryValueRef<'_> {
    /// Checks whether this value is a JSON object, which only the scoped
    /// `@context` can be.
    #[must_use]
    pub fn is_object(&self) -> bool {
        match self {
            Self::Context(c) => c.is_object(),
            _ => false,
        }
    }

    /// Checks whether this value is a JSON array, which only a multi-valued
    /// `@container` can be.
    #[must_use]
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
            len += 1;
        }

        if self.type_.is_some() {
            len += 1;
        }

        if self.context.is_some() {
            len += 1;
        }

        if self.reverse.is_some() {
            len += 1;
        }

        if self.index.is_some() {
            len += 1;
        }

        if self.language.is_some() {
            len += 1;
        }

        if self.direction.is_some() {
            len += 1;
        }

        if self.container.is_some() {
            len += 1;
        }

        if self.nest.is_some() {
            len += 1;
        }

        if self.prefix.is_some() {
            len += 1;
        }

        if self.propagate.is_some() {
            len += 1;
        }

        if self.protected.is_some() {
            len += 1;
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

impl ExactSizeIterator for Entries<'_> {}

/// Fragment of a term definition.
pub enum FragmentRef<'a> {
    /// An entry of the definition, key and value together.
    Entry(EntryRef<'a>),

    /// The key of an entry of the definition.
    Key(EntryKey),

    /// The value of an entry of the definition.
    Value(EntryValueRef<'a>),

    /// One item of an array-valued `@container` entry.
    ContainerFragment(&'a ContainerKind),
}

impl<'a> FragmentRef<'a> {
    /// Checks whether this fragment is an entry key.
    #[must_use]
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Checks whether this fragment is an entry, key and value together.
    #[must_use]
    pub fn is_entry(&self) -> bool {
        matches!(self, Self::Entry(_))
    }

    /// Checks whether this fragment is a JSON array.
    #[must_use]
    pub fn is_array(&self) -> bool {
        match self {
            Self::Value(v) => v.is_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment is a JSON object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        match self {
            Self::Value(v) => v.is_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    #[must_use]
    pub fn sub_fragments(&self) -> SubFragments<'a> {
        match self {
            Self::Value(EntryValueRef::Container(Nullable::Some(c))) => SubFragments::Container(c.sub_fragments()),
            _ => SubFragments::None,
        }
    }
}

/// Iterator over the fragments held by a term definition entry.
pub enum SubFragments<'a> {
    /// Nothing to iterate over.
    None,
    /// The items of an array-valued `@container` entry.
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
