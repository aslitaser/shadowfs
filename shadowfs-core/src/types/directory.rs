use crate::types::{FileMetadata, FileType};

/// Represents a single entry in a directory listing.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    /// The name of the file or directory
    pub name: String,
    /// The metadata for this entry
    pub metadata: FileMetadata,
}

impl DirectoryEntry {
    /// Creates a new DirectoryEntry.
    pub fn new(name: String, metadata: FileMetadata) -> Self {
        Self { name, metadata }
    }

    /// Returns the name of the entry.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the metadata of the entry.
    pub fn metadata(&self) -> &FileMetadata {
        &self.metadata
    }

    /// Returns true if this entry is a directory.
    pub fn is_directory(&self) -> bool {
        matches!(self.metadata.file_type, FileType::Directory)
    }

    /// Returns true if this entry is a file.
    pub fn is_file(&self) -> bool {
        matches!(self.metadata.file_type, FileType::File)
    }

    /// Returns true if this entry is a symlink.
    pub fn is_symlink(&self) -> bool {
        matches!(self.metadata.file_type, FileType::Symlink)
    }

    /// Sorts a vector of directory entries by name (case-insensitive).
    pub fn sort_by_name(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    /// Sorts a vector of directory entries by size.
    pub fn sort_by_size(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by_key(|entry| entry.metadata.size);
    }

    /// Sorts a vector of directory entries by modification time.
    pub fn sort_by_modified(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by_key(|entry| entry.metadata.modified);
    }

    /// Sorts a vector of directory entries by type (directories first, then files, then symlinks).
    pub fn sort_by_type(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by_key(|entry| match entry.metadata.file_type {
            FileType::Directory => 0,
            FileType::File => 1,
            FileType::Symlink => 2,
        });
    }

    /// Filters entries to only include directories.
    pub fn filter_directories(entries: Vec<DirectoryEntry>) -> Vec<DirectoryEntry> {
        entries.into_iter().filter(|e| e.is_directory()).collect()
    }

    /// Filters entries to only include files.
    pub fn filter_files(entries: Vec<DirectoryEntry>) -> Vec<DirectoryEntry> {
        entries.into_iter().filter(|e| e.is_file()).collect()
    }

    /// Filters entries to only include symlinks.
    pub fn filter_symlinks(entries: Vec<DirectoryEntry>) -> Vec<DirectoryEntry> {
        entries.into_iter().filter(|e| e.is_symlink()).collect()
    }

    /// Filters entries by a custom predicate.
    pub fn filter_by<F>(entries: Vec<DirectoryEntry>, predicate: F) -> Vec<DirectoryEntry>
    where
        F: Fn(&DirectoryEntry) -> bool,
    {
        entries.into_iter().filter(predicate).collect()
    }

    /// Filters entries by name pattern (case-insensitive substring match).
    pub fn filter_by_name_pattern(entries: Vec<DirectoryEntry>, pattern: &str) -> Vec<DirectoryEntry> {
        let pattern_lower = pattern.to_lowercase();
        entries.into_iter()
            .filter(|e| e.name.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Filters entries by file extension (case-insensitive).
    pub fn filter_by_extension(entries: Vec<DirectoryEntry>, extension: &str) -> Vec<DirectoryEntry> {
        let ext_lower = extension.to_lowercase();
        entries.into_iter()
            .filter(|e| {
                if e.is_file() {
                    e.name.to_lowercase().ends_with(&format!(".{}", ext_lower))
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FilePermissions, PlatformMetadata};
    use std::time::SystemTime;

    #[test]
    fn test_directory_entry() {
        let metadata = FileMetadata::new(
            1024,
            SystemTime::now(),
            SystemTime::now(),
            SystemTime::now(),
            FilePermissions::default_file(),
            FileType::File,
            PlatformMetadata::Linux { inode: 123, nlink: 1 },
        );
        
        let entry = DirectoryEntry::new("test.txt".to_string(), metadata);
        assert_eq!(entry.name(), "test.txt");
        assert!(entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
    }

    #[test]
    fn test_directory_entry_sorting() {
        let now = SystemTime::now();
        let entries = vec![
            DirectoryEntry::new(
                "b.txt".to_string(),
                FileMetadata::new(
                    200,
                    now,
                    now,
                    now,
                    FilePermissions::default_file(),
                    FileType::File,
                    PlatformMetadata::Linux { inode: 1, nlink: 1 },
                ),
            ),
            DirectoryEntry::new(
                "a.txt".to_string(),
                FileMetadata::new(
                    100,
                    now,
                    now,
                    now,
                    FilePermissions::default_file(),
                    FileType::File,
                    PlatformMetadata::Linux { inode: 2, nlink: 1 },
                ),
            ),
            DirectoryEntry::new(
                "dir".to_string(),
                FileMetadata::new(
                    0,
                    now,
                    now,
                    now,
                    FilePermissions::default_directory(),
                    FileType::Directory,
                    PlatformMetadata::Linux { inode: 3, nlink: 2 },
                ),
            ),
        ];

        // Test sort by name
        let mut sorted = entries.clone();
        DirectoryEntry::sort_by_name(&mut sorted);
        assert_eq!(sorted[0].name(), "a.txt");
        assert_eq!(sorted[1].name(), "b.txt");
        assert_eq!(sorted[2].name(), "dir");

        // Test sort by size
        let mut sorted = entries.clone();
        DirectoryEntry::sort_by_size(&mut sorted);
        assert_eq!(sorted[0].name(), "dir");
        assert_eq!(sorted[1].name(), "a.txt");
        assert_eq!(sorted[2].name(), "b.txt");

        // Test sort by type
        let mut sorted = entries.clone();
        DirectoryEntry::sort_by_type(&mut sorted);
        assert_eq!(sorted[0].name(), "dir");
        assert!(sorted[0].is_directory());
    }

    #[test]
    fn test_directory_entry_filtering() {
        let now = SystemTime::now();
        let entries = vec![
            DirectoryEntry::new(
                "file1.txt".to_string(),
                FileMetadata::new(
                    100,
                    now,
                    now,
                    now,
                    FilePermissions::default_file(),
                    FileType::File,
                    PlatformMetadata::Linux { inode: 1, nlink: 1 },
                ),
            ),
            DirectoryEntry::new(
                "file2.rs".to_string(),
                FileMetadata::new(
                    200,
                    now,
                    now,
                    now,
                    FilePermissions::default_file(),
                    FileType::File,
                    PlatformMetadata::Linux { inode: 2, nlink: 1 },
                ),
            ),
            DirectoryEntry::new(
                "dir".to_string(),
                FileMetadata::new(
                    0,
                    now,
                    now,
                    now,
                    FilePermissions::default_directory(),
                    FileType::Directory,
                    PlatformMetadata::Linux { inode: 3, nlink: 2 },
                ),
            ),
        ];

        // Test filter directories
        let dirs = DirectoryEntry::filter_directories(entries.clone());
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].name(), "dir");

        // Test filter files
        let files = DirectoryEntry::filter_files(entries.clone());
        assert_eq!(files.len(), 2);

        // Test filter by extension
        let txt_files = DirectoryEntry::filter_by_extension(entries.clone(), "txt");
        assert_eq!(txt_files.len(), 1);
        assert_eq!(txt_files[0].name(), "file1.txt");

        // Test filter by name pattern
        let pattern_files = DirectoryEntry::filter_by_name_pattern(entries.clone(), "file");
        assert_eq!(pattern_files.len(), 2);
    }
}