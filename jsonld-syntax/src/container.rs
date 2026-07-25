use crate::Keyword;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// A single `@container` value.
pub enum ContainerKind {
    #[cfg_attr(feature = "serde", serde(rename = "@graph"))]
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph,

    #[cfg_attr(feature = "serde", serde(rename = "@id"))]
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id,

    #[cfg_attr(feature = "serde", serde(rename = "@index"))]
    /// The `@index` entry, indexing the value within its container.
    Index,

    #[cfg_attr(feature = "serde", serde(rename = "@language"))]
    /// The `@language` entry, tagging string values with a language.
    Language,

    #[cfg_attr(feature = "serde", serde(rename = "@list"))]
    /// The `@list` entry, marking the values as an ordered list.
    List,

    #[cfg_attr(feature = "serde", serde(rename = "@set"))]
    /// The `@set` entry, marking the values as an unordered set.
    Set,

    #[cfg_attr(feature = "serde", serde(rename = "@type"))]
    /// The `@type` entry, giving the type of the node or the values.
    Type,
}

impl ContainerKind {
    /// Consumes this `ContainerKind`, returning its keyword.
    pub fn into_keyword(self) -> Keyword {
        self.into()
    }

    /// Returns the keyword of this `ContainerKind`.
    pub fn keyword(&self) -> Keyword {
        self.into_keyword()
    }

    /// Returns this value as a string slice.
    pub fn as_str(&self) -> &'static str {
        self.into_keyword().into_str()
    }
}

impl<'a> TryFrom<&'a str> for ContainerKind {
    type Error = &'a str;

    fn try_from(str: &'a str) -> Result<ContainerKind, &'a str> {
        use ContainerKind::*;
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
        use ContainerKind::*;
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
        use ContainerKind::*;
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
    /// Exactly one value.
    One(ContainerKind),
    /// Several values.
    Many(Vec<ContainerKind>),
}

impl Container {
    /// Checks whether this `Container` is array.
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Many(_))
    }

    /// Returns the sub fragments of this `Container`.
    pub fn sub_fragments(&self) -> SubValues<'_> {
        match self {
            Self::One(_) => SubValues::None,
            Self::Many(m) => SubValues::Many(m.iter()),
        }
    }
}

/// Iterator over the values of a `@container` entry.
pub enum SubValues<'a> {
    /// No value.
    None,
    /// Several values.
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

impl<'a> ExactSizeIterator for SubValues<'a> {}

impl<'a> DoubleEndedIterator for SubValues<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::Many(m) => m.next_back(),
        }
    }
}
