use super::provider::FSKitProvider;
use objc2::rc::Weak;
use objc2::{msg_send, msg_send_id, ClassType};
use objc2::runtime::{AnyObject, ProtocolObject};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::ffi::CStr;

#[cfg(unix)]
use libc;

#[derive(Debug)]
pub struct FSOperationsImpl {
    provider: Weak<FSKitProvider>,
    state: Arc<RwLock<OperationsState>>,
    override_store: Arc<RwLock<OverrideStore>>,
    case_sensitive: bool,
}

#[derive(Debug, Default)]
struct OperationsState {
    open_files: HashMap<u64, FileHandle>,
    active_operations: HashMap<u64, OperationType>,
    next_handle_id: u64,
}

#[derive(Debug, Default)]
struct OverrideStore {
    items: HashMap<PathBuf, OverrideItem>,
    deleted_paths: HashSet<PathBuf>,
}

#[derive(Debug, Clone)]
struct OverrideItem {
    path: PathBuf,
    item_type: FSItemType,
    attributes: FileAttributes,
    data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
enum FSItemType {
    File,
    Directory,
    SymbolicLink,
}

#[derive(Debug, Clone)]
struct DirectoryEntry {
    name: String,
    path: PathBuf,
    item_type: FSItemType,
    attributes: FileAttributes,
    is_deleted: bool,
    is_override: bool,
}

use std::collections::HashSet;

#[derive(Debug, Clone)]
struct FileHandle {
    id: u64,
    path: PathBuf,
    flags: u32,
    ref_count: usize,
}

#[derive(Debug, Clone)]
enum OperationType {
    Read { offset: u64, length: usize },
    Write { offset: u64, data: Vec<u8> },
    Create { path: PathBuf },
    Delete { path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
}

impl FSOperationsImpl {
    pub fn new(provider: Weak<FSKitProvider>) -> Self {
        Self {
            provider,
            state: Arc::new(RwLock::new(OperationsState::default())),
            override_store: Arc::new(RwLock::new(OverrideStore::default())),
            case_sensitive: false, // Default to case-insensitive for macOS
        }
    }

    pub fn new_with_options(provider: Weak<FSKitProvider>, case_sensitive: bool) -> Self {
        Self {
            provider,
            state: Arc::new(RwLock::new(OperationsState::default())),
            override_store: Arc::new(RwLock::new(OverrideStore::default())),
            case_sensitive,
        }
    }

    pub fn lookup_item_named(&self, parent: &AnyObject, name: &str) -> Result<*mut AnyObject, String> {
        // Build the full path for the item
        let parent_path = self.get_item_path(parent)?;
        let item_path = parent_path.join(name);
        
        // Check override store first
        if let Some(item) = self.check_override_store(&item_path)? {
            return Ok(item);
        }

        // Check if item is marked as deleted in override store
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            if override_store.deleted_paths.contains(&item_path) {
                return Err(format!("Item '{}' has been deleted", name));
            }
        }

        // Fall back to source filesystem
        self.lookup_source_filesystem(parent, name, &item_path)
    }

    fn check_override_store(&self, path: &Path) -> Result<Option<*mut AnyObject>, String> {
        let override_store = self.override_store.read()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;

        // Handle case sensitivity
        let lookup_path = if self.case_sensitive {
            path.to_path_buf()
        } else {
            // For case-insensitive lookup, normalize the path
            self.normalize_path_case(path, &override_store)?
        };

        if let Some(override_item) = override_store.items.get(&lookup_path) {
            // Create appropriate FSItem subclass based on type
            let fs_item = self.create_fs_item(override_item)?;
            return Ok(Some(fs_item));
        }

        Ok(None)
    }

    fn lookup_source_filesystem(&self, parent: &AnyObject, name: &str, item_path: &Path) -> Result<*mut AnyObject, String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        unsafe {
            // Query the source filesystem
            let source_path = self.get_source_path(item_path)?;
            
            // Check if file exists on source filesystem
            if !Path::new(&source_path).exists() {
                return Err(format!("Item '{}' not found", name));
            }

            // Get file metadata from source
            let metadata = std::fs::metadata(&source_path)
                .map_err(|e| format!("Failed to get metadata: {}", e))?;

            // Create appropriate FSItem based on file type
            let item_type = if metadata.is_dir() {
                FSItemType::Directory
            } else if metadata.is_symlink() {
                FSItemType::SymbolicLink
            } else {
                FSItemType::File
            };

            // Create FSItem with source filesystem attributes
            let attributes = FileAttributes {
                size: metadata.len(),
                mode: self.get_file_mode(&metadata),
                uid: self.get_uid(&metadata),
                gid: self.get_gid(&metadata),
                atime: 0, // Will be populated from metadata
                mtime: 0, // Will be populated from metadata
                ctime: 0, // Will be populated from metadata
            };

            let fs_item = self.create_fs_item_with_attrs(item_path, item_type, attributes)?;
            Ok(fs_item)
        }
    }

    fn create_fs_item(&self, override_item: &OverrideItem) -> Result<*mut AnyObject, String> {
        self.create_fs_item_with_attrs(
            &override_item.path,
            override_item.item_type.clone(),
            override_item.attributes.clone()
        )
    }

    fn create_fs_item_with_attrs(&self, path: &Path, item_type: FSItemType, attrs: FileAttributes) -> Result<*mut AnyObject, String> {
        unsafe {
            let path_str = path.to_str()
                .ok_or_else(|| "Invalid path encoding".to_string())?;
            
            let path_nsstring: *mut AnyObject = msg_send![
                class!(NSString),
                stringWithUTF8String: path_str.as_ptr()
            ];

            // Create appropriate FSItem subclass based on type
            let item_class = match item_type {
                FSItemType::File => class!(FSKitFile),
                FSItemType::Directory => class!(FSKitDirectory),
                FSItemType::SymbolicLink => class!(FSKitSymlink),
            };

            let item: *mut AnyObject = msg_send![item_class, alloc];
            let item: *mut AnyObject = msg_send![item, initWithPath: path_nsstring];

            // Set attributes on the item
            let _: () = msg_send![item, setFileSize: attrs.size];
            let _: () = msg_send![item, setFileMode: attrs.mode];
            let _: () = msg_send![item, setOwnerUID: attrs.uid];
            let _: () = msg_send![item, setOwnerGID: attrs.gid];

            Ok(item)
        }
    }

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

    fn get_source_path(&self, virtual_path: &Path) -> Result<String, String> {
        // Map virtual path to source filesystem path
        // This would typically involve removing a mount prefix and adding source root
        Ok(virtual_path.to_str()
            .ok_or_else(|| "Invalid path".to_string())?
            .to_string())
    }

    fn normalize_path_case(&self, path: &Path, override_store: &OverrideStore) -> Result<PathBuf, String> {
        // For case-insensitive systems, find the canonical casing
        let path_lower = path.to_str()
            .ok_or_else(|| "Invalid path encoding".to_string())?
            .to_lowercase();

        for stored_path in override_store.items.keys() {
            if let Some(stored_str) = stored_path.to_str() {
                if stored_str.to_lowercase() == path_lower {
                    return Ok(stored_path.clone());
                }
            }
        }

        Ok(path.to_path_buf())
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

    // Keep the original lookup method for backward compatibility
    pub fn lookup(&self, parent: &AnyObject, name: &str) -> Result<*mut AnyObject, String> {
        self.lookup_item_named(parent, name)
    }

    pub fn read_directory(&self, directory: &AnyObject) -> Result<*mut AnyObject, String> {
        let dir_path = self.get_item_path(directory)?;
        
        // Create a map to track all directory entries
        let mut entries: HashMap<String, DirectoryEntry> = HashMap::new();
        
        // First, enumerate source filesystem entries
        self.enumerate_source_entries(&dir_path, &mut entries)?;
        
        // Then, apply override store modifications
        self.apply_override_entries(&dir_path, &mut entries)?;
        
        // Finally, create and return FSDirectoryContent
        self.create_directory_content(entries)
    }

    fn enumerate_source_entries(&self, dir_path: &Path, entries: &mut HashMap<String, DirectoryEntry>) -> Result<(), String> {
        let source_path = self.get_source_path(dir_path)?;
        
        // Check if directory exists on source filesystem
        if !Path::new(&source_path).exists() {
            return Ok(()); // Directory doesn't exist on source, only use override entries
        }
        
        // Read directory entries from source filesystem
        let dir_entries = std::fs::read_dir(&source_path)
            .map_err(|e| format!("Failed to read directory: {}", e))?;
        
        for entry in dir_entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let file_name = entry.file_name()
                .to_str()
                .ok_or_else(|| "Invalid filename encoding".to_string())?
                .to_string();
            
            let metadata = entry.metadata()
                .map_err(|e| format!("Failed to get metadata: {}", e))?;
            
            let item_type = if metadata.is_dir() {
                FSItemType::Directory
            } else if metadata.is_symlink() {
                FSItemType::SymbolicLink
            } else {
                FSItemType::File
            };
            
            let attributes = FileAttributes {
                size: metadata.len(),
                mode: self.get_file_mode(&metadata),
                uid: self.get_uid(&metadata),
                gid: self.get_gid(&metadata),
                atime: 0,
                mtime: 0,
                ctime: 0,
            };
            
            entries.insert(file_name.clone(), DirectoryEntry {
                name: file_name,
                path: dir_path.join(&entry.file_name()),
                item_type,
                attributes,
                is_deleted: false,
                is_override: false,
            });
        }
        
        Ok(())
    }

    fn apply_override_entries(&self, dir_path: &Path, entries: &mut HashMap<String, DirectoryEntry>) -> Result<(), String> {
        let override_store = self.override_store.read()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        // Process deleted items (tombstones)
        for deleted_path in &override_store.deleted_paths {
            if let Some(parent) = deleted_path.parent() {
                if parent == dir_path {
                    if let Some(file_name) = deleted_path.file_name() {
                        let name = file_name.to_str()
                            .ok_or_else(|| "Invalid filename encoding".to_string())?;
                        
                        // Handle case sensitivity for deletion
                        if self.case_sensitive {
                            entries.remove(name);
                        } else {
                            // Case-insensitive removal
                            let name_lower = name.to_lowercase();
                            entries.retain(|k, _| k.to_lowercase() != name_lower);
                        }
                    }
                }
            }
        }
        
        // Add or update entries from override store
        for (override_path, override_item) in &override_store.items {
            if let Some(parent) = override_path.parent() {
                if parent == dir_path {
                    if let Some(file_name) = override_path.file_name() {
                        let name = file_name.to_str()
                            .ok_or_else(|| "Invalid filename encoding".to_string())?
                            .to_string();
                        
                        // Handle case sensitivity for override entries
                        let entry_key = if self.case_sensitive {
                            name.clone()
                        } else {
                            // Find existing entry with case-insensitive match
                            entries.keys()
                                .find(|k| k.to_lowercase() == name.to_lowercase())
                                .cloned()
                                .unwrap_or(name.clone())
                        };
                        
                        // Insert or update the entry
                        entries.insert(entry_key, DirectoryEntry {
                            name,
                            path: override_path.clone(),
                            item_type: override_item.item_type.clone(),
                            attributes: override_item.attributes.clone(),
                            is_deleted: false,
                            is_override: true,
                        });
                    }
                }
            }
        }
        
        Ok(())
    }

    fn create_directory_content(&self, entries: HashMap<String, DirectoryEntry>) -> Result<*mut AnyObject, String> {
        unsafe {
            // Create FSDirectoryContent object
            let content_class = class!(FSDirectoryContent);
            let content: *mut AnyObject = msg_send![content_class, alloc];
            let content: *mut AnyObject = msg_send![content, init];
            
            // Create NSMutableArray for entries
            let entries_array: *mut AnyObject = msg_send![class!(NSMutableArray), array];
            
            // Add each entry to the array
            for (_name, entry) in entries {
                let fs_item = self.create_fs_item_with_attrs(
                    &entry.path,
                    entry.item_type,
                    entry.attributes
                )?;
                
                // Add metadata to indicate if this is an override entry
                if entry.is_override {
                    let _: () = msg_send![fs_item, setIsOverride: true];
                }
                
                let _: () = msg_send![entries_array, addObject: fs_item];
            }
            
            // Set the entries array on the content object
            let _: () = msg_send![content, setEntries: entries_array];
            
            Ok(content)
        }
    }

    pub fn get_attributes(&self, item: &AnyObject) -> Result<FileAttributes, String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        let state = self.state.read()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        unsafe {
            let attrs: *mut AnyObject = msg_send![
                &**provider,
                attributesOfItem: item
            ];

            if attrs.is_null() {
                return Err("Failed to get attributes".to_string());
            }

            let size: u64 = msg_send![attrs, fileSize];
            let mode: u32 = msg_send![attrs, fileMode];
            let uid: u32 = msg_send![attrs, ownerUID];
            let gid: u32 = msg_send![attrs, ownerGID];

            Ok(FileAttributes {
                size,
                mode,
                uid,
                gid,
                atime: 0,
                mtime: 0,
                ctime: 0,
            })
        }
    }

    pub fn open_file(&self, item: &AnyObject, flags: u32) -> Result<u64, String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        let mut state = self.state.write()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        unsafe {
            let path: *mut AnyObject = msg_send![item, path];
            let path_string: *const i8 = msg_send![path, UTF8String];
            let path_str = std::ffi::CStr::from_ptr(path_string)
                .to_string_lossy()
                .into_owned();

            let handle_id = state.next_handle_id;
            state.next_handle_id += 1;

            let handle = FileHandle {
                id: handle_id,
                path: PathBuf::from(path_str),
                flags,
                ref_count: 1,
            };

            state.open_files.insert(handle_id, handle);

            let open_result: *mut AnyObject = msg_send![
                &**provider,
                openItem: item,
                withMode: flags as i32
            ];

            if open_result.is_null() {
                state.open_files.remove(&handle_id);
                Err("Failed to open file".to_string())
            } else {
                Ok(handle_id)
            }
        }
    }

    pub fn close_file(&self, handle_id: u64) -> Result<(), String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        let mut state = self.state.write()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        if let Some(mut handle) = state.open_files.get_mut(&handle_id) {
            handle.ref_count = handle.ref_count.saturating_sub(1);
            
            if handle.ref_count == 0 {
                state.open_files.remove(&handle_id);
                
                unsafe {
                    let _: () = msg_send![
                        &**provider,
                        closeFileHandle: handle_id as i64
                    ];
                }
            }
            Ok(())
        } else {
            Err(format!("Invalid file handle: {}", handle_id))
        }
    }

    pub fn read_file(&self, handle_id: u64, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        let mut state = self.state.write()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        if !state.open_files.contains_key(&handle_id) {
            return Err(format!("Invalid file handle: {}", handle_id));
        }

        let op_id = state.next_handle_id;
        state.next_handle_id += 1;
        
        state.active_operations.insert(op_id, OperationType::Read { offset, length });

        unsafe {
            let data: *mut AnyObject = msg_send![
                &**provider,
                readFromFileHandle: handle_id as i64,
                offset: offset as i64,
                length: length
            ];

            state.active_operations.remove(&op_id);

            if data.is_null() {
                Err("Failed to read file".to_string())
            } else {
                let bytes: *const u8 = msg_send![data, bytes];
                let len: usize = msg_send![data, length];
                
                let mut result = Vec::with_capacity(len);
                std::ptr::copy_nonoverlapping(bytes, result.as_mut_ptr(), len);
                result.set_len(len);
                
                Ok(result)
            }
        }
    }

    pub fn write_file(&self, handle_id: u64, offset: u64, data: &[u8]) -> Result<usize, String> {
        let provider = self.provider.upgrade()
            .ok_or_else(|| "Provider deallocated".to_string())?;

        let mut state = self.state.write()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        if !state.open_files.contains_key(&handle_id) {
            return Err(format!("Invalid file handle: {}", handle_id));
        }

        let op_id = state.next_handle_id;
        state.next_handle_id += 1;
        
        state.active_operations.insert(op_id, OperationType::Write { 
            offset, 
            data: data.to_vec() 
        });

        unsafe {
            let ns_data: *mut AnyObject = msg_send![
                class!(NSData),
                dataWithBytes: data.as_ptr() as *const std::ffi::c_void,
                length: data.len()
            ];

            let written: i64 = msg_send![
                &**provider,
                writeToFileHandle: handle_id as i64,
                offset: offset as i64,
                data: ns_data
            ];

            state.active_operations.remove(&op_id);

            if written < 0 {
                Err("Failed to write file".to_string())
            } else {
                Ok(written as usize)
            }
        }
    }

    pub fn create_item_named(&self, parent: &AnyObject, name: &str, item_type: FSItemType, initial_attrs: Option<FileAttributes>) -> Result<*mut AnyObject, String> {
        // Get the parent directory path
        let parent_path = self.get_item_path(parent)?;
        let new_item_path = parent_path.join(name);
        
        // Check if item already exists in override store or source filesystem
        if self.item_exists(&new_item_path)? {
            return Err(format!("Item '{}' already exists", name));
        }
        
        // Create default attributes if not provided
        let attributes = initial_attrs.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            
            FileAttributes {
                size: 0,
                mode: match item_type {
                    FSItemType::Directory => 0o755,
                    _ => 0o644,
                },
                uid: self.get_current_uid(),
                gid: self.get_current_gid(),
                atime: now,
                mtime: now,
                ctime: now,
            }
        });
        
        // Add item to override store
        self.add_to_override_store(&new_item_path, item_type.clone(), attributes.clone())?;
        
        // Create and return the new FSItem
        self.create_fs_item_with_attrs(&new_item_path, item_type, attributes)
    }
    
    fn item_exists(&self, path: &Path) -> Result<bool, String> {
        // Check override store first
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            // Check if item is in override store
            if override_store.items.contains_key(path) {
                return Ok(true);
            }
            
            // Check if item is marked as deleted
            if override_store.deleted_paths.contains(path) {
                return Ok(false);
            }
        }
        
        // Check source filesystem
        let source_path = self.get_source_path(path)?;
        Ok(Path::new(&source_path).exists())
    }
    
    fn add_to_override_store(&self, path: &Path, item_type: FSItemType, attributes: FileAttributes) -> Result<(), String> {
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        // Remove from deleted paths if it was there
        override_store.deleted_paths.remove(path);
        
        // Create the override item
        let override_item = OverrideItem {
            path: path.to_path_buf(),
            item_type,
            attributes,
            data: if item_type == FSItemType::File {
                Some(Vec::new()) // Empty file initially
            } else {
                None
            },
        };
        
        // Insert into override store
        override_store.items.insert(path.to_path_buf(), override_item);
        
        Ok(())
    }
    
    fn get_current_uid(&self) -> u32 {
        #[cfg(unix)]
        {
            unsafe { libc::getuid() }
        }
        #[cfg(not(unix))]
        {
            501 // Default user ID
        }
    }
    
    fn get_current_gid(&self) -> u32 {
        #[cfg(unix)]
        {
            unsafe { libc::getgid() }
        }
        #[cfg(not(unix))]
        {
            20 // Default group ID
        }
    }
    
    // Keep the original create_item method for backward compatibility
    pub fn create_item(&self, parent: &AnyObject, name: &str, is_directory: bool) -> Result<*mut AnyObject, String> {
        let item_type = if is_directory {
            FSItemType::Directory
        } else {
            FSItemType::File
        };
        self.create_item_named(parent, name, item_type, None)
    }

    pub fn remove_item(&self, item: &AnyObject) -> Result<(), String> {
        // Get the item path
        let item_path = self.get_item_path(item)?;
        
        // Check if this is a directory and handle recursive deletion
        let is_directory = self.is_directory(&item_path)?;
        
        if is_directory {
            // Mark all children as deleted recursively
            self.remove_directory_recursive(&item_path)?;
        } else {
            // Mark single file as deleted
            self.mark_as_deleted(&item_path)?;
        }
        
        Ok(())
    }
    
    fn is_directory(&self, path: &Path) -> Result<bool, String> {
        // Check override store first
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            if let Some(override_item) = override_store.items.get(path) {
                return Ok(override_item.item_type == FSItemType::Directory);
            }
        }
        
        // Check source filesystem
        let source_path = self.get_source_path(path)?;
        if let Ok(metadata) = std::fs::metadata(&source_path) {
            Ok(metadata.is_dir())
        } else {
            Ok(false)
        }
    }
    
    fn remove_directory_recursive(&self, dir_path: &Path) -> Result<(), String> {
        // Get all children from both override store and source filesystem
        let children = self.get_all_children(dir_path)?;
        
        // Recursively remove all children
        for child_path in children {
            if self.is_directory(&child_path)? {
                self.remove_directory_recursive(&child_path)?;
            } else {
                self.mark_as_deleted(&child_path)?;
            }
        }
        
        // Finally mark the directory itself as deleted
        self.mark_as_deleted(dir_path)?;
        
        Ok(())
    }
    
    fn get_all_children(&self, dir_path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut children = Vec::new();
        
        // Get children from override store
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            for (path, _) in &override_store.items {
                if let Some(parent) = path.parent() {
                    if parent == dir_path && !override_store.deleted_paths.contains(path) {
                        children.push(path.clone());
                    }
                }
            }
        }
        
        // Get children from source filesystem
        let source_path = self.get_source_path(dir_path)?;
        if let Ok(entries) = std::fs::read_dir(&source_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let child_path = dir_path.join(entry.file_name());
                    
                    // Check if this child is already marked as deleted
                    let is_deleted = {
                        let override_store = self.override_store.read()
                            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
                        override_store.deleted_paths.contains(&child_path)
                    };
                    
                    if !is_deleted && !children.contains(&child_path) {
                        children.push(child_path);
                    }
                }
            }
        }
        
        Ok(children)
    }
    
    fn mark_as_deleted(&self, path: &Path) -> Result<(), String> {
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        // Add to deleted paths (tombstone)
        override_store.deleted_paths.insert(path.to_path_buf());
        
        // Remove from items if it was an override item
        override_store.items.remove(path);
        
        // Note: We never touch the actual source filesystem files
        // The deletion only exists in our override layer
        
        Ok(())
    }

    pub fn rename_item(&self, item: &AnyObject, new_name: &str, new_parent: Option<&AnyObject>) -> Result<(), String> {
        // Get the current item path
        let old_path = self.get_item_path(item)?;
        
        // Determine the new parent directory
        let new_parent_path = if let Some(parent) = new_parent {
            self.get_item_path(parent)?
        } else {
            // If no new parent specified, use the current parent
            old_path.parent()
                .ok_or_else(|| "Item has no parent directory".to_string())?
                .to_path_buf()
        };
        
        // Build the new path
        let new_path = new_parent_path.join(new_name);
        
        // Check if target already exists
        if self.item_exists(&new_path)? {
            return Err(format!("Item '{}' already exists at target location", new_name));
        }
        
        // Check if this is a directory and handle recursive renaming
        let is_directory = self.is_directory(&old_path)?;
        
        // Perform the rename operation
        if is_directory {
            self.rename_directory_recursive(&old_path, &new_path)?;
        } else {
            self.rename_single_item(&old_path, &new_path)?;
        }
        
        Ok(())
    }
    
    fn rename_single_item(&self, old_path: &Path, new_path: &Path) -> Result<(), String> {
        let mut override_store = self.override_store.write()
            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
        
        // Check if item exists in override store
        if let Some(mut override_item) = override_store.items.remove(old_path) {
            // Update the path while preserving all metadata
            override_item.path = new_path.to_path_buf();
            override_store.items.insert(new_path.to_path_buf(), override_item);
        } else {
            // Item exists only in source filesystem
            // We need to create an override entry for the renamed item
            let source_path = self.get_source_path(old_path)?;
            
            if let Ok(metadata) = std::fs::metadata(&source_path) {
                let item_type = if metadata.is_dir() {
                    FSItemType::Directory
                } else if metadata.is_symlink() {
                    FSItemType::SymbolicLink
                } else {
                    FSItemType::File
                };
                
                // Read the file data if it's a file
                let data = if item_type == FSItemType::File {
                    std::fs::read(&source_path).ok()
                } else {
                    None
                };
                
                // Create override item with preserved metadata
                let override_item = OverrideItem {
                    path: new_path.to_path_buf(),
                    item_type,
                    attributes: FileAttributes {
                        size: metadata.len(),
                        mode: self.get_file_mode(&metadata),
                        uid: self.get_uid(&metadata),
                        gid: self.get_gid(&metadata),
                        atime: 0, // Would be populated from metadata
                        mtime: 0, // Would be populated from metadata
                        ctime: 0, // Would be populated from metadata
                    },
                    data,
                };
                
                override_store.items.insert(new_path.to_path_buf(), override_item);
            }
            
            // Mark the old path as deleted (tombstone)
            override_store.deleted_paths.insert(old_path.to_path_buf());
        }
        
        // Update deleted paths if the old path was marked as deleted
        if override_store.deleted_paths.remove(old_path) {
            override_store.deleted_paths.insert(new_path.to_path_buf());
        }
        
        Ok(())
    }
    
    fn rename_directory_recursive(&self, old_dir_path: &Path, new_dir_path: &Path) -> Result<(), String> {
        // First rename the directory itself
        self.rename_single_item(old_dir_path, new_dir_path)?;
        
        // Get all children that need to be renamed
        let children = self.get_all_children_for_rename(old_dir_path)?;
        
        // Recursively rename all children
        for old_child_path in children {
            // Calculate the relative path from old directory
            let relative_path = old_child_path.strip_prefix(old_dir_path)
                .map_err(|e| format!("Failed to get relative path: {}", e))?;
            
            // Build new child path
            let new_child_path = new_dir_path.join(relative_path);
            
            if self.is_directory(&old_child_path)? {
                self.rename_directory_recursive(&old_child_path, &new_child_path)?;
            } else {
                self.rename_single_item(&old_child_path, &new_child_path)?;
            }
        }
        
        Ok(())
    }
    
    fn get_all_children_for_rename(&self, dir_path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut children = Vec::new();
        
        // Get children from override store
        {
            let override_store = self.override_store.read()
                .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
            
            for (path, _) in &override_store.items {
                if let Some(parent) = path.parent() {
                    if parent == dir_path {
                        children.push(path.clone());
                    }
                }
            }
        }
        
        // Get children from source filesystem (only if not deleted)
        let source_path = self.get_source_path(dir_path)?;
        if let Ok(entries) = std::fs::read_dir(&source_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let child_path = dir_path.join(entry.file_name());
                    
                    // Check if this child is marked as deleted
                    let is_deleted = {
                        let override_store = self.override_store.read()
                            .map_err(|e| format!("Failed to acquire override store lock: {}", e))?;
                        override_store.deleted_paths.contains(&child_path)
                    };
                    
                    if !is_deleted && !children.contains(&child_path) {
                        children.push(child_path);
                    }
                }
            }
        }
        
        Ok(children)
    }

    pub fn get_active_operations(&self) -> Result<Vec<(u64, OperationType)>, String> {
        let state = self.state.read()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        Ok(state.active_operations.iter()
            .map(|(&id, op)| (id, op.clone()))
            .collect())
    }

    pub fn get_open_file_count(&self) -> Result<usize, String> {
        let state = self.state.read()
            .map_err(|e| format!("Failed to acquire state lock: {}", e))?;

        Ok(state.open_files.len())
    }
}

#[derive(Debug, Clone)]
pub struct FileAttributes {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
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
    fn test_operations_state_initialization() {
        let state = OperationsState::default();
        assert_eq!(state.open_files.len(), 0);
        assert_eq!(state.active_operations.len(), 0);
        assert_eq!(state.next_handle_id, 0);
    }

    #[test]
    fn test_file_handle_creation() {
        let handle = FileHandle {
            id: 1,
            path: PathBuf::from("/test/file.txt"),
            flags: 0x01,
            ref_count: 1,
        };
        
        assert_eq!(handle.id, 1);
        assert_eq!(handle.path.to_str().unwrap(), "/test/file.txt");
        assert_eq!(handle.flags, 0x01);
        assert_eq!(handle.ref_count, 1);
    }
}