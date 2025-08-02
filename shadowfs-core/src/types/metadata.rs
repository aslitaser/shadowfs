use std::time::SystemTime;
use std::collections::HashMap;
use bytes::Bytes;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

/// Windows-specific metadata with extended attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowsMetadata {
    /// File attributes (hidden, system, archive, etc.)
    pub attributes: u32,
    /// Reparse point tag (for symlinks and other special files)
    pub reparse_tag: Option<u32>,
}

/// macOS-specific metadata with extended attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct MacOSMetadata {
    /// BSD flags
    pub flags: u32,
    /// Extended attributes as key-value pairs
    pub extended_attributes: HashMap<String, Bytes>,
}

/// Linux-specific metadata with extended attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct LinuxMetadata {
    /// Inode number
    pub inode: u64,
    /// Device ID
    pub device: u64,
    /// Extended attributes as key-value pairs
    pub extended_attributes: HashMap<String, Bytes>,
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