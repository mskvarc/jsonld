use super::{IntoSyntax, Nest};
use crate::{Container, Direction, HashMap, LenientLangTagBuf, Nullable, Term, Type, ValidId as Id};
use contextual::WithContext;
use iri_rs::IriBuf;
use jsonld_syntax::{
    KeywordType,
    context::{
        definition::{Key, TypeContainer},
        term_definition::Index,
    },
};
use rdfx::{
    BlankIdBuf,
    vocabulary::{IriVocabulary, Vocabulary},
};
use std::{borrow::Borrow, fmt, hash::Hash, sync::Arc};

/// Term binding.
pub enum Binding<T = IriBuf, B = BlankIdBuf> {
    /// Normal term definition.
    Normal(Key, NormalTermDefinition<T, B>),

    /// `@type` term definition.
    Type(TypeTermDefinition),
}

/// Term binding reference.
pub enum BindingRef<'a, T, B> {
    /// Normal term definition.
    Normal(&'a Key, &'a NormalTermDefinition<T, B>),

    /// `@type` term definition.
    Type(&'a TypeTermDefinition),
}

/// Term a context binding defines.
pub enum BindingTerm<'a> {
    /// An ordinary term.
    Normal(&'a Key),
    /// The `@type` entry of the context.
    Type,
}

impl<'a> BindingTerm<'a> {
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'a str {
        match self {
            Self::Normal(key) => key.as_str(),
            Self::Type => "@type",
        }
    }
}

impl<'a> fmt::Display for BindingTerm<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<'a, T, B> BindingRef<'a, T, B> {
    /// Returns a reference to the bound term.
    pub fn term(&self) -> BindingTerm<'a> {
        match self {
            Self::Normal(key, _) => BindingTerm::Normal(key),
            Self::Type(_) => BindingTerm::Type,
        }
    }

    /// Returns a reference to the bound term definition.
    pub fn definition(&self) -> TermDefinitionRef<'a, T, B> {
        match self {
            Self::Normal(_, d) => TermDefinitionRef::Normal(d),
            Self::Type(d) => TermDefinitionRef::Type(d),
        }
    }
}

/// Term definitions of a context, split from its `@type` definition.
pub type DefinitionParts<T, B> = (HashMap<Key, NormalTermDefinition<T, B>>, Option<TypeTermDefinition>);

/// Context term definitions.
#[derive(Clone, PartialEq, Eq)]
pub struct Definitions<T, B> {
    normal: HashMap<Key, NormalTermDefinition<T, B>>,
    type_: Option<TypeTermDefinition>,
}

impl<T, B> Default for Definitions<T, B> {
    fn default() -> Self {
        Self {
            normal: HashMap::default(),
            type_: None,
        }
    }
}

impl<T, B> Definitions<T, B> {
    /// Consumes this `Definitions`, returning its parts.
    pub fn into_parts(self) -> DefinitionParts<T, B> {
        (self.normal, self.type_)
    }

    /// Returns the number of defined terms.
    pub fn len(&self) -> usize {
        if self.type_.is_some() { self.normal.len() + 1 } else { self.normal.len() }
    }

    /// Checks if no terms are defined.
    pub fn is_empty(&self) -> bool {
        self.type_.is_none() && self.normal.is_empty()
    }

    /// Returns a reference to the definition of the given `term`, if any.
    pub fn get<Q>(&self, term: &Q) -> Option<TermDefinitionRef<'_, T, B>>
    where
        Q: ?Sized + Hash + Eq,
        Key: Borrow<Q>,
        KeywordType: Borrow<Q>,
    {
        if KeywordType.borrow() == term {
            self.type_.as_ref().map(TermDefinitionRef::Type)
        } else {
            self.normal.get(term).map(TermDefinitionRef::Normal)
        }
    }

    /// Returns a reference to the normal definition of the given `term`, if any.
    pub fn get_normal<Q>(&self, term: &Q) -> Option<&NormalTermDefinition<T, B>>
    where
        Q: ?Sized + Hash + Eq,
        Key: Borrow<Q>,
    {
        self.normal.get(term)
    }

    /// Returns a reference to the `@type` definition, if any.
    pub fn get_type(&self) -> Option<&TypeTermDefinition> {
        self.type_.as_ref()
    }

    /// Checks whether the given term is defined.
    pub fn contains_term<Q>(&self, term: &Q) -> bool
    where
        Q: ?Sized + Hash + Eq,
        Key: Borrow<Q>,
        KeywordType: Borrow<Q>,
    {
        if KeywordType.borrow() == term {
            self.type_.is_some()
        } else {
            self.normal.contains_key(term)
        }
    }

    /// Inserts the given `binding`.
    pub fn insert(&mut self, binding: Binding<T, B>) -> Option<TermDefinition<T, B>> {
        match binding {
            Binding::Normal(key, definition) => self.insert_normal(key, definition).map(TermDefinition::Normal),
            Binding::Type(definition) => self.insert_type(definition).map(TermDefinition::Type),
        }
    }

    /// Defines the given normal term.
    pub fn insert_normal(&mut self, term: Key, definition: NormalTermDefinition<T, B>) -> Option<NormalTermDefinition<T, B>> {
        self.normal.insert(term, definition)
    }

    /// Inserts the given `@type` definition.
    pub fn insert_type(&mut self, definition: TypeTermDefinition) -> Option<TypeTermDefinition> {
        self.type_.replace(definition)
    }

    /// Sets the given `term` normal definition.
    pub fn set_normal(&mut self, term: Key, definition: Option<NormalTermDefinition<T, B>>) -> Option<NormalTermDefinition<T, B>> {
        match definition {
            Some(d) => self.normal.insert(term, d),
            None => self.normal.remove(&term),
        }
    }

    /// Sets the given `@type` definition.
    pub fn set_type(&mut self, definition: Option<TypeTermDefinition>) -> Option<TypeTermDefinition> {
        std::mem::replace(&mut self.type_, definition)
    }

    /// Returns an iterator over the term definitions.
    pub fn iter(&self) -> Iter<'_, T, B> {
        Iter {
            type_: self.type_.as_ref(),
            normal: self.normal.iter(),
        }
    }

    /// Rewrites every IRI and identifier of this definition with the given
    /// functions.
    pub fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>) -> Definitions<U, C>
    where
        T: Clone,
        B: Clone,
    {
        Definitions {
            normal: self.normal.into_iter().map(|(key, d)| (key, d.map_ids(&mut map_iri, &mut map_id))).collect(),
            type_: self.type_,
        }
    }
}

/// Iterator over the bindings of a context definition.
pub struct Iter<'a, T, B> {
    type_: Option<&'a TypeTermDefinition>,
    normal: hashbrown::hash_map::Iter<'a, Key, NormalTermDefinition<T, B>>,
}

impl<'a, T, B> Iterator for Iter<'a, T, B> {
    type Item = BindingRef<'a, T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        self.type_
            .take()
            .map(BindingRef::Type)
            .or_else(|| self.normal.next().map(|(k, d)| BindingRef::Normal(k, d)))
    }
}

impl<'a, T, B> IntoIterator for &'a Definitions<T, B> {
    type Item = BindingRef<'a, T, B>;
    type IntoIter = Iter<'a, T, B>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Owning iterator over the bindings of a context definition.
pub struct IntoIter<T, B> {
    type_: Option<TypeTermDefinition>,
    normal: hashbrown::hash_map::IntoIter<Key, NormalTermDefinition<T, B>>,
}

impl<T, B> Iterator for IntoIter<T, B> {
    type Item = Binding<T, B>;

    fn next(&mut self) -> Option<Self::Item> {
        self.type_
            .take()
            .map(Binding::Type)
            .or_else(|| self.normal.next().map(|(k, d)| Binding::Normal(k, d)))
    }
}

impl<T, B> IntoIterator for Definitions<T, B> {
    type Item = Binding<T, B>;
    type IntoIter = IntoIter<T, B>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            type_: self.type_,
            normal: self.normal.into_iter(),
        }
    }
}

/// `@type` term definition.
///
/// Such definition compared to a [`NormalTermDefinition`] can only contain
/// a `@container` and `@protected` value.
#[derive(PartialEq, Eq, Clone)]
pub struct TypeTermDefinition {
    /// Type container.
    pub container: TypeContainer,

    /// Protection flag.
    pub protected: bool,
}

impl Default for TypeTermDefinition {
    fn default() -> Self {
        Self {
            container: TypeContainer::Set,
            protected: false,
        }
    }
}

impl TypeTermDefinition {
    /// Wraps this definition so that comparisons ignore the `@protected`
    /// flag, as required when checking whether a redefinition of a protected
    /// term actually changes it.
    pub fn modulo_protected_field(&self) -> ModuloProtected<&Self> {
        ModuloProtected(self)
    }

    /// Consumes this `TypeTermDefinition`, returning its syntax definition.
    pub fn into_syntax_definition(self) -> jsonld_syntax::context::definition::Type {
        jsonld_syntax::context::definition::Type {
            container: self.container,
            protected: if self.protected { Some(true) } else { None },
        }
    }
}

/// Term definition.
#[derive(PartialEq, Eq, Clone)]
pub enum TermDefinition<T, B> {
    /// `@type` term definition.
    Type(TypeTermDefinition),

    /// Normal term definition.
    Normal(NormalTermDefinition<T, B>),
}

impl<T, B> TermDefinition<T, B> {
    /// Borrows this `TermDefinition`.
    pub fn as_ref(&self) -> TermDefinitionRef<'_, T, B> {
        match self {
            Self::Type(t) => TermDefinitionRef::Type(t),
            Self::Normal(n) => TermDefinitionRef::Normal(n),
        }
    }

    /// Wraps this definition so that comparisons ignore the `@protected`
    /// flag, as required when checking whether a redefinition of a protected
    /// term actually changes it.
    pub fn modulo_protected_field(&self) -> ModuloProtected<TermDefinitionRef<'_, T, B>> {
        ModuloProtected(self.as_ref())
    }

    /// Returns the term this definition maps to, if any (always `None` for
    /// `@type` definitions).
    pub fn value(&self) -> Option<&Term<T, B>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.value.as_deref(),
        }
    }

    /// Checks whether the term may be used to expand compact IRIs.
    pub fn prefix(&self) -> bool {
        match self {
            Self::Type(_) => false,
            Self::Normal(d) => d.prefix,
        }
    }

    /// Checks whether the term is protected against redefinition.
    pub fn protected(&self) -> bool {
        match self {
            Self::Type(d) => d.protected,
            Self::Normal(d) => d.protected,
        }
    }

    /// Checks whether the term denotes a reverse property.
    pub fn reverse_property(&self) -> bool {
        match self {
            Self::Type(_) => false,
            Self::Normal(d) => d.reverse_property,
        }
    }

    /// Returns the base URL used to resolve relative IRIs of the term.
    pub fn base_url(&self) -> Option<&T> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.base_url.as_ref(),
        }
    }

    /// Returns the local context of the term (its `@context` entry), if any.
    pub fn context(&self) -> Option<&jsonld_syntax::context::Context> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.context.as_deref(),
        }
    }

    /// Returns the container mapping of the term.
    pub fn container(&self) -> Container {
        match self {
            Self::Type(d) => d.container.into(),
            Self::Normal(d) => d.container,
        }
    }

    /// Returns the base direction of the term's string values.
    pub fn direction(&self) -> Option<Nullable<Direction>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.direction,
        }
    }

    /// Returns the index mapping of the term.
    pub fn index(&self) -> Option<&Index> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.index.as_ref(),
        }
    }

    /// Returns the default language of the term's string values.
    pub fn language(&self) -> Option<Nullable<&LenientLangTagBuf>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.language.as_ref().map(Nullable::as_ref),
        }
    }

    /// Returns the nesting term the property is gathered under.
    pub fn nest(&self) -> Option<&Nest> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.nest.as_ref(),
        }
    }

    /// Returns the type mapping of the term.
    pub fn typ(&self) -> Option<&Type<T>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.typ.as_ref(),
        }
    }
}

/// Term definition reference.
#[derive(PartialEq, Eq)]
pub enum TermDefinitionRef<'a, T = IriBuf, B = BlankIdBuf> {
    /// `@type` definition.
    Type(&'a TypeTermDefinition),

    /// Normal definition.
    Normal(&'a NormalTermDefinition<T, B>),
}

impl<'a, T, B> TermDefinitionRef<'a, T, B> {
    /// Wraps this definition so that comparisons ignore the `@protected`
    /// flag, as required when checking whether a redefinition of a protected
    /// term actually changes it.
    pub fn modulo_protected_field(&self) -> ModuloProtected<Self> {
        ModuloProtected(*self)
    }

    /// Returns the term this definition maps to, if any (always `None` for
    /// `@type` definitions).
    pub fn value(&self) -> Option<&'a Term<T, B>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.value.as_deref(),
        }
    }

    /// Cheap-clonable handle to the IRI mapping, if present and the
    /// definition is a normal term definition.
    pub fn value_arc(&self) -> Option<&'a Arc<Term<T, B>>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.value.as_ref(),
        }
    }

    /// Checks whether the term may be used to expand compact IRIs.
    pub fn prefix(&self) -> bool {
        match self {
            Self::Type(_) => false,
            Self::Normal(d) => d.prefix,
        }
    }

    /// Checks whether the term is protected against redefinition.
    pub fn protected(&self) -> bool {
        match self {
            Self::Type(d) => d.protected,
            Self::Normal(d) => d.protected,
        }
    }

    /// Checks whether the term denotes a reverse property.
    pub fn reverse_property(&self) -> bool {
        match self {
            Self::Type(_) => false,
            Self::Normal(d) => d.reverse_property,
        }
    }

    /// Returns the base URL used to resolve relative IRIs of the term.
    pub fn base_url(&self) -> Option<&'a T> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.base_url.as_ref(),
        }
    }

    /// Returns the local context of the term (its `@context` entry), if any.
    pub fn context(&self) -> Option<&'a jsonld_syntax::context::Context> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.context.as_deref(),
        }
    }

    /// Returns the container mapping of the term.
    pub fn container(&self) -> Container {
        match self {
            Self::Type(d) => d.container.into(),
            Self::Normal(d) => d.container,
        }
    }

    /// Returns the base direction of the term's string values.
    pub fn direction(&self) -> Option<Nullable<Direction>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.direction,
        }
    }

    /// Returns the index mapping of the term.
    pub fn index(&self) -> Option<&'a Index> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.index.as_ref(),
        }
    }

    /// Returns the default language of the term's string values.
    pub fn language(&self) -> Option<Nullable<&'a LenientLangTagBuf>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.language.as_ref().map(Nullable::as_ref),
        }
    }

    /// Returns the nesting term the property is gathered under.
    pub fn nest(&self) -> Option<&'a Nest> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.nest.as_ref(),
        }
    }

    /// Returns the type mapping of the term.
    pub fn typ(&self) -> Option<&'a Type<T>> {
        match self {
            Self::Type(_) => None,
            Self::Normal(d) => d.typ.as_ref(),
        }
    }
}

impl<'a, T, B> Clone for TermDefinitionRef<'a, T, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T, B> Copy for TermDefinitionRef<'a, T, B> {}

#[derive(PartialEq, Eq, Clone)]
/// Definition of an ordinary term, as processed from a context.
pub struct NormalTermDefinition<T = IriBuf, B = BlankIdBuf> {
    /// Term the definition maps to.
    ///
    /// Wrapped in `Arc` so the per-element-per-property hot path in
    /// `expand_iri_simple` returns a refcount bump instead of deep-cloning the
    /// (potentially `IriBuf`-backed) term. Prefer the `value()` /
    /// `value_arc()` accessors over direct field access.
    pub value: Option<Arc<Term<T, B>>>,

    /// Whether the term may expand compact IRIs.
    pub prefix: bool,

    /// Whether the term is protected against redefinition.
    pub protected: bool,

    /// Whether the term denotes a reverse property.
    pub reverse_property: bool,

    /// Base URL used to resolve relative IRIs of the term.
    pub base_url: Option<T>,

    /// Context local to the term.
    pub context: Option<Box<jsonld_syntax::context::Context>>,

    /// Container mapping of the term.
    pub container: Container,

    /// Base direction of the term's string values.
    pub direction: Option<Nullable<Direction>>,

    /// Index mapping of the term.
    pub index: Option<Index>,

    /// Default language of the term's string values.
    pub language: Option<Nullable<LenientLangTagBuf>>,

    /// Nesting term the property is gathered under.
    pub nest: Option<Nest>,

    /// Type mapping of the term.
    pub typ: Option<Type<T>>,
}

impl<T, B> NormalTermDefinition<T, B> {
    /// Wraps this definition so that comparisons ignore the `@protected`
    /// flag, as required when checking whether a redefinition of a protected
    /// term actually changes it.
    pub fn modulo_protected_field(&self) -> ModuloProtected<&Self> {
        ModuloProtected(self)
    }

    /// Returns the base URL used to resolve relative IRIs of the term.
    pub fn base_url(&self) -> Option<&T> {
        self.base_url.as_ref()
    }

    /// IRI mapping as a `Term` reference, transparent to the `Arc` wrapper.
    pub fn value(&self) -> Option<&Term<T, B>> {
        self.value.as_deref()
    }

    /// Cheap-clonable handle to the IRI mapping.
    pub fn value_arc(&self) -> Option<&Arc<Term<T, B>>> {
        self.value.as_ref()
    }

    /// Consumes this `NormalTermDefinition`, returning its syntax definition.
    pub fn into_syntax_definition(
        self,
        vocabulary: &impl Vocabulary<Iri = T, BlankId = B>,
    ) -> Result<Nullable<jsonld_syntax::context::TermDefinition>, super::InvalidContextError>
    where
        T: Clone,
        B: Clone,
    {
        use jsonld_syntax::context::term_definition::{Id, Type as SyntaxType, TypeKeyword};

        fn term_into_id<T, B>(vocabulary: &impl Vocabulary<Iri = T, BlankId = B>, term: Term<T, B>) -> Nullable<Id> {
            match term {
                Term::Null => Nullable::Null,
                Term::Keyword(k) => Nullable::Some(Id::Keyword(k)),
                Term::Id(r) => Nullable::Some(Id::Term(r.with(vocabulary).to_string())),
            }
        }

        fn term_into_key<T, B>(vocabulary: &impl Vocabulary<Iri = T, BlankId = B>, term: Term<T, B>) -> Result<Key, super::InvalidContextKey> {
            match term {
                Term::Null => Err(super::InvalidContextKey),
                Term::Keyword(k) => Ok(k.to_string().into()),
                Term::Id(r) => Ok(r.with(vocabulary).to_string().into()),
            }
        }

        fn type_into_syntax<T>(vocabulary: &impl IriVocabulary<Iri = T>, ty: Type<T>) -> Result<SyntaxType, super::InvalidContextError> {
            match ty {
                Type::Id => Ok(SyntaxType::Keyword(TypeKeyword::Id)),
                Type::Json => Ok(SyntaxType::Keyword(TypeKeyword::Json)),
                Type::None => Ok(SyntaxType::Keyword(TypeKeyword::None)),
                Type::Vocab => Ok(SyntaxType::Keyword(TypeKeyword::Vocab)),
                Type::Iri(t) => match vocabulary.iri(&t) {
                    Some(iri) => Ok(SyntaxType::Term(iri.to_string())),
                    None => Err(super::InvalidContextError::UnresolvedIri),
                },
            }
        }

        fn unwrap_term<T: Clone, B: Clone>(arc: Arc<Term<T, B>>) -> Term<T, B> {
            Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone())
        }

        let (id, reverse) = if self.reverse_property {
            let reverse = self.value.map(|t| term_into_key(vocabulary, unwrap_term(t))).transpose()?;
            (None, reverse)
        } else {
            (self.value.map(|t| term_into_id(vocabulary, unwrap_term(t))), None)
        };

        let container = self.container.into_syntax();

        let context = match self.context {
            Some(e) => Some(Box::new(e.into_syntax(vocabulary)?)),
            None => None,
        };

        Ok(jsonld_syntax::context::term_definition::Expanded {
            id,
            type_: self.typ.map(|t| type_into_syntax(vocabulary, t).map(Nullable::Some)).transpose()?,
            context,
            reverse,
            index: self.index.clone(),
            language: self.language,
            direction: self.direction,
            container: container.map(Nullable::Some),
            nest: self.nest.clone(),
            prefix: if self.prefix { Some(true) } else { None },
            propagate: None,
            protected: if self.protected { Some(true) } else { None },
        }
        .simplify())
    }

    fn map_ids<U, C>(self, mut map_iri: impl FnMut(T) -> U, map_id: impl FnOnce(Id<T, B>) -> Id<U, C>) -> NormalTermDefinition<U, C>
    where
        T: Clone,
        B: Clone,
    {
        NormalTermDefinition {
            value: self.value.map(|t| {
                let term = Arc::try_unwrap(t).unwrap_or_else(|a| (*a).clone());
                Arc::new(term.map_id(map_id))
            }),
            prefix: self.prefix,
            protected: self.protected,
            reverse_property: self.reverse_property,
            base_url: self.base_url.map(&mut map_iri),
            context: self.context,
            container: self.container,
            direction: self.direction,
            index: self.index,
            language: self.language,
            nest: self.nest,
            typ: self.typ.map(|t| t.map(map_iri)),
        }
    }
}

impl<T, B> Default for NormalTermDefinition<T, B> {
    fn default() -> NormalTermDefinition<T, B> {
        NormalTermDefinition {
            value: None,
            prefix: false,
            protected: false,
            reverse_property: false,
            base_url: None,
            typ: None,
            language: None,
            direction: None,
            context: None,
            nest: None,
            index: None,
            container: Container::new(),
        }
    }
}

/// Wrapper to consider a term definition without the `@protected` flag.
pub struct ModuloProtected<T>(T);

impl<'b, T: PartialEq, B: PartialEq> PartialEq<ModuloProtected<&'b NormalTermDefinition<T, B>>> for ModuloProtected<&NormalTermDefinition<T, B>> {
    fn eq(&self, other: &ModuloProtected<&'b NormalTermDefinition<T, B>>) -> bool {
        // NOTE we ignore the `protected` flag.
        self.0.prefix == other.0.prefix
            && self.0.reverse_property == other.0.reverse_property
            && self.0.language == other.0.language
            && self.0.direction == other.0.direction
            && self.0.nest == other.0.nest
            && self.0.index == other.0.index
            && self.0.container == other.0.container
            && self.0.base_url == other.0.base_url
            && self.0.value == other.0.value
            && self.0.typ == other.0.typ
            && self.0.context == other.0.context
    }
}

impl<T: Eq, B: Eq> Eq for ModuloProtected<&NormalTermDefinition<T, B>> {}

impl<'b> PartialEq<ModuloProtected<&'b TypeTermDefinition>> for ModuloProtected<&TypeTermDefinition> {
    fn eq(&self, other: &ModuloProtected<&'b TypeTermDefinition>) -> bool {
        // NOTE we ignore the `protected` flag.
        self.0.container == other.0.container
    }
}

impl Eq for ModuloProtected<&TypeTermDefinition> {}

impl<'a, 'b, T: PartialEq, B: PartialEq> PartialEq<ModuloProtected<TermDefinitionRef<'b, T, B>>> for ModuloProtected<TermDefinitionRef<'a, T, B>> {
    fn eq(&self, other: &ModuloProtected<TermDefinitionRef<'b, T, B>>) -> bool {
        // NOTE we ignore the `protected` flag.
        self.0.prefix() == other.0.prefix()
            && self.0.reverse_property() == other.0.reverse_property()
            && self.0.language() == other.0.language()
            && self.0.direction() == other.0.direction()
            && self.0.nest() == other.0.nest()
            && self.0.index() == other.0.index()
            && self.0.container() == other.0.container()
            && self.0.base_url() == other.0.base_url()
            && self.0.value() == other.0.value()
            && self.0.typ() == other.0.typ()
            && self.0.context() == other.0.context()
    }
}

impl<'a, T: Eq, B: Eq> Eq for ModuloProtected<TermDefinitionRef<'a, T, B>> {}
