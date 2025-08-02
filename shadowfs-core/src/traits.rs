//! Core traits that define the ShadowFS filesystem interface.
//!
//! This module contains the primary traits that platform-specific implementations
//! must implement to provide ShadowFS functionality.

use async_trait::async_trait;
use crate::types::{
    ShadowPath, FileHandle, FileMetadata, DirectoryEntry, 
    OperationResult, OpenFlags, Bytes, MountOptions, MountHandle
};

// Re-export Platform from types::mount module
pub use crate::types::mount::Platform;

// Extension trait to add name() method to Platform
pub trait PlatformExt {
    /// Returns the name of the platform as a string slice.
    fn name(&self) -> &'static str;
}

impl PlatformExt for Platform {
    fn name(&self) -> &'static str {
        match self {
            Platform::Windows => "Windows",
            Platform::MacOS => "macOS", 
            Platform::Linux => "Linux",
        }
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

/// Trait for detecting platform capabilities and creating platform-specific implementations.
pub trait PlatformDetector: Send + Sync {
    /// Detects the current platform's capabilities.
    fn detect() -> PlatformCapabilities;
    
    /// Checks if all platform requirements are met.
    /// 
    /// # Returns
    /// - `Ok(())` if all requirements are satisfied
    /// - `Err(Vec<String>)` with a list of missing requirements
    fn check_requirements() -> Result<(), Vec<String>>;
    
    /// Creates a platform-specific filesystem implementation.
    /// 
    /// # Returns
    /// A boxed FileSystem implementation appropriate for the current platform.
    fn get_mount_helper() -> Box<dyn FileSystem>;
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

/// Platform-specific capabilities and requirements.
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// The platform this capability set describes
    pub platform: Platform,
    
    /// Whether Windows Projected File System (ProjFS) is available
    pub has_projfs: bool,
    
    /// Whether macOS File System Kit (FSKit) is available
    pub has_fskit: bool,
    
    /// Whether FUSE (Filesystem in Userspace) is available
    pub has_fuse: bool,
    
    /// FUSE version if available (major, minor)
    pub fuse_version: Option<(u8, u8)>,
    
    /// Whether administrative privileges are required
    pub requires_admin: bool,
    
    /// Maximum path length supported by the platform
    pub max_path_length: usize,
}

impl PlatformCapabilities {
    /// Creates platform capabilities for the current platform.
    pub fn current() -> Self {
        let platform = Platform::current();
        
        match platform {
            Platform::Windows => Self {
                platform,
                has_projfs: true,  // Windows 10 1809+ has ProjFS
                has_fskit: false,
                has_fuse: false,
                fuse_version: None,
                requires_admin: false,  // ProjFS doesn't require admin
                max_path_length: 260,   // MAX_PATH on Windows
            },
            Platform::MacOS => Self {
                platform,
                has_projfs: false,
                has_fskit: true,   // macOS 15.0+ has FSKit
                has_fuse: true,    // macFUSE can be installed
                fuse_version: None, // Would need to detect at runtime
                requires_admin: true,  // FSKit requires admin
                max_path_length: 1024, // PATH_MAX on macOS
            },
            Platform::Linux => Self {
                platform,
                has_projfs: false,
                has_fskit: false,
                has_fuse: true,    // FUSE is standard on Linux
                fuse_version: Some((3, 0)), // FUSE 3 is common
                requires_admin: false,  // FUSE can run as user
                max_path_length: 4096,  // PATH_MAX on Linux
            },
        }
    }
    
    /// Creates platform capabilities with specific settings.
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            has_projfs: false,
            has_fskit: false,
            has_fuse: false,
            fuse_version: None,
            requires_admin: false,
            max_path_length: 260,
        }
    }
    
    /// Checks if the platform has any supported filesystem provider.
    pub fn has_any_provider(&self) -> bool {
        self.has_projfs || self.has_fskit || self.has_fuse
    }
    
    /// Gets the name of the recommended filesystem provider for this platform.
    pub fn recommended_provider(&self) -> Option<&'static str> {
        match self.platform {
            Platform::Windows if self.has_projfs => Some("ProjFS"),
            Platform::MacOS if self.has_fskit => Some("FSKit"),
            Platform::MacOS if self.has_fuse => Some("macFUSE"),
            Platform::Linux if self.has_fuse => Some("FUSE"),
            _ => None,
        }
    }
    
    /// Checks if a given path length is valid for this platform.
    pub fn is_valid_path_length(&self, length: usize) -> bool {
        length <= self.max_path_length
    }
}

impl Default for PlatformCapabilities {
    fn default() -> Self {
        Self::current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_capabilities_current() {
        let caps = PlatformCapabilities::current();
        assert_eq!(caps.platform, Platform::current());
        
        // Verify platform-specific defaults
        match caps.platform {
            Platform::Windows => {
                assert!(caps.has_projfs);
                assert!(!caps.has_fskit);
                assert!(!caps.has_fuse);
                assert_eq!(caps.max_path_length, 260);
            }
            Platform::MacOS => {
                assert!(!caps.has_projfs);
                assert!(caps.has_fskit);
                assert!(caps.has_fuse);
                assert_eq!(caps.max_path_length, 1024);
            }
            Platform::Linux => {
                assert!(!caps.has_projfs);
                assert!(!caps.has_fskit);
                assert!(caps.has_fuse);
                assert_eq!(caps.max_path_length, 4096);
            }
        }
    }
    
    #[test]
    fn test_platform_capabilities_new() {
        let caps = PlatformCapabilities::new(Platform::Windows);
        assert_eq!(caps.platform, Platform::Windows);
        assert!(!caps.has_projfs);
        assert!(!caps.has_fskit);
        assert!(!caps.has_fuse);
        assert!(caps.fuse_version.is_none());
        assert!(!caps.requires_admin);
        assert_eq!(caps.max_path_length, 260);
    }
    
    #[test]
    fn test_has_any_provider() {
        let mut caps = PlatformCapabilities::new(Platform::Linux);
        assert!(!caps.has_any_provider());
        
        caps.has_fuse = true;
        assert!(caps.has_any_provider());
        
        caps.has_projfs = true;
        assert!(caps.has_any_provider());
    }
    
    #[test]
    fn test_recommended_provider() {
        // Windows with ProjFS
        let mut caps = PlatformCapabilities::new(Platform::Windows);
        caps.has_projfs = true;
        assert_eq!(caps.recommended_provider(), Some("ProjFS"));
        
        // macOS with FSKit
        let mut caps = PlatformCapabilities::new(Platform::MacOS);
        caps.has_fskit = true;
        assert_eq!(caps.recommended_provider(), Some("FSKit"));
        
        // macOS with only FUSE
        let mut caps = PlatformCapabilities::new(Platform::MacOS);
        caps.has_fuse = true;
        assert_eq!(caps.recommended_provider(), Some("macFUSE"));
        
        // Linux with FUSE
        let mut caps = PlatformCapabilities::new(Platform::Linux);
        caps.has_fuse = true;
        assert_eq!(caps.recommended_provider(), Some("FUSE"));
        
        // No provider
        let caps = PlatformCapabilities::new(Platform::Windows);
        assert_eq!(caps.recommended_provider(), None);
    }
    
    #[test]
    fn test_is_valid_path_length() {
        let caps = PlatformCapabilities {
            platform: Platform::Windows,
            has_projfs: true,
            has_fskit: false,
            has_fuse: false,
            fuse_version: None,
            requires_admin: false,
            max_path_length: 260,
        };
        
        assert!(caps.is_valid_path_length(100));
        assert!(caps.is_valid_path_length(260));
        assert!(!caps.is_valid_path_length(261));
        assert!(!caps.is_valid_path_length(1000));
    }
}