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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilePermissions {
    /// Unix-style permissions (rwxrwxrwx)
    pub mode: u32,
    /// Whether the file is read-only
    pub readonly: bool,
}

impl FilePermissions {
    /// Creates a new FilePermissions instance.
    pub fn new(mode: u32, readonly: bool) -> Self {
        Self { mode, readonly }
    }

    /// Returns default permissions for a file.
    pub fn default_file() -> Self {
        Self {
            mode: 0o644,
            readonly: false,
        }
    }

    /// Returns default permissions for a directory.
    pub fn default_directory() -> Self {
        Self {
            mode: 0o755,
            readonly: false,
        }
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
}