//! High-performance in-memory storage for file and directory overrides.
//! 
//! The override store provides efficient caching and management of file system
//! overrides with advanced features like compression, deduplication, pattern
//! matching, and copy-on-write semantics.
//! 
//! # Quick Start
//! 
//! ```rust
//! use shadowfs_core::override_store::{OverrideStoreBuilder, EvictionPolicy, OverrideStore};
//! 
//! // Create a basic store
//! let store = OverrideStoreBuilder::new()
//!     .with_memory_limit(64 * 1024 * 1024) // 64MB
//!     .build()
//!     .expect("Failed to create store");
//! 
//! // Or use defaults
//! let store = OverrideStore::with_defaults();
//! ```
//! 
//! # Key Features
//! 
//! - **Memory Management**: Automatic eviction with configurable policies
//! - **Performance**: BLAKE3 content deduplication and LRU caching
//! - **Compression**: Transparent zstd compression for large files
//! - **Patterns**: Advanced pattern matching with transformations
//! - **Persistence**: Snapshot and WAL support for durability
//! - **Statistics**: Comprehensive monitoring and health checks
//! 
//! # Thread Safety
//! 
//! All operations are thread-safe. The store uses lock-free data structures
//! where possible and fine-grained locking elsewhere to maximize concurrency.

// Internal modules (private)
mod entry;
mod memory;
mod lru;
mod size;
mod directory;
mod persistence;
mod optimization;
mod stats;
mod patterns;
mod api;

// Public API exports
pub use api::{
    OverrideStoreBuilder, HealthStatus, ExportFormat, Migration
};

// Core types (public)
// OverrideStore and OverrideStoreConfig are defined below
pub use entry::{OverrideEntry, OverrideContent};
pub use lru::EvictionPolicy;
pub use optimization::PrefetchStrategy;
pub use stats::{
    OverrideStoreStats, StatsSnapshot, MemoryBreakdown, StatsReport,
    PerformanceMetrics, EfficiencyMetrics, AlertConfig, HotPathStats
};

// Pattern matching (public)
pub use patterns::{
    OverrideRule, RuleSet, RulePriority, TransformChain, TransformFn, transforms,
    OverrideCondition, OverrideTemplate, CowContent, ContentLoader, OverrideRuleEntry,
    OverrideContentType
};

// Advanced features (public but less common)
pub use persistence::{OverrideSnapshot, PersistenceConfig};
pub use optimization::{ContentDeduplication, compression};

// Internal utilities (kept private)
use memory::MemoryTracker;
use lru::LruTracker;
use size::calculate_entry_size;
use directory::{DirectoryCache, PathTraversal};
use optimization::{ReadThroughCache, DirectoryPrefetcher, ShardedMap};

use crate::types::{FileMetadata, ShadowPath, DirectoryEntry};
use crate::error::ShadowError;
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Configuration for the override store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverrideStoreConfig {
    /// Maximum memory usage in bytes
    pub max_memory: usize,
    
    /// Eviction policy to use when memory limits are reached
    pub eviction_policy: EvictionPolicy,
    
    /// Whether to enable memory pressure detection
    pub enable_memory_pressure: bool,
    
    /// Threshold for triggering eviction (0.0 to 1.0)
    pub eviction_threshold: f64,
    
    /// Size of the read-through cache
    pub cache_size: usize,
    
    /// Directory prefetch strategy
    pub prefetch_strategy: PrefetchStrategy,
    
    /// Whether to enable compression for large files
    pub enable_compression: bool,
}

impl Default for OverrideStoreConfig {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024, // 64MB default
            eviction_policy: EvictionPolicy::Lru,
            enable_memory_pressure: true,
            eviction_threshold: 0.9,
            cache_size: 1000,
            prefetch_strategy: PrefetchStrategy::Children,
            enable_compression: true,
        }
    }
}

/// Store for managing file and directory overrides with memory limits.
pub struct OverrideStore {
    /// Sharded map of path to override entries with Arc for zero-copy reads
    pub(crate) entries: Arc<ShardedMap<ShadowPath, Arc<OverrideEntry>>>,
    
    /// Memory tracker for allocation management
    pub(crate) memory_tracker: Arc<MemoryTracker>,
    
    /// LRU tracker for access patterns and eviction
    pub(crate) lru_tracker: Arc<LruTracker>,
    
    /// Directory cache for parent-child relationships
    pub(crate) directory_cache: Arc<DirectoryCache>,
    
    /// Content deduplication system
    pub(crate) content_dedup: Arc<ContentDeduplication>,
    
    /// Read-through cache for hot entries
    pub(crate) hot_cache: Arc<ReadThroughCache<OverrideEntry>>,
    
    /// Directory prefetcher
    pub(crate) prefetcher: Arc<RwLock<DirectoryPrefetcher>>,
    
    /// Statistics tracker
    pub(crate) stats: Arc<OverrideStoreStats>,
    
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
        let directory_cache = Arc::new(DirectoryCache::new());
        let entries = Arc::new(ShardedMap::new());
        let content_dedup = Arc::new(ContentDeduplication::new());
        let hot_cache = Arc::new(ReadThroughCache::new(config.cache_size));
        let prefetcher = Arc::new(RwLock::new(DirectoryPrefetcher::new(config.prefetch_strategy)));
        let stats = Arc::new(OverrideStoreStats::new());
        
        Self {
            entries,
            memory_tracker,
            lru_tracker,
            directory_cache,
            content_dedup,
            hot_cache,
            prefetcher,
            stats,
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
        let config = self.config.read().unwrap();
        let enable_compression = config.enable_compression;
        drop(config);
        
        let original_size = content.len() as u64;
        let mut data = content;
        let mut is_compressed = false;
        
        // Apply compression if enabled and content is large enough
        if enable_compression && compression::should_compress(&data) {
            match compression::compress(&data) {
                Ok(compressed) => {
                    data = compressed;
                    is_compressed = true;
                }
                Err(_) => {
                    // Fall back to uncompressed if compression fails
                }
            }
        }
        
        // Use BLAKE3 for content deduplication
        let (content_hash, dedup_data) = self.content_dedup.store_content(data.clone());
        
        let override_content = OverrideContent::File {
            data: (*dedup_data).clone(),
            content_hash,
            is_compressed,
        };
        
        let override_metadata = FileMetadata {
            size: original_size, // Store original uncompressed size
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
    pub(crate) fn insert_entry(
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
        let old_entry = self.entries.insert(path.clone(), entry_arc.clone());
        
        // Calculate stats for the new entry
        let compression_saved = match &entry_arc.content {
            OverrideContent::File { is_compressed, .. } if *is_compressed => {
                // Estimate compression savings (would be more accurate with actual data)
                entry_size / 4  // Assume 25% compression savings
            },
            _ => 0,
        };
        
        let dedup_saved = 0; // Would need actual dedup tracking
        
        // If this is a new entry (not a replacement), allocate memory and update stats
        if old_entry.is_none() {
            let _guard = self.memory_tracker.try_allocate(entry_size)?;
            std::mem::forget(_guard); // Keep the allocation
            
            // Update stats for new entry
            self.stats.update_on_insert(&entry_arc, entry_size, compression_saved, dedup_saved);
        } else {
            // For replacements, we need to handle the stats differently
            if let Some(old) = &old_entry {
                let old_size = calculate_entry_size(old);
                let old_compression_saved = match &old.content {
                    OverrideContent::File { is_compressed, .. } if *is_compressed => old_size / 4,
                    _ => 0,
                };
                
                // Remove old entry stats
                self.stats.update_on_remove(old, old_size, old_compression_saved, 0);
                // Add new entry stats
                self.stats.update_on_insert(&entry_arc, entry_size, compression_saved, dedup_saved);
            }
        }
        
        // Update LRU tracker
        self.lru_tracker.record_access(&path);
        
        // Update directory cache if this is a new entry
        if old_entry.is_none() {
            if let Some(parent) = path.parent() {
                let filename = PathTraversal::get_filename(&path);
                if !filename.is_empty() {
                    self.directory_cache.add_child(&parent, &filename);
                }
            }
        }
        
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
        // Check hot cache first
        if let Some(entry) = self.hot_cache.get(path) {
            // Cache hit!
            self.stats.update_cache_access(true);
            
            // Update hot path tracking
            let bytes = entry.override_metadata.size;
            self.stats.update_hot_path_access(path, bytes);
            
            // Update LRU tracker on access
            self.lru_tracker.record_access(path);
            
            // Update last accessed time
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entry.last_accessed.store(now, Ordering::Relaxed);
            
            return Some(entry);
        }
        
        // Check main store
        if let Some(entry) = self.entries.get(path) {
            let entry_arc = entry.clone();
            
            // Cache miss, but found in main store
            self.stats.update_cache_access(false);
            
            // Update hot path tracking
            let bytes = entry_arc.override_metadata.size;
            self.stats.update_hot_path_access(path, bytes);
            
            // Add to hot cache
            self.hot_cache.put(path.clone(), entry_arc.clone());
            
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
            // Complete miss - not in cache or main store
            self.stats.update_cache_access(false);
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
            // Calculate removal stats
            let entry_size = calculate_entry_size(&entry);
            let compression_saved = match &entry.content {
                OverrideContent::File { is_compressed, .. } if *is_compressed => entry_size / 4,
                _ => 0,
            };
            
            // Update stats for removal
            self.stats.update_on_remove(&entry, entry_size, compression_saved, 0);
            
            // Remove from hot cache
            self.hot_cache.remove(path);
            
            // Remove from LRU tracker
            self.lru_tracker.remove_entry(path);
            
            // Remove from directory cache
            if let Some(parent) = path.parent() {
                let filename = PathTraversal::get_filename(path);
                if !filename.is_empty() {
                    self.directory_cache.remove_child(&parent, &filename);
                }
            }
            
            // Handle content deduplication cleanup if this was a file
            if let OverrideContent::File { .. } = &entry.content {
                // Note: In a full implementation, we'd need reference counting
                // to know when to actually remove from dedup store
                // For now, we leave it to avoid breaking other references
            }
            
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
    fn evict_entries(&self, _policy: EvictionPolicy, target_bytes: usize) -> Result<usize, ShadowError> {
        // For now, use a simple LRU eviction without complex victim selection
        let lru_paths = self.lru_tracker.get_least_recently_used(10); // Get up to 10 candidates
        let victims = lru_paths;
        let mut freed_bytes = 0;
        
        let mut evicted_count = 0;
        for path in victims {
            if let Some(entry) = self.remove(&path) {
                let entry_size = calculate_entry_size(&entry);
                freed_bytes += entry_size;
                evicted_count += 1;
                if freed_bytes >= target_bytes {
                    break;
                }
            }
        }
        
        // Update eviction stats
        if evicted_count > 0 {
            self.stats.update_on_eviction(evicted_count, freed_bytes);
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
                OverrideContent::File { data, is_compressed, .. } => {
                    // For batch insert, use the original data and let insert_file handle optimization
                    let original_data = if is_compressed {
                        // If data was compressed, decompress it first
                        compression::decompress(&data)
                            .map_err(|e| ShadowError::IoError { 
                                source: e 
                            })?
                    } else {
                        data
                    };
                    self.insert_file(path, original_data, None)?;
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
    
    // === Directory Operations ===
    
    /// Creates a directory hierarchy, ensuring all parent directories exist.
    ///
    /// # Arguments
    /// * `path` - Path to create (all parent directories will be created if needed)
    ///
    /// # Returns
    /// Ok(()) on success, or an error if memory limits would be exceeded
    pub fn create_directory_hierarchy(&self, path: &ShadowPath) -> Result<(), ShadowError> {
        let parent_chain = PathTraversal::get_parent_chain(path);
        
        // Create parents from root to immediate parent
        for parent_path in parent_chain.iter().rev() {
            if !self.exists(parent_path) {
                self.insert_directory(parent_path.clone(), None)?;
            }
        }
        
        // Create the target directory if it doesn't exist
        if !self.exists(path) {
            self.insert_directory(path.clone(), None)?;
        }
        
        Ok(())
    }
    
    /// Lists the contents of a directory, merging override entries.
    ///
    /// # Arguments
    /// * `path` - Directory path to list
    ///
    /// # Returns
    /// Vector of directory entries, or an error if the path is not a directory
    pub fn list_directory(&self, path: &ShadowPath) -> Result<Vec<DirectoryEntry>, ShadowError> {
        // Check if this is a directory in our overrides
        if let Some(entry) = self.get(path) {
            match &entry.content {
                OverrideContent::Directory { .. } => {
                    // It's a directory override, get children from cache
                    let children = self.directory_cache.get_children(path);
                    let mut entries = Vec::new();
                    
                    // Apply prefetching strategy
                    let prefetcher = self.prefetcher.read().unwrap();
                    let prefetch_paths = prefetcher.get_prefetch_paths(path, &children);
                    drop(prefetcher);
                    
                    // Prefetch likely-to-be-accessed children
                    for prefetch_path in prefetch_paths {
                        // Trigger a get to load into hot cache
                        self.get(&prefetch_path);
                    }
                    
                    for child_name in children {
                        let child_path = path.join(&child_name);
                        
                        if let Some(child_entry) = self.get(&child_path) {
                            // Skip deleted entries
                            if matches!(child_entry.content, OverrideContent::Deleted) {
                                continue;
                            }
                            
                            let entry = DirectoryEntry {
                                name: child_name,
                                metadata: child_entry.override_metadata.clone(),
                            };
                            entries.push(entry);
                        }
                    }
                    
                    Ok(entries)
                }
                OverrideContent::Deleted => {
                    Err(ShadowError::NotFound {
                        path: path.clone(),
                    })
                }
                OverrideContent::File { .. } => {
                    Err(ShadowError::NotADirectory {
                        path: path.clone(),
                    })
                }
            }
        } else {
            // No override, would need to check underlying filesystem
            // For now, return empty list for non-existent directories
            Ok(Vec::new())
        }
    }
    
    /// Checks if a directory is empty (has no children).
    ///
    /// # Arguments
    /// * `path` - Directory path to check
    ///
    /// # Returns
    /// true if the directory exists and is empty, false otherwise
    pub fn is_empty_directory(&self, path: &ShadowPath) -> bool {
        if let Some(entry) = self.get(path) {
            match &entry.content {
                OverrideContent::Directory { .. } => {
                    // Check if directory has any non-deleted children
                    let children = self.directory_cache.get_children(path);
                    for child_name in children {
                        let child_path = path.join(&child_name);
                        if let Some(child_entry) = self.get(&child_path) {
                            if !matches!(child_entry.content, OverrideContent::Deleted) {
                                return false;
                            }
                        }
                    }
                    true
                }
                _ => false, // Not a directory
            }
        } else {
            false // Directory doesn't exist
        }
    }
    
    /// Recursively deletes a directory and all its contents.
    ///
    /// # Arguments
    /// * `path` - Directory path to delete
    ///
    /// # Returns
    /// Vector of paths that were deleted
    pub fn delete_directory_recursive(&self, path: &ShadowPath) -> Result<Vec<ShadowPath>, ShadowError> {
        let mut deleted_paths = Vec::new();
        
        // Find all affected children (direct and indirect)
        let all_paths: Vec<ShadowPath> = self.entries.iter()
            .map(|entry| entry.key().clone())
            .collect();
        
        let affected_children = PathTraversal::find_affected_children(path, &all_paths);
        
        // Delete children first (depth-first)
        for child_path in affected_children {
            if self.remove(&child_path).is_some() {
                deleted_paths.push(child_path);
            }
        }
        
        // Delete the directory itself by marking it as deleted
        self.mark_deleted(path.clone())?;
        deleted_paths.push(path.clone());
        
        Ok(deleted_paths)
    }
    
    /// Cleans up empty parent directories after deletion.
    ///
    /// # Arguments
    /// * `path` - Starting path to clean up parents from
    pub fn cleanup_empty_parents(&self, path: &ShadowPath) {
        let parent_chain = PathTraversal::get_parent_chain(path);
        
        for parent_path in parent_chain {
            if self.is_empty_directory(&parent_path) {
                // Only remove if it was an override (not from underlying filesystem)
                if let Some(parent_entry) = self.get(&parent_path) {
                    if matches!(parent_entry.content, OverrideContent::Directory { .. }) {
                        self.remove(&parent_path);
                    }
                }
            } else {
                // Stop at first non-empty parent
                break;
            }
        }
    }
    
    /// Gets all paths under a given directory path.
    ///
    /// # Arguments
    /// * `path` - Directory path to find children for
    ///
    /// # Returns
    /// Vector of all child paths (direct and indirect)
    pub fn get_children_recursive(&self, path: &ShadowPath) -> Vec<ShadowPath> {
        let all_paths: Vec<ShadowPath> = self.entries.iter()
            .map(|entry| entry.key().clone())
            .collect();
        
        PathTraversal::find_affected_children(path, &all_paths)
    }
    
    /// Gets statistics about the directory cache.
    ///
    /// # Returns
    /// (directory_count, total_child_count)
    pub fn directory_stats(&self) -> (usize, usize) {
        (
            self.directory_cache.directory_count(),
            self.directory_cache.total_child_count()
        )
    }
    
    /// Gets all parent directories being tracked in the cache.
    ///
    /// # Returns
    /// Vector of all parent directory paths
    pub fn get_all_parent_directories(&self) -> Vec<ShadowPath> {
        self.directory_cache.get_all_parents()
    }
    
    /// Gets children of a specific directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    ///
    /// # Returns
    /// Vector of child names
    pub fn get_directory_children(&self, parent: &ShadowPath) -> Vec<String> {
        self.directory_cache.get_children(parent)
    }
    
    /// Gets optimization statistics.
    ///
    /// # Returns
    /// Tuple of (dedup_entries, dedup_bytes, cache_entries, cache_capacity)
    pub fn optimization_stats(&self) -> (usize, usize, usize, usize) {
        let (dedup_entries, dedup_bytes) = self.content_dedup.stats();
        let (cache_entries, cache_capacity) = self.hot_cache.stats();
        (dedup_entries, dedup_bytes, cache_entries, cache_capacity)
    }
    
    /// Updates the prefetch strategy.
    ///
    /// # Arguments
    /// * `strategy` - New prefetch strategy
    pub fn set_prefetch_strategy(&self, strategy: PrefetchStrategy) {
        let mut prefetcher = self.prefetcher.write().unwrap();
        prefetcher.set_strategy(strategy);
    }
    
    /// Gets the current prefetch strategy.
    ///
    /// # Returns
    /// Current prefetch strategy
    pub fn get_prefetch_strategy(&self) -> PrefetchStrategy {
        let prefetcher = self.prefetcher.read().unwrap();
        prefetcher.strategy()
    }
    
    /// Clears the hot cache.
    pub fn clear_cache(&self) {
        self.hot_cache.clear();
    }
    
    /// Gets detailed performance metrics.
    ///
    /// # Returns
    /// String containing formatted performance statistics
    pub fn performance_report(&self) -> String {
        let (current_memory, max_memory, pressure) = self.memory_stats();
        let (dedup_entries, dedup_bytes, cache_entries, cache_capacity) = self.optimization_stats();
        let (dir_count, total_children) = self.directory_stats();
        let entry_count = self.entry_count();
        
        format!(
            "OverrideStore Performance Report:\n\
             Memory: {}/{} bytes ({:.1}% pressure)\n\
             Entries: {} total, {} sharded\n\
             Deduplication: {} unique contents, {} bytes saved\n\
             Hot Cache: {}/{} entries\n\
             Directories: {} tracked, {} total children\n\
             Prefetch Strategy: {:?}",
            current_memory, max_memory, pressure * 100.0,
            entry_count, entry_count,
            dedup_entries, dedup_bytes,
            cache_entries, cache_capacity,
            dir_count, total_children,
            self.get_prefetch_strategy()
        )
    }
    
    /// Gets comprehensive statistics report.
    ///
    /// # Returns
    /// Detailed statistics report with performance metrics
    pub fn get_stats_report(&self) -> StatsReport {
        self.stats.generate_report()
    }
    
    /// Gets current statistics snapshot.
    ///
    /// # Returns
    /// Current statistics values
    pub fn get_stats_snapshot(&self) -> StatsSnapshot {
        self.stats.get_snapshot()
    }
    
    /// Gets memory usage breakdown.
    ///
    /// # Returns
    /// Detailed memory breakdown
    pub fn get_memory_breakdown(&self) -> MemoryBreakdown {
        self.stats.get_memory_breakdown()
    }
    
    /// Gets hot paths (most accessed paths).
    ///
    /// # Arguments
    /// * `limit` - Maximum number of paths to return
    ///
    /// # Returns
    /// Vector of hot paths with their access statistics
    pub fn get_hot_paths(&self, limit: usize) -> Vec<(ShadowPath, HotPathStats)> {
        self.stats.get_hot_paths(limit)
    }
    
    /// Registers a callback for statistics changes.
    ///
    /// # Arguments
    /// * `callback` - Callback function to be called when stats change
    pub fn register_stats_callback<F>(&self, callback: F) 
    where 
        F: Fn(&StatsSnapshot) + Send + Sync + 'static 
    {
        self.stats.register_callback(callback);
    }
    
    /// Updates alert configuration for monitoring.
    ///
    /// # Arguments
    /// * `config` - New alert configuration
    pub fn update_alert_config(&self, config: AlertConfig) {
        self.stats.update_alert_config(config);
    }
    
    /// Resets all statistics.
    pub fn reset_stats(&self) {
        self.stats.reset();
    }
}