//! Override entry types and content structures.

use crate::types::{FileMetadata, ShadowPath};
use bytes::Bytes;
use std::sync::atomic::AtomicU64;
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