//! Mount-related types and configuration.

use std::collections::HashMap;
use crate::types::FilePermissions;

/// Configuration options for mounting a shadow filesystem.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled
    pub enabled: bool,
    
    /// Maximum number of cached metadata entries
    pub max_metadata_entries: usize,
    
    /// Maximum total size of cached file data in bytes
    pub max_data_size: usize,
    
    /// Time-to-live for metadata cache entries in seconds
    pub metadata_ttl_secs: u64,
    
    /// Time-to-live for data cache entries in seconds
    pub data_ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_metadata_entries: 10_000,
            max_data_size: 100 * 1024 * 1024, // 100 MB
            metadata_ttl_secs: 60,
            data_ttl_secs: 300,
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
            max_metadata_entries: 1_000,
            max_data_size: 10 * 1024 * 1024, // 10 MB
            metadata_ttl_secs: 10,
            data_ttl_secs: 30,
        }
    }
    
    /// Creates an aggressive cache configuration.
    pub fn aggressive() -> Self {
        Self {
            enabled: true,
            max_metadata_entries: 100_000,
            max_data_size: 1024 * 1024 * 1024, // 1 GB
            metadata_ttl_secs: 600,
            data_ttl_secs: 3600,
        }
    }
}

/// Configuration for the override store.
#[derive(Debug, Clone)]
pub struct OverrideConfig {
    /// Maximum total size of overrides in bytes (0 = unlimited)
    pub max_total_size: usize,
    
    /// Maximum size of a single override in bytes
    pub max_file_size: usize,
    
    /// Whether to persist overrides to disk
    pub persist_to_disk: bool,
    
    /// Path to store persistent overrides (if enabled)
    pub persistence_path: Option<String>,
    
    /// Whether to compress overrides in memory
    pub compress: bool,
    
    /// Compression level (1-9) if compression is enabled
    pub compression_level: u8,
}

impl Default for OverrideConfig {
    fn default() -> Self {
        Self {
            max_total_size: 0, // unlimited
            max_file_size: 100 * 1024 * 1024, // 100 MB per file
            persist_to_disk: false,
            persistence_path: None,
            compress: false,
            compression_level: 6,
        }
    }
}

impl OverrideConfig {
    /// Creates a memory-only configuration with no limits.
    pub fn memory_only() -> Self {
        Self::default()
    }
    
    /// Creates a configuration with size limits.
    pub fn with_limits(max_total: usize, max_file: usize) -> Self {
        Self {
            max_total_size: max_total,
            max_file_size: max_file,
            ..Default::default()
        }
    }
    
    /// Creates a configuration with disk persistence.
    pub fn persistent(path: impl Into<String>) -> Self {
        Self {
            persist_to_disk: true,
            persistence_path: Some(path.into()),
            ..Default::default()
        }
    }
    
    /// Creates a configuration with compression enabled.
    pub fn compressed(level: u8) -> Self {
        Self {
            compress: true,
            compression_level: level.clamp(1, 9),
            ..Default::default()
        }
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
        assert_eq!(minimal.max_metadata_entries, 1_000);
        assert_eq!(minimal.max_data_size, 10 * 1024 * 1024);
        
        let aggressive = CacheConfig::aggressive();
        assert!(aggressive.enabled);
        assert_eq!(aggressive.max_metadata_entries, 100_000);
        assert_eq!(aggressive.max_data_size, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_override_config_presets() {
        let memory = OverrideConfig::memory_only();
        assert!(!memory.persist_to_disk);
        assert_eq!(memory.max_total_size, 0);
        
        let limited = OverrideConfig::with_limits(100 * 1024 * 1024, 10 * 1024 * 1024);
        assert_eq!(limited.max_total_size, 100 * 1024 * 1024);
        assert_eq!(limited.max_file_size, 10 * 1024 * 1024);
        
        let persistent = OverrideConfig::persistent("/tmp/shadowfs");
        assert!(persistent.persist_to_disk);
        assert_eq!(persistent.persistence_path, Some("/tmp/shadowfs".to_string()));
        
        let compressed = OverrideConfig::compressed(9);
        assert!(compressed.compress);
        assert_eq!(compressed.compression_level, 9);
        
        // Test clamping
        let over_compressed = OverrideConfig::compressed(15);
        assert_eq!(over_compressed.compression_level, 9);
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
}