use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A normalized path representation for ShadowFS that provides
/// platform-agnostic path handling and comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShadowPath {
    inner: PathBuf,
}

impl ShadowPath {
    /// Creates a new ShadowPath from a PathBuf, normalizing it.
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: Self::normalize_path(path),
        }
    }

    /// Normalizes a path by removing . and .. components.
    fn normalize_path(path: PathBuf) -> PathBuf {
        let mut components = Vec::new();
        
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {
                    // Skip . components
                }
                std::path::Component::ParentDir => {
                    // Handle .. by popping the last component if possible
                    if !components.is_empty() {
                        components.pop();
                    }
                }
                _ => {
                    components.push(component);
                }
            }
        }
        
        components.iter().collect()
    }

    /// Converts the ShadowPath to a host-specific PathBuf.
    pub fn to_host_path(&self) -> PathBuf {
        self.inner.clone()
    }

    /// Returns true if the path is absolute.
    pub fn is_absolute(&self) -> bool {
        self.inner.is_absolute()
    }

    /// Strips the given prefix from the path.
    pub fn strip_prefix<P: AsRef<Path>>(&self, base: P) -> Option<ShadowPath> {
        self.inner
            .strip_prefix(base)
            .ok()
            .map(|p| ShadowPath::new(p.to_path_buf()))
    }

    /// Returns the inner PathBuf reference.
    pub fn as_path(&self) -> &Path {
        &self.inner
    }
}

impl fmt::Display for ShadowPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display paths with forward slashes on all platforms
        let display_str = if cfg!(windows) {
            self.inner.display().to_string().replace('\\', "/")
        } else {
            self.inner.display().to_string()
        };
        write!(f, "{}", display_str)
    }
}

impl From<&str> for ShadowPath {
    fn from(s: &str) -> Self {
        ShadowPath::new(PathBuf::from(s))
    }
}

impl From<String> for ShadowPath {
    fn from(s: String) -> Self {
        ShadowPath::new(PathBuf::from(s))
    }
}

impl From<PathBuf> for ShadowPath {
    fn from(path: PathBuf) -> Self {
        ShadowPath::new(path)
    }
}

impl AsRef<Path> for ShadowPath {
    fn as_ref(&self) -> &Path {
        &self.inner
    }
}

/// Represents the type of a file system entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
}

/// Represents file permissions in a platform-agnostic way.
/// Abstracts Unix permissions (rwx) and Windows ACLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilePermissions {
    /// Whether the file is read-only
    pub readonly: bool,
    /// Owner read permission
    pub owner_read: bool,
    /// Owner write permission
    pub owner_write: bool,
    /// Owner execute permission
    pub owner_execute: bool,
    /// Group read permission
    pub group_read: bool,
    /// Group write permission
    pub group_write: bool,
    /// Group execute permission
    pub group_execute: bool,
    /// Other read permission
    pub other_read: bool,
    /// Other write permission
    pub other_write: bool,
    /// Other execute permission
    pub other_execute: bool,
}

impl FilePermissions {
    /// Creates a new FilePermissions instance from Unix mode bits.
    pub fn from_unix_mode(mode: u32) -> Self {
        Self {
            readonly: (mode & 0o200) == 0, // No owner write permission
            owner_read: (mode & 0o400) != 0,
            owner_write: (mode & 0o200) != 0,
            owner_execute: (mode & 0o100) != 0,
            group_read: (mode & 0o040) != 0,
            group_write: (mode & 0o020) != 0,
            group_execute: (mode & 0o010) != 0,
            other_read: (mode & 0o004) != 0,
            other_write: (mode & 0o002) != 0,
            other_execute: (mode & 0o001) != 0,
        }
    }

    /// Converts the permissions to Unix mode bits.
    pub fn to_unix_mode(&self) -> u32 {
        let mut mode = 0;
        
        if self.owner_read { mode |= 0o400; }
        if self.owner_write { mode |= 0o200; }
        if self.owner_execute { mode |= 0o100; }
        if self.group_read { mode |= 0o040; }
        if self.group_write { mode |= 0o020; }
        if self.group_execute { mode |= 0o010; }
        if self.other_read { mode |= 0o004; }
        if self.other_write { mode |= 0o002; }
        if self.other_execute { mode |= 0o001; }
        
        mode
    }

    /// Returns true if the file is executable by anyone.
    pub fn is_executable(&self) -> bool {
        self.owner_execute || self.group_execute || self.other_execute
    }

    /// Returns default permissions for a file.
    pub fn default_file() -> Self {
        Self::from_unix_mode(0o644)
    }

    /// Returns default permissions for a directory.
    pub fn default_directory() -> Self {
        Self::from_unix_mode(0o755)
    }
}

/// Platform-specific metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformMetadata {
    /// Windows-specific metadata
    Windows {
        /// File attributes (hidden, system, archive, etc.)
        attributes: u32,
        /// Reparse point tag (for symlinks and other special files)
        reparse_tag: Option<u32>,
    },
    /// macOS-specific metadata
    MacOS {
        /// BSD flags
        flags: u32,
        /// Extended attributes count
        xattr_count: usize,
    },
    /// Linux-specific metadata
    Linux {
        /// Inode number
        inode: u64,
        /// Number of hard links
        nlink: u64,
    },
}

/// Complete metadata for a file system entry.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMetadata {
    /// Size in bytes
    pub size: u64,
    /// Creation time
    pub created: SystemTime,
    /// Last modification time
    pub modified: SystemTime,
    /// Last access time
    pub accessed: SystemTime,
    /// File permissions
    pub permissions: FilePermissions,
    /// Type of file system entry
    pub file_type: FileType,
    /// Platform-specific metadata
    pub platform_specific: PlatformMetadata,
}

impl FileMetadata {
    /// Creates a new FileMetadata instance.
    pub fn new(
        size: u64,
        created: SystemTime,
        modified: SystemTime,
        accessed: SystemTime,
        permissions: FilePermissions,
        file_type: FileType,
        platform_specific: PlatformMetadata,
    ) -> Self {
        Self {
            size,
            created,
            modified,
            accessed,
            permissions,
            file_type,
            platform_specific,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_normalization() {
        let path = ShadowPath::from("./foo/../bar/./baz");
        assert_eq!(path.to_host_path(), PathBuf::from("bar/baz"));
    }

    #[test]
    fn test_absolute_path() {
        let abs_path = ShadowPath::from("/foo/bar");
        assert!(abs_path.is_absolute());
        
        let rel_path = ShadowPath::from("foo/bar");
        assert!(!rel_path.is_absolute());
    }

    #[test]
    fn test_strip_prefix() {
        let path = ShadowPath::from("/foo/bar/baz");
        let stripped = path.strip_prefix("/foo").unwrap();
        assert_eq!(stripped.to_host_path(), PathBuf::from("bar/baz"));
    }

    #[test]
    fn test_display_forward_slashes() {
        let path = ShadowPath::from("foo/bar/baz");
        assert_eq!(path.to_string(), "foo/bar/baz");
    }

    #[test]
    fn test_file_permissions_from_unix_mode() {
        let perms = FilePermissions::from_unix_mode(0o755);
        assert_eq!(perms.owner_read, true);
        assert_eq!(perms.owner_write, true);
        assert_eq!(perms.owner_execute, true);
        assert_eq!(perms.group_read, true);
        assert_eq!(perms.group_write, false);
        assert_eq!(perms.group_execute, true);
        assert_eq!(perms.other_read, true);
        assert_eq!(perms.other_write, false);
        assert_eq!(perms.other_execute, true);
        assert_eq!(perms.readonly, false);
    }

    #[test]
    fn test_file_permissions_to_unix_mode() {
        let perms = FilePermissions::from_unix_mode(0o644);
        assert_eq!(perms.to_unix_mode(), 0o644);
        
        let perms2 = FilePermissions::from_unix_mode(0o755);
        assert_eq!(perms2.to_unix_mode(), 0o755);
    }

    #[test]
    fn test_file_permissions_is_executable() {
        let perms_exec = FilePermissions::from_unix_mode(0o755);
        assert!(perms_exec.is_executable());
        
        let perms_no_exec = FilePermissions::from_unix_mode(0o644);
        assert!(!perms_no_exec.is_executable());
    }

    #[test]
    fn test_file_permissions_readonly() {
        let perms_readonly = FilePermissions::from_unix_mode(0o444);
        assert!(perms_readonly.readonly);
        
        let perms_writeable = FilePermissions::from_unix_mode(0o644);
        assert!(!perms_writeable.readonly);
    }
}

/// A handle to an open file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle(pub u64);

/// Flags for opening a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenFlags {
    /// Whether to open for reading
    pub read: bool,
    /// Whether to open for writing
    pub write: bool,
    /// Whether to create the file if it doesn't exist
    pub create: bool,
    /// Whether to truncate the file on open
    pub truncate: bool,
    /// Whether to append to the file
    pub append: bool,
    /// Whether to create a new file (fail if exists)
    pub create_new: bool,
}

impl OpenFlags {
    /// Creates a new OpenFlags instance with all flags set to false.
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            create: false,
            truncate: false,
            append: false,
            create_new: false,
        }
    }

    /// Creates flags for read-only access.
    pub fn read_only() -> Self {
        Self {
            read: true,
            ..Self::new()
        }
    }

    /// Creates flags for write-only access.
    pub fn write_only() -> Self {
        Self {
            write: true,
            ..Self::new()
        }
    }

    /// Creates flags for read-write access.
    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            ..Self::new()
        }
    }
}

impl Default for OpenFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper around byte data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    /// Creates a new Bytes instance from a vector.
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    /// Creates an empty Bytes instance.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Returns the length of the data.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the data is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the data as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Consumes self and returns the underlying vector.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl From<&[u8]> for Bytes {
    fn from(data: &[u8]) -> Self {
        Self::new(data.to_vec())
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Represents all possible filesystem operations.
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    /// Open a file with specified flags.
    Open {
        /// Path to the file to open
        path: ShadowPath,
        /// Flags for opening the file
        flags: OpenFlags,
    },
    /// Read data from an open file.
    Read {
        /// Handle to the open file
        handle: FileHandle,
        /// Offset to start reading from
        offset: u64,
        /// Number of bytes to read
        length: usize,
    },
    /// Write data to an open file.
    Write {
        /// Handle to the open file
        handle: FileHandle,
        /// Offset to start writing at
        offset: u64,
        /// Data to write
        data: Bytes,
    },
    /// Close an open file.
    Close {
        /// Handle to the file to close
        handle: FileHandle,
    },
    /// Get metadata for a file or directory.
    GetMetadata {
        /// Path to get metadata for
        path: ShadowPath,
    },
    /// Set metadata for a file or directory.
    SetMetadata {
        /// Path to set metadata for
        path: ShadowPath,
        /// New metadata to set
        metadata: FileMetadata,
    },
    /// Read the contents of a directory.
    ReadDirectory {
        /// Path to the directory to read
        path: ShadowPath,
    },
    /// Create a new file.
    CreateFile {
        /// Path where the file should be created
        path: ShadowPath,
        /// Permissions for the new file
        permissions: FilePermissions,
    },
    /// Create a new directory.
    CreateDirectory {
        /// Path where the directory should be created
        path: ShadowPath,
        /// Permissions for the new directory
        permissions: FilePermissions,
    },
    /// Delete a file or directory.
    Delete {
        /// Path to delete
        path: ShadowPath,
    },
    /// Rename a file or directory.
    Rename {
        /// Source path
        from: ShadowPath,
        /// Destination path
        to: ShadowPath,
    },
}