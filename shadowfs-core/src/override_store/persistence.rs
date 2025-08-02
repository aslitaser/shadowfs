//! Persistence layer for override store with snapshots and write-ahead logging.

use crate::types::{FileMetadata, ShadowPath};
use crate::error::ShadowError;
use crate::override_store::{OverrideStore, OverrideStoreConfig, OverrideEntry, OverrideContent};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Operations that can be persisted to the write-ahead log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PersistenceOp {
    /// Insert or update an entry
    Insert {
        path: ShadowPath,
        content: OverrideContent,
        metadata: FileMetadata,
        timestamp: u64,
    },
    /// Remove an entry
    Remove {
        path: ShadowPath,
        timestamp: u64,
    },
    /// Clear all entries
    Clear {
        timestamp: u64,
    },
    /// Mark for snapshot (internal operation)
    Snapshot {
        timestamp: u64,
    },
}

impl PersistenceOp {
    /// Creates a new Insert operation with current timestamp.
    pub fn insert(path: ShadowPath, content: OverrideContent, metadata: FileMetadata) -> Self {
        Self::Insert {
            path,
            content,
            metadata,
            timestamp: current_timestamp(),
        }
    }
    
    /// Creates a new Remove operation with current timestamp.
    pub fn remove(path: ShadowPath) -> Self {
        Self::Remove {
            path,
            timestamp: current_timestamp(),
        }
    }
    
    /// Creates a new Clear operation with current timestamp.
    pub fn clear() -> Self {
        Self::Clear {
            timestamp: current_timestamp(),
        }
    }
    
    /// Creates a new Snapshot operation with current timestamp.
    pub fn snapshot() -> Self {
        Self::Snapshot {
            timestamp: current_timestamp(),
        }
    }
    
    /// Returns the timestamp of this operation.
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::Insert { timestamp, .. } => *timestamp,
            Self::Remove { timestamp, .. } => *timestamp,
            Self::Clear { timestamp } => *timestamp,
            Self::Snapshot { timestamp } => *timestamp,
        }
    }
}

/// Serializable snapshot of the override store state.
#[derive(Debug, Serialize, Deserialize)]
pub struct OverrideSnapshot {
    /// Store configuration
    pub config: OverrideStoreConfig,
    /// All entries in the store
    pub entries: HashMap<ShadowPath, OverrideEntry>,
    /// Directory cache relationships
    pub directory_children: HashMap<ShadowPath, Vec<String>>,
    /// Snapshot timestamp
    pub timestamp: u64,
    /// Checksum for integrity verification
    pub checksum: u64,
}

impl OverrideSnapshot {
    /// Creates a new snapshot from an override store.
    pub fn from_store(store: &OverrideStore) -> Self {
        let config = store.get_config();
        let timestamp = current_timestamp();
        
        // Extract all entries
        let entries: HashMap<ShadowPath, OverrideEntry> = store.entries
            .iter()
            .map(|entry| {
                let path = entry.key().clone();
                let override_entry = (**entry.value()).clone();
                (path, override_entry)
            })
            .collect();
        
        // Extract directory cache state
        let directory_children: HashMap<ShadowPath, Vec<String>> = store.get_all_parent_directories()
            .into_iter()
            .map(|parent| {
                let children = store.get_directory_children(&parent);
                (parent, children)
            })
            .collect();
        
        let mut snapshot = Self {
            config,
            entries,
            directory_children,
            timestamp,
            checksum: 0,
        };
        
        // Calculate checksum
        snapshot.checksum = snapshot.calculate_checksum();
        snapshot
    }
    
    /// Calculates a checksum for integrity verification.
    fn calculate_checksum(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash configuration
        format!("{:?}", self.config).hash(&mut hasher);
        
        // Hash entries (sorted for deterministic checksum)
        let mut sorted_entries: Vec<_> = self.entries.iter().collect();
        sorted_entries.sort_by_key(|(path, _)| path.to_string());
        for (path, entry) in sorted_entries {
            path.to_string().hash(&mut hasher);
            format!("{:?}", entry).hash(&mut hasher);
        }
        
        // Hash directory relationships (sorted for deterministic checksum)
        let mut sorted_dirs: Vec<_> = self.directory_children.iter().collect();
        sorted_dirs.sort_by_key(|(path, _)| path.to_string());
        for (path, children) in sorted_dirs {
            path.to_string().hash(&mut hasher);
            let mut sorted_children = children.clone();
            sorted_children.sort();
            sorted_children.hash(&mut hasher);
        }
        
        // Hash timestamp
        self.timestamp.hash(&mut hasher);
        
        hasher.finish()
    }
    
    /// Verifies the snapshot integrity.
    pub fn verify_integrity(&self) -> bool {
        let calculated_checksum = self.calculate_checksum();
        calculated_checksum == self.checksum
    }
    
    /// Restores an override store from this snapshot.
    pub fn restore_to_store(&self) -> Result<OverrideStore, ShadowError> {
        if !self.verify_integrity() {
            return Err(ShadowError::PlatformError {
                platform: crate::error::Platform::Linux, // Use current platform
                message: "Snapshot integrity check failed".to_string(),
                code: None,
            });
        }
        
        let store = OverrideStore::new(self.config.clone());
        
        // Restore entries
        for (path, entry) in &self.entries {
            let entry_arc = Arc::new(entry.clone());
            store.entries.insert(path.clone(), entry_arc);
            
            // Update LRU tracker
            store.lru_tracker.record_access(path);
        }
        
        // Restore directory cache
        for (parent, children) in &self.directory_children {
            for child_name in children {
                store.directory_cache.add_child(parent, child_name);
            }
        }
        
        Ok(store)
    }
}

/// Persistence configuration and settings.
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Path to store snapshots
    pub snapshot_path: PathBuf,
    /// Path to store write-ahead log
    pub wal_path: PathBuf,
    /// Enable compression (zstd)
    pub enable_compression: bool,
    /// Compression level (1-22 for zstd)
    pub compression_level: i32,
    /// Maximum WAL size before triggering snapshot
    pub max_wal_size: usize,
    /// Interval between automatic snapshots (in seconds)
    pub snapshot_interval: u64,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            snapshot_path: PathBuf::from("shadowfs_snapshot.bin"),
            wal_path: PathBuf::from("shadowfs_wal.log"),
            enable_compression: true,
            compression_level: 3, // Balanced compression/speed
            max_wal_size: 64 * 1024 * 1024, // 64MB
            snapshot_interval: 3600, // 1 hour
        }
    }
}

/// Trait for persisting override store state.
#[async_trait]
pub trait OverridePersistence: Send + Sync {
    /// Saves a complete snapshot of the store state.
    async fn save_snapshot(&self, store: &OverrideStore) -> Result<(), ShadowError>;
    
    /// Loads a store from the most recent snapshot.
    async fn load_snapshot(&self) -> Result<OverrideStore, ShadowError>;
    
    /// Appends an operation to the write-ahead log.
    async fn append_operation(&self, op: PersistenceOp) -> Result<(), ShadowError>;
    
    /// Replays operations from the WAL to update the store.
    async fn replay_operations(&self, store: &OverrideStore, from_timestamp: u64) -> Result<(), ShadowError>;
    
    /// Compacts the WAL by creating a new snapshot and truncating the log.
    async fn compact(&self, store: &OverrideStore) -> Result<(), ShadowError>;
    
    /// Checks if a snapshot exists.
    async fn snapshot_exists(&self) -> bool;
    
    /// Checks if a WAL exists and returns its size.
    async fn wal_info(&self) -> Result<Option<u64>, ShadowError>;
}

/// File-based persistence implementation with compression and checksums.
pub struct FileBasedPersistence {
    config: PersistenceConfig,
}

impl FileBasedPersistence {
    /// Creates a new file-based persistence with the given configuration.
    pub fn new(config: PersistenceConfig) -> Self {
        Self { config }
    }
    
    /// Creates a new file-based persistence with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(PersistenceConfig::default())
    }
    
    /// Compresses data using zstd if compression is enabled.
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, ShadowError> {
        if self.config.enable_compression {
            zstd::encode_all(data, self.config.compression_level)
                .map_err(|e| ShadowError::PlatformError {
                    platform: crate::error::Platform::Linux,
                    message: format!("Compression failed: {}", e),
                    code: None,
                })
        } else {
            Ok(data.to_vec())
        }
    }
    
    /// Decompresses data using zstd if compression is enabled.
    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>, ShadowError> {
        if self.config.enable_compression {
            zstd::decode_all(data)
                .map_err(|e| ShadowError::PlatformError {
                    platform: crate::error::Platform::Linux,
                    message: format!("Decompression failed: {}", e),
                    code: None,
                })
        } else {
            Ok(data.to_vec())
        }
    }
    
    /// Serializes data using bincode.
    fn serialize<T: Serialize>(&self, data: &T) -> Result<Vec<u8>, ShadowError> {
        bincode::serialize(data)
            .map_err(|e| ShadowError::PlatformError {
                platform: crate::error::Platform::Linux,
                message: format!("Serialization failed: {}", e),
                code: None,
            })
    }
    
    /// Deserializes data using bincode.
    fn deserialize<T: for<'de> Deserialize<'de>>(&self, data: &[u8]) -> Result<T, ShadowError> {
        bincode::deserialize(data)
            .map_err(|e| ShadowError::PlatformError {
                platform: crate::error::Platform::Linux,
                message: format!("Deserialization failed: {}", e),
                code: None,
            })
    }
}

#[async_trait]
impl OverridePersistence for FileBasedPersistence {
    async fn save_snapshot(&self, store: &OverrideStore) -> Result<(), ShadowError> {
        let snapshot = OverrideSnapshot::from_store(store);
        
        // Serialize snapshot
        let serialized = self.serialize(&snapshot)?;
        
        // Compress if enabled
        let compressed = self.compress_data(&serialized)?;
        
        // Write to file atomically
        let temp_path = self.config.snapshot_path.with_extension("tmp");
        let mut file = File::create(&temp_path).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        file.write_all(&compressed).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        file.sync_all().await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        // Atomic rename
        tokio::fs::rename(temp_path, &self.config.snapshot_path).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        Ok(())
    }
    
    async fn load_snapshot(&self) -> Result<OverrideStore, ShadowError> {
        let mut file = File::open(&self.config.snapshot_path).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        // Decompress if enabled
        let serialized = self.decompress_data(&compressed)?;
        
        // Deserialize snapshot
        let snapshot: OverrideSnapshot = self.deserialize(&serialized)?;
        
        // Restore store from snapshot
        snapshot.restore_to_store()
    }
    
    async fn append_operation(&self, op: PersistenceOp) -> Result<(), ShadowError> {
        // Serialize operation
        let serialized = self.serialize(&op)?;
        
        // Create operation entry with length prefix
        let op_len = serialized.len() as u32;
        let mut entry = Vec::with_capacity(4 + serialized.len() + 8);
        entry.extend_from_slice(&op_len.to_le_bytes());
        entry.extend_from_slice(&serialized);
        
        // Add checksum
        let checksum = crc32fast::hash(&serialized);
        entry.extend_from_slice(&checksum.to_le_bytes());
        
        // Append to WAL
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.wal_path)
            .await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        file.write_all(&entry).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        file.sync_all().await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        Ok(())
    }
    
    async fn replay_operations(&self, store: &OverrideStore, from_timestamp: u64) -> Result<(), ShadowError> {
        let mut file = match File::open(&self.config.wal_path).await {
            Ok(file) => file,
            Err(_) => return Ok(()), // No WAL file exists
        };
        
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        let mut offset = 0;
        
        while offset + 8 < buffer.len() {
            // Read length prefix
            let op_len = u32::from_le_bytes([
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ]) as usize;
            offset += 4;
            
            if offset + op_len + 4 > buffer.len() {
                // Incomplete entry, stop replay
                break;
            }
            
            // Read operation data
            let op_data = &buffer[offset..offset + op_len];
            offset += op_len;
            
            // Read and verify checksum
            let stored_checksum = u32::from_le_bytes([
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ]);
            offset += 4;
            
            let calculated_checksum = crc32fast::hash(op_data);
            if stored_checksum != calculated_checksum {
                return Err(ShadowError::PlatformError {
                    platform: crate::error::Platform::Linux,
                    message: "WAL corruption detected: checksum mismatch".to_string(),
                    code: None,
                });
            }
            
            // Deserialize and apply operation
            let op: PersistenceOp = self.deserialize(op_data)?;
            
            // Skip operations before the timestamp
            if op.timestamp() < from_timestamp {
                continue;
            }
            
            // Apply operation to store
            match op {
                PersistenceOp::Insert { path, content, metadata, .. } => {
                    let _ = store.insert_entry(path, content, None, metadata);
                }
                PersistenceOp::Remove { path, .. } => {
                    store.remove(&path);
                }
                PersistenceOp::Clear { .. } => {
                    // Clear all entries
                    let all_paths: Vec<_> = store.entries.iter()
                        .map(|entry| entry.key().clone())
                        .collect();
                    for path in all_paths {
                        store.remove(&path);
                    }
                }
                PersistenceOp::Snapshot { .. } => {
                    // Snapshot markers are informational only
                }
            }
        }
        
        Ok(())
    }
    
    async fn compact(&self, store: &OverrideStore) -> Result<(), ShadowError> {
        // Save a new snapshot
        self.save_snapshot(store).await?;
        
        // Add snapshot marker to WAL
        self.append_operation(PersistenceOp::snapshot()).await?;
        
        // Truncate WAL (create new empty file)
        let _file = File::create(&self.config.wal_path).await
            .map_err(|e| ShadowError::IoError { source: e })?;
        
        Ok(())
    }
    
    async fn snapshot_exists(&self) -> bool {
        tokio::fs::metadata(&self.config.snapshot_path).await.is_ok()
    }
    
    async fn wal_info(&self) -> Result<Option<u64>, ShadowError> {
        match tokio::fs::metadata(&self.config.wal_path).await {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(_) => Ok(None),
        }
    }
}

/// Helper function to get current Unix timestamp.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileType, FilePermissions, PlatformMetadata};
    use bytes::Bytes;
    use tempfile::tempdir;
    
    #[test]
    fn test_persistence_op_creation() {
        let path = ShadowPath::new("/test".into());
        let content = OverrideContent::File {
            data: Bytes::from("test data"),
            content_hash: [0u8; 32],
        };
        let metadata = FileMetadata {
            size: 9,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: FilePermissions::default_file(),
            file_type: FileType::File,
            platform_specific: PlatformMetadata::default(),
        };
        
        let insert_op = PersistenceOp::insert(path.clone(), content, metadata);
        let remove_op = PersistenceOp::remove(path.clone());
        let clear_op = PersistenceOp::clear();
        let snapshot_op = PersistenceOp::snapshot();
        
        assert!(insert_op.timestamp() > 0);
        assert!(remove_op.timestamp() > 0);
        assert!(clear_op.timestamp() > 0);
        assert!(snapshot_op.timestamp() > 0);
    }
    
    #[test]
    fn test_override_snapshot_integrity() {
        let store = OverrideStore::with_defaults();
        let snapshot = OverrideSnapshot::from_store(&store);
        
        assert!(snapshot.verify_integrity());
        
        // Test restoration
        let restored_store = snapshot.restore_to_store().unwrap();
        assert_eq!(restored_store.entry_count(), store.entry_count());
    }
    
    #[test]
    fn test_snapshot_with_data() {
        let store = OverrideStore::with_defaults();
        
        // Add some test data
        let path = ShadowPath::new("/test/file.txt".into());
        let content = Bytes::from("test content");
        store.insert_file(path.clone(), content, None).unwrap();
        
        let snapshot = OverrideSnapshot::from_store(&store);
        assert!(snapshot.verify_integrity());
        assert_eq!(snapshot.entries.len(), 1);
        
        // Test restoration
        let restored_store = snapshot.restore_to_store().unwrap();
        assert_eq!(restored_store.entry_count(), 1);
        assert!(restored_store.exists(&path));
    }
    
    #[tokio::test]
    async fn test_file_based_persistence_snapshot() {
        let temp_dir = tempdir().unwrap();
        let config = PersistenceConfig {
            snapshot_path: temp_dir.path().join("test_snapshot.bin"),
            wal_path: temp_dir.path().join("test_wal.log"),
            enable_compression: true,
            compression_level: 1,
            max_wal_size: 1024 * 1024,
            snapshot_interval: 3600,
        };
        
        let persistence = FileBasedPersistence::new(config);
        let store = OverrideStore::with_defaults();
        
        // Add test data
        let path = ShadowPath::new("/test/file.txt".into());
        let content = Bytes::from("test content for persistence");
        store.insert_file(path.clone(), content, None).unwrap();
        
        // Save snapshot
        persistence.save_snapshot(&store).await.unwrap();
        assert!(persistence.snapshot_exists().await);
        
        // Load snapshot
        let loaded_store = persistence.load_snapshot().await.unwrap();
        assert_eq!(loaded_store.entry_count(), 1);
        assert!(loaded_store.exists(&path));
    }
    
    #[tokio::test]
    async fn test_file_based_persistence_wal() {
        let temp_dir = tempdir().unwrap();
        let config = PersistenceConfig {
            snapshot_path: temp_dir.path().join("test_snapshot.bin"),
            wal_path: temp_dir.path().join("test_wal.log"),
            enable_compression: false,
            compression_level: 1,
            max_wal_size: 1024 * 1024,
            snapshot_interval: 3600,
        };
        
        let persistence = FileBasedPersistence::new(config);
        
        // Test WAL operations
        let path = ShadowPath::new("/test/file.txt".into());
        let content = OverrideContent::File {
            data: Bytes::from("test data"),
            content_hash: [0u8; 32],
        };
        let metadata = FileMetadata {
            size: 9,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: FilePermissions::default_file(),
            file_type: FileType::File,
            platform_specific: PlatformMetadata::default(),
        };
        
        let insert_op = PersistenceOp::insert(path.clone(), content, metadata);
        let remove_op = PersistenceOp::remove(path.clone());
        
        // Append operations
        persistence.append_operation(insert_op).await.unwrap();
        persistence.append_operation(remove_op).await.unwrap();
        
        // Check WAL info
        let wal_info = persistence.wal_info().await.unwrap();
        assert!(wal_info.is_some());
        assert!(wal_info.unwrap() > 0);
    }
    
    #[tokio::test]
    async fn test_persistence_replay() {
        let temp_dir = tempdir().unwrap();
        let config = PersistenceConfig {
            snapshot_path: temp_dir.path().join("test_snapshot.bin"),
            wal_path: temp_dir.path().join("test_wal.log"),
            enable_compression: false,
            compression_level: 1,
            max_wal_size: 1024 * 1024,
            snapshot_interval: 3600,
        };
        
        let persistence = FileBasedPersistence::new(config);
        let store = OverrideStore::with_defaults();
        
        // Create operations
        let path = ShadowPath::new("/test/file.txt".into());
        let content = OverrideContent::File {
            data: Bytes::from("test data"),
            content_hash: [0u8; 32],
        };
        let metadata = FileMetadata {
            size: 9,
            created: SystemTime::now(),
            modified: SystemTime::now(),
            accessed: SystemTime::now(),
            permissions: FilePermissions::default_file(),
            file_type: FileType::File,
            platform_specific: PlatformMetadata::default(),
        };
        
        let insert_op = PersistenceOp::insert(path.clone(), content, metadata);
        
        // Append to WAL
        persistence.append_operation(insert_op).await.unwrap();
        
        // Replay operations
        persistence.replay_operations(&store, 0).await.unwrap();
        
        // Verify the entry was restored
        assert!(store.exists(&path));
    }
    
    #[tokio::test]
    async fn test_persistence_compaction() {
        let temp_dir = tempdir().unwrap();
        let config = PersistenceConfig {
            snapshot_path: temp_dir.path().join("test_snapshot.bin"),
            wal_path: temp_dir.path().join("test_wal.log"),
            enable_compression: true,
            compression_level: 3,
            max_wal_size: 1024 * 1024,
            snapshot_interval: 3600,
        };
        
        let persistence = FileBasedPersistence::new(config);
        let store = OverrideStore::with_defaults();
        
        // Add test data
        let path = ShadowPath::new("/test/file.txt".into());
        let content = Bytes::from("test content for compaction");
        store.insert_file(path.clone(), content, None).unwrap();
        
        // Add some WAL operations
        let remove_op = PersistenceOp::remove(ShadowPath::new("/old/file".into()));
        persistence.append_operation(remove_op).await.unwrap();
        
        // Check WAL exists and has content
        let wal_info_before = persistence.wal_info().await.unwrap();
        assert!(wal_info_before.is_some());
        assert!(wal_info_before.unwrap() > 0);
        
        // Compact
        persistence.compact(&store).await.unwrap();
        
        // Verify snapshot exists
        assert!(persistence.snapshot_exists().await);
        
        // Verify WAL was truncated
        let wal_info_after = persistence.wal_info().await.unwrap();
        assert!(wal_info_after.is_some());
        // WAL should be much smaller after compaction (only snapshot marker)
        assert!(wal_info_after.unwrap() < wal_info_before.unwrap());
    }
}