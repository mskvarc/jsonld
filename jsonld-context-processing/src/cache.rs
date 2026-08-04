//! Memoization for processed contexts.
//!
//! Building a [`Context`] from a syntactic [`jsonld_syntax::context::Context`]
//! is expensive: every entity in a JSON-LD document with the same `@context`
//! triggers the full [context-processing algorithm][1]. Real-world payloads
//! (NGSI-LD, schema.org) commonly share one `@context` across thousands of
//! nodes, so memoizing the result of `process_full` against a stable key
//! collapses that work to a single run.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
//!
//! # Soundness
//!
//! A cached result is only safe to reuse when the inputs to processing match
//! those of the cached run: the active context, the local context value, the
//! base URL, and the [`Options`]. The cache is also implicitly scoped to a
//! single [`Loader`][L] instance — different loaders may resolve the same IRI
//! to different documents.
//!
//! [L]: jsonld_core::Loader
//!
//! # Cache key
//!
//! - **active context fingerprint**: hash of `Arc::as_ptr` for `definitions`
//!   and `previous_context` plus the scalar fields. Two contexts that share
//!   their `Arc<Definitions>` (because one is a `Clone` of the other and
//!   neither has been mutated since) have the same fingerprint — cheap and
//!   sound thanks to copy-on-write via `Arc::make_mut`.
//! - **local context content hash**: the syntactic context is rendered via its
//!   [`Print`] implementation and hashed. Costs one allocation per lookup but
//!   stays far smaller than running the algorithm.
//! - **base URL** and **options**: trivial scalar hashing.
//!
//! # Hit verification
//!
//! The hash alone is not trusted. Each entry retains a clone of the inputs
//! and a hit is verified by exact comparison, which defends against two
//! failure modes:
//!
//! - **64-bit collisions**: two different input tuples hashing to the same
//!   key must not serve each other's results.
//! - **`Arc` pointer reuse (ABA)**: the fingerprint identifies `definitions`
//!   by address, and the allocator may hand a dropped context's address to a
//!   new one. The retained clone keeps the `Arc` alive for the entry's
//!   lifetime, so its address cannot be recycled — and any mutation of a
//!   sharing context goes through `Arc::make_mut`, which is forced to
//!   reallocate while the entry holds a second reference.

use crate::Options;
use jsonld_core::{Context, HashMap};
use jsonld_syntax::Print;
use parking_lot::Mutex;
use std::{
    fmt::Write as _,
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};

/// `fmt::Write` adapter that streams written bytes into a [`Hasher`],
/// avoiding the temporary `String` that `pretty_print().to_string()` would
/// otherwise allocate.
struct HashWriter<'a, H: Hasher>(&'a mut H);

impl<'a, H: Hasher> std::fmt::Write for HashWriter<'a, H> {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

/// One cache entry: the memoized result plus a retained clone of every input
/// the hash fingerprints, for exact verification on hit.
struct CacheEntry<T, B> {
    /// Clone of the active context captured at insertion. Cloning retains
    /// the `definitions`/`previous_context` `Arc`s, which is what makes the
    /// pointer-identity comparison in [`Self::matches`] sound (see the
    /// module docs on hit verification).
    active: Context<T, B>,
    local: jsonld_syntax::context::Context,
    base_url: Option<T>,
    options: Options,
    result: Arc<Context<T, B>>,
}

impl<T: PartialEq, B: PartialEq> CacheEntry<T, B> {
    /// Exact comparison of every input folded into [`cache_key`].
    fn matches(&self, active: &Context<T, B>, local: &jsonld_syntax::context::Context, base_url: Option<&T>, options: Options) -> bool {
        self.active.definitions_arc_ptr() == active.definitions_arc_ptr()
            && self.active.previous_context_arc_ptr() == active.previous_context_arc_ptr()
            && self.active.original_base_url() == active.original_base_url()
            && self.active.base_iri() == active.base_iri()
            && self.active.vocabulary() == active.vocabulary()
            && self.active.default_language() == active.default_language()
            && self.active.default_base_direction() == active.default_base_direction()
            && self.base_url.as_ref() == base_url
            && self.options == options
            && self.local == *local
    }
}

/// Cache of processed contexts.
///
/// Pass one to
/// [`Process::process_full_with_cache`][crate::Process::process_full_with_cache].
/// Processing options and base URL are part of the key, so they may vary freely
/// between calls; the [`Loader`][jsonld_core::Loader] is not, so a cache must not
/// outlive the loader it was populated against. One cache per document is the
/// natural unit.
///
/// Entries are never evicted, so the cache grows with the number of distinct
/// contexts processed through it. [`Self::clear`] releases them.
pub struct ProcessingCache<T, B> {
    entries: Mutex<HashMap<u64, CacheEntry<T, B>>>,
}

impl<T, B> ProcessingCache<T, B> {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::default()),
        }
    }

    /// Discards every memoized context.
    pub fn clear(&self) {
        self.entries.lock().clear();
    }

    /// Returns the number of memoized contexts.
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Checks whether nothing has been memoized yet.
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    /// Looks the key up and verifies the hit against the actual inputs. A
    /// hash collision verifies false and reads as a miss.
    pub(crate) fn get(&self, key: u64, active: &Context<T, B>, local: &jsonld_syntax::context::Context, base_url: Option<&T>, options: Options) -> Option<Arc<Context<T, B>>>
    where
        T: PartialEq,
        B: PartialEq,
    {
        let entries = self.entries.lock();
        let entry = entries.get(&key)?;
        entry.matches(active, local, base_url, options).then(|| Arc::clone(&entry.result))
    }

    /// Stores `result` under `key`, retaining clones of the inputs for hit
    /// verification. A colliding entry is overwritten — correctness never
    /// depends on which of the colliding tuples occupies the slot.
    pub(crate) fn insert(&self, key: u64, active: Context<T, B>, local: jsonld_syntax::context::Context, base_url: Option<T>, options: Options, result: Arc<Context<T, B>>) {
        self.entries.lock().insert(
            key,
            CacheEntry {
                active,
                local,
                base_url,
                options,
                result,
            },
        );
    }
}

impl<T, B> Default for ProcessingCache<T, B> {
    fn default() -> Self {
        Self::new()
    }
}

/// Computes the lookup key for a `(active, local, base_url, options)` tuple.
///
/// All inputs that influence the algorithm's result must be folded into the
/// hash for the cache to be sound.
pub(crate) fn cache_key<T, B>(active_context: &Context<T, B>, local_context: &jsonld_syntax::context::Context, base_url: Option<&T>, options: Options) -> u64
where
    T: Hash,
    B: Hash,
{
    let mut hasher = foldhash::fast::FixedState::default().build_hasher();

    // Active context fingerprint: Arc-pointer identity + scalar fields.
    active_context.definitions_arc_ptr().hash(&mut hasher);
    active_context.previous_context_arc_ptr().hash(&mut hasher);
    active_context.original_base_url().hash(&mut hasher);
    active_context.base_iri().hash(&mut hasher);
    active_context.vocabulary().hash(&mut hasher);
    active_context.default_language().hash(&mut hasher);
    active_context.default_base_direction().hash(&mut hasher);

    // Local context content hash: stream Print output into the hasher
    // instead of materializing it as a `String`. The ignored `Result` is
    // always `Ok`: `HashWriter`'s `fmt::Write` impl is infallible.
    let _ = write!(HashWriter(&mut hasher), "{}", local_context.pretty_print());

    base_url.hash(&mut hasher);

    options.processing_mode.hash(&mut hasher);
    options.override_protected.hash(&mut hasher);
    options.propagate.hash(&mut hasher);
    options.vocab.hash(&mut hasher);

    hasher.finish()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use iri_rs::IriBuf;
    use jsonld_core::ProcessingMode;
    use rdfx::BlankIdBuf;

    /// Same `u64` key, different inputs: verification must read as a miss
    /// instead of serving the other tuple's result (collision defense; the
    /// pointer-reuse defense is structural — the entry retains the `Arc`s).
    #[test]
    fn colliding_key_is_a_miss_not_a_false_hit() {
        let cache: ProcessingCache<IriBuf, BlankIdBuf> = ProcessingCache::new();
        let local = jsonld_syntax::context::Context::One(jsonld_syntax::ContextEntry::Null);

        let active_a: Context<IriBuf, BlankIdBuf> = Context::new(Some(IriBuf::new("http://a/".to_string()).unwrap()));
        let active_b: Context<IriBuf, BlankIdBuf> = Context::new(Some(IriBuf::new("http://b/".to_string()).unwrap()));

        cache.insert(42, active_a.clone(), local.clone(), None, Options::default(), Arc::new(Context::new(None)));

        // Different active context under the same key: miss.
        assert!(cache.get(42, &active_b, &local, None, Options::default()).is_none());

        // Different options under the same key: miss.
        let options_1_0 = Options {
            processing_mode: ProcessingMode::JsonLd1_0,
            ..Default::default()
        };
        assert!(cache.get(42, &active_a, &local, None, options_1_0).is_none());

        // Matching inputs: hit.
        assert!(cache.get(42, &active_a, &local, None, Options::default()).is_some());
    }
}
