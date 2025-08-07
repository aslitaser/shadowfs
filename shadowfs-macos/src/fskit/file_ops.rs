use super::provider::FSKitProvider;
use super::operations::{FSOperationsImpl, OverrideStore, OverrideItem, FSItemType, FileAttributes};
use super::file_locking::{FileLockManager, LockType as FileLockType, ByteRange};
use objc2::rc::Weak;
use objc2::{msg_send, msg_send_id, ClassType};
use objc2::runtime::{AnyObject, ProtocolObject};
use std::sync::{Arc, RwLock, Mutex};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::ffi::CStr;
use std::io::{self, Read, Seek, SeekFrom};
use std::fs::File;
use std::cmp::min;
use std::time::Duration;

/// Represents an open file handle in the FSKit filesystem
#[derive(Debug, Clone)]
pub struct FSFileHandle {
    /// Unique identifier for this handle
    pub id: u64,
    /// Path to the file
    pub path: PathBuf,
    /// Open mode flags (read, write, append, etc.)
    pub mode: OpenMode,
    /// Current position in the file for sequential operations
    pub position: u64,
    /// Reference count for shared handles
    pub ref_count: usize,
    /// Optional read/write context data
    pub context: Option<FileContext>,
}

/// Open mode flags for file operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpenMode {
    ReadOnly = 0x01,
    WriteOnly = 0x02,
    ReadWrite = 0x03,
    Append = 0x08,
    Create = 0x10,
    Truncate = 0x20,
    Exclusive = 0x40,
}

impl OpenMode {
    /// Convert from raw flags to OpenMode
    pub fn from_flags(flags: u32) -> Self {
        match flags & 0x03 {
            0x01 => OpenMode::ReadOnly,
            0x02 => OpenMode::WriteOnly,
            0x03 => OpenMode::ReadWrite,
            _ => OpenMode::ReadOnly,
        }
    }
    
    /// Check if mode allows reading
    pub fn can_read(&self) -> bool {
        matches!(self, OpenMode::ReadOnly | OpenMode::ReadWrite)
    }
    
    /// Check if mode allows writing
    pub fn can_write(&self) -> bool {
        matches!(self, OpenMode::WriteOnly | OpenMode::ReadWrite | OpenMode::Append)
    }
}

/// Context data for read/write operations
#[derive(Debug, Clone)]
pub struct FileContext {
    /// Buffer for pending writes
    pub write_buffer: Vec<u8>,
    /// Read cache for performance
    pub read_cache: Option<Vec<u8>>,
    /// Cache validity range
    pub cache_offset: u64,
    pub cache_length: usize,
    /// Dirty flag for write buffer
    pub is_dirty: bool,
}

impl FileContext {
    pub fn new() -> Self {
        Self {
            write_buffer: Vec::new(),
            read_cache: None,
            cache_offset: 0,
            cache_length: 0,
            is_dirty: false,
        }
    }
}

/// FSFile operations implementation
pub struct FSFileOps {
    /// Weak reference to the provider
    provider: Weak<FSKitProvider>,
    /// Open file handles
    handles: Arc<RwLock<HashMap<u64, FSFileHandle>>>,
    /// Next handle ID counter
    next_handle_id: Arc<Mutex<u64>>,
    /// File lock manager for concurrent access control
    lock_manager: Arc<FileLockManager>,
    /// Reference to override store for virtual file operations
    override_store: Arc<RwLock<OverrideStore>>,
}

impl FSFileOps {
    /// Create a new FSFileOps instance
    pub fn new(provider: Weak<FSKitProvider>, override_store: Arc<RwLock<OverrideStore>>) -> Self {
        Self {
            provider,
            handles: Arc::new(RwLock::new(HashMap::new())),
            next_handle_id: Arc::new(Mutex::new(1)),
            lock_manager: Arc::new(FileLockManager::new()),
            override_store,
        }
    }
    
    /// Open a file with the specified mode
    pub fn open_with_mode(&self, file_item: &AnyObject, mode: OpenMode) -> Result<FSFileHandle, String> {
        // Extract file path from the FSItem
        let file_path = self.get_item_path(file_item)?;
        
        // Generate new handle ID
        let handle_id = {
            let mut id_counter = self.next_handle_id.lock()
                .map_err(|e| format!("Failed to acquire handle ID lock: {}", e))?;
            let id = *id_counter;
            *id_counter += 1;
            id
        };
        
        // Create the file handle
        let mut handle = FSFileHandle {
            id: handle_id,
            path: file_path.clone(),
            mode,
            position: 0,
            ref_count: 1,
            context: Some(FileContext::new()),
        };
        
        // Set initial position for append mode
        if mode == OpenMode::Append {
            handle.position = self.get_file_size(&file_path)?;
        }
        
        // Truncate file if requested
        if mode == OpenMode::Truncate && mode.can_write() {
            self.truncate_file(&file_path)?;
        }
        
        // Store the handle
        {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            handles.insert(handle_id, handle.clone());
        }
        
        // Optionally acquire initial lock based on mode
        // Note: This is advisory locking - applications must explicitly request locks
        // The open mode itself doesn't automatically lock the file
        
        Ok(handle)
    }
    
    /// Track an open file handle
    pub fn track_handle(&self, handle: FSFileHandle) -> Result<(), String> {
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        // Check if handle already exists
        if handles.contains_key(&handle.id) {
            return Err(format!("Handle {} already exists", handle.id));
        }
        
        handles.insert(handle.id, handle);
        Ok(())
    }
    
    /// Set up read context for a handle
    pub fn setup_read_context(&self, handle_id: u64, buffer_size: usize) -> Result<(), String> {
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        let handle = handles.get_mut(&handle_id)
            .ok_or_else(|| format!("Handle {} not found", handle_id))?;
        
        if !handle.mode.can_read() {
            return Err("Handle not opened for reading".to_string());
        }
        
        if let Some(ref mut context) = handle.context {
            // Allocate read cache
            context.read_cache = Some(Vec::with_capacity(buffer_size));
            context.cache_offset = 0;
            context.cache_length = 0;
        }
        
        Ok(())
    }
    
    /// Set up write context for a handle
    pub fn setup_write_context(&self, handle_id: u64, buffer_size: usize) -> Result<(), String> {
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        let handle = handles.get_mut(&handle_id)
            .ok_or_else(|| format!("Handle {} not found", handle_id))?;
        
        if !handle.mode.can_write() {
            return Err("Handle not opened for writing".to_string());
        }
        
        if let Some(ref mut context) = handle.context {
            // Allocate write buffer
            context.write_buffer.reserve(buffer_size);
            context.is_dirty = false;
        }
        
        Ok(())
    }
    
    /// Get a handle by ID
    pub fn get_handle(&self, handle_id: u64) -> Result<FSFileHandle, String> {
        let handles = self.handles.read()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        handles.get(&handle_id)
            .cloned()
            .ok_or_else(|| format!("Handle {} not found", handle_id))
    }
    
    /// Close a file handle
    pub fn close_handle(&self, handle_id: u64) -> Result<(), String> {
        // Flush any pending writes
        self.flush_handle(handle_id)?;
        
        // Release all locks held by this handle
        self.lock_manager.release_all_locks(handle_id)?;
        
        // Remove the handle
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        handles.remove(&handle_id)
            .ok_or_else(|| format!("Handle {} not found", handle_id))?;
        
        Ok(())
    }
    
    /// Flush pending writes for a handle
    pub fn flush_handle(&self, handle_id: u64) -> Result<(), String> {
        let (path, position, buffer_data) = {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get_mut(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            if let Some(ref mut context) = handle.context {
                if context.is_dirty && !context.write_buffer.is_empty() {
                    let data = context.write_buffer.clone();
                    let path = handle.path.clone();
                    let pos = handle.position;
                    
                    // Clear the buffer and reset dirty flag
                    context.write_buffer.clear();
                    context.is_dirty = false;
                    
                    (path, pos, Some(data))
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };
        
        // Write buffered data if any
        if let Some(data) = buffer_data {
            self.ensure_in_override(&path)?;
            self.write_to_override(&path, position, &data)?;
        }
        
        Ok(())
    }
    
    /// Update the file position for a handle
    pub fn seek(&self, handle_id: u64, offset: i64, whence: SeekWhence) -> Result<u64, String> {
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        let handle = handles.get_mut(&handle_id)
            .ok_or_else(|| format!("Handle {} not found", handle_id))?;
        
        let file_size = self.get_file_size(&handle.path)?;
        
        let new_position = match whence {
            SeekWhence::Start => offset as u64,
            SeekWhence::Current => (handle.position as i64 + offset) as u64,
            SeekWhence::End => (file_size as i64 + offset) as u64,
        };
        
        // Validate new position
        if new_position > file_size && !handle.mode.can_write() {
            return Err("Cannot seek past end of file in read-only mode".to_string());
        }
        
        handle.position = new_position;
        
        // Invalidate read cache if position changed
        if let Some(ref mut context) = handle.context {
            if let Some(ref cache) = context.read_cache {
                let cache_end = context.cache_offset + context.cache_length as u64;
                if new_position < context.cache_offset || new_position >= cache_end {
                    context.cache_length = 0; // Invalidate cache
                }
            }
        }
        
        Ok(new_position)
    }
    
    /// Get all open handles for a specific file
    pub fn get_handles_for_file(&self, file_path: &Path) -> Result<Vec<FSFileHandle>, String> {
        let handles = self.handles.read()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        Ok(handles.values()
            .filter(|h| h.path == file_path)
            .cloned()
            .collect())
    }
    
    /// Get the count of open handles
    pub fn get_open_handle_count(&self) -> Result<usize, String> {
        let handles = self.handles.read()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        Ok(handles.len())
    }
    
    /// Check if a file has any open handles
    pub fn has_open_handles(&self, file_path: &Path) -> Result<bool, String> {
        let handles = self.handles.read()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        Ok(handles.values().any(|h| h.path == file_path))
    }
    
    /// Acquire a file lock
    pub fn lock_file(
        &self,
        handle_id: u64,
        lock_type: FileLockType,
        range: Option<ByteRange>,
        timeout: Option<Duration>,
    ) -> Result<u64, String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        // Acquire the lock
        self.lock_manager.acquire_lock(&file_path, handle_id, lock_type, range, timeout)
    }
    
    /// Try to acquire a file lock without blocking
    pub fn try_lock_file(
        &self,
        handle_id: u64,
        lock_type: FileLockType,
        range: Option<ByteRange>,
    ) -> Result<Option<u64>, String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        // Try to acquire the lock
        self.lock_manager.try_acquire_lock(&file_path, handle_id, lock_type, range)
    }
    
    /// Release a file lock
    pub fn unlock_file(&self, handle_id: u64, lock_id: u64) -> Result<(), String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        // Release the lock
        self.lock_manager.release_lock(&file_path, lock_id)
    }
    
    /// Release all locks held by a handle
    pub fn unlock_all(&self, handle_id: u64) -> Result<(), String> {
        self.lock_manager.release_all_locks(handle_id)
    }
    
    /// Upgrade a shared lock to exclusive
    pub fn upgrade_lock(&self, handle_id: u64, lock_id: u64) -> Result<(), String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        self.lock_manager.upgrade_lock(&file_path, lock_id)
    }
    
    /// Downgrade an exclusive lock to shared
    pub fn downgrade_lock(&self, handle_id: u64, lock_id: u64) -> Result<(), String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        self.lock_manager.downgrade_lock(&file_path, lock_id)
    }
    
    /// Check if a byte range is locked
    pub fn is_range_locked(&self, handle_id: u64, range: &ByteRange, for_write: bool) -> Result<bool, String> {
        // Get file path from handle
        let file_path = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            handle.path.clone()
        };
        
        self.lock_manager.is_range_locked(&file_path, range, for_write)
    }
    
    /// Read data from a file handle
    pub fn read(&self, handle_id: u64, buffer: &mut [u8]) -> Result<usize, String> {
        // Get the handle and update position atomically
        let (file_path, start_position, mode) = {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get_mut(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            if !handle.mode.can_read() {
                return Err("Handle not opened for reading".to_string());
            }
            
            let path = handle.path.clone();
            let pos = handle.position;
            let mode = handle.mode;
            
            (path, pos, mode)
        };
        
        // Try to read from override store first
        let bytes_read = {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            if let Some(override_item) = override_store.items.get(&file_path) {
                // Read from override data
                if let Some(ref data) = override_item.data {
                    self.read_from_buffer(data, start_position, buffer)?
                } else {
                    // Override exists but has no data (empty file)
                    0
                }
            } else if override_store.deleted_paths.contains(&file_path) {
                // File has been deleted in override layer
                return Err("File has been deleted".to_string());
            } else {
                // Read from source filesystem
                self.read_from_source(&file_path, start_position, buffer)?
            }
        };
        
        // Update the file position
        if bytes_read > 0 {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            if let Some(handle) = handles.get_mut(&handle_id) {
                handle.position = start_position + bytes_read as u64;
                
                // Update read cache if present
                if let Some(ref mut context) = handle.context {
                    if context.read_cache.is_some() {
                        // Cache the read data for potential reuse
                        context.cache_offset = start_position;
                        context.cache_length = bytes_read;
                    }
                }
            }
        }
        
        Ok(bytes_read)
    }
    
    /// Read data with offset and length (does not update position)
    pub fn pread(&self, handle_id: u64, offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        // Get the handle without updating position
        let (file_path, mode) = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            if !handle.mode.can_read() {
                return Err("Handle not opened for reading".to_string());
            }
            
            (handle.path.clone(), handle.mode)
        };
        
        // Try to read from override store first
        let bytes_read = {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            if let Some(override_item) = override_store.items.get(&file_path) {
                // Read from override data
                if let Some(ref data) = override_item.data {
                    self.read_from_buffer(data, offset, buffer)?
                } else {
                    // Override exists but has no data (empty file)
                    0
                }
            } else if override_store.deleted_paths.contains(&file_path) {
                // File has been deleted in override layer
                return Err("File has been deleted".to_string());
            } else {
                // Read from source filesystem
                self.read_from_source(&file_path, offset, buffer)?
            }
        };
        
        Ok(bytes_read)
    }
    
    /// Write data to a file handle
    pub fn write(&self, handle_id: u64, data: &[u8]) -> Result<usize, String> {
        // Get handle info and check permissions
        let (file_path, start_position, mode, should_append) = {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get_mut(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            if !handle.mode.can_write() {
                return Err("Handle not opened for writing".to_string());
            }
            
            let path = handle.path.clone();
            let pos = if handle.mode == OpenMode::Append {
                // For append mode, always write at end of file
                self.get_file_size(&handle.path)?
            } else {
                handle.position
            };
            let mode = handle.mode;
            let should_append = handle.mode == OpenMode::Append;
            
            (path, pos, mode, should_append)
        };
        
        // Ensure file is in override store (copy-on-write)
        self.ensure_in_override(&file_path)?;
        
        // Write to the override store
        let bytes_written = self.write_to_override(&file_path, start_position, data)?;
        
        // Update handle position
        if bytes_written > 0 {
            let mut handles = self.handles.write()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            if let Some(handle) = handles.get_mut(&handle_id) {
                handle.position = start_position + bytes_written as u64;
                
                // Mark write buffer as dirty if using buffering
                if let Some(ref mut context) = handle.context {
                    context.is_dirty = true;
                }
            }
        }
        
        Ok(bytes_written)
    }
    
    /// Write data at specific offset (does not update position)
    pub fn pwrite(&self, handle_id: u64, offset: u64, data: &[u8]) -> Result<usize, String> {
        // Get handle info without updating position
        let (file_path, mode) = {
            let handles = self.handles.read()
                .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
            
            let handle = handles.get(&handle_id)
                .ok_or_else(|| format!("Handle {} not found", handle_id))?;
            
            if !handle.mode.can_write() {
                return Err("Handle not opened for writing".to_string());
            }
            
            (handle.path.clone(), handle.mode)
        };
        
        // Ensure file is in override store (copy-on-write)
        self.ensure_in_override(&file_path)?;
        
        // Write to the override store
        self.write_to_override(&file_path, offset, data)
    }
    
    /// Buffer writes for efficiency
    pub fn write_buffered(&self, handle_id: u64, data: &[u8]) -> Result<usize, String> {
        let mut handles = self.handles.write()
            .map_err(|e| format!("Failed to acquire handles lock: {}", e))?;
        
        let handle = handles.get_mut(&handle_id)
            .ok_or_else(|| format!("Handle {} not found", handle_id))?;
        
        if !handle.mode.can_write() {
            return Err("Handle not opened for writing".to_string());
        }
        
        // Add data to write buffer
        if let Some(ref mut context) = handle.context {
            context.write_buffer.extend_from_slice(data);
            context.is_dirty = true;
            
            // Auto-flush if buffer exceeds threshold (e.g., 64KB)
            const BUFFER_THRESHOLD: usize = 65536;
            if context.write_buffer.len() >= BUFFER_THRESHOLD {
                // Flush the buffer
                let path = handle.path.clone();
                let position = handle.position;
                let buffer_data = context.write_buffer.clone();
                context.write_buffer.clear();
                
                drop(handles); // Release lock before writing
                
                self.ensure_in_override(&path)?;
                return self.write_to_override(&path, position, &buffer_data);
            }
            
            Ok(data.len())
        } else {
            // No context, write directly
            let path = handle.path.clone();
            let position = handle.position;
            drop(handles);
            
            self.write(handle_id, data)
        }
    }
    
    // Helper methods for write operations
    
    fn ensure_in_override(&self, path: &Path) -> Result<(), String> {
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        // Check if already in override store
        if override_store.items.contains_key(path) {
            return Ok(());
        }
        
        // Remove from deleted paths if it was there
        override_store.deleted_paths.remove(path);
        
        // Copy file from source to override store
        let (file_data, attributes) = if path.exists() {
            // Read the entire source file
            let data = std::fs::read(path)
                .map_err(|e| format!("Failed to read source file for copy-on-write: {}", e))?;
            
            let metadata = std::fs::metadata(path)
                .map_err(|e| format!("Failed to get source file metadata: {}", e))?;
            
            let attrs = FileAttributes {
                size: metadata.len(),
                mode: self.get_file_mode(&metadata),
                uid: self.get_uid(&metadata),
                gid: self.get_gid(&metadata),
                atime: 0,
                mtime: 0,
                ctime: 0,
            };
            
            (data, attrs)
        } else {
            // New file, create empty
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            
            let attrs = FileAttributes {
                size: 0,
                mode: 0o644,
                uid: self.get_current_uid(),
                gid: self.get_current_gid(),
                atime: now,
                mtime: now,
                ctime: now,
            };
            
            (Vec::new(), attrs)
        };
        
        // Create override item
        let override_item = OverrideItem {
            path: path.to_path_buf(),
            item_type: FSItemType::File,
            attributes,
            data: Some(file_data),
        };
        
        override_store.items.insert(path.to_path_buf(), override_item);
        
        Ok(())
    }
    
    fn write_to_override(&self, path: &Path, offset: u64, data: &[u8]) -> Result<usize, String> {
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        let override_item = override_store.items.get_mut(path)
            .ok_or_else(|| "File not found in override store".to_string())?;
        
        // Get or create the data buffer
        if override_item.data.is_none() {
            override_item.data = Some(Vec::new());
        }
        
        let file_data = override_item.data.as_mut().unwrap();
        let offset = offset as usize;
        
        // Extend the buffer if writing past the current end
        if offset > file_data.len() {
            // Fill gap with zeros
            file_data.resize(offset, 0);
        }
        
        // Calculate end position
        let end_position = offset + data.len();
        
        // Extend buffer if necessary
        if end_position > file_data.len() {
            file_data.resize(end_position, 0);
        }
        
        // Write the data
        file_data[offset..end_position].copy_from_slice(data);
        
        // Update file size in attributes
        override_item.attributes.size = file_data.len() as u64;
        
        // Update modification time
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        override_item.attributes.mtime = now;
        
        Ok(data.len())
    }
    
    fn get_file_mode(&self, metadata: &std::fs::Metadata) -> u32 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.mode()
        }
        #[cfg(not(unix))]
        {
            if metadata.is_dir() {
                0o755
            } else {
                0o644
            }
        }
    }
    
    fn get_uid(&self, metadata: &std::fs::Metadata) -> u32 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.uid()
        }
        #[cfg(not(unix))]
        {
            501 // Default user ID
        }
    }
    
    fn get_gid(&self, metadata: &std::fs::Metadata) -> u32 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.gid()
        }
        #[cfg(not(unix))]
        {
            20 // Default group ID
        }
    }
    
    fn get_current_uid(&self) -> u32 {
        #[cfg(unix)]
        {
            unsafe { 
                extern "C" {
                    fn getuid() -> u32;
                }
                getuid()
            }
        }
        #[cfg(not(unix))]
        {
            501 // Default user ID
        }
    }
    
    fn get_current_gid(&self) -> u32 {
        #[cfg(unix)]
        {
            unsafe { 
                extern "C" {
                    fn getgid() -> u32;
                }
                getgid()
            }
        }
        #[cfg(not(unix))]
        {
            20 // Default group ID
        }
    }
    
    // Helper methods for read operations
    
    fn read_from_buffer(&self, data: &[u8], offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        let offset = offset as usize;
        
        // Check if offset is beyond the data
        if offset >= data.len() {
            return Ok(0); // EOF
        }
        
        // Calculate how much we can read
        let available = data.len() - offset;
        let to_read = min(buffer.len(), available);
        
        // Copy the data
        buffer[..to_read].copy_from_slice(&data[offset..offset + to_read]);
        
        Ok(to_read)
    }
    
    fn read_from_source(&self, path: &Path, offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        // Open the source file
        let mut file = File::open(path)
            .map_err(|e| format!("Failed to open source file: {}", e))?;
        
        // Seek to the requested offset
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Failed to seek in source file: {}", e))?;
        
        // Read the data
        let bytes_read = file.read(buffer)
            .map_err(|e| format!("Failed to read from source file: {}", e))?;
        
        Ok(bytes_read)
    }
    
    // Helper methods
    
    fn get_item_path(&self, item: &AnyObject) -> Result<PathBuf, String> {
        unsafe {
            let path: *mut AnyObject = msg_send![item, path];
            if path.is_null() {
                return Err("Failed to get item path".to_string());
            }
            
            let path_cstr: *const i8 = msg_send![path, UTF8String];
            let path_str = CStr::from_ptr(path_cstr)
                .to_str()
                .map_err(|e| format!("Invalid path encoding: {}", e))?;
            
            Ok(PathBuf::from(path_str))
        }
    }
    
    fn get_file_size(&self, path: &Path) -> Result<u64, String> {
        // Check override store first
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            if let Some(override_item) = override_store.items.get(path) {
                // Return size from override item
                return Ok(override_item.attributes.size);
            } else if override_store.deleted_paths.contains(path) {
                // File has been deleted
                return Err("File has been deleted".to_string());
            }
        }
        
        // Fall back to source filesystem
        match std::fs::metadata(path) {
            Ok(metadata) => Ok(metadata.len()),
            Err(e) => Err(format!("Failed to get file size: {}", e))
        }
    }
    
    fn truncate_file(&self, path: &Path) -> Result<(), String> {
        // Ensure file is in override store
        self.ensure_in_override(path)?;
        
        // Truncate the file in override store
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        if let Some(override_item) = override_store.items.get_mut(path) {
            // Clear the data
            override_item.data = Some(Vec::new());
            override_item.attributes.size = 0;
            
            // Update modification time
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            override_item.attributes.mtime = now;
        }
        
        Ok(())
    }
    
}

/// Seek whence values for file positioning
#[derive(Debug, Clone, Copy)]
pub enum SeekWhence {
    Start,
    Current,
    End,
}

#[macro_export]
macro_rules! class {
    ($name:ident) => {{
        static CLASS: std::sync::OnceLock<&'static objc2::runtime::AnyClass> = std::sync::OnceLock::new();
        CLASS.get_or_init(|| {
            objc2::runtime::AnyClass::get(stringify!($name))
                .expect(concat!("Class ", stringify!($name), " not found"))
        })
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_open_mode_flags() {
        assert!(OpenMode::ReadOnly.can_read());
        assert!(!OpenMode::ReadOnly.can_write());
        
        assert!(!OpenMode::WriteOnly.can_read());
        assert!(OpenMode::WriteOnly.can_write());
        
        assert!(OpenMode::ReadWrite.can_read());
        assert!(OpenMode::ReadWrite.can_write());
        
        assert!(!OpenMode::Append.can_read());
        assert!(OpenMode::Append.can_write());
    }
    
    #[test]
    fn test_file_context_creation() {
        let context = FileContext::new();
        assert!(context.write_buffer.is_empty());
        assert!(context.read_cache.is_none());
        assert_eq!(context.cache_offset, 0);
        assert_eq!(context.cache_length, 0);
        assert!(!context.is_dirty);
    }
    
    #[test]
    fn test_file_handle_creation() {
        let handle = FSFileHandle {
            id: 1,
            path: PathBuf::from("/test/file.txt"),
            mode: OpenMode::ReadWrite,
            position: 0,
            ref_count: 1,
            context: Some(FileContext::new()),
        };
        
        assert_eq!(handle.id, 1);
        assert_eq!(handle.path, PathBuf::from("/test/file.txt"));
        assert_eq!(handle.mode, OpenMode::ReadWrite);
        assert_eq!(handle.position, 0);
        assert_eq!(handle.ref_count, 1);
        assert!(handle.context.is_some());
    }
}