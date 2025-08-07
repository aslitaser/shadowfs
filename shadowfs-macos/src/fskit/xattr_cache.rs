use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::ffi::OsString;
use std::time::{Duration, Instant};
use std::sync::{Arc, RwLock, Mutex};

/// Cache entry for extended attributes
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached attribute value
    value: Option<Vec<u8>>,
    /// When this entry was last accessed
    last_access: Instant,
    /// When this entry was created
    created: Instant,
    /// Size in bytes (for memory tracking)
    size: usize,
    /// Access count for popularity tracking
    access_count: u64,
}

/// LRU cache key
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    path: PathBuf,
    attr_name: OsString,
}

/// Statistics for cache monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub invalidations: u64,
    pub current_size: usize,
    pub current_entries: usize,
    pub peak_size: usize,
}

/// Configuration for the xattr cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum memory usage in bytes
    pub max_memory: usize,
    /// Maximum number of entries
    pub max_entries: usize,
    /// TTL for cache entries
    pub ttl: Duration,
    /// Whether to cache negative results (non-existent attrs)
    pub cache_negatives: bool,
    /// Minimum access count to keep during eviction
    pub min_access_count: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024,  // 64MB
            max_entries: 10000,
            ttl: Duration::from_secs(300),  // 5 minutes
            cache_negatives: true,
            min_access_count: 2,
        }
    }
}

/// Extended attributes cache with LRU eviction
#[derive(Debug)]
pub struct XattrCache {
    /// The actual cache storage
    cache: Arc<RwLock<HashMap<CacheKey, CacheEntry>>>,
    /// LRU tracking queue
    lru_queue: Arc<Mutex<VecDeque<CacheKey>>>,
    /// Cache configuration
    config: CacheConfig,
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
    /// Current memory usage
    memory_usage: Arc<RwLock<usize>>,
}

impl XattrCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            lru_queue: Arc::new(Mutex::new(VecDeque::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            memory_usage: Arc::new(RwLock::new(0)),
        }
    }
    
    pub fn with_default_config() -> Self {
        Self::new(CacheConfig::default())
    }
    
    /// Get an attribute from cache
    pub fn get(&self, path: &Path, attr_name: &OsString) -> Option<Option<Vec<u8>>> {
        let key = CacheKey {
            path: path.to_path_buf(),
            attr_name: attr_name.clone(),
        };
        
        let mut cache = self.cache.write().ok()?;
        let mut stats = self.stats.write().ok()?;
        
        if let Some(entry) = cache.get_mut(&key) {
            // Check if entry is expired
            if entry.created.elapsed() > self.config.ttl {
                // Remove expired entry
                self.remove_entry(&key, &mut cache, &mut stats);
                stats.misses += 1;
                return None;
            }
            
            // Update access time and count
            entry.last_access = Instant::now();
            entry.access_count += 1;
            
            // Update LRU queue
            self.update_lru(&key);
            
            stats.hits += 1;
            Some(entry.value.clone())
        } else {
            stats.misses += 1;
            None
        }
    }
    
    /// Put an attribute into cache
    pub fn put(&self, path: &Path, attr_name: OsString, value: Option<Vec<u8>>) {
        if !self.config.cache_negatives && value.is_none() {
            return;
        }
        
        let key = CacheKey {
            path: path.to_path_buf(),
            attr_name,
        };
        
        let size = value.as_ref().map(|v| v.len()).unwrap_or(0) 
            + key.path.as_os_str().len() 
            + key.attr_name.len();
        
        let entry = CacheEntry {
            value,
            last_access: Instant::now(),
            created: Instant::now(),
            size,
            access_count: 1,
        };
        
        let mut cache = self.cache.write().unwrap();
        let mut stats = self.stats.write().unwrap();
        let mut memory = self.memory_usage.write().unwrap();
        
        // Check if we need to evict entries
        while (*memory + size > self.config.max_memory) || 
              (cache.len() >= self.config.max_entries) {
            if !self.evict_lru(&mut cache, &mut stats, &mut memory) {
                break;
            }
        }
        
        // Insert new entry
        if let Some(old_entry) = cache.insert(key.clone(), entry) {
            *memory = memory.saturating_sub(old_entry.size);
        }
        
        *memory += size;
        stats.current_size = *memory;
        stats.current_entries = cache.len();
        
        if *memory > stats.peak_size {
            stats.peak_size = *memory;
        }
        
        // Add to LRU queue
        let mut lru = self.lru_queue.lock().unwrap();
        lru.push_back(key);
    }
    
    /// Invalidate cache entries for a path
    pub fn invalidate(&self, path: &Path) {
        let mut cache = self.cache.write().unwrap();
        let mut stats = self.stats.write().unwrap();
        let mut memory = self.memory_usage.write().unwrap();
        
        let keys_to_remove: Vec<CacheKey> = cache
            .keys()
            .filter(|k| k.path == path)
            .cloned()
            .collect();
        
        for key in keys_to_remove {
            if let Some(entry) = cache.remove(&key) {
                *memory = memory.saturating_sub(entry.size);
                stats.invalidations += 1;
            }
        }
        
        stats.current_size = *memory;
        stats.current_entries = cache.len();
    }
    
    /// Invalidate a specific attribute
    pub fn invalidate_attr(&self, path: &Path, attr_name: &OsString) {
        let key = CacheKey {
            path: path.to_path_buf(),
            attr_name: attr_name.clone(),
        };
        
        let mut cache = self.cache.write().unwrap();
        let mut stats = self.stats.write().unwrap();
        
        self.remove_entry(&key, &mut cache, &mut stats);
    }
    
    /// Clear the entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut lru = self.lru_queue.lock().unwrap();
        let mut stats = self.stats.write().unwrap();
        let mut memory = self.memory_usage.write().unwrap();
        
        cache.clear();
        lru.clear();
        *memory = 0;
        
        stats.current_size = 0;
        stats.current_entries = 0;
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }
    
    /// Update LRU position for a key
    fn update_lru(&self, key: &CacheKey) {
        let mut lru = self.lru_queue.lock().unwrap();
        
        // Remove old position
        if let Some(pos) = lru.iter().position(|k| k == key) {
            lru.remove(pos);
        }
        
        // Add to back (most recently used)
        lru.push_back(key.clone());
    }
    
    /// Evict the least recently used entry
    fn evict_lru(
        &self, 
        cache: &mut HashMap<CacheKey, CacheEntry>,
        stats: &mut CacheStats,
        memory: &mut usize,
    ) -> bool {
        let mut lru = self.lru_queue.lock().unwrap();
        
        // Find an entry to evict (skip popular entries if configured)
        while let Some(key) = lru.pop_front() {
            if let Some(entry) = cache.get(&key) {
                if entry.access_count >= self.config.min_access_count {
                    // This entry is too popular, try next
                    lru.push_back(key);
                    continue;
                }
                
                // Evict this entry
                if let Some(entry) = cache.remove(&key) {
                    *memory = memory.saturating_sub(entry.size);
                    stats.evictions += 1;
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Remove a specific entry
    fn remove_entry(
        &self,
        key: &CacheKey,
        cache: &mut HashMap<CacheKey, CacheEntry>,
        stats: &mut CacheStats,
    ) {
        if let Some(entry) = cache.remove(key) {
            let mut memory = self.memory_usage.write().unwrap();
            *memory = memory.saturating_sub(entry.size);
            stats.current_size = *memory;
            stats.current_entries = cache.len();
            stats.invalidations += 1;
        }
    }
    
    /// Prune expired entries
    pub fn prune_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut stats = self.stats.write().unwrap();
        let mut memory = self.memory_usage.write().unwrap();
        
        let now = Instant::now();
        let expired_keys: Vec<CacheKey> = cache
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.created) > self.config.ttl)
            .map(|(key, _)| key.clone())
            .collect();
        
        for key in expired_keys {
            if let Some(entry) = cache.remove(&key) {
                *memory = memory.saturating_sub(entry.size);
                stats.invalidations += 1;
            }
        }
        
        stats.current_size = *memory;
        stats.current_entries = cache.len();
    }
    
    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let stats = self.stats.read().unwrap();
        let total = stats.hits + stats.misses;
        if total == 0 {
            0.0
        } else {
            stats.hits as f64 / total as f64
        }
    }
    
    /// Check if cache has room for an entry of given size
    pub fn has_room_for(&self, size: usize) -> bool {
        let memory = self.memory_usage.read().unwrap();
        *memory + size <= self.config.max_memory
    }
    
    /// Batch invalidate multiple paths
    pub fn invalidate_batch(&self, paths: &[PathBuf]) {
        let mut cache = self.cache.write().unwrap();
        let mut stats = self.stats.write().unwrap();
        let mut memory = self.memory_usage.write().unwrap();
        
        for path in paths {
            let keys_to_remove: Vec<CacheKey> = cache
                .keys()
                .filter(|k| &k.path == path)
                .cloned()
                .collect();
            
            for key in keys_to_remove {
                if let Some(entry) = cache.remove(&key) {
                    *memory = memory.saturating_sub(entry.size);
                    stats.invalidations += 1;
                }
            }
        }
        
        stats.current_size = *memory;
        stats.current_entries = cache.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic_operations() {
        let cache = XattrCache::with_default_config();
        let path = Path::new("/test/file");
        let attr = OsString::from("user.test");
        let value = vec![1, 2, 3, 4, 5];
        
        // Initially not in cache
        assert_eq!(cache.get(path, &attr), None);
        
        // Put into cache
        cache.put(path, attr.clone(), Some(value.clone()));
        
        // Should be in cache now
        assert_eq!(cache.get(path, &attr), Some(Some(value)));
        
        // Stats should reflect operations
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }
    
    #[test]
    fn test_cache_invalidation() {
        let cache = XattrCache::with_default_config();
        let path = Path::new("/test/file");
        let attr1 = OsString::from("user.test1");
        let attr2 = OsString::from("user.test2");
        
        cache.put(path, attr1.clone(), Some(vec![1, 2, 3]));
        cache.put(path, attr2.clone(), Some(vec![4, 5, 6]));
        
        // Both should be cached
        assert!(cache.get(path, &attr1).is_some());
        assert!(cache.get(path, &attr2).is_some());
        
        // Invalidate the path
        cache.invalidate(path);
        
        // Both should be gone
        assert_eq!(cache.get(path, &attr1), None);
        assert_eq!(cache.get(path, &attr2), None);
    }
    
    #[test]
    fn test_cache_memory_limit() {
        let mut config = CacheConfig::default();
        config.max_memory = 100; // Very small limit
        
        let cache = XattrCache::new(config);
        
        // Add entries until we exceed the limit
        for i in 0..10 {
            let path = PathBuf::from(format!("/test/file{}", i));
            let attr = OsString::from("attr");
            let value = vec![0u8; 20]; // Each entry is ~20 bytes
            
            cache.put(&path, attr, Some(value));
        }
        
        // Should have evicted some entries
        let stats = cache.stats();
        assert!(stats.evictions > 0);
        assert!(stats.current_size <= 100);
    }
    
    #[test]
    fn test_negative_caching() {
        let mut config = CacheConfig::default();
        config.cache_negatives = true;
        
        let cache = XattrCache::new(config);
        let path = Path::new("/test/file");
        let attr = OsString::from("nonexistent");
        
        // Cache a negative result
        cache.put(path, attr.clone(), None);
        
        // Should return cached negative
        assert_eq!(cache.get(path, &attr), Some(None));
    }
}