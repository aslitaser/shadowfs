//! Optimization components for the override store.

use crate::types::ShadowPath;
use bytes::Bytes;
use dashmap::DashMap;
use lru::LruCache;
use std::sync::{Arc, Mutex};
use std::num::NonZeroUsize;

/// Content hash type for deduplication
pub type ContentHash = [u8; 32];

/// Content deduplication system for eliminating duplicate data
pub struct ContentDeduplication {
    /// Map from content hash to reference-counted data
    content_hashes: DashMap<ContentHash, Arc<Bytes>>,
}

impl ContentDeduplication {
    /// Creates a new content deduplication system
    pub fn new() -> Self {
        Self {
            content_hashes: DashMap::new(),
        }
    }

    /// Stores content and returns deduplicated reference
    pub fn store_content(&self, data: Bytes) -> (ContentHash, Arc<Bytes>) {
        let hash = hash_content(&data);
        
        // Check if we already have this content
        if let Some(existing) = self.content_hashes.get(&hash) {
            return (hash, existing.clone());
        }
        
        // Store new content
        let arc_data = Arc::new(data);
        self.content_hashes.insert(hash, arc_data.clone());
        (hash, arc_data)
    }

    /// Gets content by hash if it exists
    pub fn get_content(&self, hash: &ContentHash) -> Option<Arc<Bytes>> {
        self.content_hashes.get(hash).map(|entry| entry.clone())
    }

    /// Removes content by hash (called when last reference is dropped)
    pub fn remove_content(&self, hash: &ContentHash) -> bool {
        self.content_hashes.remove(hash).is_some()
    }

    /// Gets statistics about deduplicated content
    pub fn stats(&self) -> (usize, usize) {
        let unique_entries = self.content_hashes.len();
        let total_bytes = self.content_hashes
            .iter()
            .map(|entry| entry.value().len())
            .sum();
        (unique_entries, total_bytes)
    }
}

/// Read-through cache for hot entries
pub struct ReadThroughCache<T> {
    /// LRU cache for hot entries
    hot_entries: Arc<Mutex<LruCache<ShadowPath, Arc<T>>>>,
}

impl<T> ReadThroughCache<T> {
    /// Creates a new read-through cache with specified capacity
    pub fn new(capacity: usize) -> Self {
        let cache_size = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1000).unwrap());
        Self {
            hot_entries: Arc::new(Mutex::new(LruCache::new(cache_size))),
        }
    }

    /// Gets an entry from cache if it exists
    pub fn get(&self, path: &ShadowPath) -> Option<Arc<T>> {
        let mut cache = self.hot_entries.lock().unwrap();
        cache.get(path).cloned()
    }

    /// Puts an entry into the cache
    pub fn put(&self, path: ShadowPath, entry: Arc<T>) {
        let mut cache = self.hot_entries.lock().unwrap();
        cache.put(path, entry);
    }

    /// Removes an entry from cache
    pub fn remove(&self, path: &ShadowPath) -> Option<Arc<T>> {
        let mut cache = self.hot_entries.lock().unwrap();
        cache.pop(path)
    }

    /// Clears the entire cache
    pub fn clear(&self) {
        let mut cache = self.hot_entries.lock().unwrap();
        cache.clear();
    }

    /// Gets cache statistics
    pub fn stats(&self) -> (usize, usize) {
        let cache = self.hot_entries.lock().unwrap();
        (cache.len(), cache.cap().get())
    }
}

/// Strategy for prefetching directory contents
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PrefetchStrategy {
    /// No prefetching
    None,
    /// Prefetch immediate children when accessing a directory
    Children,
    /// Recursively prefetch entire subtree
    Recursive,
}

impl Default for PrefetchStrategy {
    fn default() -> Self {
        PrefetchStrategy::Children
    }
}

/// Directory prefetching system
pub struct DirectoryPrefetcher {
    /// Prefetch strategy to use
    strategy: PrefetchStrategy,
}

impl DirectoryPrefetcher {
    /// Creates a new directory prefetcher
    pub fn new(strategy: PrefetchStrategy) -> Self {
        Self { strategy }
    }

    /// Gets paths that should be prefetched when accessing a directory
    pub fn get_prefetch_paths(&self, directory_path: &ShadowPath, available_children: &[String]) -> Vec<ShadowPath> {
        match self.strategy {
            PrefetchStrategy::None => Vec::new(),
            PrefetchStrategy::Children => {
                // Prefetch immediate children
                available_children
                    .iter()
                    .map(|child| directory_path.join(child))
                    .collect()
            }
            PrefetchStrategy::Recursive => {
                // For recursive, we'd need to traverse deeper
                // For now, just return immediate children
                // In a full implementation, this would recursively find all descendants
                available_children
                    .iter()
                    .map(|child| directory_path.join(child))
                    .collect()
            }
        }
    }

    /// Updates the prefetch strategy
    pub fn set_strategy(&mut self, strategy: PrefetchStrategy) {
        self.strategy = strategy;
    }

    /// Gets the current prefetch strategy
    pub fn strategy(&self) -> PrefetchStrategy {
        self.strategy
    }
}

/// Sharded storage for better concurrency
pub struct ShardedMap<K, V> {
    /// Array of sharded maps
    shards: [DashMap<K, V>; 16],
}

impl<K, V> ShardedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    /// Creates a new sharded map
    pub fn new() -> Self {
        Self {
            shards: [
                DashMap::new(), DashMap::new(), DashMap::new(), DashMap::new(),
                DashMap::new(), DashMap::new(), DashMap::new(), DashMap::new(),
                DashMap::new(), DashMap::new(), DashMap::new(), DashMap::new(),
                DashMap::new(), DashMap::new(), DashMap::new(), DashMap::new(),
            ],
        }
    }

    /// Gets the shard index for a key
    fn shard_index(&self, key: &K) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % 16
    }

    /// Inserts a key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let shard_idx = self.shard_index(&key);
        self.shards[shard_idx].insert(key, value)
    }

    /// Gets a value by key
    pub fn get(&self, key: &K) -> Option<dashmap::mapref::one::Ref<'_, K, V>> {
        let shard_idx = self.shard_index(key);
        self.shards[shard_idx].get(key)
    }

    /// Removes a key-value pair
    pub fn remove(&self, key: &K) -> Option<(K, V)> {
        let shard_idx = self.shard_index(key);
        self.shards[shard_idx].remove(key)
    }

    /// Checks if a key exists
    pub fn contains_key(&self, key: &K) -> bool {
        let shard_idx = self.shard_index(key);
        self.shards[shard_idx].contains_key(key)
    }

    /// Gets the total number of entries across all shards
    pub fn len(&self) -> usize {
        self.shards.iter().map(|shard| shard.len()).sum()
    }

    /// Checks if the map is empty
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|shard| shard.is_empty())
    }

    /// Iterates over all key-value pairs
    pub fn iter(&self) -> impl Iterator<Item = dashmap::mapref::multiple::RefMulti<'_, K, V>> {
        self.shards.iter().flat_map(|shard| shard.iter())
    }
}

impl<K, V> Default for ShardedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Hashes content using BLAKE3
pub fn hash_content(data: &[u8]) -> ContentHash {
    blake3::hash(data).into()
}

/// Compression utilities for large entries
pub mod compression {
    use bytes::Bytes;
    use std::io::{Read, Write};

    /// Minimum size for compression (1MB)
    pub const COMPRESSION_THRESHOLD: usize = 1024 * 1024;

    /// Compresses data using zstd
    pub fn compress(data: &[u8]) -> Result<Bytes, std::io::Error> {
        let mut encoder = zstd::Encoder::new(Vec::new(), 3)?;
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;
        Ok(Bytes::from(compressed))
    }

    /// Decompresses data using zstd
    pub fn decompress(compressed_data: &[u8]) -> Result<Bytes, std::io::Error> {
        let mut decoder = zstd::Decoder::new(compressed_data)?;
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        Ok(Bytes::from(decompressed))
    }

    /// Checks if data should be compressed
    pub fn should_compress(data: &[u8]) -> bool {
        data.len() >= COMPRESSION_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_deduplication() {
        let dedup = ContentDeduplication::new();
        let data1 = Bytes::from("hello world");
        let data2 = Bytes::from("hello world");
        let data3 = Bytes::from("different");

        let (hash1, arc1) = dedup.store_content(data1);
        let (hash2, arc2) = dedup.store_content(data2);
        let (hash3, _arc3) = dedup.store_content(data3);

        // Same content should have same hash and same Arc
        assert_eq!(hash1, hash2);
        assert!(Arc::ptr_eq(&arc1, &arc2));
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_read_through_cache() {
        let cache: ReadThroughCache<String> = ReadThroughCache::new(2);
        let path1 = ShadowPath::new("/test1".into());
        let path2 = ShadowPath::new("/test2".into());
        let path3 = ShadowPath::new("/test3".into());

        let entry1 = Arc::new("entry1".to_string());
        let entry2 = Arc::new("entry2".to_string());
        let entry3 = Arc::new("entry3".to_string());

        cache.put(path1.clone(), entry1.clone());
        cache.put(path2.clone(), entry2.clone());

        assert!(cache.get(&path1).is_some());
        assert!(cache.get(&path2).is_some());

        // Adding third entry should evict first (LRU)
        cache.put(path3.clone(), entry3.clone());
        assert!(cache.get(&path1).is_none());
        assert!(cache.get(&path2).is_some());
        assert!(cache.get(&path3).is_some());
    }

    #[test]
    fn test_sharded_map() {
        let map: ShardedMap<String, i32> = ShardedMap::new();
        
        map.insert("key1".to_string(), 1);
        map.insert("key2".to_string(), 2);
        
        assert_eq!(map.get(&"key1".to_string()).unwrap().value(), &1);
        assert_eq!(map.get(&"key2".to_string()).unwrap().value(), &2);
        assert_eq!(map.len(), 2);
        
        map.remove(&"key1".to_string());
        assert!(map.get(&"key1".to_string()).is_none());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_compression() {
        use compression::*;
        
        let large_data = vec![b'x'; COMPRESSION_THRESHOLD + 1000];
        let small_data = vec![b'y'; 100];
        
        assert!(should_compress(&large_data));
        assert!(!should_compress(&small_data));
        
        let compressed = compress(&large_data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        
        assert_eq!(large_data, decompressed.as_ref());
        assert!(compressed.len() < large_data.len());
    }

    #[test]
    fn test_hash_content() {
        let data1 = b"hello world";
        let data2 = b"hello world";
        let data3 = b"different";
        
        let hash1 = hash_content(data1);
        let hash2 = hash_content(data2);
        let hash3 = hash_content(data3);
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}