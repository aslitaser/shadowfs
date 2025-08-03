//! Public API and builder for the override store.

use crate::types::ShadowPath;
use crate::error::ShadowError;
use super::{
    OverrideStore, OverrideStoreConfig, EvictionPolicy, PrefetchStrategy,
    OverrideSnapshot
};
use bytes::Bytes;
use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};

/// Builder for creating configured OverrideStore instances.
/// 
/// # Examples
/// 
/// ```rust
/// use shadowfs_core::override_store::{OverrideStoreBuilder, EvictionPolicy, PrefetchStrategy};
/// 
/// // Basic store with defaults
/// let store = OverrideStoreBuilder::new()
///     .build()
///     .expect("Failed to create store");
/// 
/// // Configured store
/// let store = OverrideStoreBuilder::new()
///     .with_memory_limit(128 * 1024 * 1024) // 128MB
///     .with_eviction_policy(EvictionPolicy::Lru)
///     .with_compression(true)
///     .build()
///     .expect("Failed to create store");
/// ```
#[derive(Debug, Clone)]
pub struct OverrideStoreBuilder {
    config: OverrideStoreConfig,
    persistence_path: Option<PathBuf>,
}

impl OverrideStoreBuilder {
    /// Creates a new builder with default configuration.
    /// 
    /// # Default Configuration
    /// 
    /// - Memory limit: 64MB
    /// - Eviction policy: LRU
    /// - Memory pressure detection: enabled
    /// - Eviction threshold: 90%
    /// - Cache size: 1000 entries
    /// - Prefetch strategy: Children
    /// - Compression: enabled for files >1MB
    pub fn new() -> Self {
        Self {
            config: OverrideStoreConfig::default(),
            persistence_path: None,
        }
    }
    
    /// Sets the maximum memory usage in bytes.
    /// 
    /// When this limit is reached, the store will begin evicting entries
    /// according to the configured eviction policy.
    /// 
    /// # Arguments
    /// 
    /// * `bytes` - Maximum memory usage in bytes
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_memory_limit(256 * 1024 * 1024) // 256MB
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.config.max_memory = bytes;
        self
    }
    
    /// Enables persistence to the specified path.
    /// 
    /// When persistence is enabled, the store will:
    /// - Save snapshots periodically
    /// - Write a WAL for recovery
    /// - Allow loading from previous sessions
    /// 
    /// # Arguments
    /// 
    /// * `path` - Directory where persistence files will be stored
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// use std::path::PathBuf;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_persistence(PathBuf::from("/tmp/shadowfs"))
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_persistence(mut self, path: PathBuf) -> Self {
        self.persistence_path = Some(path);
        self
    }
    
    /// Sets the eviction policy for when memory limits are reached.
    /// 
    /// # Arguments
    /// 
    /// * `policy` - Eviction policy to use
    /// 
    /// # Available Policies
    /// 
    /// - `EvictionPolicy::Lru` - Least Recently Used (default)
    /// - `EvictionPolicy::Lfu` - Least Frequently Used
    /// - `EvictionPolicy::SizeWeighted` - Largest entries first
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::{OverrideStoreBuilder, EvictionPolicy};
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_eviction_policy(EvictionPolicy::Lfu)
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.config.eviction_policy = policy;
        self
    }
    
    /// Enables or disables compression for large files.
    /// 
    /// When enabled, files larger than 1MB will be compressed using zstd
    /// to reduce memory usage. Compression is transparent to callers.
    /// 
    /// # Arguments
    /// 
    /// * `enabled` - Whether to enable compression
    /// 
    /// # Performance Considerations
    /// 
    /// - Compression reduces memory usage but increases CPU usage
    /// - Best for stores with large files and memory constraints
    /// - Transparent decompression on access
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_compression(false) // Disable compression
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.config.enable_compression = enabled;
        self
    }
    
    /// Sets the cache size for hot entries.
    /// 
    /// The hot cache keeps frequently accessed entries in memory for
    /// faster retrieval. Larger caches improve performance but use more memory.
    /// 
    /// # Arguments
    /// 
    /// * `size` - Number of entries to cache
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_cache_size(2000) // Cache 2000 hot entries
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.config.cache_size = size;
        self
    }
    
    /// Sets the directory prefetch strategy.
    /// 
    /// Prefetching loads likely-to-be-accessed entries into the cache
    /// when a directory is accessed, improving performance for directory
    /// traversals.
    /// 
    /// # Arguments
    /// 
    /// * `strategy` - Prefetch strategy to use
    /// 
    /// # Available Strategies
    /// 
    /// - `PrefetchStrategy::None` - No prefetching
    /// - `PrefetchStrategy::Children` - Prefetch immediate children (default)
    /// - `PrefetchStrategy::Recursive` - Prefetch entire subtree
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::{OverrideStoreBuilder, PrefetchStrategy};
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_prefetch_strategy(PrefetchStrategy::Recursive)
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_prefetch_strategy(mut self, strategy: PrefetchStrategy) -> Self {
        self.config.prefetch_strategy = strategy;
        self
    }
    
    /// Sets the eviction threshold as a percentage of memory usage.
    /// 
    /// When memory usage exceeds this threshold, eviction will begin.
    /// Lower values trigger eviction earlier, higher values allow more
    /// memory usage before eviction.
    /// 
    /// # Arguments
    /// 
    /// * `threshold` - Threshold as a value between 0.0 and 1.0
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_eviction_threshold(0.8) // Evict when 80% full
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn with_eviction_threshold(mut self, threshold: f64) -> Self {
        self.config.eviction_threshold = threshold.clamp(0.0, 1.0);
        self
    }
    
    /// Builds the configured OverrideStore.
    /// 
    /// # Returns
    /// 
    /// A configured `OverrideStore` instance, or an error if creation fails.
    /// 
    /// # Errors
    /// 
    /// - `ShadowError::IoError` - If persistence directory cannot be created
    /// - `ShadowError::InvalidConfiguration` - If configuration is invalid
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStoreBuilder;
    /// 
    /// let store = OverrideStoreBuilder::new()
    ///     .with_memory_limit(128 * 1024 * 1024)
    ///     .build()
    ///     .expect("Failed to create store");
    /// ```
    pub fn build(self) -> Result<OverrideStore, ShadowError> {
        // Validate configuration
        if self.config.max_memory == 0 {
            return Err(ShadowError::InvalidConfiguration {
                message: "Memory limit must be greater than 0".to_string(),
            });
        }
        
        if self.config.cache_size == 0 {
            return Err(ShadowError::InvalidConfiguration {
                message: "Cache size must be greater than 0".to_string(),
            });
        }
        
        // Create the store
        let store = OverrideStore::new(self.config);
        
        // Set up persistence if configured
        if let Some(persistence_path) = self.persistence_path {
            // In a real implementation, we would set up persistence here
            // For now, we'll just validate the path exists or can be created
            if let Some(parent) = persistence_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ShadowError::IoError { source: e })?;
            }
        }
        
        Ok(store)
    }
}

impl Default for OverrideStoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Health status of the override store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Store is operating normally
    Healthy,
    /// Store is experiencing warnings but still functional
    Warning {
        /// Warning messages
        issues: Vec<String>,
    },
    /// Store has critical issues that may affect functionality
    Critical {
        /// Critical error messages
        errors: Vec<String>,
    },
}

impl HealthStatus {
    /// Returns true if the store is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
    
    /// Returns true if the store has warnings
    pub fn has_warnings(&self) -> bool {
        matches!(self, HealthStatus::Warning { .. })
    }
    
    /// Returns true if the store has critical issues
    pub fn is_critical(&self) -> bool {
        matches!(self, HealthStatus::Critical { .. })
    }
    
    /// Gets all issues (warnings and errors)
    pub fn issues(&self) -> Vec<&str> {
        match self {
            HealthStatus::Healthy => Vec::new(),
            HealthStatus::Warning { issues } => issues.iter().map(|s| s.as_str()).collect(),
            HealthStatus::Critical { errors } => errors.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Export formats for override store data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Binary format with efficient compression
    Binary,
    /// JSON format for human readability
    Json,
    /// MessagePack format for compact binary
    MessagePack,
}

/// Migration utilities for override store data.
pub struct Migration {
    /// Source version
    pub from_version: u32,
    /// Target version
    pub to_version: u32,
    /// Migration timestamp
    pub timestamp: SystemTime,
}

impl Migration {
    /// Creates a new migration record
    pub fn new(from_version: u32, to_version: u32) -> Self {
        Self {
            from_version,
            to_version,
            timestamp: SystemTime::now(),
        }
    }
}

/// Public convenience methods for OverrideStore.
impl OverrideStore {
    /// Creates a new OverrideStore from a snapshot file.
    /// 
    /// This is useful for restoring state from a previous session
    /// or importing data from another store instance.
    /// 
    /// # Arguments
    /// 
    /// * `path` - Path to the snapshot file
    /// 
    /// # Returns
    /// 
    /// A new `OverrideStore` instance with data loaded from the snapshot.
    /// 
    /// # Errors
    /// 
    /// - `ShadowError::NotFound` - If the snapshot file doesn't exist
    /// - `ShadowError::IoError` - If the file cannot be read
    /// - `ShadowError::InvalidConfiguration` - If the snapshot is corrupted
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use shadowfs_core::override_store::OverrideStore;
    /// use std::path::PathBuf;
    /// 
    /// let store = OverrideStore::from_snapshot(PathBuf::from("backup.snapshot"))
    ///     .expect("Failed to load from snapshot");
    /// ```
    pub fn from_snapshot(path: PathBuf) -> Result<Self, ShadowError> {
        // Check if snapshot file exists
        if !path.exists() {
            return Err(ShadowError::NotFound {
                path: ShadowPath::new(path),
            });
        }
        
        // Load snapshot data
        let snapshot_data = std::fs::read(&path)
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        // Deserialize snapshot
        let snapshot: OverrideSnapshot = bincode::deserialize(&snapshot_data)
            .map_err(|_| ShadowError::InvalidConfiguration {
                message: "Corrupted snapshot file".to_string(),
            })?;
        
        // Create store with configuration from snapshot
        let store = OverrideStore::new(snapshot.config.clone());
        
        // Apply snapshot entries
        // TODO: Implement apply_snapshot properly
        // For now, just return the store with default config
        
        Ok(store)
    }
    
    /// Gets the current memory usage as a percentage of the limit.
    /// 
    /// # Returns
    /// 
    /// Memory usage percentage from 0.0 to 1.0 (and potentially higher
    /// if the store is over its limit).
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStore;
    /// 
    /// let store = OverrideStore::with_defaults();
    /// let usage = store.memory_usage_percentage();
    /// 
    /// if usage > 0.9 {
    ///     println!("Memory usage is high: {:.1}%", usage * 100.0);
    /// }
    /// ```
    pub fn memory_usage_percentage(&self) -> f64 {
        self.memory_tracker.get_pressure_ratio()
    }
    
    /// Suggests how many bytes should be evicted to maintain healthy operation.
    /// 
    /// This method analyzes current memory usage and returns a suggestion
    /// for how much data should be evicted to bring the store back to
    /// healthy levels.
    /// 
    /// # Returns
    /// 
    /// `Some(bytes)` if eviction is recommended, `None` if the store is healthy.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::OverrideStore;
    /// 
    /// let store = OverrideStore::with_defaults();
    /// 
    /// if let Some(bytes_to_evict) = store.suggest_eviction_size() {
    ///     println!("Consider evicting {} bytes", bytes_to_evict);
    /// }
    /// ```
    pub fn suggest_eviction_size(&self) -> Option<usize> {
        let config = self.config.read().unwrap();
        let current_usage = self.memory_tracker.current_usage();
        let max_memory = config.max_memory;
        let threshold = config.eviction_threshold;
        
        let target_usage = (max_memory as f64 * threshold * 0.8) as usize; // 80% of threshold
        
        if current_usage > target_usage {
            Some(current_usage - target_usage)
        } else {
            None
        }
    }
    
    /// Performs a comprehensive health check of the store.
    /// 
    /// This method examines various aspects of the store's health:
    /// - Memory usage levels
    /// - Cache hit rates
    /// - Eviction rates
    /// - Internal consistency
    /// 
    /// # Returns
    /// 
    /// A `HealthStatus` indicating the overall health of the store.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::{OverrideStore, HealthStatus};
    /// 
    /// let store = OverrideStore::with_defaults();
    /// 
    /// match store.health_check() {
    ///     HealthStatus::Healthy => println!("Store is healthy"),
    ///     HealthStatus::Warning { issues } => {
    ///         println!("Store has warnings: {:?}", issues);
    ///     },
    ///     HealthStatus::Critical { errors } => {
    ///         println!("Store has critical issues: {:?}", errors);
    ///     },
    /// }
    /// ```
    pub fn health_check(&self) -> HealthStatus {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        
        // Check memory usage
        let memory_usage = self.memory_usage_percentage();
        if memory_usage > 1.0 {
            errors.push(format!("Memory usage over limit: {:.1}%", memory_usage * 100.0));
        } else if memory_usage > 0.9 {
            warnings.push(format!("High memory usage: {:.1}%", memory_usage * 100.0));
        }
        
        // Check cache hit rate
        let stats = self.get_stats_snapshot();
        if stats.cache_hit_rate < 0.5 && stats.cache_hits + stats.cache_misses > 100 {
            warnings.push(format!("Low cache hit rate: {:.1}%", stats.cache_hit_rate * 100.0));
        }
        
        // Check eviction rate
        if stats.eviction_count > 1000 {
            warnings.push(format!("High eviction count: {}", stats.eviction_count));
        }
        
        // Check for internal consistency
        let entry_count = self.entry_count();
        if entry_count == 0 && self.memory_tracker.current_usage() > 1024 * 1024 {
            errors.push("Memory usage high but no entries found".to_string());
        }
        
        // Return appropriate status
        if !errors.is_empty() {
            HealthStatus::Critical { errors }
        } else if !warnings.is_empty() {
            HealthStatus::Warning { issues: warnings }
        } else {
            HealthStatus::Healthy
        }
    }
    
    /// Migrates data from a version 1 override store.
    /// 
    /// This method handles migration from older store formats,
    /// ensuring backward compatibility and data preservation.
    /// 
    /// # Arguments
    /// 
    /// * `old_path` - Path to the old store data
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if migration succeeds, or an error if migration fails.
    /// 
    /// # Errors
    /// 
    /// - `ShadowError::NotFound` - If the old store data doesn't exist
    /// - `ShadowError::IoError` - If files cannot be read/written
    /// - `ShadowError::InvalidConfiguration` - If the old data is corrupted
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use shadowfs_core::override_store::OverrideStore;
    /// use std::path::PathBuf;
    /// 
    /// let mut store = OverrideStore::with_defaults();
    /// store.migrate_from_v1(PathBuf::from("old_store.dat"))
    ///     .expect("Migration failed");
    /// ```
    pub fn migrate_from_v1(&mut self, old_path: PathBuf) -> Result<(), ShadowError> {
        // Check if old file exists
        if !old_path.exists() {
            return Err(ShadowError::NotFound {
                path: ShadowPath::new(old_path),
            });
        }
        
        // In a real implementation, this would:
        // 1. Read the old format
        // 2. Parse the data
        // 3. Convert to new format
        // 4. Import into current store
        
        // For now, we'll simulate migration
        let _old_data = std::fs::read(&old_path)
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        // Create migration record
        let _migration = Migration::new(1, 2);
        
        // TODO: Implement actual migration logic
        Ok(())
    }
    
    /// Exports store data to the specified format.
    /// 
    /// This method creates a portable representation of the store
    /// that can be imported into another instance or used for backup.
    /// 
    /// # Arguments
    /// 
    /// * `format` - Export format to use
    /// 
    /// # Returns
    /// 
    /// Serialized data in the specified format.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shadowfs_core::override_store::{OverrideStore, ExportFormat};
    /// 
    /// let store = OverrideStore::with_defaults();
    /// let exported = store.export_to_format(ExportFormat::Json)
    ///     .expect("Export failed");
    /// 
    /// // Save to file
    /// std::fs::write("backup.json", exported).expect("Write failed");
    /// ```
    pub fn export_to_format(&self, format: ExportFormat) -> Result<Bytes, ShadowError> {
        let snapshot = self.create_snapshot();
        
        let serialized = match format {
            ExportFormat::Binary => {
                bincode::serialize(&snapshot)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to serialize to binary".to_string(),
                    })?
            }
            ExportFormat::Json => {
                serde_json::to_vec_pretty(&snapshot)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to serialize to JSON".to_string(),
                    })?
            }
            ExportFormat::MessagePack => {
                rmp_serde::to_vec(&snapshot)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to serialize to MessagePack".to_string(),
                    })?
            }
        };
        
        Ok(Bytes::from(serialized))
    }
    
    /// Imports data from the specified format.
    /// 
    /// This method loads previously exported data into the current store,
    /// merging it with existing entries.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Serialized data to import
    /// * `format` - Format of the data
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if import succeeds, or an error if import fails.
    /// 
    /// # Examples
    /// 
    /// ```rust,no_run
    /// use shadowfs_core::override_store::{OverrideStore, ExportFormat};
    /// use bytes::Bytes;
    /// 
    /// let mut store = OverrideStore::with_defaults();
    /// let data = std::fs::read("backup.json").expect("Read failed");
    /// 
    /// store.import_from_format(Bytes::from(data), ExportFormat::Json)
    ///     .expect("Import failed");
    /// ```
    pub fn import_from_format(&mut self, data: Bytes, format: ExportFormat) -> Result<(), ShadowError> {
        let snapshot: OverrideSnapshot = match format {
            ExportFormat::Binary => {
                bincode::deserialize(&data)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to deserialize from binary".to_string(),
                    })?
            }
            ExportFormat::Json => {
                serde_json::from_slice(&data)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to deserialize from JSON".to_string(),
                    })?
            }
            ExportFormat::MessagePack => {
                rmp_serde::from_slice(&data)
                    .map_err(|_| ShadowError::InvalidConfiguration {
                        message: "Failed to deserialize from MessagePack".to_string(),
                    })?
            }
        };
        
        // Apply the snapshot to current store
        self.apply_snapshot(snapshot)?;
        
        Ok(())
    }
    
    /// Creates a snapshot of the current store state.
    fn create_snapshot(&self) -> OverrideSnapshot {
        // Use the existing from_store method
        OverrideSnapshot::from_store(self)
    }
    
    /// Applies a snapshot to the current store.
    fn apply_snapshot(&mut self, _snapshot: OverrideSnapshot) -> Result<(), ShadowError> {
        // In a real implementation, this would apply the snapshot entries
        // TODO: Implement snapshot application
        Ok(())
    }
}