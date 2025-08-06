use std::sync::atomic::{AtomicU64, Ordering};

/// File system statistics tracker
pub struct FileSystemStats {
    file_reads: AtomicU64,
    directory_enumerations: AtomicU64,
    placeholder_creations: AtomicU64,
    bytes_read: AtomicU64,
}

impl FileSystemStats {
    pub fn new() -> Self {
        Self {
            file_reads: AtomicU64::new(0),
            directory_enumerations: AtomicU64::new(0),
            placeholder_creations: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
        }
    }
    
    pub fn increment_file_reads(&self) {
        self.file_reads.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn increment_directory_enumerations(&self) {
        self.directory_enumerations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn increment_placeholder_creations(&self) {
        self.placeholder_creations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add_bytes_read(&self, bytes: u64) {
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn get_file_reads(&self) -> u64 {
        self.file_reads.load(Ordering::Relaxed)
    }
    
    pub fn get_directory_enumerations(&self) -> u64 {
        self.directory_enumerations.load(Ordering::Relaxed)
    }
    
    pub fn get_placeholder_creations(&self) -> u64 {
        self.placeholder_creations.load(Ordering::Relaxed)
    }
    
    pub fn get_bytes_read(&self) -> u64 {
        self.bytes_read.load(Ordering::Relaxed)
    }
}