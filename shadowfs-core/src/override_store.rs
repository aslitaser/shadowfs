//! In-memory storage for file and directory overrides.

use crate::types::{FileMetadata, ShadowPath};
use crate::error::ShadowError;
use bytes::Bytes;
use dashmap::DashMap;
use indexmap::IndexMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, Instant};

/// Content stored in an override entry.
#[derive(Debug, Clone)]
pub enum OverrideContent {
    /// File content with hash for integrity checking
    File {
        data: Bytes,
        content_hash: [u8; 32],
    },
    /// Directory with list of entries
    Directory {
        entries: Vec<String>,
    },
    /// Tombstone marking a deleted file/directory
    Deleted,
}

/// An entry in the override store representing a file or directory override.
#[derive(Debug)]
pub struct OverrideEntry {
    /// Path of the overridden file/directory
    pub path: ShadowPath,
    
    /// The override content
    pub content: OverrideContent,
    
    /// Original metadata from the underlying filesystem (if it existed)
    pub original_metadata: Option<FileMetadata>,
    
    /// Metadata for the override
    pub override_metadata: FileMetadata,
    
    /// When this override was created
    pub created_at: SystemTime,
    
    /// Last access time as Unix timestamp (for LRU tracking)
    pub last_accessed: AtomicU64,
}

/// Tracks memory usage with atomic operations for thread-safe allocation.
#[derive(Debug)]
pub struct MemoryTracker {
    /// Current memory usage in bytes
    current_usage: AtomicUsize,
    
    /// Maximum allowed memory in bytes
    max_allowed: usize,
    
    /// Total number of allocations made
    allocation_count: AtomicU64,
}

impl MemoryTracker {
    /// Creates a new memory tracker with the specified limit.
    pub fn new(max_allowed: usize) -> Self {
        Self {
            current_usage: AtomicUsize::new(0),
            max_allowed,
            allocation_count: AtomicU64::new(0),
        }
    }
    
    /// Attempts to allocate memory, returning a guard if successful.
    ///
    /// # Arguments
    /// * `size` - Number of bytes to allocate
    ///
    /// # Returns
    /// * `Ok(MemoryGuard)` - Guard that will release memory when dropped
    /// * `Err(ShadowError)` - If allocation would exceed limits
    pub fn try_allocate(&self, size: usize) -> Result<MemoryGuard, ShadowError> {
        // Try to allocate atomically
        let mut current = self.current_usage.load(Ordering::Relaxed);
        
        loop {
            let new_usage = current.saturating_add(size);
            
            // Check if allocation would exceed limit
            if new_usage > self.max_allowed {
                let _available = self.max_allowed.saturating_sub(current);
                return Err(ShadowError::OverrideStoreFull {
                    current_size: current,
                    max_size: self.max_allowed,
                });
            }
            
            // Try to update atomically
            match self.current_usage.compare_exchange_weak(
                current,
                new_usage,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Successfully allocated
                    self.allocation_count.fetch_add(1, Ordering::Relaxed);
                    return Ok(MemoryGuard {
                        size,
                        tracker: self,
                    });
                }
                Err(actual) => {
                    // Another thread changed the value, retry
                    current = actual;
                }
            }
        }
    }
    
    /// Returns the current memory usage in bytes.
    pub fn current_usage(&self) -> usize {
        self.current_usage.load(Ordering::Relaxed)
    }
    
    /// Returns the available space in bytes.
    pub fn available_space(&self) -> usize {
        let current = self.current_usage.load(Ordering::Relaxed);
        self.max_allowed.saturating_sub(current)
    }
    
    /// Returns true if memory usage is above 90%.
    pub fn is_under_pressure(&self) -> bool {
        self.get_pressure_ratio() > 0.9
    }
    
    /// Returns the memory pressure ratio (0.0 to 1.0).
    pub fn get_pressure_ratio(&self) -> f64 {
        let current = self.current_usage.load(Ordering::Relaxed) as f64;
        let max = self.max_allowed as f64;
        if max > 0.0 {
            current / max
        } else {
            1.0
        }
    }
    
    /// Internal method to release memory (called by MemoryGuard).
    fn release(&self, size: usize) {
        self.current_usage.fetch_sub(size, Ordering::AcqRel);
    }
}

/// RAII guard for allocated memory that releases it when dropped.
#[derive(Debug)]
pub struct MemoryGuard<'a> {
    /// Size of the allocation
    size: usize,
    
    /// Reference to the tracker
    #[allow(dead_code)]
    tracker: &'a MemoryTracker,
}

impl<'a> MemoryGuard<'a> {
    /// Returns the size of this allocation.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl<'a> Drop for MemoryGuard<'a> {
    fn drop(&mut self) {
        self.tracker.release(self.size);
    }
}

// MemoryGuard is neither Clone nor Copy by design
// This ensures memory is only released once when the guard is dropped

/// Statistics about path access patterns.
#[derive(Debug, Clone)]
pub struct AccessStats {
    /// When the path was last accessed
    pub last_accessed: Instant,
    
    /// Total number of times accessed
    pub access_count: u64,
    
    /// Age in seconds since last access
    pub age_seconds: u64,
}

impl AccessStats {
    /// Creates new access stats for the current time.
    pub fn new() -> Self {
        Self {
            last_accessed: Instant::now(),
            access_count: 1,
            age_seconds: 0,
        }
    }
    
    /// Updates the age based on current time.
    pub fn update_age(&mut self) {
        self.age_seconds = self.last_accessed.elapsed().as_secs();
    }
}

/// Eviction policy for when memory limits are reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Evict least recently used entries
    Lru,
    
    /// Evict least frequently used entries
    Lfu,
    
    /// Evict oldest entries first (FIFO)
    Fifo,
    
    /// Evict largest entries first
    SizeWeighted,
}

/// Tracks access patterns for LRU eviction.
pub struct LruTracker {
    /// Ordered map of paths to last access time
    access_order: Mutex<IndexMap<ShadowPath, Instant>>,
    
    /// Access count for each path
    access_count: DashMap<ShadowPath, AtomicU64>,
    
    /// Generation counter for versioning
    generation: AtomicU64,
}

impl LruTracker {
    /// Creates a new LRU tracker.
    pub fn new() -> Self {
        Self {
            access_order: Mutex::new(IndexMap::new()),
            access_count: DashMap::new(),
            generation: AtomicU64::new(0),
        }
    }
    
    /// Records an access to a path.
    pub fn record_access(&self, path: &ShadowPath) {
        let now = Instant::now();
        
        // Update access count
        self.access_count
            .entry(path.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
        
        // Update access order
        let mut order = self.access_order.lock().unwrap();
        
        // Remove and re-insert to move to end (most recent)
        // shift_remove preserves the order of remaining elements
        order.shift_remove(path);
        order.insert(path.clone(), now);
        
        // Increment generation
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Gets the least recently used paths.
    ///
    /// # Arguments
    /// * `count` - Maximum number of paths to return
    ///
    /// # Returns
    /// Vector of paths ordered from least to most recently used
    pub fn get_least_recently_used(&self, count: usize) -> Vec<ShadowPath> {
        let order = self.access_order.lock().unwrap();
        
        order.keys()
            .take(count)
            .cloned()
            .collect()
    }
    
    /// Removes tracking data for a path.
    pub fn remove_entry(&self, path: &ShadowPath) {
        self.access_count.remove(path);
        
        let mut order = self.access_order.lock().unwrap();
        order.shift_remove(path);
    }
    
    /// Gets access statistics for a path.
    pub fn get_access_stats(&self, path: &ShadowPath) -> Option<AccessStats> {
        let order = self.access_order.lock().unwrap();
        
        if let Some(&last_accessed) = order.get(path) {
            let access_count = self.access_count
                .get(path)
                .map(|entry| entry.load(Ordering::Relaxed))
                .unwrap_or(0);
            
            let mut stats = AccessStats {
                last_accessed,
                access_count,
                age_seconds: 0,
            };
            stats.update_age();
            
            Some(stats)
        } else {
            None
        }
    }
    
    /// Gets all tracked paths with their access stats.
    pub fn get_all_stats(&self) -> Vec<(ShadowPath, AccessStats)> {
        let order = self.access_order.lock().unwrap();
        
        order.iter()
            .map(|(path, &last_accessed)| {
                let access_count = self.access_count
                    .get(path)
                    .map(|entry| entry.load(Ordering::Relaxed))
                    .unwrap_or(0);
                
                let mut stats = AccessStats {
                    last_accessed,
                    access_count,
                    age_seconds: 0,
                };
                stats.update_age();
                
                (path.clone(), stats)
            })
            .collect()
    }
    
    /// Selects paths for eviction based on the given policy.
    ///
    /// # Arguments
    /// * `policy` - Eviction policy to use
    /// * `entries` - Map of paths to their entries (for size information)
    /// * `target_bytes` - Target number of bytes to free
    ///
    /// # Returns
    /// Vector of paths to evict
    pub fn select_victims(
        &self,
        policy: EvictionPolicy,
        entries: &DashMap<ShadowPath, OverrideEntry>,
        target_bytes: usize,
    ) -> Vec<ShadowPath> {
        let mut candidates: Vec<(ShadowPath, f64)> = match policy {
            EvictionPolicy::Lru => {
                // Get all paths ordered by access time
                // IndexMap preserves insertion order, so first entries are oldest
                let order = self.access_order.lock().unwrap();
                order.iter()
                    .map(|(path, instant)| {
                        let age = instant.elapsed().as_secs_f64();
                        (path.clone(), age)
                    })
                    .collect()
            }
            
            EvictionPolicy::Lfu => {
                // Get all paths ordered by access frequency
                self.access_count
                    .iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let count = entry.value().load(Ordering::Relaxed) as f64;
                        // Lower count = higher priority for eviction
                        (path, -count)
                    })
                    .collect()
            }
            
            EvictionPolicy::Fifo => {
                // Get all paths ordered by creation time
                entries.iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let age = entry.value().created_at.elapsed()
                            .unwrap_or_default().as_secs_f64();
                        (path, age)
                    })
                    .collect()
            }
            
            EvictionPolicy::SizeWeighted => {
                // Get all paths ordered by size (largest first)
                entries.iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let size = calculate_entry_size(entry.value()) as f64;
                        // Larger size = higher priority for eviction
                        (path, -size)
                    })
                    .collect()
            }
        };
        
        // Sort by score 
        // For LRU/FIFO: higher age = higher priority for eviction (sort descending)
        // For LFU: negative count used, so lower score = higher priority (sort ascending)
        // For SizeWeighted: negative size used, so lower score = higher priority (sort ascending)
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Select victims until we reach target bytes
        let mut victims = Vec::new();
        let mut freed_bytes = 0;
        
        for (path, _score) in candidates {
            if freed_bytes >= target_bytes {
                break;
            }
            
            if let Some(entry) = entries.get(&path) {
                freed_bytes += calculate_entry_size(entry.value());
                victims.push(path);
            }
        }
        
        victims
    }
}

impl Default for LruTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Size calculation helpers

/// Calculates the memory size of a Bytes object including overhead.
pub fn calculate_bytes_size(data: &Bytes) -> usize {
    // Bytes has a small overhead for reference counting and metadata
    const BYTES_OVERHEAD: usize = 32; // Arc pointer + length + capacity
    data.len() + BYTES_OVERHEAD
}

/// Calculates the total memory size of an OverrideEntry.
pub fn calculate_entry_size(entry: &OverrideEntry) -> usize {
    // Base struct size
    let mut size = std::mem::size_of::<OverrideEntry>();
    
    // Add path string size
    size += entry.path.to_string().len();
    
    // Add content size
    size += match &entry.content {
        OverrideContent::File { data, content_hash } => {
            calculate_bytes_size(data) + std::mem::size_of_val(content_hash)
        }
        OverrideContent::Directory { entries } => {
            // Vector overhead
            let vec_overhead = std::mem::size_of::<Vec<String>>();
            // String overhead per entry (24 bytes on 64-bit)
            let string_overhead = std::mem::size_of::<String>() * entries.len();
            // Actual string data
            let string_data: usize = entries.iter().map(|s| s.len()).sum();
            vec_overhead + string_overhead + string_data
        }
        OverrideContent::Deleted => 0,
    };
    
    // Add metadata sizes (rough estimates)
    if entry.original_metadata.is_some() {
        size += std::mem::size_of::<FileMetadata>();
    }
    size += std::mem::size_of::<FileMetadata>(); // override_metadata
    
    // Add some overhead for DashMap entry
    const DASHMAP_ENTRY_OVERHEAD: usize = 64;
    size + DASHMAP_ENTRY_OVERHEAD
}

/// Store for managing file and directory overrides with memory limits.
pub struct OverrideStore {
    /// Map of path to override entries
    pub entries: DashMap<ShadowPath, OverrideEntry>,
    
    /// Current memory usage in bytes
    pub memory_usage: AtomicUsize,
    
    /// Maximum allowed memory usage
    pub max_memory: usize,
    
    /// LRU tracker for eviction
    pub lru_tracker: Mutex<VecDeque<ShadowPath>>,
}

impl OverrideStore {
    /// Creates a new OverrideStore with the specified memory limit.
    ///
    /// # Arguments
    /// * `max_memory` - Maximum memory usage in bytes
    pub fn new(_max_memory: usize) -> Self {
        // TODO: Implement
        unimplemented!("OverrideStore::new")
    }
    
    /// Inserts or updates an override entry.
    ///
    /// # Arguments
    /// * `path` - Path to override
    /// * `content` - Override content
    /// * `metadata` - Metadata for the override
    ///
    /// # Returns
    /// Ok(()) on success, or an error if memory limits would be exceeded
    pub fn insert(
        &self,
        _path: ShadowPath,
        _content: OverrideContent,
        _metadata: FileMetadata,
    ) -> Result<(), crate::error::ShadowError> {
        // TODO: Implement
        unimplemented!("OverrideStore::insert")
    }
    
    /// Gets an override entry if it exists.
    ///
    /// # Arguments
    /// * `path` - Path to look up
    ///
    /// # Returns
    /// Arc to the override entry if found
    pub fn get(&self, _path: &ShadowPath) -> Option<Arc<OverrideEntry>> {
        // TODO: Implement
        unimplemented!("OverrideStore::get")
    }
    
    /// Removes an override entry.
    ///
    /// # Arguments
    /// * `path` - Path to remove
    ///
    /// # Returns
    /// The removed entry if it existed
    pub fn remove(&self, _path: &ShadowPath) -> Option<OverrideEntry> {
        // TODO: Implement
        unimplemented!("OverrideStore::remove")
    }
    
    /// Evicts the least recently used entry.
    ///
    /// # Returns
    /// The path that was evicted, if any
    pub fn evict_lru(&self) -> Option<ShadowPath> {
        // TODO: Implement
        unimplemented!("OverrideStore::evict_lru")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_tracker_allocation() {
        let tracker = MemoryTracker::new(1024);
        
        // Initial state
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.available_space(), 1024);
        assert!(!tracker.is_under_pressure());
        assert_eq!(tracker.get_pressure_ratio(), 0.0);
        
        // Allocate some memory
        let guard1 = tracker.try_allocate(256).unwrap();
        assert_eq!(tracker.current_usage(), 256);
        assert_eq!(tracker.available_space(), 768);
        assert_eq!(guard1.size(), 256);
        
        // Allocate more
        let guard2 = tracker.try_allocate(512).unwrap();
        assert_eq!(tracker.current_usage(), 768);
        assert_eq!(tracker.available_space(), 256);
        
        // Drop first guard
        drop(guard1);
        assert_eq!(tracker.current_usage(), 512);
        assert_eq!(tracker.available_space(), 512);
        
        // Drop second guard
        drop(guard2);
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.available_space(), 1024);
    }
    
    #[test]
    fn test_memory_tracker_allocation_failure() {
        let tracker = MemoryTracker::new(1024);
        
        // Allocate most of the memory
        let _guard = tracker.try_allocate(1000).unwrap();
        
        // Try to allocate more than available
        let result = tracker.try_allocate(100);
        assert!(result.is_err());
        
        match result.err().unwrap() {
            ShadowError::OverrideStoreFull { current_size, max_size } => {
                assert_eq!(current_size, 1000);
                assert_eq!(max_size, 1024);
            }
            _ => panic!("Expected OverrideStoreFull error"),
        }
    }
    
    #[test]
    fn test_memory_pressure_detection() {
        let tracker = MemoryTracker::new(1000);
        
        // No pressure initially
        assert!(!tracker.is_under_pressure());
        assert!(tracker.get_pressure_ratio() < 0.1);
        
        // Allocate 80% - still no pressure
        let _guard1 = tracker.try_allocate(800).unwrap();
        assert!(!tracker.is_under_pressure());
        assert!((tracker.get_pressure_ratio() - 0.8).abs() < 0.01);
        
        // Allocate to 95% - now under pressure
        let _guard2 = tracker.try_allocate(150).unwrap();
        assert!(tracker.is_under_pressure());
        assert!((tracker.get_pressure_ratio() - 0.95).abs() < 0.01);
    }
    
    #[test]
    fn test_concurrent_allocation() {
        use std::sync::Arc;
        use std::thread;
        
        let tracker = Arc::new(MemoryTracker::new(10000));
        let mut handles = vec![];
        
        // Spawn multiple threads that allocate memory
        for i in 0..10 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                let mut total_allocated = 0;
                for j in 0..10 {
                    if let Ok(_guard) = tracker_clone.try_allocate(10 + i * 10 + j) {
                        total_allocated += 10 + i * 10 + j;
                        // Guard is dropped here, freeing memory immediately
                    }
                }
                total_allocated
            });
            handles.push(handle);
        }
        
        // Wait for all threads and sum allocations
        let total_attempted: usize = handles.into_iter()
            .map(|h| h.join().unwrap())
            .sum();
        
        // All memory should be freed by now
        assert_eq!(tracker.current_usage(), 0);
        
        // Check that we attempted reasonable allocations
        assert!(total_attempted > 0);
        assert!(total_attempted <= 10000);
    }
    
    #[test]
    fn test_calculate_bytes_size() {
        let data = Bytes::from(vec![0u8; 100]);
        let size = calculate_bytes_size(&data);
        assert_eq!(size, 100 + 32); // 100 bytes data + 32 overhead
        
        let empty = Bytes::new();
        let size = calculate_bytes_size(&empty);
        assert_eq!(size, 32); // Just overhead
    }
    
    #[test]
    fn test_calculate_entry_size() {
        use crate::types::{FileType, FilePermissions, PlatformMetadata};
        
        // Create a test entry with file content
        let entry = OverrideEntry {
            path: ShadowPath::new("/test/file.txt".into()),
            content: OverrideContent::File {
                data: Bytes::from(vec![0u8; 1000]),
                content_hash: [0u8; 32],
            },
            original_metadata: None,
            override_metadata: FileMetadata {
                size: 1000,
                created: SystemTime::now(),
                modified: SystemTime::now(),
                accessed: SystemTime::now(),
                permissions: FilePermissions::default_file(),
                file_type: FileType::File,
                platform_specific: PlatformMetadata::Linux { inode: 0, nlink: 1 },
            },
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(0),
        };
        
        let size = calculate_entry_size(&entry);
        
        // Should include struct size, path string, file data, metadata, etc.
        assert!(size > 1000); // At least the file data size
        assert!(size < 2000); // But not too much overhead
    }
    
    #[test]
    fn test_calculate_directory_entry_size() {
        use crate::types::{FileType, FilePermissions, PlatformMetadata};
        
        // Create a directory entry
        let entry = OverrideEntry {
            path: ShadowPath::new("/test/dir".into()),
            content: OverrideContent::Directory {
                entries: vec![
                    "file1.txt".to_string(),
                    "file2.txt".to_string(),
                    "subdir".to_string(),
                ],
            },
            original_metadata: None,
            override_metadata: FileMetadata {
                size: 0,
                created: SystemTime::now(),
                modified: SystemTime::now(),
                accessed: SystemTime::now(),
                permissions: FilePermissions::default_directory(),
                file_type: FileType::Directory,
                platform_specific: PlatformMetadata::Linux { inode: 0, nlink: 3 },
            },
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(0),
        };
        
        let size = calculate_entry_size(&entry);
        
        // Should include struct size, path string, entry strings, metadata
        assert!(size > 100); // Some reasonable minimum
        assert!(size < 1000); // Not too large for a simple directory
    }
    
    #[test]
    fn test_lru_tracker_basic_operations() {
        let tracker = LruTracker::new();
        
        let path1 = ShadowPath::new("/file1".into());
        let path2 = ShadowPath::new("/file2".into());
        let path3 = ShadowPath::new("/file3".into());
        
        // Record accesses
        tracker.record_access(&path1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.record_access(&path2);
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.record_access(&path3);
        
        // Check LRU order
        let lru = tracker.get_least_recently_used(3);
        assert_eq!(lru.len(), 3);
        assert_eq!(lru[0], path1);
        assert_eq!(lru[1], path2);
        assert_eq!(lru[2], path3);
        
        // Access path1 again to move it to end
        tracker.record_access(&path1);
        
        let lru = tracker.get_least_recently_used(3);
        assert_eq!(lru[0], path2);
        assert_eq!(lru[1], path3);
        assert_eq!(lru[2], path1);
    }
    
    #[test]
    fn test_lru_tracker_access_stats() {
        let tracker = LruTracker::new();
        let path = ShadowPath::new("/test".into());
        
        // No stats initially
        assert!(tracker.get_access_stats(&path).is_none());
        
        // Record first access
        tracker.record_access(&path);
        
        let stats = tracker.get_access_stats(&path).unwrap();
        assert_eq!(stats.access_count, 1);
        assert!(stats.age_seconds < 1);
        
        // Record more accesses
        tracker.record_access(&path);
        tracker.record_access(&path);
        
        let stats = tracker.get_access_stats(&path).unwrap();
        assert_eq!(stats.access_count, 3);
    }
    
    #[test]
    fn test_lru_tracker_remove_entry() {
        let tracker = LruTracker::new();
        let path = ShadowPath::new("/test".into());
        
        tracker.record_access(&path);
        assert!(tracker.get_access_stats(&path).is_some());
        
        tracker.remove_entry(&path);
        assert!(tracker.get_access_stats(&path).is_none());
        
        let lru = tracker.get_least_recently_used(10);
        assert!(!lru.contains(&path));
    }
    
    #[test]
    fn test_eviction_policy_lru() {
        use crate::types::{FileType, FilePermissions, PlatformMetadata};
        
        let tracker = LruTracker::new();
        let entries = DashMap::new();
        
        // Create test entries
        let paths: Vec<_> = (0..5)
            .map(|i| ShadowPath::new(format!("/file{}", i).into()))
            .collect();
        
        // Add entries and record accesses with delays
        for (i, path) in paths.iter().enumerate() {
            let entry = OverrideEntry {
                path: path.clone(),
                content: OverrideContent::File {
                    data: Bytes::from(vec![0u8; 100]),
                    content_hash: [0u8; 32],
                },
                original_metadata: None,
                override_metadata: FileMetadata {
                    size: 100,
                    created: SystemTime::now(),
                    modified: SystemTime::now(),
                    accessed: SystemTime::now(),
                    permissions: FilePermissions::default_file(),
                    file_type: FileType::File,
                    platform_specific: PlatformMetadata::Linux { inode: i as u64, nlink: 1 },
                },
                created_at: SystemTime::now(),
                last_accessed: AtomicU64::new(0),
            };
            
            entries.insert(path.clone(), entry);
            tracker.record_access(path);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        
        // Select victims using LRU policy
        let victims = tracker.select_victims(EvictionPolicy::Lru, &entries, 300);
        
        // Should evict oldest accessed first
        assert!(!victims.is_empty());
        assert_eq!(victims[0], paths[0]); // Oldest
        if victims.len() > 1 {
            assert_eq!(victims[1], paths[1]); // Second oldest
        }
    }
    
    #[test]
    fn test_eviction_policy_lfu() {
        use crate::types::{FileType, FilePermissions, PlatformMetadata};
        
        let tracker = LruTracker::new();
        let entries = DashMap::new();
        
        let path1 = ShadowPath::new("/file1".into());
        let path2 = ShadowPath::new("/file2".into());
        let path3 = ShadowPath::new("/file3".into());
        
        // Create entries
        for (i, path) in [&path1, &path2, &path3].iter().enumerate() {
            let entry = OverrideEntry {
                path: (*path).clone(),
                content: OverrideContent::File {
                    data: Bytes::from(vec![0u8; 100]),
                    content_hash: [0u8; 32],
                },
                original_metadata: None,
                override_metadata: FileMetadata {
                    size: 100,
                    created: SystemTime::now(),
                    modified: SystemTime::now(),
                    accessed: SystemTime::now(),
                    permissions: FilePermissions::default_file(),
                    file_type: FileType::File,
                    platform_specific: PlatformMetadata::Linux { inode: i as u64, nlink: 1 },
                },
                created_at: SystemTime::now(),
                last_accessed: AtomicU64::new(0),
            };
            entries.insert((*path).clone(), entry);
        }
        
        // Access with different frequencies
        tracker.record_access(&path1); // 1 time
        
        tracker.record_access(&path2); // 3 times
        tracker.record_access(&path2);
        tracker.record_access(&path2);
        
        tracker.record_access(&path3); // 2 times
        tracker.record_access(&path3);
        
        // Select victims using LFU policy
        let victims = tracker.select_victims(EvictionPolicy::Lfu, &entries, 200);
        
        // Should evict least frequently accessed first
        assert!(!victims.is_empty());
        assert_eq!(victims[0], path1); // Least frequent (1 access)
        if victims.len() > 1 {
            assert_eq!(victims[1], path3); // Second least frequent (2 accesses)
        }
    }
    
    #[test]
    fn test_eviction_policy_size_weighted() {
        use crate::types::{FileType, FilePermissions, PlatformMetadata};
        
        let tracker = LruTracker::new();
        let entries = DashMap::new();
        
        // Create entries with different sizes
        let sizes = [1000, 500, 2000, 300];
        let paths: Vec<_> = sizes.iter().enumerate()
            .map(|(i, &size)| {
                let path = ShadowPath::new(format!("/file{}", i).into());
                let entry = OverrideEntry {
                    path: path.clone(),
                    content: OverrideContent::File {
                        data: Bytes::from(vec![0u8; size]),
                        content_hash: [0u8; 32],
                    },
                    original_metadata: None,
                    override_metadata: FileMetadata {
                        size: size as u64,
                        created: SystemTime::now(),
                        modified: SystemTime::now(),
                        accessed: SystemTime::now(),
                        permissions: FilePermissions::default_file(),
                        file_type: FileType::File,
                        platform_specific: PlatformMetadata::Linux { inode: i as u64, nlink: 1 },
                    },
                    created_at: SystemTime::now(),
                    last_accessed: AtomicU64::new(0),
                };
                
                entries.insert(path.clone(), entry);
                tracker.record_access(&path);
                
                path
            })
            .collect();
        
        // Select victims using size-weighted policy
        let victims = tracker.select_victims(EvictionPolicy::SizeWeighted, &entries, 2500);
        
        // Should evict largest entries first
        assert!(!victims.is_empty());
        assert_eq!(victims[0], paths[2]); // Largest (2000 bytes)
        if victims.len() > 1 {
            assert_eq!(victims[1], paths[0]); // Second largest (1000 bytes)
        }
    }
    
    #[test]
    fn test_access_stats_age() {
        let mut stats = AccessStats::new();
        
        // Initial age should be 0
        stats.update_age();
        assert_eq!(stats.age_seconds, 0);
        
        // Sleep a bit and check age
        std::thread::sleep(std::time::Duration::from_millis(100));
        stats.update_age();
        assert!(stats.age_seconds < 1); // Less than 1 second
        
        // Access count should be 1
        assert_eq!(stats.access_count, 1);
    }
    
    #[test]
    fn test_concurrent_lru_access() {
        use std::sync::Arc;
        use std::thread;
        
        let tracker = Arc::new(LruTracker::new());
        let mut handles = vec![];
        
        // Spawn threads that access different paths
        for i in 0..5 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                let path = ShadowPath::new(format!("/file{}", i).into());
                for _ in 0..10 {
                    tracker_clone.record_access(&path);
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Check that all paths were recorded
        let all_stats = tracker.get_all_stats();
        assert_eq!(all_stats.len(), 5);
        
        // Each path should have 10 accesses
        for (_path, stats) in all_stats {
            assert_eq!(stats.access_count, 10);
        }
    }
}