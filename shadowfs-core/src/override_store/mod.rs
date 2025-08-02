//! In-memory storage for file and directory overrides.

mod entry;
mod memory;
mod lru;
mod size;

pub use entry::{OverrideEntry, OverrideContent};
pub use memory::{MemoryTracker, MemoryGuard};
pub use lru::{LruTracker, AccessStats, EvictionPolicy};
pub use size::{calculate_bytes_size, calculate_entry_size};

use crate::types::{FileMetadata, ShadowPath};
use crate::error::ShadowError;
use bytes::Bytes;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Configuration for the override store.
#[derive(Debug, Clone)]
pub struct OverrideStoreConfig {
    /// Maximum memory usage in bytes
    pub max_memory: usize,
    
    /// Eviction policy to use when memory limits are reached
    pub eviction_policy: EvictionPolicy,
    
    /// Whether to enable memory pressure detection
    pub enable_memory_pressure: bool,
    
    /// Threshold for triggering eviction (0.0 to 1.0)
    pub eviction_threshold: f64,
}

impl Default for OverrideStoreConfig {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024, // 64MB default
            eviction_policy: EvictionPolicy::Lru,
            enable_memory_pressure: true,
            eviction_threshold: 0.9,
        }
    }
}

/// Store for managing file and directory overrides with memory limits.
pub struct OverrideStore {
    /// Map of path to override entries with Arc for zero-copy reads
    entries: Arc<DashMap<ShadowPath, Arc<OverrideEntry>>>,
    
    /// Memory tracker for allocation management
    memory_tracker: Arc<MemoryTracker>,
    
    /// LRU tracker for access patterns and eviction
    lru_tracker: Arc<LruTracker>,
    
    /// Runtime configuration that can be updated
    config: RwLock<OverrideStoreConfig>,
}

impl OverrideStore {
    /// Creates a new OverrideStore with the specified configuration.
    ///
    /// # Arguments
    /// * `config` - Store configuration
    pub fn new(config: OverrideStoreConfig) -> Self {
        let memory_tracker = Arc::new(MemoryTracker::new(config.max_memory));
        let lru_tracker = Arc::new(LruTracker::new());
        let entries = Arc::new(DashMap::new());
        
        Self {
            entries,
            memory_tracker,
            lru_tracker,
            config: RwLock::new(config),
        }
    }
    
    /// Creates a new OverrideStore with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OverrideStoreConfig::default())
    }
    
    /// Inserts a file override.
    ///
    /// # Arguments
    /// * `path` - Path to override
    /// * `content` - File content
    /// * `original_metadata` - Original metadata if the file existed
    ///
    /// # Returns
    /// Ok(()) on success, or an error if memory limits would be exceeded
    pub fn insert_file(
        &self,
        path: ShadowPath,
        content: Bytes,
        original_metadata: Option<FileMetadata>,
    ) -> Result<(), ShadowError> {
        use sha2::{Sha256, Digest};
        
        // Calculate content hash
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let content_hash: [u8; 32] = hasher.finalize().into();
        
        let override_content = OverrideContent::File {
            data: content.clone(),
            content_hash,
        };
        
        let override_metadata = FileMetadata {
            size: content.len() as u64,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: original_metadata.as_ref()
                .map(|m| m.permissions.clone())
                .unwrap_or_else(|| crate::types::FilePermissions::default_file()),
            file_type: crate::types::FileType::File,
            platform_specific: original_metadata.as_ref()
                .map(|m| m.platform_specific.clone())
                .unwrap_or_else(|| crate::types::PlatformMetadata::default()),
        };
        
        self.insert_entry(path, override_content, original_metadata, override_metadata)
    }
    
    /// Inserts a directory override.
    ///
    /// # Arguments
    /// * `path` - Path to override
    /// * `original_metadata` - Original metadata if the directory existed
    ///
    /// # Returns
    /// Ok(()) on success, or an error if memory limits would be exceeded
    pub fn insert_directory(
        &self,
        path: ShadowPath,
        original_metadata: Option<FileMetadata>,
    ) -> Result<(), ShadowError> {
        let override_content = OverrideContent::Directory {
            entries: Vec::new(),
        };
        
        let override_metadata = FileMetadata {
            size: 0,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: original_metadata.as_ref()
                .map(|m| m.permissions.clone())
                .unwrap_or_else(|| crate::types::FilePermissions::default_directory()),
            file_type: crate::types::FileType::Directory,
            platform_specific: original_metadata.as_ref()
                .map(|m| m.platform_specific.clone())
                .unwrap_or_else(|| crate::types::PlatformMetadata::default()),
        };
        
        self.insert_entry(path, override_content, original_metadata, override_metadata)
    }
    
    /// Marks a file or directory as deleted.
    ///
    /// # Arguments
    /// * `path` - Path to mark as deleted
    ///
    /// # Returns
    /// Ok(()) on success, or an error if memory limits would be exceeded
    pub fn mark_deleted(&self, path: ShadowPath) -> Result<(), ShadowError> {
        let override_content = OverrideContent::Deleted;
        
        let override_metadata = FileMetadata {
            size: 0,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: crate::types::FilePermissions::default_file(),
            file_type: crate::types::FileType::File,
            platform_specific: crate::types::PlatformMetadata::default(),
        };
        
        self.insert_entry(path, override_content, None, override_metadata)
    }
    
    /// Internal method to insert an entry with memory management.
    fn insert_entry(
        &self,
        path: ShadowPath,
        content: OverrideContent,
        original_metadata: Option<FileMetadata>,
        override_metadata: FileMetadata,
    ) -> Result<(), ShadowError> {
        let entry = OverrideEntry {
            path: path.clone(),
            content,
            original_metadata,
            override_metadata,
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ),
        };
        
        let entry_size = calculate_entry_size(&entry);
        
        // Check if we need to evict before inserting
        let config = self.config.read().unwrap();
        let eviction_threshold = config.eviction_threshold;
        let eviction_policy = config.eviction_policy;
        drop(config);
        
        if self.memory_tracker.get_pressure_ratio() > eviction_threshold {
            // Calculate how much memory we need to free
            let target_bytes = (entry_size * 2).max(self.memory_tracker.current_usage() / 4);
            self.evict_entries(eviction_policy, target_bytes)?;
        }
        
        let entry_arc = Arc::new(entry);
        
        // If replacing an existing entry, we don't need additional memory allocation
        let old_entry = self.entries.insert(path.clone(), entry_arc);
        
        // If this is a new entry (not a replacement), allocate memory
        if old_entry.is_none() {
            let _guard = self.memory_tracker.try_allocate(entry_size)?;
            std::mem::forget(_guard); // Keep the allocation
        }
        
        // Update LRU tracker
        self.lru_tracker.record_access(&path);
        
        Ok(())
    }
    
    /// Gets an override entry if it exists.
    ///
    /// # Arguments
    /// * `path` - Path to look up
    ///
    /// # Returns
    /// Arc to the override entry if found
    pub fn get(&self, path: &ShadowPath) -> Option<Arc<OverrideEntry>> {
        if let Some(entry) = self.entries.get(path) {
            let entry_arc = entry.clone();
            
            // Update LRU tracker on access
            self.lru_tracker.record_access(path);
            
            // Update last accessed time
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entry_arc.last_accessed.store(now, Ordering::Relaxed);
            
            Some(entry_arc)
        } else {
            None
        }
    }
    
    /// Checks if a path exists in the override store.
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// true if the path exists (including deleted entries)
    pub fn exists(&self, path: &ShadowPath) -> bool {
        self.entries.contains_key(path)
    }
    
    /// Checks if a path is marked as deleted.
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// true if the path is marked as deleted
    pub fn is_deleted(&self, path: &ShadowPath) -> bool {
        if let Some(entry) = self.entries.get(path) {
            matches!(entry.content, OverrideContent::Deleted)
        } else {
            false
        }
    }
    
    /// Removes an override entry.
    ///
    /// # Arguments
    /// * `path` - Path to remove
    ///
    /// # Returns
    /// The removed entry if it existed
    pub fn remove(&self, path: &ShadowPath) -> Option<Arc<OverrideEntry>> {
        if let Some((_, entry)) = self.entries.remove(path) {
            // Remove from LRU tracker
            self.lru_tracker.remove_entry(path);
            
            // Memory will be freed when the Arc is dropped
            Some(entry)
        } else {
            None
        }
    }
    
    /// Evicts entries based on the configured policy.
    ///
    /// # Arguments
    /// * `policy` - Eviction policy to use
    /// * `target_bytes` - Target number of bytes to free
    ///
    /// # Returns
    /// Number of bytes actually freed
    fn evict_entries(&self, policy: EvictionPolicy, target_bytes: usize) -> Result<usize, ShadowError> {
        let victims = self.lru_tracker.select_victims(policy, &self.entries, target_bytes);
        let mut freed_bytes = 0;
        
        for path in victims {
            if let Some(entry) = self.remove(&path) {
                freed_bytes += calculate_entry_size(&entry);
                if freed_bytes >= target_bytes {
                    break;
                }
            }
        }
        
        Ok(freed_bytes)
    }
    
    /// Evicts the least recently used entry.
    ///
    /// # Returns
    /// The path that was evicted, if any
    pub fn evict_lru(&self) -> Option<ShadowPath> {
        let lru_paths = self.lru_tracker.get_least_recently_used(1);
        if let Some(path) = lru_paths.first() {
            self.remove(path);
            Some(path.clone())
        } else {
            None
        }
    }
    
    /// Inserts multiple entries in a batch operation.
    ///
    /// # Arguments
    /// * `entries` - Vector of (path, content) pairs to insert
    ///
    /// # Returns
    /// Ok(()) if all entries were inserted successfully, or the first error encountered
    pub fn insert_batch(&self, entries: Vec<(ShadowPath, OverrideContent)>) -> Result<(), ShadowError> {
        for (path, content) in entries {
            match content {
                OverrideContent::File { data, .. } => {
                    self.insert_file(path, data, None)?;
                }
                OverrideContent::Directory { .. } => {
                    self.insert_directory(path, None)?;
                }
                OverrideContent::Deleted => {
                    self.mark_deleted(path)?;
                }
            }
        }
        Ok(())
    }
    
    /// Removes multiple entries in a batch operation.
    ///
    /// # Arguments
    /// * `paths` - Slice of paths to remove
    ///
    /// # Returns
    /// Vector of removed entries (None for paths that didn't exist)
    pub fn remove_batch(&self, paths: &[ShadowPath]) -> Vec<Option<Arc<OverrideEntry>>> {
        paths.iter().map(|path| self.remove(path)).collect()
    }
    
    /// Updates the store configuration.
    ///
    /// # Arguments
    /// * `new_config` - New configuration to apply
    pub fn update_config(&self, new_config: OverrideStoreConfig) -> Result<(), ShadowError> {
        let mut config = self.config.write().unwrap();
        *config = new_config;
        Ok(())
    }
    
    /// Gets a copy of the current configuration.
    pub fn get_config(&self) -> OverrideStoreConfig {
        self.config.read().unwrap().clone()
    }
    
    /// Gets current memory usage statistics.
    pub fn memory_stats(&self) -> (usize, usize, f64) {
        let current = self.memory_tracker.current_usage();
        let max = self.config.read().unwrap().max_memory;
        let pressure = self.memory_tracker.get_pressure_ratio();
        (current, max, pressure)
    }
    
    /// Gets the number of entries in the store.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}