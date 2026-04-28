use crate::{
    Term,
    object::{InvalidExpandedJson, TryFromJson},
};
use contextual::{AsRefWithContext, DisplayWithContext, WithContext};
use hashbrown::{HashMap, HashSet};
use iri_rs::{Iri, IriBuf};
use jsonld_syntax::IntoJsonWithContext;
use rdf_rs::{
    BlankId,
    BlankIdBuf,
    InvalidBlankId,
    LocalGenerator,
    LocalTerm,
    Term as RdfTerm,
    vocabulary::{BlankIdVocabulary, IriVocabulary, Vocabulary, VocabularyMut},
};
use std::{convert::TryFrom, fmt, hash::Hash};

/// Valid node identifier (IRI or blank node).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ValidId<I = IriBuf, B = BlankIdBuf> {
    Iri(I),
    Blank(B),
}

impl<I, B> ValidId<I, B> {
    pub fn map<J, C>(self, f: impl FnOnce(Self) -> ValidId<J, C>) -> ValidId<J, C> {
        f(self)
    }

    pub fn as_ref(&self) -> ValidId<&I, &B> {
        match self {
            Self::Iri(i) => ValidId::Iri(i),
            Self::Blank(b) => ValidId::Blank(b),
        }
    }

    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank(_))
    }

    pub fn is_iri(&self) -> bool {
        matches!(self, Self::Iri(_))
    }
}

impl<I: AsRef<str>, B: AsRef<str>> ValidId<I, B> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Iri(i) => i.as_ref(),
            Self::Blank(b) => b.as_ref(),
        }
    }
}

impl<I: fmt::Display, B: fmt::Display> fmt::Display for ValidId<I, B> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Iri(i) => i.fmt(f),
            Self::Blank(b) => b.fmt(f),
        }
    }
}

impl<V: IriVocabulary + BlankIdVocabulary> DisplayWithContext<V> for ValidId<V::Iri, V::BlankId> {
    fn fmt_with(&self, vocabulary: &V, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Iri(i) => write!(f, "{}", vocabulary.iri(i).unwrap()),
            Self::Blank(b) => write!(f, "{}", vocabulary.blank_id(b).unwrap()),
        }
    }
}

impl<I: rdf_rs::Interpretation, T, B> ld_core::LinkedDataResource<I> for ValidId<T, B>
where
    T: ld_core::LinkedDataResource<I>,
    B: ld_core::LinkedDataResource<I>,
{
    fn interpretation(&self, interpretation: &mut I) -> ld_core::ResourceInterpretation<'_, I> {
        match self {
            Self::Iri(i) => i.interpretation(interpretation),
            Self::Blank(b) => b.interpretation(interpretation),
        }
    }
}

impl<I: rdf_rs::Interpretation, T, B> ld_core::LinkedDataSubject<I> for ValidId<T, B>
where
    T: ld_core::LinkedDataSubject<I>,
    B: ld_core::LinkedDataSubject<I>,
{
    fn visit_subject<S>(&self, visitor: S) -> Result<S::Ok, S::Error>
    where
        S: ld_core::SubjectVisitor<I>,
    {
        match self {
            Self::Iri(i) => i.visit_subject(visitor),
            Self::Blank(b) => b.visit_subject(visitor),
        }
    }
}

pub type ValidVocabularyId<V> = ValidId<<V as IriVocabulary>::Iri, <V as BlankIdVocabulary>::BlankId>;

pub type VocabularyId<V> = Id<<V as IriVocabulary>::Iri, <V as BlankIdVocabulary>::BlankId>;

/// Node identifier.
///
/// Used to reference a node across a document or to a remote document.
/// It can be an identifier (IRI), a blank node identifier for local blank nodes
/// or an invalid reference (a string that is neither an IRI nor blank node identifier).
///
/// # `Hash` implementation
///
/// It is guaranteed that the `Hash` implementation of `Id` is *transparent*,
/// meaning that the hash of `Id::Valid(id)` the same as `id`, and the hash of
/// `Id::Invalid(id)` is the same as `id`.
///
/// This may be useful to define custom [`indexmap::Equivalent<Id<I, B>>`]
/// implementation.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Id<I = IriBuf, B = BlankIdBuf> {
    /// Valid node identifier.
    Valid(ValidId<I, B>),

    /// Invalid reference.
    Invalid(String),
}

#[allow(clippy::derived_hash_with_manual_eq)]
impl<I: Hash, B: Hash> Hash for Id<I, B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Valid(id) => id.hash(state),
            Self::Invalid(id) => id.hash(state),
        }
    }
}

impl<I: PartialEq, B: PartialEq> indexmap::Equivalent<Id<I, B>> for ValidId<I, B> {
    fn equivalent(&self, key: &Id<I, B>) -> bool {
        match key {
            Id::Valid(id) => self == id,
            _ => false,
        }
    }
}

impl<'a, B> indexmap::Equivalent<Id<IriBuf, B>> for Iri<&'a str> {
    fn equivalent(&self, key: &Id<IriBuf, B>) -> bool {
        match key {
            Id::Valid(ValidId::Iri(iri)) => self.as_str() == iri.as_str(),
            _ => false,
        }
    }
}

impl<B> indexmap::Equivalent<Id<IriBuf, B>> for iri_rs::IriBuf {
    fn equivalent(&self, key: &Id<IriBuf, B>) -> bool {
        match key {
            Id::Valid(ValidId::Iri(iri)) => self == iri,
            _ => false,
        }
    }
}

impl<I> indexmap::Equivalent<Id<I, BlankIdBuf>> for BlankId {
    fn equivalent(&self, key: &Id<I, BlankIdBuf>) -> bool {
        match key {
            Id::Valid(ValidId::Blank(b)) => self == b.as_blank_id_ref(),
            _ => false,
        }
    }
}

impl<I> indexmap::Equivalent<Id<I, BlankIdBuf>> for BlankIdBuf {
    fn equivalent(&self, key: &Id<I, BlankIdBuf>) -> bool {
        match key {
            Id::Valid(ValidId::Blank(b)) => self == b,
            _ => false,
        }
    }
}

impl<I, B> TryFromJson<I, B> for Id<I, B> {
    fn try_from_json_in(vocabulary: &mut impl VocabularyMut<Iri = I, BlankId = B>, value: json_syntax::Value) -> Result<Self, InvalidExpandedJson> {
        match value {
            json_syntax::Value::String(s) => match Iri::parse(s.as_str()) {
                Ok(iri) => Ok(Self::Valid(ValidId::Iri(vocabulary.insert(iri)))),
                Err(_) => match BlankId::new(s.as_str()) {
                    Ok(blank_id) => Ok(Self::Valid(ValidId::Blank(vocabulary.insert_blank_id(blank_id)))),
                    Err(_) => Ok(Self::Invalid(s.into_string())),
                },
            },
            _ => Err(InvalidExpandedJson::InvalidId),
        }
    }
}

impl<I: From<IriBuf>, B: From<BlankIdBuf>> Id<I, B> {
    pub fn from_string(s: String) -> Self {
        match IriBuf::new(s) {
            Ok(iri) => Self::Valid(ValidId::Iri(iri.into())),
            Err(e) => match BlankIdBuf::new(e.0) {
                Ok(blank) => Self::Valid(ValidId::Blank(blank.into())),
                Err(InvalidBlankId(s)) => Self::Invalid(s),
            },
        }
    }
}

impl<I, B> Id<I, B> {
    pub fn iri(iri: I) -> Self {
        Self::Valid(ValidId::Iri(iri))
    }

    pub fn blank(b: B) -> Self {
        Self::Valid(ValidId::Blank(b))
    }

    pub fn from_string_in(vocabulary: &mut impl VocabularyMut<Iri = I, BlankId = B>, s: String) -> Self {
        thread_local! {
            static VALIDATED_IRIS: std::cell::RefCell<HashSet<String>> =
                std::cell::RefCell::new(HashSet::new());
        }

        let cached = VALIDATED_IRIS.with(|c| c.borrow().contains(&s));
        if cached {
            // SAFETY: `s` was validated as an IRI in a prior call; IRI grammar
            // validity is a pure function of the byte sequence.
            let iri = unsafe { IriBuf::new_unchecked(s) };
            return Self::Valid(ValidId::Iri(vocabulary.insert_owned(iri)));
        }

        match Iri::parse(s.as_str()) {
            Ok(iri) => {
                VALIDATED_IRIS.with(|c| c.borrow_mut().insert(s.clone()));
                Self::Valid(ValidId::Iri(vocabulary.insert(iri)))
            }
            Err(_) => match BlankId::new(s.as_str()) {
                Ok(blank) => Self::Valid(ValidId::Blank(vocabulary.insert_blank_id(blank))),
                Err(_) => Self::Invalid(s),
            },
        }
    }

    /// Checks if this is a valid reference.
    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        !matches!(self, Self::Invalid(_))
    }

    pub fn into_blank(self) -> Option<B> {
        match self {
            Self::Valid(ValidId::Blank(b)) => Some(b),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn is_blank(&self) -> bool {
        matches!(self, Id::Valid(ValidId::Blank(_)))
    }

    #[inline(always)]
    pub fn as_blank(&self) -> Option<&B> {
        match self {
            Id::Valid(ValidId::Blank(k)) => Some(k),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn is_iri(&self) -> bool {
        matches!(self, Id::Valid(ValidId::Iri(_)))
    }

    #[inline(always)]
    pub fn as_iri(&self) -> Option<&I> {
        match self {
            Id::Valid(ValidId::Iri(k)) => Some(k),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn into_term(self) -> Term<I, B> {
        Term::Id(self)
    }

    pub fn as_ref(&self) -> Ref<'_, I, B> {
        match self {
            Self::Valid(ValidId::Iri(t)) => Ref::Iri(t),
            Self::Valid(ValidId::Blank(id)) => Ref::Blank(id),
            Self::Invalid(id) => Ref::Invalid(id.as_str()),
        }
    }

    pub fn map<J, C>(self, f: impl FnOnce(ValidId<I, B>) -> ValidId<J, C>) -> Id<J, C> {
        match self {
            Self::Valid(id) => Id::Valid(f(id)),
            Self::Invalid(id) => Id::Invalid(id),
        }
    }
}

impl<I: AsRef<str>, B: AsRef<str>> Id<I, B> {
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        match self {
            Id::Valid(ValidId::Iri(id)) => id.as_ref(),
            Id::Valid(ValidId::Blank(id)) => id.as_ref(),
            Id::Invalid(id) => id.as_str(),
        }
    }
}

impl<T, B, N: Vocabulary<Iri = T, BlankId = B>> AsRefWithContext<str, N> for Id<T, B> {
    fn as_ref_with<'a>(&'a self, vocabulary: &'a N) -> &'a str {
        match self {
            Id::Valid(ValidId::Iri(id)) => vocabulary.iri(id).unwrap().into_inner(),
            Id::Valid(ValidId::Blank(id)) => vocabulary.blank_id(id).unwrap().as_str(),
            Id::Invalid(id) => id.as_str(),
        }
    }
}

impl<I: PartialEq, B> PartialEq<I> for Id<I, B> {
    fn eq(&self, other: &I) -> bool {
        match self {
            Id::Valid(ValidId::Iri(id)) => id == other,
            _ => false,
        }
    }
}

impl<T: PartialEq<str>, B: PartialEq<str>> PartialEq<str> for Id<T, B> {
    fn eq(&self, other: &str) -> bool {
        match self {
            Id::Valid(ValidId::Iri(iri)) => iri == other,
            Id::Valid(ValidId::Blank(blank)) => blank == other,
            Id::Invalid(id) => id == other,
        }
    }
}

impl<'a, T, B> From<&'a Id<T, B>> for Id<&'a T, &'a B> {
    fn from(r: &'a Id<T, B>) -> Id<&'a T, &'a B> {
        match r {
            Id::Valid(ValidId::Iri(id)) => Id::Valid(ValidId::Iri(id)),
            Id::Valid(ValidId::Blank(id)) => Id::Valid(ValidId::Blank(id)),
            Id::Invalid(id) => Id::Invalid(id.clone()),
        }
    }
}

impl<T, B> From<T> for Id<T, B> {
    #[inline(always)]
    fn from(id: T) -> Id<T, B> {
        Id::Valid(ValidId::Iri(id))
    }
}

impl<T: PartialEq, B: PartialEq> PartialEq<Term<T, B>> for Id<T, B> {
    #[inline]
    fn eq(&self, term: &Term<T, B>) -> bool {
        match term {
            Term::Id(prop) => self == prop,
            _ => false,
        }
    }
}

impl<T: PartialEq, B: PartialEq> PartialEq<Id<T, B>> for Term<T, B> {
    #[inline]
    fn eq(&self, r: &Id<T, B>) -> bool {
        match self {
            Term::Id(prop) => prop == r,
            _ => false,
        }
    }
}

impl<T, B> TryFrom<Term<T, B>> for Id<T, B> {
    type Error = Term<T, B>;

    #[inline]
    fn try_from(term: Term<T, B>) -> Result<Id<T, B>, Term<T, B>> {
        match term {
            Term::Id(prop) => Ok(prop),
            term => Err(term),
        }
    }
}

impl<T: fmt::Display, B: fmt::Display> fmt::Display for Id<T, B> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Id::Valid(id) => id.fmt(f),
            Id::Invalid(id) => id.fmt(f),
        }
    }
}

impl<V: IriVocabulary + BlankIdVocabulary> DisplayWithContext<V> for Id<V::Iri, V::BlankId> {
    fn fmt_with(&self, vocabulary: &V, f: &mut fmt::Formatter) -> fmt::Result {
        use fmt::Display;
        match self {
            Id::Valid(id) => id.fmt_with(vocabulary, f),
            Id::Invalid(id) => id.fmt(f),
        }
    }
}

impl<T: fmt::Debug, B: fmt::Debug> fmt::Debug for Id<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Id::Valid(id) => write!(f, "Id::Valid({id:?})"),
            Id::Invalid(id) => write!(f, "Id::Invalid({id:?})"),
        }
    }
}

impl<T, B, N: Vocabulary<Iri = T, BlankId = B>> IntoJsonWithContext<N> for Id<T, B> {
    fn into_json_with(self, context: &N) -> json_syntax::Value {
        self.with(context).to_string().into()
    }
}

impl<T, B> From<ValidId<T, B>> for Id<T, B> {
    fn from(r: ValidId<T, B>) -> Self {
        Id::Valid(r)
    }
}

impl<T, B> TryFrom<Id<T, B>> for ValidId<T, B> {
    type Error = String;

    fn try_from(r: Id<T, B>) -> Result<Self, Self::Error> {
        match r {
            Id::Valid(r) => Ok(r),
            Id::Invalid(id) => Err(id),
        }
    }
}

impl<'a, T, B> TryFrom<&'a Id<T, B>> for &'a ValidId<T, B> {
    type Error = &'a String;

    fn try_from(r: &'a Id<T, B>) -> Result<Self, Self::Error> {
        match r {
            Id::Valid(r) => Ok(r),
            Id::Invalid(id) => Err(id),
        }
    }
}

impl<'a, T, B> TryFrom<&'a mut Id<T, B>> for &'a mut ValidId<T, B> {
    type Error = &'a mut String;

    fn try_from(r: &'a mut Id<T, B>) -> Result<Self, Self::Error> {
        match r {
            Id::Valid(r) => Ok(r),
            Id::Invalid(id) => Err(id),
        }
    }
}

/// Id to a reference.
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Ref<'a, T = IriBuf, B = BlankIdBuf> {
    /// Node identifier, essentially an IRI.
    Iri(&'a T),

    /// Blank node identifier.
    Blank(&'a B),

    /// Invalid reference.
    Invalid(&'a str),
}

/// Helper used in place of the legacy `generator.next(vocabulary)` API:
/// produces a [`ValidId`] from the next term yielded by `generator`, inserting
/// it into `vocabulary`.
pub fn generator_next_id<V, G>(vocabulary: &mut V, generator: &mut G) -> ValidId<V::Iri, V::BlankId>
where
    V: VocabularyMut,
    G: LocalGenerator,
{
    match generator.next_local_term() {
        LocalTerm::BlankId(b) => ValidId::Blank(vocabulary.insert_owned_blank_id(b)),
        LocalTerm::Named(RdfTerm::Iri(iri)) => ValidId::Iri(vocabulary.insert_owned(iri)),
        _ => panic!("generator yielded unsupported term type"),
    }
}

pub trait IdentifyAll<T, B> {
    fn identify_all_with<N: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(&mut self, vocabulary: &mut N, generator: &mut G)
    where
        T: Eq + Hash,
        B: Eq + Hash,
        N: VocabularyMut;

    fn identify_all<G: LocalGenerator>(&mut self, generator: &mut G)
    where
        T: Eq + Hash,
        B: Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.identify_all_with(rdf_rs::vocabulary::no_vocabulary_mut(), generator)
    }
}

pub trait Relabel<T, B> {
    fn relabel_with<N: Vocabulary<Iri = T, BlankId = B>, G: LocalGenerator>(
        &mut self,
        vocabulary: &mut N,
        generator: &mut G,
        relabeling: &mut HashMap<B, ValidId<T, B>>,
    ) where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
        N: VocabularyMut;

    fn relabel<G: LocalGenerator>(&mut self, generator: &mut G, relabeling: &mut HashMap<B, ValidId<T, B>>)
    where
        T: Clone + Eq + Hash,
        B: Clone + Eq + Hash,
        (): Vocabulary<Iri = T, BlankId = B>,
    {
        self.relabel_with(rdf_rs::vocabulary::no_vocabulary_mut(), generator, relabeling)
    }
}
