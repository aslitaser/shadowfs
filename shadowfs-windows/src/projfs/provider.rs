use std::path::PathBuf;
use std::sync::Arc;
use std::fmt;
use dashmap::DashMap;
use windows::core::GUID;
use windows::Win32::Storage::ProjectedFileSystem::{PRJ_INSTANCE_HANDLE, PrjStopVirtualizing};
use shadowfs_core::override_store::OverrideStore;
use crate::stats::FileSystemStats;

/// Safe wrapper around PRJ_INSTANCE_HANDLE
pub struct ProjFSHandle {
    handle: PRJ_INSTANCE_HANDLE,
}

impl ProjFSHandle {
    /// Creates a new ProjFS handle wrapper
    pub fn new(handle: PRJ_INSTANCE_HANDLE) -> Self {
        Self { handle }
    }
    
    /// Gets the underlying handle
    pub fn get(&self) -> PRJ_INSTANCE_HANDLE {
        self.handle
    }
}

impl Drop for ProjFSHandle {
    fn drop(&mut self) {
        unsafe {
            // Stop virtualization when handle is dropped
            let _ = PrjStopVirtualizing(self.handle);
        }
    }
}

impl fmt::Debug for ProjFSHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjFSHandle")
            .field("handle", &format!("{:?}", self.handle))
            .finish()
    }
}

/// Safe wrapper for enumeration handle
pub struct EnumerationHandle {
    id: GUID,
    _marker: std::marker::PhantomData<*const ()>, // Prevent Send/Sync
}

impl EnumerationHandle {
    /// Creates a new enumeration handle
    pub fn new(id: GUID) -> Self {
        Self {
            id,
            _marker: std::marker::PhantomData,
        }
    }
    
    /// Gets the enumeration ID
    pub fn id(&self) -> &GUID {
        &self.id
    }
}

impl fmt::Debug for EnumerationHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnumerationHandle")
            .field("id", &self.id)
            .finish()
    }
}

/// Notification mapping configuration
pub struct NotificationMapping {
    pub notification_root: PathBuf,
    pub notifications: u32,
}

/// Configuration for ProjFS provider
pub struct ProjFSConfig {
    /// Number of threads in the ProjFS thread pool (default: CPU count)
    pub pool_thread_count: u32,
    
    /// Notification mappings for specific paths
    pub notification_mappings: Vec<NotificationMapping>,
    
    /// Enable negative path caching
    pub enable_negative_cache: bool,
    
    /// Optional virtualization instance ID
    pub virtualization_instance_id: Option<GUID>,
}

impl Default for ProjFSConfig {
    fn default() -> Self {
        Self {
            pool_thread_count: num_cpus::get() as u32,
            notification_mappings: Vec::new(),
            enable_negative_cache: true,
            virtualization_instance_id: None,
        }
    }
}

/// Represents an active enumeration session
pub struct EnumerationSession {
    /// Optional search pattern for filtering results
    pub search_expression: Option<String>,
    
    /// Path to the directory being enumerated
    pub directory_path: PathBuf,
    
    /// Whether this is a restart of the enumeration
    pub is_restart: bool,
    
    /// Continuation token for resuming enumeration
    pub continuation_token: Option<Vec<u8>>,
}

/// Represents an open file handle
pub struct FileHandle {
    pub path: PathBuf,
    pub is_directory: bool,
    pub file_id: GUID,
}

/// ProjFS provider implementation for Windows
pub struct ProjFSProvider {
    /// Handle to the ProjFS virtualization instance
    pub instance: ProjFSHandle,
    
    /// Root path of the virtualized directory
    pub virtualization_root: PathBuf,
    
    /// Root path of the source files
    pub source_root: PathBuf,
    
    /// Store for managing file overrides
    pub override_store: Arc<OverrideStore>,
    
    /// Active enumeration sessions indexed by GUID
    pub active_enumerations: DashMap<GUID, EnumerationSession>,
    
    /// Open file handles indexed by GUID
    pub file_handles: DashMap<GUID, FileHandle>,
    
    /// File system statistics
    pub stats: Arc<FileSystemStats>,
}

impl ProjFSProvider {
    /// Creates a new ProjFS provider instance
    pub fn new(
        instance: PRJ_INSTANCE_HANDLE,
        virtualization_root: PathBuf,
        source_root: PathBuf,
        override_store: Arc<OverrideStore>,
        stats: Arc<FileSystemStats>,
    ) -> Self {
        Self {
            instance: ProjFSHandle::new(instance),
            virtualization_root,
            source_root,
            override_store,
            active_enumerations: DashMap::new(),
            file_handles: DashMap::new(),
            stats,
        }
    }
}

impl fmt::Debug for ProjFSProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjFSProvider")
            .field("instance", &self.instance)
            .field("virtualization_root", &self.virtualization_root)
            .field("source_root", &self.source_root)
            .field("active_enumerations", &self.active_enumerations.len())
            .field("file_handles", &self.file_handles.len())
            .finish()
    }
}