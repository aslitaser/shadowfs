use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::ffi::c_void;
use dashmap::DashMap;
use dispatch::Queue as DispatchQueue;
use uuid::Uuid;
use shadowfs_core::override_store::OverrideStore;
use shadowfs_core::stats::FileSystemStats;

pub enum QueuePriority {
    High,
    Default,
    Low,
    Background,
}

pub struct FSKitConfig {
    pub volume_name: String,
    pub volume_uuid: Uuid,
    pub case_sensitive: bool,
    pub supports_extended_attrs: bool,
    pub dispatch_queue_priority: QueuePriority,
    pub max_readahead_size: usize,
}

pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub exclusive: bool,
    pub truncate: bool,
    pub append: bool,
}

pub struct FileContext {
    pub handle_id: u64,
    pub path: PathBuf,
    pub flags: OpenFlags,
    pub position: AtomicU64,
    pub is_override: bool,
}

pub struct FSKitProvider {
    config: FSKitConfig,
    source_root: PathBuf,
    mount_point: PathBuf,
    override_store: Arc<OverrideStore>,
    dispatch_queue: DispatchQueue,
    file_handles: DashMap<u64, FileContext>,
    stats: Arc<FileSystemStats>,
    next_handle_id: AtomicU64,
}

#[repr(C)]
pub struct ObjCBridge {
    pub ptr: *mut c_void,
}

unsafe impl Send for ObjCBridge {}
unsafe impl Sync for ObjCBridge {}

impl ObjCBridge {
    pub fn new(ptr: *mut c_void) -> Self {
        unsafe {
            if !ptr.is_null() {
                objc_retain(ptr);
            }
        }
        ObjCBridge { ptr }
    }
    
    pub fn as_ptr(&self) -> *mut c_void {
        self.ptr
    }
}

impl Clone for ObjCBridge {
    fn clone(&self) -> Self {
        unsafe {
            if !self.ptr.is_null() {
                objc_retain(self.ptr);
            }
        }
        ObjCBridge { ptr: self.ptr }
    }
}

impl Drop for ObjCBridge {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                objc_release(self.ptr);
            }
        }
    }
}

#[link(name = "objc", kind = "dylib")]
extern "C" {
    fn objc_retain(obj: *mut c_void) -> *mut c_void;
    fn objc_release(obj: *mut c_void);
    fn objc_autorelease(obj: *mut c_void) -> *mut c_void;
}

pub struct ArcBridge<T> {
    inner: Arc<T>,
}

impl<T> ArcBridge<T> {
    pub fn new(value: T) -> Self {
        ArcBridge {
            inner: Arc::new(value),
        }
    }
    
    pub fn from_arc(arc: Arc<T>) -> Self {
        ArcBridge { inner: arc }
    }
    
    pub fn into_raw(self) -> *const T {
        Arc::into_raw(self.inner)
    }
    
    pub unsafe fn from_raw(ptr: *const T) -> Self {
        ArcBridge {
            inner: Arc::from_raw(ptr),
        }
    }
    
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
    
    pub fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> Clone for ArcBridge<T> {
    fn clone(&self) -> Self {
        ArcBridge {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub unsafe fn retain_objc<T>(obj: *mut T) -> *mut T {
    objc_retain(obj as *mut c_void) as *mut T
}

pub unsafe fn release_objc<T>(obj: *mut T) {
    objc_release(obj as *mut c_void);
}

pub unsafe fn autorelease_objc<T>(obj: *mut T) -> *mut T {
    objc_autorelease(obj as *mut c_void) as *mut T
}

impl FSKitProvider {
    pub fn new(config: FSKitConfig) -> Self {
        let dispatch_queue = match config.dispatch_queue_priority {
            QueuePriority::High => DispatchQueue::global(dispatch::QueuePriority::High),
            QueuePriority::Default => DispatchQueue::global(dispatch::QueuePriority::Default),
            QueuePriority::Low => DispatchQueue::global(dispatch::QueuePriority::Low),
            QueuePriority::Background => DispatchQueue::global(dispatch::QueuePriority::Background),
        };
        
        Self {
            config,
            source_root: PathBuf::new(),
            mount_point: PathBuf::new(),
            override_store: Arc::new(OverrideStore::with_defaults()),
            dispatch_queue,
            file_handles: DashMap::new(),
            stats: Arc::new(FileSystemStats::default()),
            next_handle_id: AtomicU64::new(1),
        }
    }
    
    pub fn set_paths(&mut self, source: PathBuf, mount: PathBuf) {
        self.source_root = source;
        self.mount_point = mount;
    }
    
    pub fn allocate_handle(&self) -> u64 {
        self.next_handle_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
    
    pub fn register_handle(&self, handle_id: u64, context: FileContext) {
        self.file_handles.insert(handle_id, context);
    }
    
    pub fn get_handle(&self, handle_id: u64) -> Option<dashmap::mapref::one::Ref<u64, FileContext>> {
        self.file_handles.get(&handle_id)
    }
    
    pub fn remove_handle(&self, handle_id: u64) -> Option<(u64, FileContext)> {
        self.file_handles.remove(&handle_id)
    }
    
    pub fn override_store(&self) -> &Arc<OverrideStore> {
        &self.override_store
    }
    
    pub fn stats(&self) -> &Arc<FileSystemStats> {
        &self.stats
    }
}