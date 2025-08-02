//! In-memory storage for file and directory overrides.

use crate::types::{FileMetadata, ShadowPath};
use crate::error::ShadowError;
use bytes::Bytes;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

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
}