use crate::{Direction, LangString, LenientLangTag, object};
use educe::Educe;
use iri_rs::{Iri, IriBuf};
use jsonld_syntax::{IntoJsonWithContext, Keyword};
use jstrict::{Number, NumberBuf};
use rdfx::vocabulary::{IriVocabulary, IriVocabularyMut};
use std::{hash::Hash, marker::PhantomData};

use super::InvalidExpandedJson;

/// Value type reference.
// `bound(false)`: every variant payload is a shared reference or a `Copy` value
// type, so this borrow type is unconditionally `Clone + Copy` regardless of `T`.
// Educe's automatic bounds would instead propagate a predicate per field type,
// which makes the impl conditional and breaks `&self` methods that consume
// `self` by copy.
#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
pub enum TypeRef<'a, T> {
    /// A JSON literal.
    Json,
    /// A type given by an IRI.
    Id(&'a T),
}

impl<'a, T> TypeRef<'a, T> {
    /// Converts this value type into the more general term type used by
    /// context definitions.
    pub fn as_syntax_type(&self) -> crate::Type<&'a T> {
        match self {
            Self::Json => crate::Type::Json,
            Self::Id(id) => crate::Type::Iri(id),
        }
    }

    /// Returns the node identifier naming this type, or `None` for the
    /// `@json` type, which names a literal rather than a node.
    pub fn into_reference<B>(self) -> Option<crate::id::Ref<'a, T, B>> {
        match self {
            Self::Json => None,
            Self::Id(t) => Some(crate::id::Ref::Iri(t)),
        }
    }
}

/// Literal value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Literal {
    /// The `null` value.
    Null,

    /// Boolean value.
    Boolean(bool),

    /// Number.
    Number(NumberBuf),

    /// String.
    String(jstrict::String),
}

impl Literal {
    /// Returns this value as a string if it is one.
    #[inline(always)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Literal::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    /// Returns this value as a boolean if it is one.
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Literal::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns this value as a number if it is one.
    #[inline(always)]
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Literal::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Converts the literal into the equivalent JSON value.
    pub fn into_json(self) -> jstrict::Value {
        match self {
            Self::Null => jstrict::Value::Null,
            Self::Boolean(b) => jstrict::Value::Boolean(b),
            Self::Number(n) => jstrict::Value::Number(n),
            Self::String(s) => jstrict::Value::String(s),
        }
    }

    /// Puts the literal into canonical form, using the given `buffer` to
    /// render numbers. Only numbers have a non-trivial canonical form.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer) {
        if let Self::Number(n) = self {
            *n = NumberBuf::from_number(n.canonical_with(buffer))
        }
    }

    /// Puts this literal into canonical form.
    pub fn canonicalize(&mut self) {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer)
    }
}

/// Value object.
///
/// Either a typed literal value, or an internationalized language string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value<T = IriBuf> {
    /// Typed literal value.
    Literal(Literal, Option<T>),

    /// Language tagged string.
    LangString(LangString),

    /// JSON literal value.
    Json(jstrict::Value),
}

impl<T> Value<T> {
    /// Creates a `null` value object.
    #[inline(always)]
    pub fn null() -> Self {
        Self::Literal(Literal::Null, None)
    }

    #[inline(always)]
    /// Returns this value as a string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Literal(lit, _) => lit.as_str(),
            Value::LangString(str) => Some(str.as_str()),
            Value::Json(_) => None,
        }
    }

    #[inline(always)]
    /// Returns the literal and its type, if this is a typed literal value.
    pub fn as_literal(&self) -> Option<(&Literal, Option<&T>)> {
        match self {
            Self::Literal(lit, ty) => Some((lit, ty.as_ref())),
            _ => None,
        }
    }

    /// Returns the `@type` IRI of this value, if it is a typed literal with
    /// one.
    pub fn literal_type(&self) -> Option<&T> {
        match self {
            Self::Literal(_, ty) => ty.as_ref(),
            _ => None,
        }
    }

    /// Sets the `@type` IRI of a typed literal value, returning the type it
    /// replaced.
    ///
    /// Has no effect and returns `None` if the value is not a typed literal.
    pub fn set_literal_type(&mut self, mut ty: Option<T>) -> Option<T> {
        match self {
            Self::Literal(_, old_ty) => {
                std::mem::swap(old_ty, &mut ty);
                ty
            }
            _ => None,
        }
    }

    /// Replaces the `@type` IRI of a typed literal value with the result of
    /// `f`.
    ///
    /// Has no effect if the value is not a typed literal.
    pub fn map_literal_type<F: FnOnce(Option<T>) -> Option<T>>(&mut self, f: F) {
        if let Self::Literal(_, ty) = self {
            *ty = f(ty.take())
        }
    }

    #[inline(always)]
    /// Returns the value as a boolean, if it is a literal holding one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Literal(lit, _) => lit.as_bool(),
            _ => None,
        }
    }

    #[inline(always)]
    /// Returns the value as a number, if it is a literal holding one.
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Value::Literal(lit, _) => lit.as_number(),
            _ => None,
        }
    }

    /// Returns the `@type` of the value, if it has one.
    ///
    /// JSON literal values report [`TypeRef::Json`]; language-tagged strings
    /// have no type and report `None`.
    pub fn typ(&self) -> Option<TypeRef<'_, T>> {
        match self {
            Value::Literal(_, Some(ty)) => Some(TypeRef::Id(ty)),
            Value::Json(_) => Some(TypeRef::Json),
            _ => None,
        }
    }

    /// Returns the `@language` of the value.
    ///
    /// Returns `None` if the value is not a language-tagged string, or is one
    /// carrying only a `@direction`.
    #[inline(always)]
    pub fn language(&self) -> Option<&LenientLangTag> {
        match self {
            Value::LangString(tag) => tag.language(),
            _ => None,
        }
    }

    /// Returns the `@direction` of the value.
    ///
    /// Returns `None` if the value is not a language-tagged string, or is one
    /// carrying only a `@language`.
    #[inline(always)]
    pub fn direction(&self) -> Option<Direction> {
        match self {
            Value::LangString(str) => str.direction(),
            _ => None,
        }
    }

    #[inline(always)]
    /// Returns an iterator over the entries of the JSON representation of
    /// the value object.
    pub fn entries(&self) -> Entries<'_, T> {
        match self {
            Self::Literal(l, ty) => Entries {
                value: Some(ValueEntryRef::Literal(l)),
                type_: ty.as_ref().map(TypeRef::Id),
                language: None,
                direction: None,
            },
            Self::LangString(l) => Entries {
                value: Some(ValueEntryRef::LangString(l.as_str())),
                type_: None,
                language: l.language(),
                direction: l.direction(),
            },
            Self::Json(j) => Entries {
                value: Some(ValueEntryRef::Json(j)),
                type_: Some(TypeRef::Json),
                language: None,
                direction: None,
            },
        }
    }

    pub(crate) fn try_from_json_object_in(
        vocabulary: &mut impl IriVocabularyMut<Iri = T>,
        mut object: jstrict::Object,
        value_entry: jstrict::object::Entry,
    ) -> Result<Self, InvalidExpandedJson> {
        match object.remove_unique("@type").map_err(InvalidExpandedJson::duplicate_key)? {
            Some(type_entry) => match type_entry.value {
                jstrict::Value::String(ty) => match ty.as_str() {
                    "@json" => Ok(Self::Json(value_entry.value)),
                    iri => match Iri::parse(iri) {
                        Ok(iri) => {
                            let ty = vocabulary.insert(iri);
                            let lit = value_entry.value.try_into()?;
                            Ok(Self::Literal(lit, Some(ty)))
                        }
                        Err(_) => Err(InvalidExpandedJson::InvalidValueType),
                    },
                },
                _ => Err(InvalidExpandedJson::InvalidValueType),
            },
            None => {
                let language = object
                    .remove_unique("@language")
                    .map_err(InvalidExpandedJson::duplicate_key)?
                    .map(jstrict::object::Entry::into_value);
                let direction = object
                    .remove_unique("@direction")
                    .map_err(InvalidExpandedJson::duplicate_key)?
                    .map(jstrict::object::Entry::into_value);

                if language.is_some() || direction.is_some() {
                    Ok(Self::LangString(LangString::try_from_json(object, value_entry.value, language, direction)?))
                } else {
                    let lit = value_entry.value.try_into()?;
                    Ok(Self::Literal(lit, None))
                }
            }
        }
    }

    /// Puts the literal held by this value object into canonical form, using
    /// the given `buffer` to render numbers.
    pub fn canonicalize_with(&mut self, buffer: &mut ryu_js::Buffer) {
        match self {
            Self::Json(json) => json.canonicalize_with(buffer),
            Self::Literal(l, _) => l.canonicalize_with(buffer),
            Self::LangString(_) => (),
        }
    }

    /// Puts the literal held by this value object into canonical form.
    pub fn canonicalize(&mut self) {
        let mut buffer = ryu_js::Buffer::new();
        self.canonicalize_with(&mut buffer)
    }

    /// Rewrites the `@type` IRI of the value, if it has one, with the given
    /// function.
    pub fn map_ids<U>(self, map_iri: impl FnOnce(T) -> U) -> Value<U> {
        match self {
            Self::Literal(l, type_) => Value::Literal(l, type_.map(map_iri)),
            Self::LangString(s) => Value::LangString(s),
            Self::Json(json) => Value::Json(json),
        }
    }
}

impl TryFrom<jstrict::Value> for Literal {
    type Error = InvalidExpandedJson;

    fn try_from(value: jstrict::Value) -> Result<Self, Self::Error> {
        match value {
            jstrict::Value::Null => Ok(Self::Null),
            jstrict::Value::Boolean(b) => Ok(Self::Boolean(b)),
            jstrict::Value::Number(n) => Ok(Self::Number(n)),
            jstrict::Value::String(s) => Ok(Self::String(s)),
            _ => Err(InvalidExpandedJson::InvalidLiteral),
        }
    }
}

impl<T, B> object::Any<T, B> for Value<T> {
    #[inline(always)]
    fn as_ref(&self) -> object::Ref<'_, T, B> {
        object::Ref::Value(self)
    }
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Entry of a value object, key and value together.
pub enum EntryRef<'a, T> {
    /// The `@value` entry, holding the literal value.
    Value(ValueEntryRef<'a>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(TypeRef<'a, T>),
    /// The `@language` entry, tagging string values with a language.
    Language(&'a LenientLangTag),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Direction),
}

impl<'a, T> EntryRef<'a, T> {
    /// Consumes this `EntryRef`, returning its key.
    pub fn into_key(self) -> EntryKey {
        match self {
            Self::Value(_) => EntryKey::Value,
            Self::Type(_) => EntryKey::Type,
            Self::Language(_) => EntryKey::Language,
            Self::Direction(_) => EntryKey::Direction,
        }
    }

    /// Returns the key of this `EntryRef`.
    pub fn key(&self) -> EntryKey {
        self.into_key()
    }

    /// Consumes this `EntryRef`, returning its value.
    pub fn into_value(self) -> EntryValueRef<'a, T> {
        match self {
            Self::Value(v) => EntryValueRef::Value(v),
            Self::Type(v) => EntryValueRef::Type(v),
            Self::Language(v) => EntryValueRef::Language(v),
            Self::Direction(v) => EntryValueRef::Direction(v),
        }
    }

    /// Returns the value of this `EntryRef`.
    pub fn value(&self) -> EntryValueRef<'a, T> {
        match self {
            Self::Value(v) => EntryValueRef::Value(*v),
            Self::Type(v) => EntryValueRef::Type(*v),
            Self::Language(v) => EntryValueRef::Language(v),
            Self::Direction(v) => EntryValueRef::Direction(*v),
        }
    }
}

#[derive(Educe)]
#[educe(Clone(bound(false)), Copy(bound(false)))]
/// Value of a value object entry.
pub enum EntryValueRef<'a, T> {
    /// The `@value` entry, holding the literal value.
    Value(ValueEntryRef<'a>),
    /// The `@type` entry, giving the type of the node or the values.
    Type(TypeRef<'a, T>),
    /// The `@language` entry, tagging string values with a language.
    Language(&'a LenientLangTag),
    /// The `@direction` entry, setting the base direction of string values.
    Direction(Direction),
}
/// Value held by the `@value` entry.
pub enum ValueEntryRef<'a> {
    /// An RDF literal.
    Literal(&'a Literal),
    /// A language-tagged string.
    LangString(&'a str),
    /// A JSON literal.
    Json(&'a jstrict::Value),
}

impl<'a> Clone for ValueEntryRef<'a> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> Copy for ValueEntryRef<'a> {}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Key of a value object entry.
pub enum EntryKey {
    /// The `@value` entry, holding the literal value.
    Value,
    /// The `@type` entry, giving the type of the node or the values.
    Type,
    /// The `@language` entry, tagging string values with a language.
    Language,
    /// The `@direction` entry, setting the base direction of string values.
    Direction,
}

impl EntryKey {
    /// Returns the JSON-LD keyword this key stands for.
    pub fn into_keyword(self) -> Keyword {
        match self {
            Self::Value => Keyword::Value,
            Self::Type => Keyword::Type,
            Self::Language => Keyword::Language,
            Self::Direction => Keyword::Direction,
        }
    }

    /// Returns the JSON-LD keyword this key stands for.
    pub fn as_keyword(&self) -> Keyword {
        self.into_keyword()
    }

    /// Returns the key as a string slice.
    pub fn into_str(&self) -> &'static str {
        match self {
            Self::Value => "@value",
            Self::Type => "@type",
            Self::Language => "@language",
            Self::Direction => "@direction",
        }
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        self.into_str()
    }
}

#[derive(Educe)]
#[educe(Clone)]
/// Iterator over the entries of a value object.
pub struct Entries<'a, T> {
    value: Option<ValueEntryRef<'a>>,
    type_: Option<TypeRef<'a, T>>,
    language: Option<&'a LenientLangTag>,
    direction: Option<Direction>,
}

impl<'a, T> Iterator for Entries<'a, T> {
    type Item = EntryRef<'a, T>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = 0;

        if self.value.is_some() {
            len += 1
        }

        if self.type_.is_some() {
            len += 1
        }

        if self.language.is_some() {
            len += 1
        }

        if self.direction.is_some() {
            len += 1
        }

        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.value.take().map(EntryRef::Value).or_else(|| {
            self.type_.take().map(EntryRef::Type).or_else(|| {
                self.language
                    .take()
                    .map(EntryRef::Language)
                    .or_else(|| self.direction.take().map(EntryRef::Direction))
            })
        })
    }
}

impl<'a, T> ExactSizeIterator for Entries<'a, T> {}

impl<'a, T> DoubleEndedIterator for Entries<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.direction.take().map(EntryRef::Direction).or_else(|| {
            self.language
                .take()
                .map(EntryRef::Language)
                .or_else(|| self.type_.take().map(EntryRef::Type).or_else(|| self.value.take().map(EntryRef::Value)))
        })
    }
}

/// Reference to any fragment that can appear in a value object.
pub enum FragmentRef<'a, T> {
    /// Value object entry.
    Entry(EntryRef<'a, T>),

    /// Value object entry key.
    Key(EntryKey),

    /// Value object entry value.
    Value(EntryValueRef<'a, T>),

    /// JSON fragment in a "@json" typed value.
    JsonFragment(jstrict::FragmentRef<'a>),
}

impl<'a, T> FragmentRef<'a, T> {
    /// Returns the IRI this fragment stands for, if it is one.
    pub fn into_iri(self) -> Option<&'a T> {
        match self {
            Self::Value(EntryValueRef::Type(TypeRef::Id(id))) => Some(id),
            _ => None,
        }
    }

    /// Returns the IRI this fragment stands for, if it is one.
    pub fn as_iri(&self) -> Option<&'a T> {
        match self {
            Self::Value(EntryValueRef::Type(TypeRef::Id(id))) => Some(id),
            _ => None,
        }
    }

    /// Checks whether this fragment renders as a JSON array.
    pub fn is_json_array(&self) -> bool {
        match self {
            Self::Value(EntryValueRef::Value(ValueEntryRef::Json(json))) => json.is_array(),
            Self::JsonFragment(json) => json.is_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment renders as a JSON object.
    pub fn is_json_object(&self) -> bool {
        match self {
            Self::Value(EntryValueRef::Value(ValueEntryRef::Json(json))) => json.is_object(),
            Self::JsonFragment(json) => json.is_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    pub fn sub_fragments(&self) -> SubFragments<'a, T> {
        match self {
            Self::Entry(e) => SubFragments::Entry(Some(e.key()), Some(e.value())),
            Self::Value(EntryValueRef::Value(ValueEntryRef::Json(json))) => match json {
                jstrict::Value::Array(a) => SubFragments::JsonFragment(jstrict::SubFragments::Array(a.iter())),
                jstrict::Value::Object(o) => SubFragments::JsonFragment(jstrict::SubFragments::Object(o.iter())),
                _ => SubFragments::None(PhantomData),
            },
            Self::JsonFragment(f) => SubFragments::JsonFragment(f.sub_fragments()),
            _ => SubFragments::None(PhantomData),
        }
    }
}

/// Iterator over the fragments held by a value object.
pub enum SubFragments<'a, T> {
    /// No value.
    None(PhantomData<T>),
    /// An object entry.
    Entry(Option<EntryKey>, Option<EntryValueRef<'a, T>>),
    /// A fragment of a JSON literal.
    JsonFragment(jstrict::SubFragments<'a>),
}

impl<'a, T: 'a> Iterator for SubFragments<'a, T> {
    type Item = FragmentRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None(_) => None,
            Self::Entry(k, v) => k.take().map(FragmentRef::Key).or_else(|| v.take().map(FragmentRef::Value)),
            Self::JsonFragment(f) => f.next().map(|v| FragmentRef::JsonFragment(v)),
        }
    }
}

impl<T, N: IriVocabulary<Iri = T>> IntoJsonWithContext<N> for Value<T> {
    fn into_json_with(self, vocabulary: &N) -> jstrict::Value {
        let mut obj = jstrict::Object::new();

        let value = match self {
            Self::Literal(lit, ty) => {
                if let Some(ty) = ty {
                    let ty_str = vocabulary.iri(&ty).map(|i| i.into_inner()).unwrap_or("<unresolved iri>");
                    obj.insert("@type".into(), ty_str.into());
                }

                lit.into_json()
            }
            Self::LangString(s) => {
                if let Some(language) = s.language() {
                    obj.insert("@language".into(), language.as_str().into());
                }

                if let Some(direction) = s.direction() {
                    obj.insert("@direction".into(), direction.as_str().into());
                }

                s.as_str().into()
            }
            Self::Json(json) => {
                obj.insert("@type".into(), "@json".into());

                json
            }
        };

        obj.insert("@value".into(), value);
        obj.into()
    }
}
