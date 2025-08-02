//! Statistics and monitoring for the override store.

use crate::types::ShadowPath;
use crate::override_store::{OverrideEntry, OverrideContent};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, Duration};
use std::collections::HashMap;

/// Atomic floating point type for cache hit rates
#[derive(Debug)]
pub struct AtomicF64 {
    value: AtomicU64,
}

impl AtomicF64 {
    pub fn new(value: f64) -> Self {
        Self {
            value: AtomicU64::new(value.to_bits()),
        }
    }

    pub fn load(&self, ordering: Ordering) -> f64 {
        f64::from_bits(self.value.load(ordering))
    }

    pub fn store(&self, value: f64, ordering: Ordering) {
        self.value.store(value.to_bits(), ordering);
    }

    pub fn fetch_add(&self, value: f64, ordering: Ordering) -> f64 {
        let mut current = self.load(ordering);
        loop {
            let new_value = current + value;
            match self.value.compare_exchange_weak(
                current.to_bits(),
                new_value.to_bits(),
                ordering,
                Ordering::Relaxed,
            ) {
                Ok(_) => return current,
                Err(actual) => current = f64::from_bits(actual),
            }
        }
    }
}

impl Default for AtomicF64 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Entry type for statistics tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Deleted,
}

impl From<&OverrideContent> for EntryType {
    fn from(content: &OverrideContent) -> Self {
        match content {
            OverrideContent::File { .. } => EntryType::File,
            OverrideContent::Directory { .. } => EntryType::Directory,
            OverrideContent::Deleted => EntryType::Deleted,
        }
    }
}

/// Comprehensive statistics for the override store
pub struct OverrideStoreStats {
    /// Total number of entries in the store
    pub total_entries: AtomicU64,
    /// Number of file entries
    pub file_entries: AtomicU64,
    /// Number of directory entries
    pub directory_entries: AtomicU64,
    /// Number of deleted entries (tombstones)
    pub deleted_entries: AtomicU64,
    /// Total memory usage in bytes
    pub total_memory_bytes: AtomicUsize,
    /// Bytes saved through compression
    pub compressed_bytes_saved: AtomicUsize,
    /// Bytes saved through deduplication
    pub dedup_bytes_saved: AtomicUsize,
    /// Current cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: AtomicF64,
    /// Number of evictions performed
    pub eviction_count: AtomicU64,
    
    // Internal tracking for hit rate calculation
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    
    // Callback registry for real-time monitoring
    callbacks: Arc<RwLock<Vec<Box<dyn Fn(&StatsSnapshot) + Send + Sync>>>>,
    
    // Alert thresholds
    alert_config: Arc<RwLock<AlertConfig>>,
    
    // Hot path tracking
    hot_paths: Arc<Mutex<HashMap<ShadowPath, HotPathStats>>>,
}

/// Configuration for statistical alerts
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// Memory pressure threshold (0.0 to 1.0)
    pub memory_pressure_threshold: f64,
    /// High eviction rate threshold (evictions per second)
    pub eviction_rate_threshold: f64,
    /// Low cache hit rate threshold (0.0 to 1.0)
    pub cache_hit_rate_threshold: f64,
    /// Whether alerts are enabled
    pub alerts_enabled: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            memory_pressure_threshold: 0.85,
            eviction_rate_threshold: 10.0,
            cache_hit_rate_threshold: 0.7,
            alerts_enabled: true,
        }
    }
}

/// Hot path statistics
#[derive(Debug, Clone)]
pub struct HotPathStats {
    /// Number of accesses
    pub access_count: u64,
    /// Last access time
    pub last_accessed: SystemTime,
    /// Average access interval
    pub avg_interval: Duration,
    /// Total bytes accessed
    pub bytes_accessed: u64,
}

impl HotPathStats {
    fn new() -> Self {
        Self {
            access_count: 0,
            last_accessed: SystemTime::now(),
            avg_interval: Duration::from_secs(0),
            bytes_accessed: 0,
        }
    }

    fn update_access(&mut self, bytes: u64) {
        let now = SystemTime::now();
        
        if self.access_count > 0 {
            if let Ok(interval) = now.duration_since(self.last_accessed) {
                // Update running average of access intervals
                let total_duration = self.avg_interval.as_nanos() as u64 * self.access_count + interval.as_nanos() as u64;
                self.avg_interval = Duration::from_nanos(total_duration / (self.access_count + 1));
            }
        }
        
        self.access_count += 1;
        self.last_accessed = now;
        self.bytes_accessed += bytes;
    }
}

/// Snapshot of current statistics
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub timestamp: SystemTime,
    pub total_entries: u64,
    pub file_entries: u64,
    pub directory_entries: u64,
    pub deleted_entries: u64,
    pub total_memory_bytes: usize,
    pub compressed_bytes_saved: usize,
    pub dedup_bytes_saved: usize,
    pub cache_hit_rate: f64,
    pub eviction_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Detailed memory usage breakdown
#[derive(Debug, Clone)]
pub struct MemoryBreakdown {
    /// Raw file data (uncompressed)
    pub raw_file_data: usize,
    /// Compressed file data
    pub compressed_file_data: usize,
    /// Directory metadata
    pub directory_metadata: usize,
    /// Path strings
    pub path_strings: usize,
    /// Cache overhead
    pub cache_overhead: usize,
    /// Index overhead (maps, trees, etc.)
    pub index_overhead: usize,
    /// Total allocated
    pub total_allocated: usize,
}

/// Comprehensive statistics report
#[derive(Debug, Clone)]
pub struct StatsReport {
    /// Basic statistics snapshot
    pub snapshot: StatsSnapshot,
    /// Memory breakdown
    pub memory_breakdown: MemoryBreakdown,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Hot paths (most accessed)
    pub hot_paths: Vec<(ShadowPath, HotPathStats)>,
    /// Efficiency ratios
    pub efficiency: EfficiencyMetrics,
}

/// Performance-related metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Average entry size
    pub avg_entry_size: f64,
    /// Compression ratio (compressed / original)
    pub compression_ratio: f64,
    /// Deduplication ratio (unique / total)
    pub deduplication_ratio: f64,
    /// Memory efficiency (useful data / total memory)
    pub memory_efficiency: f64,
}

/// Efficiency metrics
#[derive(Debug, Clone)]
pub struct EfficiencyMetrics {
    /// Space saved through compression (percentage)
    pub compression_efficiency: f64,
    /// Space saved through deduplication (percentage)
    pub deduplication_efficiency: f64,
    /// Cache effectiveness (hit rate weighted by access frequency)
    pub cache_effectiveness: f64,
    /// Overall storage efficiency
    pub storage_efficiency: f64,
}

impl OverrideStoreStats {
    /// Creates a new statistics tracker
    pub fn new() -> Self {
        Self {
            total_entries: AtomicU64::new(0),
            file_entries: AtomicU64::new(0),
            directory_entries: AtomicU64::new(0),
            deleted_entries: AtomicU64::new(0),
            total_memory_bytes: AtomicUsize::new(0),
            compressed_bytes_saved: AtomicUsize::new(0),
            dedup_bytes_saved: AtomicUsize::new(0),
            cache_hit_rate: AtomicF64::new(0.0),
            eviction_count: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            callbacks: Arc::new(RwLock::new(Vec::new())),
            alert_config: Arc::new(RwLock::new(AlertConfig::default())),
            hot_paths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Updates statistics when inserting an entry
    pub fn update_on_insert(&self, entry: &OverrideEntry, memory_size: usize, compression_saved: usize, dedup_saved: usize) {
        let entry_type = EntryType::from(&entry.content);
        
        // Update entry counts
        self.total_entries.fetch_add(1, Ordering::Relaxed);
        match entry_type {
            EntryType::File => self.file_entries.fetch_add(1, Ordering::Relaxed),
            EntryType::Directory => self.directory_entries.fetch_add(1, Ordering::Relaxed),
            EntryType::Deleted => self.deleted_entries.fetch_add(1, Ordering::Relaxed),
        };

        // Update memory usage
        self.total_memory_bytes.fetch_add(memory_size, Ordering::Relaxed);
        self.compressed_bytes_saved.fetch_add(compression_saved, Ordering::Relaxed);
        self.dedup_bytes_saved.fetch_add(dedup_saved, Ordering::Relaxed);

        // Update hot path tracking
        if let EntryType::File = entry_type {
            let mut hot_paths = self.hot_paths.lock().unwrap();
            let stats = hot_paths.entry(entry.path.clone()).or_insert_with(HotPathStats::new);
            stats.update_access(entry.override_metadata.size);
        }

        // Trigger callbacks
        self.trigger_callbacks();
    }

    /// Updates statistics when removing an entry
    pub fn update_on_remove(&self, entry: &OverrideEntry, memory_size: usize, compression_saved: usize, dedup_saved: usize) {
        let entry_type = EntryType::from(&entry.content);
        
        // Update entry counts
        self.total_entries.fetch_sub(1, Ordering::Relaxed);
        match entry_type {
            EntryType::File => self.file_entries.fetch_sub(1, Ordering::Relaxed),
            EntryType::Directory => self.directory_entries.fetch_sub(1, Ordering::Relaxed),
            EntryType::Deleted => self.deleted_entries.fetch_sub(1, Ordering::Relaxed),
        };

        // Update memory usage
        self.total_memory_bytes.fetch_sub(memory_size, Ordering::Relaxed);
        self.compressed_bytes_saved.fetch_sub(compression_saved, Ordering::Relaxed);
        self.dedup_bytes_saved.fetch_sub(dedup_saved, Ordering::Relaxed);

        // Trigger callbacks
        self.trigger_callbacks();
    }

    /// Updates statistics when eviction occurs
    pub fn update_on_eviction(&self, count: u64, _bytes_freed: usize) {
        self.eviction_count.fetch_add(count, Ordering::Relaxed);
        
        // Check for high eviction rate alert
        self.check_eviction_rate_alert();
        
        // Trigger callbacks
        self.trigger_callbacks();
    }

    /// Updates cache hit/miss statistics
    pub fn update_cache_access(&self, hit: bool) {
        if hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
        
        // Recalculate hit rate
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total > 0 {
            let hit_rate = hits as f64 / total as f64;
            self.cache_hit_rate.store(hit_rate, Ordering::Relaxed);
            
            // Check for low hit rate alert
            self.check_cache_hit_rate_alert(hit_rate);
        }
    }

    /// Updates hot path statistics on access
    pub fn update_hot_path_access(&self, path: &ShadowPath, bytes: u64) {
        let mut hot_paths = self.hot_paths.lock().unwrap();
        let stats = hot_paths.entry(path.clone()).or_insert_with(HotPathStats::new);
        stats.update_access(bytes);
    }

    /// Generates a comprehensive statistics report
    pub fn generate_report(&self) -> StatsReport {
        let snapshot = self.get_snapshot();
        let memory_breakdown = self.get_memory_breakdown();
        let hot_paths = self.get_hot_paths(20); // Top 20 hot paths
        
        let performance_metrics = self.calculate_performance_metrics(&snapshot, &memory_breakdown);
        let efficiency = self.calculate_efficiency_metrics(&snapshot, &memory_breakdown);

        StatsReport {
            snapshot,
            memory_breakdown,
            performance_metrics,
            hot_paths,
            efficiency,
        }
    }

    /// Gets current statistics snapshot
    pub fn get_snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            timestamp: SystemTime::now(),
            total_entries: self.total_entries.load(Ordering::Relaxed),
            file_entries: self.file_entries.load(Ordering::Relaxed),
            directory_entries: self.directory_entries.load(Ordering::Relaxed),
            deleted_entries: self.deleted_entries.load(Ordering::Relaxed),
            total_memory_bytes: self.total_memory_bytes.load(Ordering::Relaxed),
            compressed_bytes_saved: self.compressed_bytes_saved.load(Ordering::Relaxed),
            dedup_bytes_saved: self.dedup_bytes_saved.load(Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate.load(Ordering::Relaxed),
            eviction_count: self.eviction_count.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
        }
    }

    /// Gets detailed memory usage breakdown
    pub fn get_memory_breakdown(&self) -> MemoryBreakdown {
        let total = self.total_memory_bytes.load(Ordering::Relaxed);
        let compressed_saved = self.compressed_bytes_saved.load(Ordering::Relaxed);
        let _dedup_saved = self.dedup_bytes_saved.load(Ordering::Relaxed);
        
        // Estimate breakdown (these would be more accurate with detailed tracking)
        let estimated_raw_data = total + compressed_saved;
        let compressed_data = total.saturating_sub(compressed_saved);
        
        MemoryBreakdown {
            raw_file_data: estimated_raw_data,
            compressed_file_data: compressed_data,
            directory_metadata: total / 20, // Rough estimate: 5% for directory metadata
            path_strings: total / 10, // Rough estimate: 10% for path strings
            cache_overhead: total / 50, // Rough estimate: 2% for cache overhead
            index_overhead: total / 25, // Rough estimate: 4% for index overhead
            total_allocated: total,
        }
    }

    /// Gets the most accessed paths
    pub fn get_hot_paths(&self, limit: usize) -> Vec<(ShadowPath, HotPathStats)> {
        let hot_paths = self.hot_paths.lock().unwrap();
        let mut paths: Vec<_> = hot_paths.iter()
            .map(|(path, stats)| (path.clone(), stats.clone()))
            .collect();
        
        // Sort by access count descending
        paths.sort_by(|a, b| b.1.access_count.cmp(&a.1.access_count));
        paths.truncate(limit);
        paths
    }

    /// Registers a callback for statistics changes
    pub fn register_callback<F>(&self, callback: F) 
    where 
        F: Fn(&StatsSnapshot) + Send + Sync + 'static 
    {
        let mut callbacks = self.callbacks.write().unwrap();
        callbacks.push(Box::new(callback));
    }

    /// Updates alert configuration
    pub fn update_alert_config(&self, config: AlertConfig) {
        *self.alert_config.write().unwrap() = config;
    }

    /// Resets all statistics
    pub fn reset(&self) {
        self.total_entries.store(0, Ordering::Relaxed);
        self.file_entries.store(0, Ordering::Relaxed);
        self.directory_entries.store(0, Ordering::Relaxed);
        self.deleted_entries.store(0, Ordering::Relaxed);
        self.total_memory_bytes.store(0, Ordering::Relaxed);
        self.compressed_bytes_saved.store(0, Ordering::Relaxed);
        self.dedup_bytes_saved.store(0, Ordering::Relaxed);
        self.cache_hit_rate.store(0.0, Ordering::Relaxed);
        self.eviction_count.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        
        self.hot_paths.lock().unwrap().clear();
    }

    // Private methods for internal calculations and alerts

    fn calculate_performance_metrics(&self, snapshot: &StatsSnapshot, memory: &MemoryBreakdown) -> PerformanceMetrics {
        let avg_entry_size = if snapshot.total_entries > 0 {
            memory.total_allocated as f64 / snapshot.total_entries as f64
        } else {
            0.0
        };

        let compression_ratio = if memory.raw_file_data > 0 {
            memory.compressed_file_data as f64 / memory.raw_file_data as f64
        } else {
            1.0
        };

        let deduplication_ratio = if snapshot.file_entries > 0 {
            // This would need actual unique content count from dedup store
            0.85 // Placeholder estimate
        } else {
            1.0
        };

        let memory_efficiency = if memory.total_allocated > 0 {
            (memory.raw_file_data + memory.directory_metadata) as f64 / memory.total_allocated as f64
        } else {
            0.0
        };

        PerformanceMetrics {
            avg_entry_size,
            compression_ratio,
            deduplication_ratio,
            memory_efficiency,
        }
    }

    fn calculate_efficiency_metrics(&self, snapshot: &StatsSnapshot, memory: &MemoryBreakdown) -> EfficiencyMetrics {
        let compression_efficiency = if memory.raw_file_data > 0 {
            (snapshot.compressed_bytes_saved as f64 / memory.raw_file_data as f64) * 100.0
        } else {
            0.0
        };

        let deduplication_efficiency = if memory.total_allocated > 0 {
            (snapshot.dedup_bytes_saved as f64 / memory.total_allocated as f64) * 100.0
        } else {
            0.0
        };

        let cache_effectiveness = snapshot.cache_hit_rate * 100.0;

        let storage_efficiency = compression_efficiency + deduplication_efficiency;

        EfficiencyMetrics {
            compression_efficiency,
            deduplication_efficiency,
            cache_effectiveness,
            storage_efficiency,
        }
    }

    fn trigger_callbacks(&self) {
        let snapshot = self.get_snapshot();
        let callbacks = self.callbacks.read().unwrap();
        
        for callback in callbacks.iter() {
            callback(&snapshot);
        }
    }

    fn check_eviction_rate_alert(&self) {
        let config = self.alert_config.read().unwrap();
        if !config.alerts_enabled {
            return;
        }
        
        // This would need more sophisticated rate calculation in production
        // For now, just check if eviction count is high
        let evictions = self.eviction_count.load(Ordering::Relaxed);
        if evictions > 100 { // Arbitrary threshold
            eprintln!("ALERT: High eviction rate detected: {} evictions", evictions);
        }
    }

    fn check_cache_hit_rate_alert(&self, hit_rate: f64) {
        let config = self.alert_config.read().unwrap();
        if !config.alerts_enabled {
            return;
        }
        
        if hit_rate < config.cache_hit_rate_threshold {
            eprintln!("ALERT: Low cache hit rate: {:.2}% (threshold: {:.2}%)", 
                     hit_rate * 100.0, config.cache_hit_rate_threshold * 100.0);
        }
    }
}

impl Default for OverrideStoreStats {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OverrideStoreStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverrideStoreStats")
            .field("total_entries", &self.total_entries.load(Ordering::Relaxed))
            .field("file_entries", &self.file_entries.load(Ordering::Relaxed))
            .field("directory_entries", &self.directory_entries.load(Ordering::Relaxed))
            .field("deleted_entries", &self.deleted_entries.load(Ordering::Relaxed))
            .field("total_memory_bytes", &self.total_memory_bytes.load(Ordering::Relaxed))
            .field("cache_hit_rate", &self.cache_hit_rate.load(Ordering::Relaxed))
            .field("eviction_count", &self.eviction_count.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileMetadata;
    use bytes::Bytes;
    use std::time::SystemTime;
    use std::sync::atomic::AtomicU64;

    fn create_test_entry(path: &str, content: OverrideContent) -> OverrideEntry {
        OverrideEntry {
            path: ShadowPath::new(path.into()),
            content,
            original_metadata: None,
            override_metadata: FileMetadata::default(),
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(0),
        }
    }

    #[test]
    fn test_stats_basic_operations() {
        let stats = OverrideStoreStats::new();
        
        // Test initial state
        let snapshot = stats.get_snapshot();
        assert_eq!(snapshot.total_entries, 0);
        assert_eq!(snapshot.file_entries, 0);
        assert_eq!(snapshot.cache_hit_rate, 0.0);

        // Test insert
        let entry = create_test_entry("/test.txt", OverrideContent::File {
            data: Bytes::from("test"),
            content_hash: [0u8; 32],
            is_compressed: false,
        });
        
        stats.update_on_insert(&entry, 1000, 100, 50);
        
        let snapshot = stats.get_snapshot();
        assert_eq!(snapshot.total_entries, 1);
        assert_eq!(snapshot.file_entries, 1);
        assert_eq!(snapshot.total_memory_bytes, 1000);
        assert_eq!(snapshot.compressed_bytes_saved, 100);
        assert_eq!(snapshot.dedup_bytes_saved, 50);
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        let stats = OverrideStoreStats::new();
        
        // Record some hits and misses
        stats.update_cache_access(true);  // hit
        stats.update_cache_access(true);  // hit
        stats.update_cache_access(false); // miss
        stats.update_cache_access(true);  // hit
        
        let snapshot = stats.get_snapshot();
        assert_eq!(snapshot.cache_hits, 3);
        assert_eq!(snapshot.cache_misses, 1);
        assert!((snapshot.cache_hit_rate - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hot_paths_tracking() {
        let stats = OverrideStoreStats::new();
        let path = ShadowPath::new("/hot/file.txt".into());
        
        // Access the path multiple times
        stats.update_hot_path_access(&path, 100);
        stats.update_hot_path_access(&path, 200);
        stats.update_hot_path_access(&path, 150);
        
        let hot_paths = stats.get_hot_paths(10);
        assert_eq!(hot_paths.len(), 1);
        assert_eq!(hot_paths[0].0, path);
        assert_eq!(hot_paths[0].1.access_count, 3);
        assert_eq!(hot_paths[0].1.bytes_accessed, 450);
    }

    #[test]
    fn test_memory_breakdown() {
        let stats = OverrideStoreStats::new();
        stats.total_memory_bytes.store(10000, Ordering::Relaxed);
        stats.compressed_bytes_saved.store(2000, Ordering::Relaxed);
        
        let breakdown = stats.get_memory_breakdown();
        assert_eq!(breakdown.total_allocated, 10000);
        assert_eq!(breakdown.raw_file_data, 12000); // 10000 + 2000
        assert_eq!(breakdown.compressed_file_data, 8000); // 10000 - 2000
    }

    #[test]
    fn test_stats_reset() {
        let stats = OverrideStoreStats::new();
        
        // Set some values
        stats.total_entries.store(10, Ordering::Relaxed);
        stats.cache_hits.store(5, Ordering::Relaxed);
        stats.total_memory_bytes.store(1000, Ordering::Relaxed);
        
        // Reset
        stats.reset();
        
        let snapshot = stats.get_snapshot();
        assert_eq!(snapshot.total_entries, 0);
        assert_eq!(snapshot.cache_hits, 0);
        assert_eq!(snapshot.total_memory_bytes, 0);
    }

    #[test]
    fn test_atomic_f64() {
        let atomic = AtomicF64::new(3.14);
        assert!((atomic.load(Ordering::Relaxed) - 3.14).abs() < f64::EPSILON);
        
        atomic.store(2.71, Ordering::Relaxed);
        assert!((atomic.load(Ordering::Relaxed) - 2.71).abs() < f64::EPSILON);
        
        let old = atomic.fetch_add(1.0, Ordering::Relaxed);
        assert!((old - 2.71).abs() < f64::EPSILON);
        assert!((atomic.load(Ordering::Relaxed) - 3.71).abs() < f64::EPSILON);
    }

    #[test]
    fn test_report_generation() {
        let stats = OverrideStoreStats::new();
        
        // Add some test data
        let entry = create_test_entry("/test.txt", OverrideContent::File {
            data: Bytes::from("test"),
            content_hash: [0u8; 32],
            is_compressed: false,
        });
        
        stats.update_on_insert(&entry, 1000, 100, 50);
        stats.update_cache_access(true);
        stats.update_cache_access(false);
        
        let report = stats.generate_report();
        
        assert_eq!(report.snapshot.total_entries, 1);
        assert!(report.performance_metrics.avg_entry_size > 0.0);
        assert!(report.efficiency.compression_efficiency >= 0.0);
    }
}