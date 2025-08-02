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
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize};
use std::sync::{Arc, Mutex};

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
    ) -> Result<(), ShadowError> {
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