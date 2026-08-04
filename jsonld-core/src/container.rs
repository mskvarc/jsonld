pub use jsonld_syntax::ContainerKind;
use jsonld_syntax::{Nullable, context::definition::TypeContainer};

/// Error raised when a set of `@container` values cannot be combined.
///
/// Only certain combinations are allowed by the JSON-LD grammar — `@set` pairs
/// with most other values, but `@list` and `@language` do not pair with
/// `@index`, for instance.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid container")]
pub struct InvalidContainer;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// Container mapping of a term: the combination of `@container` values it
/// was defined with.
pub enum Container {
    /// Empty container: no `@container` value.
    None,

    /// `@graph`: the values of the term are graph objects.
    Graph,
    /// `@id`: the values of the term form an id map, keyed by node identifier.
    Id,
    /// `@index`: the values of the term form an index map, keyed by an
    /// arbitrary index string.
    Index,
    /// `@language`: the values of the term form a language map, keyed by
    /// language tag.
    Language,
    /// `@list`: the values of the term are an ordered list.
    List,
    /// `@set`: the values of the term are always represented as an array.
    Set,
    /// `@type`: the values of the term form a type map, keyed by node type.
    Type,

    /// `@graph` and `@set`.
    GraphSet,
    /// `@graph` and `@id`.
    GraphId,
    /// `@graph` and `@index`.
    GraphIndex,
    /// `@id` and `@set`.
    IdSet,
    /// `@index` and `@set`.
    IndexSet,
    /// `@language` and `@set`.
    LanguageSet,
    /// `@set` and `@type`.
    SetType,

    /// `@graph`, `@id` and `@set`.
    GraphIdSet,
    /// `@graph`, `@index` and `@set`.
    GraphIndexSet,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// Creates a new `Container`.
    pub fn new() -> Container {
        Container::None
    }

    /// Builds a container mapping from its syntactic form.
    pub fn from_syntax(r: Nullable<&jsonld_syntax::Container>) -> Result<Self, InvalidContainer> {
        match r {
            Nullable::Null => Ok(Self::None),
            Nullable::Some(jsonld_syntax::Container::One(c)) => Ok((*c).into()),
            Nullable::Some(jsonld_syntax::Container::Many(m)) => {
                let mut container = Container::new();

                for t in m {
                    if !container.add(*t) {
                        return Err(InvalidContainer);
                    }
                }

                Ok(container)
            }
        }
    }

    /// Builds a container mapping from the `@container` values it combines.
    pub fn from<'a, I: IntoIterator<Item = &'a ContainerKind>>(iter: I) -> Result<Container, ContainerKind> {
        let mut container = Container::new();
        for item in iter {
            if !container.add(*item) {
                return Err(*item);
            }
        }

        Ok(container)
    }

    /// Returns the combined `@container` values as a slice.
    pub fn as_slice(&self) -> &[ContainerKind] {
        use Container::*;
        match self {
            None => &[],
            Graph => &[ContainerKind::Graph],
            Id => &[ContainerKind::Id],
            Index => &[ContainerKind::Index],
            Language => &[ContainerKind::Language],
            List => &[ContainerKind::List],
            Set => &[ContainerKind::Set],
            Type => &[ContainerKind::Type],
            GraphSet => &[ContainerKind::Graph, ContainerKind::Set],
            GraphId => &[ContainerKind::Graph, ContainerKind::Id],
            GraphIndex => &[ContainerKind::Graph, ContainerKind::Index],
            IdSet => &[ContainerKind::Id, ContainerKind::Set],
            IndexSet => &[ContainerKind::Index, ContainerKind::Set],
            LanguageSet => &[ContainerKind::Language, ContainerKind::Set],
            SetType => &[ContainerKind::Type, ContainerKind::Set],
            GraphIdSet => &[ContainerKind::Graph, ContainerKind::Id, ContainerKind::Set],
            GraphIndexSet => &[ContainerKind::Graph, ContainerKind::Index, ContainerKind::Set],
        }
    }

    /// Returns an iterator over the combined `@container` values.
    pub fn iter(&self) -> impl Iterator<Item = &ContainerKind> {
        self.as_slice().iter()
    }

    /// Returns the number of combined `@container` values.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Checks whether the container mapping is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Container::None)
    }

    /// Checks whether the mapping includes the given `@container` value.
    pub fn contains(&self, c: ContainerKind) -> bool {
        self.as_slice().contains(&c)
    }

    /// Returns this mapping extended with the given `@container` value, or
    /// `None` if the combination is not allowed by the JSON-LD grammar.
    pub fn with(&self, c: ContainerKind) -> Option<Container> {
        let new_container = match (self, c) {
            (Container::None, c) => c.into(),
            (Container::Graph, ContainerKind::Graph) => *self,
            (Container::Graph, ContainerKind::Set) => Container::GraphSet,
            (Container::Graph, ContainerKind::Id) => Container::GraphId,
            (Container::Graph, ContainerKind::Index) => Container::GraphIndex,
            (Container::Id, ContainerKind::Id) => *self,
            (Container::Id, ContainerKind::Graph) => Container::GraphId,
            (Container::Id, ContainerKind::Set) => Container::IdSet,
            (Container::Index, ContainerKind::Index) => *self,
            (Container::Index, ContainerKind::Graph) => Container::GraphIndex,
            (Container::Index, ContainerKind::Set) => Container::IndexSet,
            (Container::Language, ContainerKind::Language) => *self,
            (Container::Language, ContainerKind::Set) => Container::LanguageSet,
            (Container::List, ContainerKind::List) => *self,
            (Container::Set, ContainerKind::Set) => *self,
            (Container::Set, ContainerKind::Graph) => Container::GraphSet,
            (Container::Set, ContainerKind::Id) => Container::IdSet,
            (Container::Set, ContainerKind::Index) => Container::IndexSet,
            (Container::Set, ContainerKind::Language) => Container::LanguageSet,
            (Container::Set, ContainerKind::Type) => Container::SetType,
            (Container::Type, ContainerKind::Type) => *self,
            (Container::Type, ContainerKind::Set) => Container::SetType,
            (Container::GraphSet, ContainerKind::Graph) => *self,
            (Container::GraphSet, ContainerKind::Set) => *self,
            (Container::GraphSet, ContainerKind::Id) => Container::GraphIdSet,
            (Container::GraphSet, ContainerKind::Index) => Container::GraphIndexSet,
            (Container::GraphId, ContainerKind::Graph) => *self,
            (Container::GraphId, ContainerKind::Id) => *self,
            (Container::GraphId, ContainerKind::Set) => Container::GraphIdSet,
            (Container::GraphIndex, ContainerKind::Graph) => *self,
            (Container::GraphIndex, ContainerKind::Index) => *self,
            (Container::GraphIndex, ContainerKind::Set) => Container::GraphIndexSet,
            (Container::IdSet, ContainerKind::Id) => *self,
            (Container::IdSet, ContainerKind::Set) => *self,
            (Container::IdSet, ContainerKind::Graph) => Container::GraphIdSet,
            (Container::IndexSet, ContainerKind::Index) => *self,
            (Container::IndexSet, ContainerKind::Set) => *self,
            (Container::IndexSet, ContainerKind::Graph) => Container::GraphIndexSet,
            (Container::LanguageSet, ContainerKind::Language) => *self,
            (Container::LanguageSet, ContainerKind::Set) => *self,
            (Container::SetType, ContainerKind::Set) => *self,
            (Container::SetType, ContainerKind::Type) => *self,
            (Container::GraphIdSet, ContainerKind::Graph) => *self,
            (Container::GraphIdSet, ContainerKind::Id) => *self,
            (Container::GraphIdSet, ContainerKind::Set) => *self,
            (Container::GraphIndexSet, ContainerKind::Graph) => *self,
            (Container::GraphIndexSet, ContainerKind::Index) => *self,
            (Container::GraphIndexSet, ContainerKind::Set) => *self,
            _ => return None,
        };

        Some(new_container)
    }

    /// Adds the given `@container` value to the mapping in place. Returns
    /// `false` (leaving the mapping unchanged) if the combination is not
    /// allowed by the JSON-LD grammar.
    pub fn add(&mut self, c: ContainerKind) -> bool {
        match self.with(c) {
            Some(container) => {
                *self = container;
                true
            }
            None => false,
        }
    }

    /// Converts the mapping into its syntactic form: a single `@container`
    /// value when it combines exactly one, an array when it combines several,
    /// and `None` when it is empty.
    pub fn into_syntax(self) -> Option<jsonld_syntax::Container> {
        let slice = self.as_slice();

        match slice.len() {
            0 => None,
            1 => Some(jsonld_syntax::Container::One(slice[0])),
            _ => Some(jsonld_syntax::Container::Many(slice.to_vec())),
        }
    }
}

impl From<ContainerKind> for Container {
    fn from(c: ContainerKind) -> Self {
        match c {
            ContainerKind::Graph => Self::Graph,
            ContainerKind::Id => Self::Id,
            ContainerKind::Index => Self::Index,
            ContainerKind::Language => Self::Language,
            ContainerKind::List => Self::List,
            ContainerKind::Set => Self::Set,
            ContainerKind::Type => Self::Type,
        }
    }
}

impl From<TypeContainer> for Container {
    fn from(c: TypeContainer) -> Self {
        match c {
            TypeContainer::Set => Container::Set,
        }
    }
}
