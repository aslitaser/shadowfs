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

    #[test]
    fn test_open_flags_bitflags() {
        let flags = OpenFlags::READ | OpenFlags::WRITE;
        assert!(flags.contains(OpenFlags::READ));
        assert!(flags.contains(OpenFlags::WRITE));
        assert!(!flags.contains(OpenFlags::APPEND));

        let mut flags2 = OpenFlags::CREATE;
        flags2.insert(OpenFlags::TRUNCATE);
        assert!(flags2.contains(OpenFlags::CREATE | OpenFlags::TRUNCATE));

        let flags3 = OpenFlags::all();
        assert!(flags3.contains(OpenFlags::READ));
        assert!(flags3.contains(OpenFlags::WRITE));
        assert!(flags3.contains(OpenFlags::APPEND));
        assert!(flags3.contains(OpenFlags::CREATE));
        assert!(flags3.contains(OpenFlags::TRUNCATE));
        assert!(flags3.contains(OpenFlags::EXCLUSIVE));
    }

    #[test]
    fn test_open_flags_operations() {
        let flags1 = OpenFlags::READ | OpenFlags::WRITE;
        let flags2 = OpenFlags::WRITE | OpenFlags::CREATE;
        
        let union = flags1 | flags2;
        assert!(union.contains(OpenFlags::READ));
        assert!(union.contains(OpenFlags::WRITE));
        assert!(union.contains(OpenFlags::CREATE));
        
        let intersection = flags1 & flags2;
        assert!(!intersection.contains(OpenFlags::READ));
        assert!(intersection.contains(OpenFlags::WRITE));
        assert!(!intersection.contains(OpenFlags::CREATE));
        
        let difference = flags1 - flags2;
        assert!(difference.contains(OpenFlags::READ));
        assert!(!difference.contains(OpenFlags::WRITE));
        assert!(!difference.contains(OpenFlags::CREATE));
    }

    #[test]
    fn test_open_flags_from_bits() {
        let flags = OpenFlags::from_bits(0b000011).unwrap();
        assert!(flags.contains(OpenFlags::READ));
        assert!(flags.contains(OpenFlags::WRITE));
        
        let invalid = OpenFlags::from_bits(0b1000000);
        assert!(invalid.is_none());
        
        let truncated = OpenFlags::from_bits_truncate(0b1000011);
        assert!(truncated.contains(OpenFlags::READ));
        assert!(truncated.contains(OpenFlags::WRITE));
        assert_eq!(truncated.bits(), 0b000011);
    }
    
    #[test]
    fn test_file_handle() {
        let handle = FileHandle::new(42);
        assert_eq!(handle.id(), 42);
        assert!(handle.is_valid());
        
        let invalid = FileHandle::invalid();
        assert_eq!(invalid.id(), 0);
        assert!(!invalid.is_valid());
        
        let handle2 = FileHandle::new(42);
        assert_eq!(handle, handle2);
        
        let handle3 = FileHandle::new(43);
        assert_ne!(handle, handle3);
    }
    
    #[test]
    fn test_file_handle_display() {
        let handle = FileHandle::new(12345);
        assert_eq!(format!("{}", handle), "FileHandle(12345)");
    }
}

/// A handle to an open file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle(u64);

impl FileHandle {
    /// Creates a new FileHandle with the given ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// Returns the underlying handle ID.
    pub fn id(&self) -> u64 {
        self.0
    }
    
    /// Creates an invalid/null handle (useful for error cases).
    pub fn invalid() -> Self {
        Self(0)
    }
    
    /// Checks if this handle is valid (non-zero).
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

impl fmt::Display for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileHandle({})", self.0)
    }
}

/// Flags for opening a file using bitflags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenFlags(u32);

impl OpenFlags {
    /// Read access flag
    pub const READ: Self = Self(1 << 0);
    /// Write access flag
    pub const WRITE: Self = Self(1 << 1);
    /// Append mode flag
    pub const APPEND: Self = Self(1 << 2);
    /// Create file if it doesn't exist
    pub const CREATE: Self = Self(1 << 3);
    /// Truncate file to zero length
    pub const TRUNCATE: Self = Self(1 << 4);
    /// Exclusive creation (fail if file exists)
    pub const EXCLUSIVE: Self = Self(1 << 5);

    /// Creates an empty set of flags.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Creates a set containing all flags.
    pub const fn all() -> Self {
        Self(Self::READ.0 | Self::WRITE.0 | Self::APPEND.0 | Self::CREATE.0 | Self::TRUNCATE.0 | Self::EXCLUSIVE.0)
    }

    /// Returns the raw value of the flags.
    pub const fn bits(&self) -> u32 {
        self.0
    }

    /// Creates flags from raw bits.
    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::all().0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// Creates flags from raw bits, truncating invalid bits.
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & Self::all().0)
    }

    /// Returns true if no flags are set.
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns true if all flags in `other` are set.
    pub const fn contains(&self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Inserts the specified flags.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes the specified flags.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Toggles the specified flags.
    pub fn toggle(&mut self, other: Self) {
        self.0 ^= other.0;
    }

    /// Returns the union of the flags.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns the intersection of the flags.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Returns the difference of the flags.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Returns the symmetric difference of the flags.
    pub const fn symmetric_difference(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }

    /// Returns the complement of the flags.
    pub const fn complement(self) -> Self {
        Self(!self.0 & Self::all().0)
    }
}

impl Default for OpenFlags {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::ops::BitOr for OpenFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for OpenFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

impl std::ops::BitAnd for OpenFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        self.intersection(rhs)
    }
}

impl std::ops::BitAndAssign for OpenFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = self.intersection(rhs);
    }
}

impl std::ops::BitXor for OpenFlags {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self {
        self.symmetric_difference(rhs)
    }
}

impl std::ops::BitXorAssign for OpenFlags {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = self.symmetric_difference(rhs);
    }
}

impl std::ops::Sub for OpenFlags {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.difference(rhs)
    }
}

impl std::ops::SubAssign for OpenFlags {
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.difference(rhs);
    }
}

impl std::ops::Not for OpenFlags {
    type Output = Self;
    fn not(self) -> Self {
        self.complement()
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