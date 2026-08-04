//! Workspace-wide hasher selection.
//!
//! Default: hashbrown's [`hashbrown::DefaultHashBuilder`] (FoldHash on
//! hashbrown 0.15+). The `ahash` and `gxhash` features each swap in a
//! different hasher.
//!
//! Cargo unifies features across a dependency graph, so two unrelated
//! dependants asking for different hashers must not break the build: when
//! both features end up enabled, `gxhash` wins.
//!
//! # The `fast-hash` feature does not apply here
//!
//! Despite its name, the crate's default `fast-hash` feature has no effect on
//! [`DefaultBuildHasher`]: it only forwards to `iri-rs/fast-hash`, changing
//! how *that* crate hashes IRIs. Switching the hasher used by the collections
//! below requires the `ahash` or `gxhash` feature.
//!
//! Enabling `ahash` also adds no new dependency: `lasso`, which backs the
//! [string interner](crate::intern), enables its `ahasher` feature
//! unconditionally, so `ahash` is compiled either way.

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
