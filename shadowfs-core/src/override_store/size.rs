//! Size calculation utilities for memory tracking.

use super::entry::{OverrideEntry, OverrideContent};
use bytes::Bytes;

/// Calculates the memory size of a Bytes object including overhead.
pub fn calculate_bytes_size(data: &Bytes) -> usize {
    // Bytes has a small overhead for reference counting and metadata
    const BYTES_OVERHEAD: usize = 32; // Arc pointer + length + capacity
    data.len() + BYTES_OVERHEAD
}

/// Calculates the total memory size of an OverrideEntry.
pub fn calculate_entry_size(entry: &OverrideEntry) -> usize {
    // Base struct size
    let mut size = std::mem::size_of::<OverrideEntry>();
    
    // Add path string size
    size += entry.path.to_string().len();
    
    // Add content size
    size += match &entry.content {
        OverrideContent::File { data, content_hash, .. } => {
            calculate_bytes_size(data) + std::mem::size_of_val(content_hash)
        }
        OverrideContent::Directory { entries } => {
            // Vector overhead
            let vec_overhead = std::mem::size_of::<Vec<String>>();
            // String overhead per entry (24 bytes on 64-bit)
            let string_overhead = std::mem::size_of::<String>() * entries.len();
            // Actual string data
            let string_data: usize = entries.iter().map(|s| s.len()).sum();
            vec_overhead + string_overhead + string_data
        }
        OverrideContent::Deleted => 0,
    };
    
    // Add metadata sizes (rough estimates)
    if entry.original_metadata.is_some() {
        size += std::mem::size_of::<crate::types::FileMetadata>();
    }
    size += std::mem::size_of::<crate::types::FileMetadata>(); // override_metadata
    
    // Add some overhead for DashMap entry
    const DASHMAP_ENTRY_OVERHEAD: usize = 64;
    size + DASHMAP_ENTRY_OVERHEAD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ShadowPath, FileMetadata, FileType, FilePermissions, PlatformMetadata};
    use std::sync::atomic::AtomicU64;
    use std::time::SystemTime;
    
    #[test]
    fn test_calculate_bytes_size() {
        let data = Bytes::from(vec![0u8; 100]);
        let size = calculate_bytes_size(&data);
        assert_eq!(size, 100 + 32); // 100 bytes data + 32 overhead
        
        let empty = Bytes::new();
        let size = calculate_bytes_size(&empty);
        assert_eq!(size, 32); // Just overhead
    }
    
    #[test]
    fn test_calculate_entry_size() {
        // Create a test entry with file content
        let entry = OverrideEntry {
            path: ShadowPath::new("/test/file.txt".into()),
            content: OverrideContent::File {
                data: Bytes::from(vec![0u8; 1000]),
                content_hash: [0u8; 32],
                is_compressed: false,
            },
            original_metadata: None,
            override_metadata: FileMetadata {
                size: 1000,
                created: SystemTime::now(),
                modified: SystemTime::now(),
                accessed: SystemTime::now(),
                permissions: FilePermissions::default_file(),
                file_type: FileType::File,
                platform_specific: PlatformMetadata::Linux { inode: 0, nlink: 1 },
            },
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(0),
        };
        
        let size = calculate_entry_size(&entry);
        
        // Should include struct size, path string, file data, metadata, etc.
        assert!(size > 1000); // At least the file data size
        assert!(size < 2000); // But not too much overhead
    }
    
    #[test]
    fn test_calculate_directory_entry_size() {
        // Create a directory entry
        let entry = OverrideEntry {
            path: ShadowPath::new("/test/dir".into()),
            content: OverrideContent::Directory {
                entries: vec![
                    "file1.txt".to_string(),
                    "file2.txt".to_string(),
                    "subdir".to_string(),
                ],
            },
            original_metadata: None,
            override_metadata: FileMetadata {
                size: 0,
                created: SystemTime::now(),
                modified: SystemTime::now(),
                accessed: SystemTime::now(),
                permissions: FilePermissions::default_directory(),
                file_type: FileType::Directory,
                platform_specific: PlatformMetadata::Linux { inode: 0, nlink: 3 },
            },
            created_at: SystemTime::now(),
            last_accessed: AtomicU64::new(0),
        };
        
        let size = calculate_entry_size(&entry);
        
        // Should include struct size, path string, entry strings, metadata
        assert!(size > 100); // Some reasonable minimum
        assert!(size < 1000); // Not too large for a simple directory
    }
}