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

/// Cache of processed contexts.
///
/// Construct one per document (or per any unit where loader behaviour and
/// context-processing options stay constant) and pass it to
/// [`Process::process_full_with_cache`][crate::Process::process_full_with_cache].
pub struct ProcessingCache<T, B> {
    entries: Mutex<HashMap<u64, Arc<Context<T, B>>>>,
}

impl<T, B> ProcessingCache<T, B> {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::default()),
        }
    }

    pub fn clear(&self) {
        self.entries.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    pub(crate) fn get(&self, key: u64) -> Option<Arc<Context<T, B>>> {
        self.entries.lock().get(&key).map(Arc::clone)
    }

    pub(crate) fn insert(&self, key: u64, context: Arc<Context<T, B>>) {
        self.entries.lock().insert(key, context);
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
    // instead of materializing it as a `String`.
    write!(HashWriter(&mut hasher), "{}", local_context.pretty_print()).expect("hashing pretty_print output should not fail");

    base_url.hash(&mut hasher);

    options.processing_mode.hash(&mut hasher);
    options.override_protected.hash(&mut hasher);
    options.propagate.hash(&mut hasher);
    options.vocab.hash(&mut hasher);

    hasher.finish()
}
