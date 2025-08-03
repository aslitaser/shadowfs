//! Mount-related types and configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;
use tokio::sync::oneshot;
use crate::types::{FilePermissions, ShadowPath};

/// Represents the platform where the filesystem is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
}

impl Platform {
    /// Returns the current platform based on the target OS.
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        return Platform::Windows;
        
        #[cfg(target_os = "macos")]
        return Platform::MacOS;
        
        #[cfg(target_os = "linux")]
        return Platform::Linux;
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        compile_error!("Unsupported platform");
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Windows => write!(f, "Windows"),
            Platform::MacOS => write!(f, "macOS"),
            Platform::Linux => write!(f, "Linux"),
        }
    }
}

/// Handle representing a mounted filesystem with platform-specific details.
pub struct MountHandle {
    /// Unique identifier for this mount
    pub id: Uuid,
    
    /// Source path that was mounted
    pub source: ShadowPath,
    
    /// Target mount point
    pub target: ShadowPath,
    
    /// Platform where the filesystem is mounted
    pub platform: Platform,
    
    /// Time when the filesystem was mounted
    pub mount_time: SystemTime,
    
    /// Channel sender for unmount signal (private)
    unmount_sender: Option<oneshot::Sender<()>>,
}

impl MountHandle {
    /// Creates a new mount handle.
    pub fn new(
        source: ShadowPath,
        target: ShadowPath,
        platform: Platform,
        unmount_sender: oneshot::Sender<()>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            platform,
            mount_time: SystemTime::now(),
            unmount_sender: Some(unmount_sender),
        }
    }
    
    /// Creates a new mount handle with a specific ID (useful for testing).
    pub fn with_id(
        id: Uuid,
        source: ShadowPath,
        target: ShadowPath,
        platform: Platform,
        unmount_sender: oneshot::Sender<()>,
    ) -> Self {
        Self {
            id,
            source,
            target,
            platform,
            mount_time: SystemTime::now(),
            unmount_sender: Some(unmount_sender),
        }
    }
    
    /// Sends the unmount signal.
    /// Returns true if the signal was sent successfully, false if already sent.
    pub fn unmount(&mut self) -> bool {
        if let Some(sender) = self.unmount_sender.take() {
            sender.send(()).is_ok()
        } else {
            false
        }
    }
    
    /// Returns true if this mount handle is still active (unmount not called).
    pub fn is_active(&self) -> bool {
        self.unmount_sender.is_some()
    }
    
    /// Returns the duration since the filesystem was mounted.
    pub fn uptime(&self) -> Result<std::time::Duration, std::time::SystemTimeError> {
        self.mount_time.elapsed()
    }
}

impl std::fmt::Debug for MountHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MountHandle")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("target", &self.target)
            .field("platform", &self.platform)
            .field("mount_time", &self.mount_time)
            .field("is_active", &self.is_active())
            .finish()
    }
}

impl PartialEq for MountHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MountHandle {}

impl std::hash::Hash for MountHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Configuration options for mounting a shadow filesystem.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MountOptions {
    /// Whether the mount should be read-only
    pub read_only: bool,
    
    /// Whether the filesystem should be case-sensitive
    /// Default: true on Linux, false on Windows/macOS
    pub case_sensitive: bool,
    
    /// Maximum allowed path length (None = use system default)
    pub max_path_length: Option<usize>,
    
    /// UID mapping for translating user IDs (guest -> host)
    pub uid_map: Option<HashMap<u32, u32>>,
    
    /// GID mapping for translating group IDs (guest -> host)
    pub gid_map: Option<HashMap<u32, u32>>,
    
    /// Default permissions for new files/directories
    pub default_permissions: FilePermissions,
    
    /// Cache configuration
    pub cache_config: CacheConfig,
    
    /// Override store configuration
    pub override_config: OverrideConfig,
}

impl Default for MountOptions {
    fn default() -> Self {
        let case_sensitive = if cfg!(target_os = "linux") {
            true
        } else {
            false
        };
        
        Self {
            read_only: false,
            case_sensitive,
            max_path_length: None,
            uid_map: None,
            gid_map: None,
            default_permissions: FilePermissions::default_directory(),
            cache_config: CacheConfig::default(),
            override_config: OverrideConfig::default(),
        }
    }
}

impl MountOptions {
    /// Creates a new MountOptions with default settings.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates a new builder for MountOptions.
    pub fn builder() -> MountOptionsBuilder {
        MountOptionsBuilder::new()
    }
    
    /// Sets the mount as read-only.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
    
    /// Sets case sensitivity.
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }
    
    /// Sets the maximum path length.
    pub fn max_path_length(mut self, length: usize) -> Self {
        self.max_path_length = Some(length);
        self
    }
    
    /// Sets the UID mapping.
    pub fn uid_map(mut self, map: HashMap<u32, u32>) -> Self {
        self.uid_map = Some(map);
        self
    }
    
    /// Sets the GID mapping.
    pub fn gid_map(mut self, map: HashMap<u32, u32>) -> Self {
        self.gid_map = Some(map);
        self
    }
    
    /// Sets the default permissions.
    pub fn default_permissions(mut self, perms: FilePermissions) -> Self {
        self.default_permissions = perms;
        self
    }
    
    /// Sets the cache configuration.
    pub fn cache_config(mut self, config: CacheConfig) -> Self {
        self.cache_config = config;
        self
    }
    
    /// Sets the override configuration.
    pub fn override_config(mut self, config: OverrideConfig) -> Self {
        self.override_config = config;
        self
    }
}

/// Builder for MountOptions with a fluent interface.
pub struct MountOptionsBuilder {
    options: MountOptions,
}

impl MountOptionsBuilder {
    /// Creates a new builder with default options.
    pub fn new() -> Self {
        Self {
            options: MountOptions::default(),
        }
    }
    
    /// Sets the mount as read-only.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.options.read_only = read_only;
        self
    }
    
    /// Sets case sensitivity.
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.options.case_sensitive = sensitive;
        self
    }
    
    /// Sets the maximum path length.
    pub fn max_path_length(mut self, length: usize) -> Self {
        self.options.max_path_length = Some(length);
        self
    }
    
    /// Clears the maximum path length limit.
    pub fn no_path_length_limit(mut self) -> Self {
        self.options.max_path_length = None;
        self
    }
    
    /// Sets the UID mapping.
    pub fn uid_map(mut self, map: HashMap<u32, u32>) -> Self {
        self.options.uid_map = Some(map);
        self
    }
    
    /// Adds a single UID mapping.
    pub fn add_uid_mapping(mut self, guest_uid: u32, host_uid: u32) -> Self {
        let map = self.options.uid_map.get_or_insert_with(HashMap::new);
        map.insert(guest_uid, host_uid);
        self
    }
    
    /// Sets the GID mapping.
    pub fn gid_map(mut self, map: HashMap<u32, u32>) -> Self {
        self.options.gid_map = Some(map);
        self
    }
    
    /// Adds a single GID mapping.
    pub fn add_gid_mapping(mut self, guest_gid: u32, host_gid: u32) -> Self {
        let map = self.options.gid_map.get_or_insert_with(HashMap::new);
        map.insert(guest_gid, host_gid);
        self
    }
    
    /// Sets the default permissions.
    pub fn default_permissions(mut self, perms: FilePermissions) -> Self {
        self.options.default_permissions = perms;
        self
    }
    
    /// Sets the cache configuration.
    pub fn cache_config(mut self, config: CacheConfig) -> Self {
        self.options.cache_config = config;
        self
    }
    
    /// Sets the override configuration.
    pub fn override_config(mut self, config: OverrideConfig) -> Self {
        self.options.override_config = config;
        self
    }
    
    /// Builds the final MountOptions.
    pub fn build(self) -> MountOptions {
        self.options
    }
}

/// Configuration for the filesystem cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheConfig {
    /// Whether caching is enabled
    pub enabled: bool,
    
    /// Maximum size of the cache in bytes
    pub max_size_bytes: usize,
    
    /// Time-to-live for cache entries in seconds
    pub ttl_seconds: u64,
    
    /// Maximum number of entries in the stat cache
    pub stat_cache_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size_bytes: 100 * 1024 * 1024, // 100 MB
            ttl_seconds: 300, // 5 minutes
            stat_cache_size: 10_000,
        }
    }
}

impl CacheConfig {
    /// Creates a new cache configuration with caching disabled.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
    
    /// Creates a minimal cache configuration.
    pub fn minimal() -> Self {
        Self {
            enabled: true,
            max_size_bytes: 10 * 1024 * 1024, // 10 MB
            ttl_seconds: 60, // 1 minute
            stat_cache_size: 1_000,
        }
    }
    
    /// Creates an aggressive cache configuration.
    pub fn aggressive() -> Self {
        Self {
            enabled: true,
            max_size_bytes: 1024 * 1024 * 1024, // 1 GB
            ttl_seconds: 3600, // 1 hour
            stat_cache_size: 100_000,
        }
    }
}

/// Configuration for the override store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverrideConfig {
    /// Maximum memory usage for overrides in bytes
    pub max_memory_bytes: usize,
    
    /// Whether to persist overrides to disk
    pub persist_to_disk: bool,
    
    /// Path to store persistent overrides (if enabled)
    pub persist_path: Option<PathBuf>,
}

impl Default for OverrideConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 100 * 1024 * 1024, // 100 MB
            persist_to_disk: false,
            persist_path: None,
        }
    }
}

impl OverrideConfig {
    /// Creates a memory-only configuration with a specific size limit.
    pub fn memory_only(max_bytes: usize) -> Self {
        Self {
            max_memory_bytes: max_bytes,
            persist_to_disk: false,
            persist_path: None,
        }
    }
    
    /// Creates a configuration with disk persistence.
    pub fn persistent(path: impl Into<PathBuf>, max_bytes: usize) -> Self {
        Self {
            max_memory_bytes: max_bytes,
            persist_to_disk: true,
            persist_path: Some(path.into()),
        }
    }
    
    /// Enables persistence with the given path.
    pub fn with_persistence(mut self, path: impl Into<PathBuf>) -> Self {
        self.persist_to_disk = true;
        self.persist_path = Some(path.into());
        self
    }
    
    /// Sets the maximum memory usage.
    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_options_default() {
        let options = MountOptions::default();
        
        assert!(!options.read_only);
        #[cfg(target_os = "linux")]
        assert!(options.case_sensitive);
        #[cfg(not(target_os = "linux"))]
        assert!(!options.case_sensitive);
        assert!(options.max_path_length.is_none());
        assert!(options.uid_map.is_none());
        assert!(options.gid_map.is_none());
    }

    #[test]
    fn test_mount_options_builder() {
        let mut uid_map = HashMap::new();
        uid_map.insert(1000, 2000);
        
        let options = MountOptions::builder()
            .read_only(true)
            .case_sensitive(false)
            .max_path_length(255)
            .uid_map(uid_map.clone())
            .add_gid_mapping(1000, 2000)
            .build();
        
        assert!(options.read_only);
        assert!(!options.case_sensitive);
        assert_eq!(options.max_path_length, Some(255));
        assert_eq!(options.uid_map, Some(uid_map));
        assert!(options.gid_map.is_some());
        assert_eq!(options.gid_map.as_ref().unwrap().get(&1000), Some(&2000));
    }

    #[test]
    fn test_mount_options_fluent() {
        let options = MountOptions::new()
            .read_only()
            .case_sensitive(true)
            .max_path_length(1024);
        
        assert!(options.read_only);
        assert!(options.case_sensitive);
        assert_eq!(options.max_path_length, Some(1024));
    }

    #[test]
    fn test_cache_config_presets() {
        let disabled = CacheConfig::disabled();
        assert!(!disabled.enabled);
        
        let minimal = CacheConfig::minimal();
        assert!(minimal.enabled);
        assert_eq!(minimal.max_size_bytes, 10 * 1024 * 1024);
        assert_eq!(minimal.ttl_seconds, 60);
        assert_eq!(minimal.stat_cache_size, 1_000);
        
        let aggressive = CacheConfig::aggressive();
        assert!(aggressive.enabled);
        assert_eq!(aggressive.max_size_bytes, 1024 * 1024 * 1024);
        assert_eq!(aggressive.ttl_seconds, 3600);
        assert_eq!(aggressive.stat_cache_size, 100_000);
    }

    #[test]
    fn test_override_config_presets() {
        let memory = OverrideConfig::memory_only(50 * 1024 * 1024);
        assert!(!memory.persist_to_disk);
        assert_eq!(memory.max_memory_bytes, 50 * 1024 * 1024);
        assert!(memory.persist_path.is_none());
        
        let persistent = OverrideConfig::persistent("/tmp/shadowfs", 200 * 1024 * 1024);
        assert!(persistent.persist_to_disk);
        assert_eq!(persistent.max_memory_bytes, 200 * 1024 * 1024);
        assert_eq!(persistent.persist_path, Some(PathBuf::from("/tmp/shadowfs")));
        
        let default = OverrideConfig::default();
        assert!(!default.persist_to_disk);
        assert_eq!(default.max_memory_bytes, 100 * 1024 * 1024);
        assert!(default.persist_path.is_none());
    }
    
    #[test]
    fn test_override_config_builder_style() {
        let config = OverrideConfig::default()
            .with_max_memory(64 * 1024 * 1024)
            .with_persistence("/var/shadowfs");
        
        assert!(config.persist_to_disk);
        assert_eq!(config.max_memory_bytes, 64 * 1024 * 1024);
        assert_eq!(config.persist_path, Some(PathBuf::from("/var/shadowfs")));
    }

    #[test]
    fn test_builder_uid_gid_mappings() {
        let options = MountOptions::builder()
            .add_uid_mapping(1000, 2000)
            .add_uid_mapping(1001, 2001)
            .add_gid_mapping(100, 200)
            .add_gid_mapping(101, 201)
            .build();
        
        let uid_map = options.uid_map.unwrap();
        assert_eq!(uid_map.len(), 2);
        assert_eq!(uid_map.get(&1000), Some(&2000));
        assert_eq!(uid_map.get(&1001), Some(&2001));
        
        let gid_map = options.gid_map.unwrap();
        assert_eq!(gid_map.len(), 2);
        assert_eq!(gid_map.get(&100), Some(&200));
        assert_eq!(gid_map.get(&101), Some(&201));
    }
    
    #[test]
    fn test_mount_handle() {
        let (tx, _rx) = oneshot::channel();
        let source = ShadowPath::from("/source");
        let target = ShadowPath::from("/target");
        let mut handle = MountHandle::new(
            source.clone(),
            target.clone(),
            Platform::current(),
            tx,
        );
        
        assert_eq!(handle.source, source);
        assert_eq!(handle.target, target);
        assert_eq!(handle.platform, Platform::current());
        assert!(handle.is_active());
        
        // Test unmount
        assert!(handle.unmount());
        assert!(!handle.is_active());
        assert!(!handle.unmount()); // Second unmount should fail
    }
    
    #[test]
    fn test_mount_handle_with_id() {
        let id = Uuid::new_v4();
        let (tx, _rx) = oneshot::channel();
        let handle = MountHandle::with_id(
            id,
            ShadowPath::from("/source"),
            ShadowPath::from("/target"),
            Platform::Linux,
            tx,
        );
        
        assert_eq!(handle.id, id);
        assert_eq!(handle.platform, Platform::Linux);
    }
    
    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Windows.to_string(), "Windows");
        assert_eq!(Platform::MacOS.to_string(), "macOS");
        assert_eq!(Platform::Linux.to_string(), "Linux");
    }
    
    #[test]
    fn test_mount_handle_equality() {
        let id = Uuid::new_v4();
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();
        
        let handle1 = MountHandle::with_id(
            id,
            ShadowPath::from("/source1"),
            ShadowPath::from("/target1"),
            Platform::Linux,
            tx1,
        );
        
        let handle2 = MountHandle::with_id(
            id,
            ShadowPath::from("/source2"),
            ShadowPath::from("/target2"),
            Platform::Windows,
            tx2,
        );
        
        // Handles are equal if IDs are equal
        assert_eq!(handle1, handle2);
    }
}