use iri_rs::{Iri, IriRef, IriRefBuf};
use smallvec::SmallVec;

/// Context definitions and their bindings.
pub mod definition;
mod print;
/// Term definitions, simple and expanded.
pub mod term_definition;
mod try_from_json;

pub use definition::Definition;
pub use term_definition::TermDefinition;
pub use try_from_json::{InvalidContext, MAX_CONTEXT_DEPTH};

/// JSON-LD Context.
///
/// Can represent a single context entry, or a list of context entries.
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[allow(clippy::large_enum_variant)]
pub enum Context {
    /// A single entry, written on its own.
    One(ContextEntry),
    /// Several entries, written as a JSON array.
    Many(Vec<ContextEntry>),
}

impl Default for Context {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl Context {
    /// Creates a new context with a single entry.
    pub fn one(context: ContextEntry) -> Self {
        Self::One(context)
    }

    /// Creates the `null` context.
    pub fn null() -> Self {
        Self::one(ContextEntry::Null)
    }

    /// Creates a new context with a single IRI-reference entry.
    pub fn iri_ref(iri_ref: IriRefBuf) -> Self {
        Self::one(ContextEntry::IriRef(iri_ref))
    }

    /// Creates a new context with a single context definition entry.
    pub fn definition(def: Definition) -> Self {
        Self::one(ContextEntry::Definition(def))
    }
}

impl Context {
    /// Returns the number of entries of this context, which is 1 for a single
    /// entry.
    pub fn len(&self) -> usize {
        match self {
            Self::One(_) => 1,
            Self::Many(l) => l.len(),
        }
    }

    /// Checks whether this context has no entry, which only an empty array can.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::One(_) => false,
            Self::Many(l) => l.is_empty(),
        }
    }

    /// Returns the entries of this context as a slice.
    pub fn as_slice(&self) -> &[ContextEntry] {
        match self {
            Self::One(c) => std::slice::from_ref(c),
            Self::Many(list) => list,
        }
    }

    /// Checks whether this context is a single context definition, written as
    /// a JSON object.
    pub fn is_object(&self) -> bool {
        match self {
            Self::One(c) => c.is_object(),
            _ => false,
        }
    }

    /// Checks whether this context is written as a JSON array.
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Many(_))
    }

    /// Returns a depth-first iterator over this context and every fragment it
    /// contains, itself included.
    pub fn traverse(&self) -> Traverse<'_> {
        match self {
            Self::One(c) => Traverse::new(FragmentRef::Context(c)),
            Self::Many(m) => Traverse::new(FragmentRef::ContextArray(m)),
        }
    }

    /// Returns an iterator over the entries of this context, in order.
    pub fn iter(&self) -> std::slice::Iter<'_, ContextEntry> {
        self.as_slice().iter()
    }
}

/// Owning iterator over the entries of a context.
///
/// The single-entry variant boxes its entry. A [`ContextEntry`] holding a
/// [`Definition`] is large, and inline it would set the size of the whole
/// iterator — paid by the [`Many`](Self::Many) variant too, whose own payload is
/// a [`Vec`] iterator. [`Option<Box<_>>`] is null-pointer-optimised, so no
/// allocation happens once the entry has been yielded.
pub enum IntoIter {
    /// The entry of a single-entry context, until it has been yielded.
    One(Option<Box<ContextEntry>>),
    /// The remaining entries of an array of contexts.
    Many(std::vec::IntoIter<ContextEntry>),
}

impl Iterator for IntoIter {
    type Item = ContextEntry;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::One(t) => t.take().map(|t| *t),
            Self::Many(t) => t.next(),
        }
    }
}

impl IntoIterator for Context {
    type Item = ContextEntry;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::One(t) => IntoIter::One(Some(Box::new(t))),
            Self::Many(t) => IntoIter::Many(t.into_iter()),
        }
    }
}

impl<'a> IntoIterator for &'a Context {
    type IntoIter = std::slice::Iter<'a, ContextEntry>;
    type Item = &'a ContextEntry;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl From<ContextEntry> for Context {
    fn from(c: ContextEntry) -> Self {
        Self::One(c)
    }
}

impl From<IriRefBuf> for Context {
    fn from(i: IriRefBuf) -> Self {
        Self::One(ContextEntry::IriRef(i))
    }
}

impl<'a> From<IriRef<&'a str>> for Context {
    fn from(i: IriRef<&'a str>) -> Self {
        Self::One(ContextEntry::IriRef(i.into()))
    }
}

impl From<iri_rs::IriBuf> for Context {
    fn from(i: iri_rs::IriBuf) -> Self {
        Self::One(ContextEntry::IriRef(i.into()))
    }
}

impl<'a> From<Iri<&'a str>> for Context {
    fn from(i: Iri<&'a str>) -> Self {
        Self::One(ContextEntry::IriRef(IriRefBuf::from(IriRef::<&str>::from(i))))
    }
}

impl From<Definition> for Context {
    fn from(c: Definition) -> Self {
        Self::One(ContextEntry::Definition(c))
    }
}

/// Single entry of a `@context`.
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(untagged))]
pub enum ContextEntry {
    /// The `null` value, which clears the active context.
    Null,
    /// An IRI reference to a remote context, to be dereferenced and loaded.
    IriRef(IriRefBuf),
    /// An inline context definition.
    Definition(Definition),
}

impl ContextEntry {
    fn sub_items(&self) -> ContextSubFragments<'_> {
        match self {
            Self::Definition(d) => ContextSubFragments::Definition(Box::new(d.iter())),
            _ => ContextSubFragments::None,
        }
    }

    /// Checks whether this entry is an inline context definition, written as a
    /// JSON object.
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Definition(_))
    }
}

impl From<IriRefBuf> for ContextEntry {
    fn from(i: IriRefBuf) -> Self {
        ContextEntry::IriRef(i)
    }
}

impl<'a> From<IriRef<&'a str>> for ContextEntry {
    fn from(i: IriRef<&'a str>) -> Self {
        ContextEntry::IriRef(i.into())
    }
}

impl From<iri_rs::IriBuf> for ContextEntry {
    fn from(i: iri_rs::IriBuf) -> Self {
        ContextEntry::IriRef(i.into())
    }
}

impl<'a> From<Iri<&'a str>> for ContextEntry {
    fn from(i: Iri<&'a str>) -> Self {
        ContextEntry::IriRef(IriRefBuf::from(IriRef::<&str>::from(i)))
    }
}

impl From<Definition> for ContextEntry {
    fn from(c: Definition) -> Self {
        ContextEntry::Definition(c)
    }
}

/// Fragment of a context value: any JSON value reachable inside a `@context`.
pub enum FragmentRef<'a> {
    /// An array of context entries.
    ContextArray(&'a [ContextEntry]),

    /// A single context entry.
    Context(&'a ContextEntry),

    /// A fragment of a context definition.
    DefinitionFragment(definition::FragmentRef<'a>),
}

impl<'a> FragmentRef<'a> {
    /// Checks whether this fragment is a JSON array.
    pub fn is_array(&self) -> bool {
        match self {
            Self::ContextArray(_) => true,
            Self::DefinitionFragment(i) => i.is_array(),
            _ => false,
        }
    }

    /// Checks whether this fragment is a JSON object.
    pub fn is_object(&self) -> bool {
        match self {
            Self::Context(c) => c.is_object(),
            Self::DefinitionFragment(i) => i.is_object(),
            _ => false,
        }
    }

    /// Returns an iterator over the fragments directly contained in this one.
    pub fn sub_items(&self) -> SubFragments<'a> {
        match self {
            Self::ContextArray(a) => SubFragments::ContextArray(a.iter()),
            Self::Context(c) => SubFragments::Context(c.sub_items()),
            Self::DefinitionFragment(d) => SubFragments::Definition(Box::new(d.sub_items())),
        }
    }
}

/// Iterator over the fragments held by a single context entry.
pub enum ContextSubFragments<'a> {
    /// Nothing to iterate over: the entry is `null` or an IRI reference.
    None,
    /// The entries of a context definition.
    Definition(Box<definition::Entries<'a>>),
}

impl<'a> Iterator for ContextSubFragments<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Definition(e) => e.next().map(|e| FragmentRef::DefinitionFragment(definition::FragmentRef::Entry(e))),
        }
    }
}

/// Iterator over the fragments held by a context.
pub enum SubFragments<'a> {
    /// The items of an array of contexts.
    ContextArray(std::slice::Iter<'a, ContextEntry>),
    /// The fragments of a single context entry.
    Context(ContextSubFragments<'a>),
    /// The fragments of a context definition.
    Definition(Box<definition::SubItems<'a>>),
}

impl<'a> Iterator for SubFragments<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::ContextArray(a) => a.next().map(FragmentRef::Context),
            Self::Context(i) => i.next(),
            Self::Definition(i) => i.next().map(FragmentRef::DefinitionFragment),
        }
    }
}

/// Depth-first iterator over a context and everything it contains.
pub struct Traverse<'a> {
    stack: SmallVec<[FragmentRef<'a>; 8]>,
}

impl<'a> Traverse<'a> {
    pub(crate) fn new(item: FragmentRef<'a>) -> Self {
        let mut stack = SmallVec::new();
        stack.push(item);
        Self { stack }
    }
}

impl<'a> Iterator for Traverse<'a> {
    type Item = FragmentRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some(item) => {
                self.stack.extend(item.sub_items());
                Some(item)
            }
            None => None,
        }
    }
}

/// Context document.
///
/// A context document is a JSON-LD document containing an object with a single
/// `@context` entry.
#[derive(PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContextDocument {
    #[cfg_attr(feature = "serde", serde(rename = "@context"))]
    /// The `@context` entry, the sole entry of the document.
    pub context: Context,
}
