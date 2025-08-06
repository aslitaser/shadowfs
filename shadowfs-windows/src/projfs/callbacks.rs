use std::sync::{Arc, Weak};
use std::path::PathBuf;
use std::io::{Read, Seek, SeekFrom};
use std::fs::File;
use parking_lot::RwLock;
use windows::core::{GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Storage::ProjectedFileSystem::{
    PRJ_CALLBACK_DATA,
    PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
    PRJ_DIR_ENTRY_BUFFER_HANDLE,
    PRJ_PLACEHOLDER_INFO,
    PrjFileNameMatch,
    PrjFillDirEntryBuffer,
    PrjWritePlaceholderInfo,
    PrjWriteFileData,
};
use windows::Win32::Foundation::{S_OK, E_OUTOFMEMORY, E_INVALIDARG, ERROR_INSUFFICIENT_BUFFER, WIN32_ERROR};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_BASIC_INFO,
};
use super::provider::{ProjFSProvider, EnumerationSession};
use shadowfs_core::types::ShadowPath;
use shadowfs_core::override_store::DirectoryEntry;

/// Thread-safe callback context for ProjFS operations
pub struct CallbackContext {
    /// Weak reference to the provider to prevent circular references
    provider: Weak<RwLock<ProjFSProvider>>,
    
    /// Shared state for cross-callback communication
    shared_state: Arc<SharedCallbackState>,
}

/// Shared state accessible across callbacks
pub struct SharedCallbackState {
    /// Current operation ID for tracking
    pub operation_id: RwLock<u64>,
    
    /// Path to the virtualization root
    pub virtualization_root: PathBuf,
    
    /// Path to the source root
    pub source_root: PathBuf,
}

impl CallbackContext {
    /// Creates a new callback context
    pub fn new(
        provider: Weak<RwLock<ProjFSProvider>>,
        virtualization_root: PathBuf,
        source_root: PathBuf,
    ) -> Self {
        Self {
            provider,
            shared_state: Arc::new(SharedCallbackState {
                operation_id: RwLock::new(0),
                virtualization_root,
                source_root,
            }),
        }
    }
    
    /// Gets a strong reference to the provider if it still exists
    pub fn get_provider(&self) -> Option<Arc<RwLock<ProjFSProvider>>> {
        self.provider.upgrade()
    }
    
    /// Gets the shared state
    pub fn shared_state(&self) -> &Arc<SharedCallbackState> {
        &self.shared_state
    }
    
    /// Generates a new operation ID
    pub fn next_operation_id(&self) -> u64 {
        let mut id = self.shared_state.operation_id.write();
        *id += 1;
        *id
    }
}

impl SharedCallbackState {
    /// Resolves a relative path to the source file system
    pub fn resolve_source_path(&self, relative_path: &str) -> PathBuf {
        self.source_root.join(relative_path)
    }
    
    /// Resolves a relative path to the virtualization root
    pub fn resolve_virtual_path(&self, relative_path: &str) -> PathBuf {
        self.virtualization_root.join(relative_path)
    }
}

/// Helper to safely extract callback context from raw pointer
pub unsafe fn get_context(
    namespace_virtualization_context: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
) -> Option<Arc<CallbackContext>> {
    if namespace_virtualization_context.is_null() {
        return None;
    }
    
    let context_ptr = namespace_virtualization_context as *const CallbackContext;
    let context = Arc::from_raw(context_ptr);
    
    // Increment reference count to keep context alive
    let cloned = context.clone();
    
    // Don't drop the original Arc to maintain proper reference counting
    std::mem::forget(context);
    
    Some(cloned)
}

/// Helper to convert Windows path to Rust PathBuf
pub fn windows_path_to_pathbuf(path: &[u16]) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use std::ffi::OsString;
    
    let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let os_string = OsString::from_wide(&path[..len]);
    PathBuf::from(os_string)
}

/// Helper to convert PCWSTR to Option<String>
unsafe fn pcwstr_to_string(s: PCWSTR) -> Option<String> {
    if s.is_null() {
        return None;
    }
    
    let len = (0..).take_while(|&i| *s.0.offset(i) != 0).count();
    if len == 0 {
        return None;
    }
    
    let slice = std::slice::from_raw_parts(s.0, len);
    String::from_utf16(slice).ok()
}

/// Start directory enumeration callback
/// This is called when the system wants to enumerate files in a directory
pub extern "system" fn start_directory_enumeration_callback(
    callback_data: *const PRJ_CALLBACK_DATA,
    enumeration_id: *const GUID,
) -> HRESULT {
    unsafe {
        // Validate input parameters
        if callback_data.is_null() || enumeration_id.is_null() {
            return E_INVALIDARG;
        }
        
        let callback_data = &*callback_data;
        
        // Get the callback context
        let context = match get_context(callback_data.NamespaceVirtualizationContext) {
            Some(ctx) => ctx,
            None => return E_INVALIDARG,
        };
        
        // Get the provider
        let provider = match context.get_provider() {
            Some(p) => p,
            None => return E_OUTOFMEMORY,
        };
        
        // Convert the file path
        let file_path = if !callback_data.FilePathName.is_null() {
            pcwstr_to_string(callback_data.FilePathName)
                .unwrap_or_else(|| String::new())
        } else {
            String::new()
        };
        
        // Convert the search expression (wildcard pattern)
        let search_expression = if !callback_data.FilePathName.is_null() {
            // The search expression is typically stored after the path in the callback data
            // For now, we'll extract it from the callback data if available
            None // This would need proper extraction from callback data
        } else {
            None
        };
        
        // Create the enumeration session
        let session = EnumerationSession {
            search_expression,
            directory_path: PathBuf::from(&file_path),
            is_restart: false,
            continuation_token: None,
        };
        
        // Store the enumeration session
        {
            let provider = provider.read();
            provider.active_enumerations.insert(*enumeration_id, session);
        }
        
        // Log the operation
        let operation_id = context.next_operation_id();
        log::debug!(
            "StartDirectoryEnumeration[{}]: Path={}, EnumId={:?}",
            operation_id,
            file_path,
            enumeration_id
        );
        
        S_OK
    }
}

/// Get directory enumeration callback
/// This is called to get the actual directory entries during enumeration
pub extern "system" fn get_directory_enumeration_callback(
    callback_data: *const PRJ_CALLBACK_DATA,
    enumeration_id: *const GUID,
    search_expression: PCWSTR,
    dir_entry_buffer_handle: PRJ_DIR_ENTRY_BUFFER_HANDLE,
) -> HRESULT {
    unsafe {
        // Validate input parameters
        if callback_data.is_null() || enumeration_id.is_null() {
            return E_INVALIDARG;
        }
        
        let callback_data = &*callback_data;
        
        // Get the callback context
        let context = match get_context(callback_data.NamespaceVirtualizationContext) {
            Some(ctx) => ctx,
            None => return E_INVALIDARG,
        };
        
        // Get the provider
        let provider = match context.get_provider() {
            Some(p) => p,
            None => return E_OUTOFMEMORY,
        };
        
        // Get or update the enumeration session
        let (directory_path, continuation_token) = {
            let provider = provider.read();
            match provider.active_enumerations.get(enumeration_id) {
                Some(session) => {
                    let mut session = session.clone();
                    
                    // Update search expression if provided
                    if !search_expression.is_null() {
                        session.search_expression = pcwstr_to_string(search_expression);
                    }
                    
                    (session.directory_path.clone(), session.continuation_token.clone())
                }
                None => {
                    log::error!("Enumeration session not found for ID: {:?}", enumeration_id);
                    return E_INVALIDARG;
                }
            }
        };
        
        // Resolve the actual directory path
        let source_path = context.shared_state().resolve_source_path(
            directory_path.to_str().unwrap_or("")
        );
        
        // Collect entries from override store first
        let mut entries = Vec::new();
        
        // Check override store for this directory
        {
            let provider = provider.read();
            let shadow_path = ShadowPath::from(directory_path.clone());
            if let Ok(dir_entries) = provider.override_store.list_directory(&shadow_path) {
                for entry in dir_entries {
                    entries.push(entry.name);
                }
            }
        }
        
        // Read entries from source directory
        if source_path.exists() && source_path.is_dir() {
            match std::fs::read_dir(&source_path) {
                Ok(read_dir) => {
                    for entry in read_dir.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            let file_name = entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            
                            // Check if this entry matches the search pattern
                            let matches = if let Some(ref session) = provider.read().active_enumerations.get(enumeration_id) {
                                if let Some(ref pattern) = session.search_expression {
                                    // Use ProjFS pattern matching
                                    let pattern_wide = pattern.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
                                    let name_wide = file_name_str.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
                                    
                                    PrjFileNameMatch(
                                        PCWSTR::from_raw(name_wide.as_ptr()),
                                        PCWSTR::from_raw(pattern_wide.as_ptr())
                                    ).as_bool()
                                } else {
                                    true
                                }
                            } else {
                                true
                            };
                            
                            if !matches {
                                continue;
                            }
                            
                            // Create file info
                            let mut file_info = FILE_BASIC_INFO {
                                CreationTime: Default::default(),
                                LastAccessTime: Default::default(),
                                LastWriteTime: Default::default(),
                                ChangeTime: Default::default(),
                                FileAttributes: if metadata.is_dir() {
                                    FILE_ATTRIBUTE_DIRECTORY
                                } else {
                                    FILE_ATTRIBUTE_NORMAL
                                },
                            };
                            
                            // Convert file name to wide string
                            let file_name_wide = file_name_str.encode_utf16()
                                .chain(std::iter::once(0))
                                .collect::<Vec<u16>>();
                            
                            // Add entry to buffer
                            let result = PrjFillDirEntryBuffer(
                                PCWSTR::from_raw(file_name_wide.as_ptr()),
                                Some(&file_info as *const _ as *const _),
                                dir_entry_buffer_handle,
                            );
                            
                            // Check if buffer is full
                            if result == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0) {
                                // Save continuation token
                                let mut provider = provider.write();
                                if let Some(mut session) = provider.active_enumerations.get_mut(enumeration_id) {
                                    session.continuation_token = Some(file_name.as_encoded_bytes().to_vec());
                                }
                                return S_OK;
                            } else if result.is_err() {
                                log::error!("Failed to fill directory entry buffer: {:?}", result);
                                return result.into();
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to read directory {}: {}", source_path.display(), e);
                }
            }
        }
        
        // Clear continuation token when enumeration is complete
        {
            let mut provider = provider.write();
            if let Some(mut session) = provider.active_enumerations.get_mut(enumeration_id) {
                session.continuation_token = None;
            }
        }
        
        S_OK
    }
}

/// End directory enumeration callback
/// This is called when directory enumeration is complete or cancelled
pub extern "system" fn end_directory_enumeration_callback(
    callback_data: *const PRJ_CALLBACK_DATA,
    enumeration_id: *const GUID,
) -> HRESULT {
    unsafe {
        // Validate input parameters
        if callback_data.is_null() || enumeration_id.is_null() {
            return E_INVALIDARG;
        }
        
        let callback_data = &*callback_data;
        
        // Get the callback context
        let context = match get_context(callback_data.NamespaceVirtualizationContext) {
            Some(ctx) => ctx,
            None => return E_INVALIDARG,
        };
        
        // Get the provider
        let provider = match context.get_provider() {
            Some(p) => p,
            None => return E_OUTOFMEMORY,
        };
        
        // Remove the enumeration session
        let removed = {
            let mut provider = provider.write();
            provider.active_enumerations.remove(enumeration_id)
        };
        
        // Log the operation
        let operation_id = context.next_operation_id();
        if let Some((_, session)) = removed {
            log::debug!(
                "EndDirectoryEnumeration[{}]: Path={}, EnumId={:?}",
                operation_id,
                session.directory_path.display(),
                enumeration_id
            );
        } else {
            log::warn!(
                "EndDirectoryEnumeration[{}]: Session not found for EnumId={:?}",
                operation_id,
                enumeration_id
            );
        }
        
        // Update stats
        {
            let provider = provider.read();
            provider.stats.increment_directory_enumerations();
        }
        
        S_OK
    }
}

/// Get placeholder info callback
/// This is called when the system needs metadata for a virtualized file/directory
pub extern "system" fn get_placeholder_info_callback(
    callback_data: *const PRJ_CALLBACK_DATA,
) -> HRESULT {
    unsafe {
        // Validate input parameters
        if callback_data.is_null() {
            return E_INVALIDARG;
        }
        
        let callback_data = &*callback_data;
        
        // Get the callback context
        let context = match get_context(callback_data.NamespaceVirtualizationContext) {
            Some(ctx) => ctx,
            None => return E_INVALIDARG,
        };
        
        // Get the provider
        let provider = match context.get_provider() {
            Some(p) => p,
            None => return E_OUTOFMEMORY,
        };
        
        // Convert the file path
        let file_path = if !callback_data.FilePathName.is_null() {
            pcwstr_to_string(callback_data.FilePathName)
                .unwrap_or_else(|| String::new())
        } else {
            String::new()
        };
        
        let path_buf = PathBuf::from(&file_path);
        
        // First check override store for metadata
        let shadow_path = ShadowPath::from(path_buf.clone());
        let override_entry = {
            let provider = provider.read();
            provider.override_store.get(&shadow_path)
        };
        
        // If not in override store, get from source file system
        let (metadata, is_symlink) = if let Some(entry) = override_entry {
            // Get metadata from override entry
            let meta = entry.metadata().clone();
            (meta, false)
        } else {
            let source_path = context.shared_state().resolve_source_path(&file_path);
            
            match std::fs::symlink_metadata(&source_path) {
                Ok(meta) => {
                    let is_symlink = meta.file_type().is_symlink();
                    (meta, is_symlink)
                }
                Err(e) => {
                    log::error!("Failed to get metadata for {}: {}", source_path.display(), e);
                    return HRESULT::from(WIN32_ERROR(e.raw_os_error().unwrap_or(5) as u32)); // ERROR_ACCESS_DENIED
                }
            }
        };
        
        // Convert timestamps
        use windows::Win32::Foundation::FILETIME;
        
        // For now, use default timestamps - in a real implementation,
        // these would come from the metadata
        let creation_time = 0u64;
        let last_write_time = 0u64;
        let last_access_time = 0u64;
        let change_time = last_write_time; // Windows doesn't have separate change time
        
        // Determine file attributes
        let mut attributes = if metadata.is_dir() {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        
        if is_symlink {
            attributes |= FILE_ATTRIBUTE_REPARSE_POINT;
        }
        
        // Get file size (0 for directories)
        let file_size = if metadata.is_file() {
            metadata.len() as i64
        } else {
            0
        };
        
        // Create placeholder info
        let mut placeholder_info = PRJ_PLACEHOLDER_INFO {
            FileBasicInfo: FILE_BASIC_INFO {
                CreationTime: FILETIME {
                    dwLowDateTime: (creation_time & 0xFFFFFFFF) as u32,
                    dwHighDateTime: ((creation_time >> 32) & 0xFFFFFFFF) as u32,
                }.into(),
                LastAccessTime: FILETIME {
                    dwLowDateTime: (last_access_time & 0xFFFFFFFF) as u32,
                    dwHighDateTime: ((last_access_time >> 32) & 0xFFFFFFFF) as u32,
                }.into(),
                LastWriteTime: FILETIME {
                    dwLowDateTime: (last_write_time & 0xFFFFFFFF) as u32,
                    dwHighDateTime: ((last_write_time >> 32) & 0xFFFFFFFF) as u32,
                }.into(),
                ChangeTime: FILETIME {
                    dwLowDateTime: (change_time & 0xFFFFFFFF) as u32,
                    dwHighDateTime: ((change_time >> 32) & 0xFFFFFFFF) as u32,
                }.into(),
                FileAttributes: attributes.0,
            },
            ..Default::default()
        };
        
        // Set up symlink info if needed
        if is_symlink {
            // For symbolic links, we need to set up reparse data
            // This would require reading the link target and setting up the reparse buffer
            // For now, we'll just mark it as a reparse point
            placeholder_info.FileBasicInfo.FileAttributes |= FILE_ATTRIBUTE_REPARSE_POINT.0;
        }
        
        // Write placeholder info
        let result = PrjWritePlaceholderInfo(
            callback_data.NamespaceVirtualizationContext,
            callback_data.FilePathName,
            &placeholder_info,
            std::mem::size_of::<PRJ_PLACEHOLDER_INFO>() as u32,
        );
        
        if result.is_err() {
            log::error!(
                "Failed to write placeholder info for {}: {:?}",
                file_path,
                result
            );
            return result.into();
        }
        
        // Log the operation
        let operation_id = context.next_operation_id();
        log::debug!(
            "GetPlaceholderInfo[{}]: Path={}, IsDir={}, Size={}",
            operation_id,
            file_path,
            metadata.is_dir(),
            file_size
        );
        
        // Update stats
        {
            let provider = provider.read();
            provider.stats.increment_placeholder_creations();
        }
        
        S_OK
    }
}

/// Get file data callback
/// This is called when the system needs to read actual file contents
pub extern "system" fn get_file_data_callback(
    callback_data: *const PRJ_CALLBACK_DATA,
    byte_offset: u64,
    length: u32,
) -> HRESULT {
    unsafe {
        // Validate input parameters
        if callback_data.is_null() {
            return E_INVALIDARG;
        }
        
        let callback_data = &*callback_data;
        
        // Get the callback context
        let context = match get_context(callback_data.NamespaceVirtualizationContext) {
            Some(ctx) => ctx,
            None => return E_INVALIDARG,
        };
        
        // Get the provider
        let provider = match context.get_provider() {
            Some(p) => p,
            None => return E_OUTOFMEMORY,
        };
        
        // Convert the file path
        let file_path = if !callback_data.FilePathName.is_null() {
            pcwstr_to_string(callback_data.FilePathName)
                .unwrap_or_else(|| String::new())
        } else {
            String::new()
        };
        
        let path_buf = PathBuf::from(&file_path);
        
        // Check if we have override data for this file
        let shadow_path = ShadowPath::from(path_buf.clone());
        let override_entry = {
            let provider = provider.read();
            provider.override_store.get(&shadow_path)
        };
        
        // Get file data from either override store or source
        let file_data = if let Some(entry) = override_entry {
            // Get data from override entry
            match entry.get_file_data() {
                Ok(Some(data)) => Some(data),
                Ok(None) => None,
                Err(e) => {
                    log::error!("Failed to get override file data {}: {}", file_path, e);
                    return HRESULT::from(WIN32_ERROR(5)); // ERROR_ACCESS_DENIED
                }
            }
        } else {
            None
        };
        
        // If we have override data, use it; otherwise open from source
        if let Some(data) = file_data {
            // Write the data directly from memory
            let data_slice = &data[byte_offset as usize..];
            let to_write = std::cmp::min(length as usize, data_slice.len());
            
            if to_write > 0 {
                let result = PrjWriteFileData(
                    callback_data.NamespaceVirtualizationContext,
                    &callback_data.DataStreamId,
                    data_slice[..to_write].as_ptr() as *const _,
                    byte_offset,
                    to_write as u32,
                );
                
                if result.is_err() {
                    log::error!(
                        "Failed to write file data for {} at offset {}: {:?}",
                        file_path,
                        byte_offset,
                        result
                    );
                    return result.into();
                }
                
                // Update stats
                {
                    let provider = provider.read();
                    provider.stats.add_bytes_read(to_write as u64);
                }
            }
        } else {
            // Open from source file system
            let source_path = context.shared_state().resolve_source_path(&file_path);
            match File::open(&source_path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Failed to open source file {}: {}", source_path.display(), e);
                    return HRESULT::from(WIN32_ERROR(e.raw_os_error().unwrap_or(5) as u32));
                }
            }
        };
        
        // Seek to the requested offset
        if let Err(e) = file.seek(SeekFrom::Start(byte_offset)) {
            log::error!("Failed to seek to offset {} in {}: {}", byte_offset, file_path, e);
            return HRESULT::from_win32(e.raw_os_error().unwrap_or(5) as u32);
        }
        
        // Allocate buffer for reading - use 64KB chunks for efficiency
        const BUFFER_SIZE: usize = 65536;
        let mut buffer = vec![0u8; BUFFER_SIZE.min(length as usize)];
        let mut total_written = 0u32;
        let mut current_offset = byte_offset;
        
        // Read and write data in chunks
        while total_written < length {
            let to_read = ((length - total_written) as usize).min(buffer.len());
            
            match file.read(&mut buffer[..to_read]) {
                Ok(0) => {
                    // End of file reached
                    break;
                }
                Ok(bytes_read) => {
                    // Write data to ProjFS
                    let result = PrjWriteFileData(
                        callback_data.NamespaceVirtualizationContext,
                        &callback_data.DataStreamId,
                        buffer[..bytes_read].as_ptr() as *const _,
                        current_offset,
                        bytes_read as u32,
                    );
                    
                    if result.is_err() {
                        log::error!(
                            "Failed to write file data for {} at offset {}: {:?}",
                            file_path,
                            current_offset,
                            result
                        );
                        return result.into();
                    }
                    
                    total_written += bytes_read as u32;
                    current_offset += bytes_read as u64;
                    
                    // Update read statistics
                    {
                        let provider = provider.read();
                        provider.stats.add_bytes_read(bytes_read as u64);
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to read from {} at offset {}: {}",
                        file_path,
                        current_offset,
                        e
                    );
                    return HRESULT::from(WIN32_ERROR(e.raw_os_error().unwrap_or(5) as u32));
                }
            }
        }
        
        // Log the operation
        let operation_id = context.next_operation_id();
        log::debug!(
            "GetFileData[{}]: Path={}, Offset={}, Length={}, BytesRead={}",
            operation_id,
            file_path,
            byte_offset,
            length,
            total_written
        );
        
        // Update stats
        {
            let provider = provider.read();
            provider.stats.increment_file_reads();
        }
        
        S_OK
    }
}