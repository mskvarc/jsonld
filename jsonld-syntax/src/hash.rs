//! Workspace-wide hasher selection.
//!
//! Default: hashbrown's [`hashbrown::DefaultHashBuilder`] (FoldHash on
//! hashbrown 0.15+). The `ahash` and `gxhash` features each swap in a
//! different hasher.
//!
//! Cargo unifies features across a dependency graph, so two unrelated
//! dependants asking for different hashers must not break the build: when
//! both features end up enabled, `gxhash` wins.

/// Hasher used by the collections of this crate.
#[cfg(feature = "gxhash")]
pub type DefaultBuildHasher = gxhash::GxBuildHasher;

/// Hasher used by the collections of this crate.
#[cfg(all(feature = "ahash", not(feature = "gxhash")))]
pub type DefaultBuildHasher = ahash::RandomState;

/// Hasher used by the collections of this crate.
#[cfg(not(any(feature = "ahash", feature = "gxhash")))]
pub type DefaultBuildHasher = hashbrown::DefaultHashBuilder;

/// Hash map using [`DefaultBuildHasher`].
pub type HashMap<K, V, S = DefaultBuildHasher> = hashbrown::HashMap<K, V, S>;

/// Hash set using [`DefaultBuildHasher`].
pub type HashSet<T, S = DefaultBuildHasher> = hashbrown::HashSet<T, S>;

/// Insertion-ordered map using [`DefaultBuildHasher`].
pub type IndexMap<K, V, S = DefaultBuildHasher> = indexmap::IndexMap<K, V, S>;

/// Insertion-ordered set using [`DefaultBuildHasher`].
pub type IndexSet<T, S = DefaultBuildHasher> = indexmap::IndexSet<T, S>;
