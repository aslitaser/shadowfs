//! Core traits that define the ShadowFS filesystem interface.
//!
//! This module contains the primary traits that platform-specific implementations
//! must implement to provide ShadowFS functionality.

use async_trait::async_trait;
use crate::types::{
    ShadowPath, FileHandle, FileMetadata, DirectoryEntry, 
    OperationResult, OpenFlags, Bytes
};

/// Handle representing a mounted filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MountHandle {
    /// Unique identifier for this mount
    pub id: u64,
    /// Source path that was mounted
    pub source: ShadowPath,
    /// Target mount point
    pub target: ShadowPath,
}

impl MountHandle {
    /// Creates a new mount handle with the given ID and paths.
    pub fn new(id: u64, source: ShadowPath, target: ShadowPath) -> Self {
        Self { id, source, target }
    }
}

/// Options for mounting a filesystem.
#[derive(Debug, Clone, Default)]
pub struct MountOptions {
    /// Whether the mount should be read-only
    pub read_only: bool,
    /// Maximum size in bytes for the override store (0 = unlimited)
    pub max_override_size: usize,
    /// Whether to allow mounting over existing mount points
    pub allow_overlay: bool,
    /// Custom mount-specific options as key-value pairs
    pub custom_options: Vec<(String, String)>,
}

impl MountOptions {
    /// Creates new mount options with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the mount as read-only.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Sets the maximum override store size.
    pub fn max_override_size(mut self, size: usize) -> Self {
        self.max_override_size = size;
        self
    }

    /// Sets whether to allow overlay mounts.
    pub fn allow_overlay(mut self, allow: bool) -> Self {
        self.allow_overlay = allow;
        self
    }

    /// Adds a custom option.
    pub fn add_option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom_options.push((key.into(), value.into()));
        self
    }
}

/// The main filesystem trait that all platform implementations must provide.
///
/// This trait defines the core operations for interacting with a ShadowFS filesystem,
/// including mounting/unmounting, file I/O, and metadata operations.
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Mounts a shadow filesystem from source to target with the given options.
    ///
    /// # Arguments
    /// * `source` - The source directory to shadow
    /// * `target` - The mount point where the shadow filesystem will be accessible
    /// * `options` - Mount options controlling behavior
    ///
    /// # Returns
    /// A `MountHandle` that can be used to unmount the filesystem later.
    async fn mount(
        &mut self, 
        source: ShadowPath, 
        target: ShadowPath, 
        options: MountOptions
    ) -> OperationResult<MountHandle>;

    /// Unmounts a previously mounted shadow filesystem.
    ///
    /// # Arguments
    /// * `handle` - The mount handle returned from a previous mount operation
    async fn unmount(&mut self, handle: &MountHandle) -> OperationResult<()>;

    /// Opens a file at the given path with the specified flags.
    ///
    /// # Arguments
    /// * `path` - Path to the file to open
    /// * `flags` - Flags controlling how the file is opened
    ///
    /// # Returns
    /// A `FileHandle` that can be used for subsequent read/write operations.
    async fn open(&self, path: &ShadowPath, flags: OpenFlags) -> OperationResult<FileHandle>;

    /// Reads data from an open file.
    ///
    /// # Arguments
    /// * `handle` - Handle to the open file
    /// * `offset` - Byte offset to start reading from
    /// * `buffer` - Buffer to read data into
    ///
    /// # Returns
    /// The number of bytes actually read.
    async fn read(
        &self, 
        handle: &FileHandle, 
        offset: u64, 
        buffer: &mut [u8]
    ) -> OperationResult<usize>;

    /// Writes data to an open file.
    ///
    /// # Arguments
    /// * `handle` - Handle to the open file
    /// * `offset` - Byte offset to start writing at
    /// * `data` - Data to write
    ///
    /// # Returns
    /// The number of bytes actually written.
    async fn write(
        &self, 
        handle: &FileHandle, 
        offset: u64, 
        data: &[u8]
    ) -> OperationResult<usize>;

    /// Closes an open file handle.
    ///
    /// # Arguments
    /// * `handle` - Handle to close
    async fn close(&self, handle: FileHandle) -> OperationResult<()>;

    /// Gets metadata for a file or directory.
    ///
    /// # Arguments
    /// * `path` - Path to get metadata for
    ///
    /// # Returns
    /// File metadata including size, permissions, timestamps, etc.
    async fn get_metadata(&self, path: &ShadowPath) -> OperationResult<FileMetadata>;

    /// Reads the contents of a directory.
    ///
    /// # Arguments
    /// * `path` - Path to the directory to read
    ///
    /// # Returns
    /// A vector of directory entries, one for each item in the directory.
    async fn read_directory(&self, path: &ShadowPath) -> OperationResult<Vec<DirectoryEntry>>;
}

/// Trait for managing file content overrides in memory.
///
/// This trait provides an interface for storing and retrieving file content
/// overrides that shadow the real filesystem. When a file has an override,
/// operations return the override content instead of reading from disk.
pub trait OverrideProvider: Send + Sync {
    /// Sets or updates an override for a file at the given path.
    ///
    /// # Arguments
    /// * `path` - Path to override
    /// * `content` - New content for the file, or `None` to mark as deleted
    ///
    /// # Notes
    /// - If `content` is `Some(bytes)`, the file is overridden with the given content
    /// - If `content` is `None`, the file is marked as deleted in the override layer
    fn set_override(&mut self, path: ShadowPath, content: Option<Bytes>);

    /// Gets the override content for a file if one exists.
    ///
    /// # Arguments
    /// * `path` - Path to check for overrides
    ///
    /// # Returns
    /// - `Some(&Bytes)` if the file has override content
    /// - `None` if the file has no override (use real filesystem)
    fn get_override(&self, path: &ShadowPath) -> Option<&Bytes>;

    /// Checks if a path has any override (content or deletion marker).
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// `true` if the path has any kind of override, `false` otherwise
    fn has_override(&self, path: &ShadowPath) -> bool;

    /// Removes an override for the given path.
    ///
    /// # Arguments
    /// * `path` - Path whose override should be removed
    ///
    /// # Returns
    /// `true` if an override was removed, `false` if no override existed
    fn clear_override(&mut self, path: &ShadowPath) -> bool;

    /// Removes all overrides, resetting to a clean state.
    fn clear_all_overrides(&mut self);

    /// Returns the number of paths that have overrides.
    ///
    /// # Returns
    /// The count of overridden paths (including deletion markers)
    fn override_count(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_handle() {
        let source = ShadowPath::from("/source");
        let target = ShadowPath::from("/target");
        let handle = MountHandle::new(42, source.clone(), target.clone());

        assert_eq!(handle.id, 42);
        assert_eq!(handle.source, source);
        assert_eq!(handle.target, target);
    }

    #[test]
    fn test_mount_options_builder() {
        let options = MountOptions::new()
            .read_only(true)
            .max_override_size(1024 * 1024)
            .allow_overlay(true)
            .add_option("key1", "value1")
            .add_option("key2", "value2");

        assert!(options.read_only);
        assert_eq!(options.max_override_size, 1024 * 1024);
        assert!(options.allow_overlay);
        assert_eq!(options.custom_options.len(), 2);
        assert_eq!(options.custom_options[0], ("key1".to_string(), "value1".to_string()));
        assert_eq!(options.custom_options[1], ("key2".to_string(), "value2".to_string()));
    }

    #[test]
    fn test_mount_options_default() {
        let options = MountOptions::default();

        assert!(!options.read_only);
        assert_eq!(options.max_override_size, 0);
        assert!(!options.allow_overlay);
        assert!(options.custom_options.is_empty());
    }
}