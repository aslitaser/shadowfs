//! Override entry types and content structures.

use crate::types::{FileMetadata, ShadowPath};
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Content stored in an override entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
    #[serde(with = "atomic_u64_serde")]
    pub last_accessed: AtomicU64,
}

/// Custom serialization for AtomicU64
mod atomic_u64_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub fn serialize<S>(atomic: &AtomicU64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        atomic.load(Ordering::Relaxed).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<AtomicU64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Ok(AtomicU64::new(value))
    }
}

impl Clone for OverrideEntry {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            content: self.content.clone(),
            original_metadata: self.original_metadata.clone(),
            override_metadata: self.override_metadata.clone(),
            created_at: self.created_at,
            last_accessed: AtomicU64::new(self.last_accessed.load(Ordering::Relaxed)),
        }
    }
}