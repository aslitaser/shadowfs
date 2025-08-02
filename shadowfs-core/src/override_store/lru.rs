//! LRU tracking and eviction policies.

use crate::types::ShadowPath;
use super::entry::OverrideEntry;
use super::size::calculate_entry_size;
use dashmap::DashMap;
use indexmap::IndexMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

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
        entries: &DashMap<ShadowPath, std::sync::Arc<OverrideEntry>>,
        target_bytes: usize,
    ) -> Vec<ShadowPath> {
        // Get candidates based on policy
        let candidates: Vec<ShadowPath> = match policy {
            EvictionPolicy::Lru => {
                // IndexMap preserves order - first entries are least recently used
                let order = self.access_order.lock().unwrap();
                order.keys().cloned().collect()
            }
            
            EvictionPolicy::Lfu => {
                // Sort by access count (ascending)
                let mut freq_list: Vec<_> = self.access_count
                    .iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let count = entry.value().load(Ordering::Relaxed);
                        (path, count)
                    })
                    .collect();
                
                freq_list.sort_by_key(|(_, count)| *count);
                freq_list.into_iter().map(|(path, _)| path).collect()
            }
            
            EvictionPolicy::Fifo => {
                // Sort by creation time (oldest first)
                let mut time_list: Vec<_> = entries.iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let created = entry.value().created_at;
                        (path, created)
                    })
                    .collect();
                
                time_list.sort_by_key(|(_, time)| *time);
                time_list.into_iter().map(|(path, _)| path).collect()
            }
            
            EvictionPolicy::SizeWeighted => {
                // Sort by size (largest first)
                let mut size_list: Vec<_> = entries.iter()
                    .map(|entry| {
                        let path = entry.key().clone();
                        let size = calculate_entry_size(entry.value());
                        (path, size)
                    })
                    .collect();
                
                size_list.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
                size_list.into_iter().map(|(path, _)| path).collect()
            }
        };
        
        // Select victims until we reach target bytes
        let mut victims = Vec::new();
        let mut freed_bytes = 0;
        
        for path in candidates {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileMetadata, FileType, FilePermissions, PlatformMetadata};
    use bytes::Bytes;
    use super::super::entry::OverrideContent;
    use std::time::SystemTime;
    
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
            
            entries.insert(path.clone(), std::sync::Arc::new(entry));
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
                
                entries.insert(path.clone(), std::sync::Arc::new(entry));
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