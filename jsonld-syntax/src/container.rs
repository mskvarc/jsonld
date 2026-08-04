use crate::Keyword;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// A single `@container` value.
pub enum ContainerKind {
    #[cfg_attr(feature = "serde", serde(rename = "@graph"))]
    /// `@graph`: the values of the term are graph objects.
    Graph,

    #[cfg_attr(feature = "serde", serde(rename = "@id"))]
    /// `@id`: the values of the term form an id map, keyed by node identifier.
    Id,

    #[cfg_attr(feature = "serde", serde(rename = "@index"))]
    /// `@index`: the values of the term form an index map, keyed by an
    /// arbitrary index string.
    Index,

    #[cfg_attr(feature = "serde", serde(rename = "@language"))]
    /// `@language`: the values of the term form a language map, keyed by
    /// language tag.
    Language,

    #[cfg_attr(feature = "serde", serde(rename = "@list"))]
    /// `@list`: the values of the term are an ordered list.
    List,

    #[cfg_attr(feature = "serde", serde(rename = "@set"))]
    /// `@set`: the values of the term are always represented as an array.
    Set,

    #[cfg_attr(feature = "serde", serde(rename = "@type"))]
    /// `@type`: the values of the term form a type map, keyed by node type.
    Type,
}

impl ContainerKind {
    /// Returns the keyword naming this container, taking `self` by value.
    #[must_use]
    pub fn into_keyword(self) -> Keyword {
        self.into()
    }

    /// Returns the keyword naming this container.
    #[must_use]
    pub fn keyword(&self) -> Keyword {
        self.into_keyword()
    }

    /// Returns the keyword naming this container as a string, such as
    /// `"@set"`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.into_keyword().into_str()
    }
}

impl<'a> TryFrom<&'a str> for ContainerKind {
    type Error = &'a str;

    fn try_from(str: &'a str) -> Result<ContainerKind, &'a str> {
        use ContainerKind::{Graph, Id, Index, Language, List, Set, Type};
        match str {
            "@graph" => Ok(Graph),
            "@id" => Ok(Id),
            "@index" => Ok(Index),
            "@language" => Ok(Language),
            "@list" => Ok(List),
            "@set" => Ok(Set),
            "@type" => Ok(Type),
            _ => Err(str),
        }
    }
}

impl TryFrom<Keyword> for ContainerKind {
    type Error = Keyword;

    fn try_from(k: Keyword) -> Result<ContainerKind, Keyword> {
        use ContainerKind::{Graph, Id, Index, Language, List, Set, Type};
        match k {
            Keyword::Graph => Ok(Graph),
            Keyword::Id => Ok(Id),
            Keyword::Index => Ok(Index),
            Keyword::Language => Ok(Language),
            Keyword::List => Ok(List),
            Keyword::Set => Ok(Set),
            Keyword::Type => Ok(Type),
            k => Err(k),
        }
    }
}

impl From<ContainerKind> for Keyword {
    fn from(c: ContainerKind) -> Keyword {
        use ContainerKind::{Graph, Id, Index, Language, List, Set, Type};
        match c {
            Graph => Keyword::Graph,
            Id => Keyword::Id,
            Index => Keyword::Index,
            Language => Keyword::Language,
            List => Keyword::List,
            Set => Keyword::Set,
            Type => Keyword::Type,
        }
    }
}

impl From<ContainerKind> for Container {
    fn from(c: ContainerKind) -> Self {
        Container::One(c)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(untagged))]
/// The `@container` entry: one value, or an array of them.
pub enum Container {
    /// A single container, written as a string.
    One(ContainerKind),
    /// Several containers, written as an array of strings.
    Many(Vec<ContainerKind>),
}

impl Container {
    /// Checks whether this entry is written as a JSON array.
    #[must_use]
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Many(_))
    }

    /// Returns an iterator over the array items of this entry.
    ///
    /// The iterator is empty for a single container, which is written as a
    /// string and therefore has no items of its own.
    #[must_use]
    pub fn sub_fragments(&self) -> SubValues<'_> {
        match self {
            Self::One(_) => SubValues::None,
            Self::Many(m) => SubValues::Many(m.iter()),
        }
    }
}

/// Iterator over the array items of a `@container` entry.
pub enum SubValues<'a> {
    /// Nothing to iterate over: the entry held a single container.
    None,
    /// The items of an array-valued entry.
    Many(std::slice::Iter<'a, ContainerKind>),
}

impl<'a> Iterator for SubValues<'a> {
    type Item = &'a ContainerKind;

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::None => (0, Some(0)),
            Self::Many(m) => m.size_hint(),
        }
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Many(m) => m.next(),
        }
    }
}

impl ExactSizeIterator for SubValues<'_> {}

impl DoubleEndedIterator for SubValues<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Many(m) => m.next_back(),
        }
    }
}
