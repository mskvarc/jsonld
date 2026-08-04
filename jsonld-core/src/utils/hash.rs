use foldhash::fast::FixedState;
use std::hash::{BuildHasher, Hash, Hasher};

/// Hashes a set of items, independently of their iteration order.
///
/// The standard library does not (yet) provide a `Hash` implementation
/// for set types. This can be used instead.
///
/// Note that this function is not particularly strong and does
/// not protect against `DoS` attacks.
pub fn hash_set<S: IntoIterator, H: Hasher>(set: S, hasher: &mut H)
where
    S::Item: Hash,
{
    // See: https://github.com/rust-lang/rust/pull/48366
    // Elements must be combined with an associative and commutative operation •.
    // (u64, •, 0) must form a commutative monoid.
    // This is satisfied by • = u64::wrapping_add.
    //
    // The inner hasher must be deterministic across calls so equal sets
    // produce equal hashes; foldhash's FixedState gives a fixed-seed builder.
    // `BuildHasher::hash_one` (stable since 1.71) does the same work without
    // materializing a fresh hasher per item via `build_hasher()`.
    let inner = FixedState::default();
    let mut hash = 0u64;
    for item in set {
        hash = hash.wrapping_add(inner.hash_one(item));
    }

    hasher.write_u64(hash);
}

/// Hash an optional set of items.
pub fn hash_set_opt<S: IntoIterator, H: Hasher>(set_opt: Option<S>, hasher: &mut H)
where
    S::Item: Hash,
{
    if let Some(set) = set_opt {
        hash_set(set, hasher);
    }
}

/// Hashes a map's entries, independently of their iteration order.
///
/// The standard library does not (yet) provide a `Hash` implementation
/// for unordered map types. This can be used instead.
///
/// Note that this function is not particularly strong and does
/// not protect against `DoS` attacks.
pub fn hash_map<'a, K: 'a + Hash, V: 'a + Hash, H: Hasher>(map: impl 'a + IntoIterator<Item = (&'a K, &'a V)>, hasher: &mut H) {
    let inner = FixedState::default();
    let mut hash = 0u64;
    for entry in map {
        hash = hash.wrapping_add(inner.hash_one(entry));
    }

    hasher.write_u64(hash);
}
