use crate::types::{FileMetadata, ShadowPath};
use bytes::Bytes;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum OverrideContent {
    File {
        data: Bytes,
        content_hash: [u8; 32],
    },
    Directory {
        entries: Vec<String>,
    },
    Deleted,
}

#[derive(Debug)]
pub struct OverrideEntry {
    pub path: ShadowPath,
    pub content: OverrideContent,
    pub original_metadata: Option<FileMetadata>,
    pub override_metadata: FileMetadata,
    pub created_at: SystemTime,
    pub last_accessed: AtomicU64,
}

pub struct OverrideStore {
    pub entries: DashMap<ShadowPath, OverrideEntry>,
    pub memory_usage: AtomicUsize,
    pub max_memory: usize,
    pub lru_tracker: Mutex<VecDeque<ShadowPath>>,
}