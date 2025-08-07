use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use dashmap::DashMap;
use dispatch::Queue as DispatchQueue;
use uuid::Uuid;
use crate::fskit::{FSExtensionPoint, FSVolume};
use crate::override_store::OverrideStore;
use crate::stats::FileSystemStats;

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
    extension_point: FSExtensionPoint,
    volume: Option<FSVolume>,
    source_root: PathBuf,
    mount_point: PathBuf,
    override_store: Arc<OverrideStore>,
    dispatch_queue: DispatchQueue,
    file_handles: DashMap<u64, FileContext>,
    stats: Arc<FileSystemStats>,
}