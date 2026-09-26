use hashbrown::{Equivalent, HashTable, hash_table::Entry};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::hash::{BuildHasher, Hash};

const SHARDS: usize = 16;

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
    pub fn insert(&self, key: K, value: V) {
        let shards = self
            .shards
            .get_or_init(|| Box::new(std::array::from_fn(|_| Shard(Mutex::new(HashTable::new())))));
        let hash = self.hasher.hash_one(&key);
        let mut shard = Self::shard(shards, hash).lock();
        match shard.entry(hash, |(k, _)| *k == key, |(k, _)| self.hasher.hash_one(k)) {
            Entry::Occupied(mut entry) => entry.get_mut().1 = value,
            Entry::Vacant(entry) => {
                entry.insert((key, value));
            }
        }
    }
}
