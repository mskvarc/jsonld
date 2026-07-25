use iri_rs::IriBuf;

use super::{BindingRef, Context, Key};
use crate::{Container, Direction, HashMap, LenientLangTag, LenientLangTagBuf, Nullable, Term, Type};
use hashbrown::hash_map::Entry;
use std::{fmt, hash::Hash};

/// Length-then-lex ordering: `true` if `a` is "shorter or, on tie, lex-less"
/// than `b`. Matches the inverse-context build rule for picking the smaller
/// term within a slot.
#[inline]
fn term_lt(a: &Key, b: &Key) -> bool {
    let al = a.as_str().len();
    let bl = b.as_str().len();
    al < bl || (al == bl && a.as_str() < b.as_str())
}

#[inline]
fn keep_smaller_opt(slot: &mut Option<Key>, candidate: &Key) {
    match slot {
        None => *slot = Some(*candidate),
        Some(existing) if term_lt(candidate, existing) => *slot = Some(*candidate),
        _ => {}
    }
}

#[inline]
fn keep_smaller(slot: &mut Key, candidate: &Key) {
    if term_lt(candidate, slot) {
        *slot = *candidate;
    }
}

#[derive(Clone, PartialEq, Eq)]
/// Type criterion used when selecting a term from the inverse context.
pub enum TypeSelection<T = IriBuf> {
    /// Select a reverse property.
    Reverse,
    /// Accept any type.
    Any,
    /// Select a term whose type mapping matches.
    Type(Type<T>),
}

impl<T: fmt::Debug> fmt::Debug for TypeSelection<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TypeSelection::Reverse => write!(f, "Reverse"),
            TypeSelection::Any => write!(f, "Any"),
            TypeSelection::Type(ty) => write!(f, "Type({ty:?})"),
        }
    }
}

struct InverseType<T> {
    reverse: Option<Key>,
    any: Option<Key>,
    map: HashMap<Type<T>, Key>,
}

impl<T> InverseType<T> {
    fn select(&self, selection: TypeSelection<T>) -> Option<&Key>
    where
        T: Hash + Eq,
    {
        match selection {
            TypeSelection::Reverse => self.reverse.as_ref(),
            TypeSelection::Any => self.any.as_ref(),
            TypeSelection::Type(ty) => self.map.get(&ty),
        }
    }

    fn set_any(&mut self, term: &Key) {
        keep_smaller_opt(&mut self.any, term);
    }

    fn set_none(&mut self, term: &Key)
    where
        T: Clone + Hash + Eq,
    {
        self.set(&Type::None, term)
    }

    fn set(&mut self, ty: &Type<T>, term: &Key)
    where
        T: Clone + Hash + Eq,
    {
        match self.map.entry(ty.clone()) {
            Entry::Vacant(v) => {
                v.insert(*term);
            }
            Entry::Occupied(mut o) if term_lt(term, o.get()) => {
                o.insert(*term);
            }
            _ => {}
        }
    }
}

type LangDir = Nullable<(Option<LenientLangTagBuf>, Option<Direction>)>;

struct InverseLang {
    any: Option<Key>,
    map: HashMap<LangDir, Key>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// Language criterion used when selecting a term from the inverse context.
pub enum LangSelection<'a> {
    /// Accept any language.
    Any,
    /// Select a term matching the given language and base direction.
    Lang(Nullable<(Option<&'a LenientLangTag>, Option<Direction>)>),
}

impl InverseLang {
    fn select(&self, selection: LangSelection) -> Option<&Key> {
        match selection {
            LangSelection::Any => self.any.as_ref(),
            LangSelection::Lang(lang_dir) => {
                let lang_dir = lang_dir.map(|(l, d)| (l.map(|l| l.to_owned()), d));
                self.map.get(&lang_dir)
            }
        }
    }

    fn set_any(&mut self, term: &Key) {
        keep_smaller_opt(&mut self.any, term);
    }

    fn set_none(&mut self, term: &Key) {
        self.set(Nullable::Some((None, None)), term)
    }

    fn set(&mut self, lang_dir: Nullable<(Option<&LenientLangTag>, Option<Direction>)>, term: &Key) {
        let lang_dir = lang_dir.map(|(l, d)| (l.map(|l| l.to_owned()), d));
        match self.map.entry(lang_dir) {
            Entry::Vacant(v) => {
                v.insert(*term);
            }
            Entry::Occupied(mut o) if term_lt(term, o.get()) => {
                o.insert(*term);
            }
            _ => {}
        }
    }
}

struct InverseContainer<T> {
    language: InverseLang,
    typ: InverseType<T>,
    any: Any,
}

struct Any {
    none: Key,
}

impl<T> InverseContainer<T> {
    pub fn new(term: &Key) -> InverseContainer<T> {
        InverseContainer {
            language: InverseLang {
                any: None,
                map: HashMap::default(),
            },
            typ: InverseType {
                reverse: None,
                any: None,
                map: HashMap::default(),
            },
            any: Any { none: *term },
        }
    }
}

/// Inverse definition of a term, indexed by container, type and language.
pub struct InverseDefinition<T> {
    map: HashMap<Container, InverseContainer<T>>,
}

impl<T> InverseDefinition<T> {
    fn new() -> InverseDefinition<T> {
        InverseDefinition { map: HashMap::default() }
    }

    fn get(&self, container: &Container) -> Option<&InverseContainer<T>> {
        self.map.get(container)
    }

    fn contains(&self, container: &Container) -> bool {
        self.map.contains_key(container)
    }

    fn reference_mut<F: FnOnce() -> InverseContainer<T>>(&mut self, container: &Container, insert: F) -> &mut InverseContainer<T> {
        if !self.contains(container) {
            self.map.insert(*container, insert());
        }
        // SAFETY: just inserted above if not present.
        unsafe { self.map.get_mut(container).unwrap_unchecked() }
    }

    /// Returns the select of this `InverseDefinition`.
    pub fn select(&self, containers: &[Container], selection: &Selection<T>) -> Option<&Key>
    where
        T: Clone + Hash + Eq,
    {
        for container in containers {
            if let Some(type_lang_map) = self.get(container) {
                match selection {
                    Selection::Any => return Some(&type_lang_map.any.none),
                    Selection::Type(preferred_values) => {
                        for item in preferred_values {
                            if let Some(term) = type_lang_map.typ.select(item.clone()) {
                                return Some(term);
                            }
                        }
                    }
                    Selection::Lang(preferred_values) => {
                        for item in preferred_values {
                            if let Some(term) = type_lang_map.language.select(*item) {
                                return Some(term);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

/// Inverse context.
pub struct InverseContext<T, B> {
    map: HashMap<Term<T, B>, InverseDefinition<T>>,
}

/// Criterion used to pick a term out of the inverse context.
pub enum Selection<'a, T> {
    /// Accept any term.
    Any,
    /// Prefer terms matching one of the given type criteria, in order.
    Type(Vec<TypeSelection<T>>),
    /// Prefer terms matching one of the given language criteria, in order.
    Lang(Vec<LangSelection<'a>>),
}

impl<'a, T: fmt::Debug> fmt::Debug for Selection<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Selection::Any => write!(f, "Any"),
            Selection::Type(s) => write!(f, "Type({s:?})"),
            Selection::Lang(s) => write!(f, "Lang({s:?})"),
        }
    }
}

impl<T, B> InverseContext<T, B> {
    /// Creates a new `Selection`.
    pub fn new() -> Self {
        InverseContext { map: HashMap::default() }
    }
}

impl<T: Hash + Eq, B: Hash + Eq> InverseContext<T, B> {
    /// Checks whether this `Selection` contains.
    pub fn contains(&self, term: &Term<T, B>) -> bool {
        self.map.contains_key(term)
    }

    /// Inserts an entry into this `Selection`, returning the entry it replaced.
    pub fn insert(&mut self, term: Term<T, B>, value: InverseDefinition<T>) {
        self.map.insert(term, value);
    }

    /// Returns the value bound to the given key, if any.
    pub fn get(&self, term: &Term<T, B>) -> Option<&InverseDefinition<T>> {
        self.map.get(term)
    }

    /// Returns a mutable reference to the value bound to the given key, if any.
    pub fn get_mut(&mut self, term: &Term<T, B>) -> Option<&mut InverseDefinition<T>> {
        self.map.get_mut(term)
    }

    fn reference_mut<F: FnOnce() -> InverseDefinition<T>>(&mut self, term: &Term<T, B>, insert: F) -> &mut InverseDefinition<T>
    where
        T: Clone,
        B: Clone,
    {
        if !self.contains(term) {
            self.insert(term.clone(), insert());
        }
        // SAFETY: just inserted above if not present.
        unsafe { self.map.get_mut(term).unwrap_unchecked() }
    }

    /// Returns the select of this `Selection`.
    pub fn select(&self, var: &Term<T, B>, containers: &[Container], selection: &Selection<T>) -> Option<&Key>
    where
        T: Clone,
    {
        match self.get(var) {
            Some(container_map) => container_map.select(containers, selection),
            None => None,
        }
    }
}

impl<T, B> Default for InverseContext<T, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: Clone + Hash + Eq, B: Clone + Hash + Eq> From<&'a Context<T, B>> for InverseContext<T, B> {
    fn from(context: &'a Context<T, B>) -> Self {
        let mut result = InverseContext::new();

        // No upfront sort: instead, every slot keeps the smaller term
        // (length-then-lex) on collision. Same end-state as the old sort-then-
        // first-wins approach, without the O(P log P) cost.
        for binding in context.definitions().iter() {
            if let BindingRef::Normal(term, term_definition) = binding
                && let Some(var) = term_definition.value.as_ref()
            {
                let container = &term_definition.container;
                let container_map = result.reference_mut(var, InverseDefinition::new);
                let type_lang_map = container_map.reference_mut(container, || InverseContainer::new(term));

                // `any.none` is initialized on first insert by
                // `InverseContainer::new`; subsequent bindings still need
                // to update it if they are smaller.
                keep_smaller(&mut type_lang_map.any.none, term);

                let type_map = &mut type_lang_map.typ;
                let lang_map = &mut type_lang_map.language;

                if term_definition.reverse_property {
                    // If the term definition indicates that the term represents a reverse property:
                    keep_smaller_opt(&mut type_map.reverse, term);
                } else {
                    match &term_definition.typ {
                        Some(Type::None) => {
                            // Otherwise, if term definition has a type mapping which is @none:
                            type_map.set_any(term);
                            lang_map.set_any(term);
                        }
                        Some(typ) => {
                            // Otherwise, if term definition has a type mapping:
                            type_map.set(typ, term)
                        }
                        None => {
                            match (&term_definition.language, &term_definition.direction) {
                                (Some(language), Some(direction)) => {
                                    // Otherwise, if term definition has both a language mapping
                                    // and a direction mapping:
                                    match (language, direction) {
                                        (Nullable::Some(language), Nullable::Some(direction)) => {
                                            lang_map.set(Nullable::Some((Some(language.as_lenient_lang_tag_ref()), Some(*direction))), term)
                                        }
                                        (Nullable::Some(language), Nullable::Null) => {
                                            lang_map.set(Nullable::Some((Some(language.as_lenient_lang_tag_ref()), None)), term)
                                        }
                                        (Nullable::Null, Nullable::Some(direction)) => lang_map.set(Nullable::Some((None, Some(*direction))), term),
                                        (Nullable::Null, Nullable::Null) => lang_map.set(Nullable::Null, term),
                                    }
                                }
                                (Some(language), None) => {
                                    // Otherwise, if term definition has a language mapping (might
                                    // be null):
                                    match language {
                                        Nullable::Some(language) => lang_map.set(Nullable::Some((Some(language.as_lenient_lang_tag_ref()), None)), term),
                                        Nullable::Null => lang_map.set(Nullable::Null, term),
                                    }
                                }
                                (None, Some(direction)) => {
                                    // Otherwise, if term definition has a direction mapping (might
                                    // be null):
                                    match direction {
                                        Nullable::Some(direction) => lang_map.set(Nullable::Some((None, Some(*direction))), term),
                                        Nullable::Null => lang_map.set(Nullable::Some((None, None)), term),
                                    }
                                }
                                (None, None) => {
                                    lang_map.set(Nullable::Some((context.default_language(), context.default_base_direction())), term);
                                    lang_map.set_none(term);
                                    type_map.set_none(term);
                                }
                            }
                        }
                    }
                }
            }
        }

        result
    }
}
