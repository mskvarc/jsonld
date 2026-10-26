use hashbrown::{Equivalent, HashTable};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::hash::{BuildHasher, Hash};

const SHARDS: usize = 16;

/// Entries one shard holds before the next insert of a new key clears it.
///
/// 256 per shard leaves roughly ten times the headroom the NGSI-LD core
/// context needs (about 230 terms, so around 25 in the fullest shard).
const ENTRIES_PER_SHARD: usize = 256;

/// Upper bound on the number of entries a [`ShardedMemo`] holds.
pub const MAX_ENTRIES: usize = ENTRIES_PER_SHARD * SHARDS;

/// One shard of a [`ShardedMemo`], aligned so that neighbouring shards' locks
/// never share a cache line. 128 bytes is the line size on Apple silicon and
/// the span of the adjacent-line prefetcher on x86.
#[repr(align(128))]
struct Shard<K, V>(Mutex<HashTable<(K, V)>>);

/// Memo map split into independently locked shards.
///
/// A processed context is shared across threads — a server caches one and
/// hands a clone to every request — so its memos see concurrent lookups. With
/// a single lock every lookup serialises; here a key's hash picks one of
/// sixteen shards, so lookups of different keys rarely meet on the same lock.
///
/// The key is hashed once per operation: the same hash selects the shard and
/// probes that shard's table.
///
/// The memo is bounded at [`MAX_ENTRIES`]. A context that lives as long as the
/// process does not accumulate one entry per distinct key ever used: a shard
/// that is full is cleared whole before the next new key goes in, and the hot
/// set repopulates. A miss recomputes a deterministic function, so eviction
/// costs time and never yields a wrong result.
pub struct ShardedMemo<K, V> {
    // Allocated on the first insert. Expansion and context processing create a
    // fresh context, and with it a fresh memo, for every document and scoped
    // context, many of which are dropped after a lookup or two; building 2 KB
    // of padded shards for each of those costs more than the lookups save.
    shards: OnceCell<Box<[Shard<K, V>; SHARDS]>>,
    hasher: crate::DefaultBuildHasher,
}

impl<K, V> Default for ShardedMemo<K, V> {
    fn default() -> Self {
        Self {
            shards: OnceCell::new(),
            hasher: crate::DefaultBuildHasher::default(),
        }
    }
}

impl<K, V> ShardedMemo<K, V> {
    /// Returns the number of memoised entries.
    ///
    /// Locks one shard at a time, so the sum is not a consistent snapshot
    /// while other threads insert. Meant for tests and introspection, not hot
    /// paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.get().map_or(0, |shards| shards.iter().map(|shard| shard.0.lock().len()).sum())
    }

    /// Returns `true` when no entry is memoised. Subject to the same
    /// consistency caveat as [`len`](Self::len).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K: Hash + Eq, V: Clone> ShardedMemo<K, V> {
    // The table indexes buckets with the low bits of the hash and tags them
    // with the top seven, so the shard comes from bits in between, masked to
    // four bits before the cast.
    #[allow(clippy::cast_possible_truncation)]
    fn shard(shards: &[Shard<K, V>; SHARDS], hash: u64) -> &Mutex<HashTable<(K, V)>> {
        &shards[((hash >> 48) & (SHARDS as u64 - 1)) as usize].0
    }

    /// Returns a clone of the value memoised for `key`, if any.
    pub fn get<Q: Hash + Equivalent<K> + ?Sized>(&self, key: &Q) -> Option<V> {
        let shards = self.shards.get()?;
        let hash = self.hasher.hash_one(key);
        Self::shard(shards, hash).lock().find(hash, |(k, _)| key.equivalent(k)).map(|(_, v)| v.clone())
    }

    /// Memoises `value` for `key`, replacing any value already there.
    ///
    /// Inserting a new key into a full shard clears that shard first.
    pub fn insert(&self, key: K, value: V) {
        let shards = self
            .shards
            .get_or_init(|| Box::new(std::array::from_fn(|_| Shard(Mutex::new(HashTable::new())))));
        let hash = self.hasher.hash_one(&key);
        let mut shard = Self::shard(shards, hash).lock();
        if let Some(entry) = shard.find_mut(hash, |(k, _)| *k == key) {
            entry.1 = value;
        } else {
            if shard.len() >= ENTRIES_PER_SHARD {
                shard.clear();
            }
            shard.insert_unique(hash, (key, value), |(k, _)| self.hasher.hash_one(k));
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{ENTRIES_PER_SHARD, MAX_ENTRIES, ShardedMemo};
    use std::hash::BuildHasher;

    #[test]
    fn a_full_shard_is_cleared_before_the_next_insert() {
        let memo = ShardedMemo::<u64, u64>::default();
        // Allocates the shards so keys can be matched to shard 0.
        memo.insert(0, 0);
        let shards = memo.shards.get().unwrap();
        let first_shard = &shards[0].0;
        let mut keys = (1..).filter(|key| std::ptr::eq(ShardedMemo::shard(shards, memo.hasher.hash_one(key)), first_shard));

        while first_shard.lock().len() < ENTRIES_PER_SHARD {
            let key = keys.next().unwrap();
            memo.insert(key, key);
        }
        let evicted = first_shard.lock().iter().map(|(key, _)| *key).next().unwrap();
        let next = keys.next().unwrap();
        memo.insert(next, next);

        assert_eq!(first_shard.lock().len(), 1);
        assert_eq!(memo.get(&next), Some(next));
        assert_eq!(memo.get(&evicted), None);
    }

    #[test]
    fn the_entry_count_stays_at_or_below_the_cap_across_all_shards() {
        let memo = ShardedMemo::<u64, u64>::default();
        for key in 0..(2 * MAX_ENTRIES) as u64 {
            memo.insert(key, key);
        }

        // A clear resets one shard to refill from zero, so the total sits at
        // or below the cap rather than exactly on it.
        assert!(memo.len() <= MAX_ENTRIES, "memo holds {} entries", memo.len());
        assert!(!memo.is_empty());
    }

    #[test]
    fn reinserting_a_present_key_replaces_its_value_without_growing() {
        let memo = ShardedMemo::<u64, u64>::default();
        memo.insert(1, 10);
        memo.insert(1, 20);

        assert_eq!(memo.len(), 1);
        assert_eq!(memo.get(&1), Some(20));
    }
}
