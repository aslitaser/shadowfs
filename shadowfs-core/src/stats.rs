//! Performance tracking and statistics for ShadowFS operations.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::collections::HashMap;
use std::sync::RwLock;

/// Types of operations that can be tracked for statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Open,
    Read,
    Write,
    Close,
    Stat,
    ReadDir,
    Create,
    Delete,
    Rename,
}

/// Performance statistics for filesystem operations.
pub struct FileSystemStats {
    /// Number of active mounts
    pub mount_count: AtomicU64,
    
    /// Count of operations by type
    operation_counts: RwLock<HashMap<OperationType, AtomicU64>>,
    
    /// Total bytes read across all operations
    pub bytes_read: AtomicU64,
    
    /// Total bytes written across all operations
    pub bytes_written: AtomicU64,
    
    /// Number of cache hits
    pub cache_hits: AtomicU64,
    
    /// Number of cache misses
    pub cache_misses: AtomicU64,
    
    /// Current memory usage by overrides in bytes
    pub override_memory_usage: AtomicUsize,
    
    /// Number of currently active file handles
    pub active_handles: AtomicU64,
}

impl FileSystemStats {
    /// Creates a new FileSystemStats instance with all counters at zero.
    pub fn new() -> Self {
        let mut operation_counts = HashMap::new();
        
        // Initialize all operation types with zero counts
        for op_type in [
            OperationType::Open,
            OperationType::Read,
            OperationType::Write,
            OperationType::Close,
            OperationType::Stat,
            OperationType::ReadDir,
            OperationType::Create,
            OperationType::Delete,
            OperationType::Rename,
        ] {
            operation_counts.insert(op_type, AtomicU64::new(0));
        }
        
        Self {
            mount_count: AtomicU64::new(0),
            operation_counts: RwLock::new(operation_counts),
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            override_memory_usage: AtomicUsize::new(0),
            active_handles: AtomicU64::new(0),
        }
    }
    
    /// Increments the count for a specific operation type.
    pub fn increment_operation(&self, op_type: OperationType) {
        let counts = self.operation_counts.read().unwrap();
        if let Some(counter) = counts.get(&op_type) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Gets the count for a specific operation type.
    pub fn get_operation_count(&self, op_type: OperationType) -> u64 {
        let counts = self.operation_counts.read().unwrap();
        counts.get(&op_type)
            .map(|counter| counter.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
    
    /// Returns a snapshot of all operation counts.
    pub fn get_all_operation_counts(&self) -> HashMap<OperationType, u64> {
        let counts = self.operation_counts.read().unwrap();
        counts.iter()
            .map(|(op_type, counter)| (*op_type, counter.load(Ordering::Relaxed)))
            .collect()
    }
    
    /// Increments the mount count.
    pub fn increment_mounts(&self) {
        self.mount_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Decrements the mount count.
    pub fn decrement_mounts(&self) {
        self.mount_count.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// Adds to the bytes read counter.
    pub fn add_bytes_read(&self, bytes: u64) {
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// Adds to the bytes written counter.
    pub fn add_bytes_written(&self, bytes: u64) {
        self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// Increments the cache hit counter.
    pub fn increment_cache_hits(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Increments the cache miss counter.
    pub fn increment_cache_misses(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Updates the override memory usage.
    pub fn set_override_memory_usage(&self, bytes: usize) {
        self.override_memory_usage.store(bytes, Ordering::Relaxed);
    }
    
    /// Adds to the override memory usage.
    pub fn add_override_memory_usage(&self, bytes: usize) {
        self.override_memory_usage.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// Subtracts from the override memory usage.
    pub fn sub_override_memory_usage(&self, bytes: usize) {
        self.override_memory_usage.fetch_sub(bytes, Ordering::Relaxed);
    }
    
    /// Increments the active handles counter.
    pub fn increment_active_handles(&self) {
        self.active_handles.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Decrements the active handles counter.
    pub fn decrement_active_handles(&self) {
        self.active_handles.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// Returns the cache hit rate as a percentage (0.0 to 100.0).
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }
    
    /// Resets all statistics to zero.
    pub fn reset(&self) {
        self.mount_count.store(0, Ordering::Relaxed);
        self.bytes_read.store(0, Ordering::Relaxed);
        self.bytes_written.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.override_memory_usage.store(0, Ordering::Relaxed);
        self.active_handles.store(0, Ordering::Relaxed);
        
        let counts = self.operation_counts.read().unwrap();
        for counter in counts.values() {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for FileSystemStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_filesystem_stats_new() {
        let stats = FileSystemStats::new();
        
        assert_eq!(stats.mount_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_read.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_written.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_misses.load(Ordering::Relaxed), 0);
        assert_eq!(stats.override_memory_usage.load(Ordering::Relaxed), 0);
        assert_eq!(stats.active_handles.load(Ordering::Relaxed), 0);
        
        // Check all operation counts are initialized to 0
        for op_type in [
            OperationType::Open,
            OperationType::Read,
            OperationType::Write,
            OperationType::Stat,
        ] {
            assert_eq!(stats.get_operation_count(op_type), 0);
        }
    }
    
    #[test]
    fn test_operation_counting() {
        let stats = FileSystemStats::new();
        
        stats.increment_operation(OperationType::Read);
        stats.increment_operation(OperationType::Read);
        stats.increment_operation(OperationType::Write);
        
        assert_eq!(stats.get_operation_count(OperationType::Read), 2);
        assert_eq!(stats.get_operation_count(OperationType::Write), 1);
        assert_eq!(stats.get_operation_count(OperationType::Open), 0);
    }
    
    #[test]
    fn test_mount_counting() {
        let stats = FileSystemStats::new();
        
        stats.increment_mounts();
        stats.increment_mounts();
        assert_eq!(stats.mount_count.load(Ordering::Relaxed), 2);
        
        stats.decrement_mounts();
        assert_eq!(stats.mount_count.load(Ordering::Relaxed), 1);
    }
    
    #[test]
    fn test_byte_counting() {
        let stats = FileSystemStats::new();
        
        stats.add_bytes_read(1024);
        stats.add_bytes_read(2048);
        assert_eq!(stats.bytes_read.load(Ordering::Relaxed), 3072);
        
        stats.add_bytes_written(512);
        stats.add_bytes_written(256);
        assert_eq!(stats.bytes_written.load(Ordering::Relaxed), 768);
    }
    
    #[test]
    fn test_cache_statistics() {
        let stats = FileSystemStats::new();
        
        stats.increment_cache_hits();
        stats.increment_cache_hits();
        stats.increment_cache_hits();
        stats.increment_cache_misses();
        
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 3);
        assert_eq!(stats.cache_misses.load(Ordering::Relaxed), 1);
        assert_eq!(stats.cache_hit_rate(), 75.0);
    }
    
    #[test]
    fn test_cache_hit_rate_edge_cases() {
        let stats = FileSystemStats::new();
        
        // No hits or misses
        assert_eq!(stats.cache_hit_rate(), 0.0);
        
        // Only hits
        stats.increment_cache_hits();
        stats.increment_cache_hits();
        assert_eq!(stats.cache_hit_rate(), 100.0);
        
        // Only misses
        let stats2 = FileSystemStats::new();
        stats2.increment_cache_misses();
        stats2.increment_cache_misses();
        assert_eq!(stats2.cache_hit_rate(), 0.0);
    }
    
    #[test]
    fn test_override_memory_usage() {
        let stats = FileSystemStats::new();
        
        stats.set_override_memory_usage(1024);
        assert_eq!(stats.override_memory_usage.load(Ordering::Relaxed), 1024);
        
        stats.add_override_memory_usage(512);
        assert_eq!(stats.override_memory_usage.load(Ordering::Relaxed), 1536);
        
        stats.sub_override_memory_usage(256);
        assert_eq!(stats.override_memory_usage.load(Ordering::Relaxed), 1280);
    }
    
    #[test]
    fn test_active_handles() {
        let stats = FileSystemStats::new();
        
        stats.increment_active_handles();
        stats.increment_active_handles();
        assert_eq!(stats.active_handles.load(Ordering::Relaxed), 2);
        
        stats.decrement_active_handles();
        assert_eq!(stats.active_handles.load(Ordering::Relaxed), 1);
    }
    
    #[test]
    fn test_reset() {
        let stats = FileSystemStats::new();
        
        // Set various counters
        stats.increment_mounts();
        stats.add_bytes_read(1024);
        stats.add_bytes_written(512);
        stats.increment_cache_hits();
        stats.increment_cache_misses();
        stats.set_override_memory_usage(2048);
        stats.increment_active_handles();
        stats.increment_operation(OperationType::Read);
        
        // Reset all stats
        stats.reset();
        
        // Verify all counters are zero
        assert_eq!(stats.mount_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_read.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_written.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_misses.load(Ordering::Relaxed), 0);
        assert_eq!(stats.override_memory_usage.load(Ordering::Relaxed), 0);
        assert_eq!(stats.active_handles.load(Ordering::Relaxed), 0);
        assert_eq!(stats.get_operation_count(OperationType::Read), 0);
    }
}