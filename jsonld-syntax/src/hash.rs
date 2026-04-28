//! Workspace-wide hasher selection.
//!
//! Default: hashbrown's [`DefaultHashBuilder`] (FoldHash on hashbrown 0.15+).
//! Opt-in features: `ahash` and `gxhash` switch the default. They are mutually
//! exclusive — enabling both fails compilation.

#[cfg(all(feature = "ahash", feature = "gxhash"))]
compile_error!("features `ahash` and `gxhash` are mutually exclusive");

#[cfg(all(feature = "ahash", not(feature = "gxhash")))]
pub type DefaultBuildHasher = ahash::RandomState;

#[cfg(all(feature = "gxhash", not(feature = "ahash")))]
pub type DefaultBuildHasher = gxhash::GxBuildHasher;

#[cfg(not(any(feature = "ahash", feature = "gxhash")))]
pub type DefaultBuildHasher = hashbrown::DefaultHashBuilder;

pub type HashMap<K, V, S = DefaultBuildHasher> = hashbrown::HashMap<K, V, S>;
pub type HashSet<T, S = DefaultBuildHasher> = hashbrown::HashSet<T, S>;

pub type IndexMap<K, V, S = DefaultBuildHasher> = indexmap::IndexMap<K, V, S>;
pub type IndexSet<T, S = DefaultBuildHasher> = indexmap::IndexSet<T, S>;
