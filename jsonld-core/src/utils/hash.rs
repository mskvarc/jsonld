use foldhash::fast::FixedState;
use std::hash::{BuildHasher, Hash, Hasher};

/// Hash a set of items.
///
/// The standard library does not provide (yet) a `Hash` implementation
/// for set types. This can be used instead.
///
/// Note that this function not particularly strong and does
/// not protect against DoS attacks.
pub fn hash_set<S: IntoIterator, H: Hasher>(set: S, hasher: &mut H)
where
    S::Item: Hash,
{
    // See: https://github.com/rust-lang/rust/pull/48366
    // Elements must be combined with a associative and commutative operation •.
    // (u64, •, 0) must form a commutative monoid.
    // This is satisfied by • = u64::wrapping_add.
    //
    // The inner hasher must be deterministic across calls so equal sets
    // produce equal hashes; foldhash's FixedState gives a fixed-seed builder.
    let inner = FixedState::default();
    let mut hash = 0u64;
    for item in set {
        let mut h = inner.build_hasher();
        item.hash(&mut h);
        hash = hash.wrapping_add(h.finish());
    }

    hasher.write_u64(hash);
}

/// Hash an optional set of items.
pub fn hash_set_opt<S: IntoIterator, H: Hasher>(set_opt: Option<S>, hasher: &mut H)
where
    S::Item: Hash,
{
    if let Some(set) = set_opt {
        hash_set(set, hasher)
    }
}

/// Hash a map.
///
/// The standard library does not provide (yet) a `Hash` implementation
/// for unordered map types. This can be used instead.
///
/// Note that this function not particularly strong and does
/// not protect against DoS attacks.
pub fn hash_map<'a, K: 'a + Hash, V: 'a + Hash, H: Hasher>(map: impl 'a + IntoIterator<Item = (&'a K, &'a V)>, hasher: &mut H) {
    let inner = FixedState::default();
    let mut hash = 0u64;
    for entry in map {
        let mut h = inner.build_hasher();
        entry.hash(&mut h);
        hash = hash.wrapping_add(h.finish());
    }

    hasher.write_u64(hash);
}
