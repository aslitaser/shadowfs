//! Configuration types for ShadowFS.

use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::ShadowError;
use super::mount::MountOptions;

/// Log level for the ShadowFS daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    /// Only log errors
    Error,
    /// Log errors and warnings
    Warn,
    /// Log errors, warnings, and informational messages
    Info,
    /// Log errors, warnings, info, and debug messages
    Debug,
    /// Log everything including trace-level details
    Trace,
}

impl LogLevel {
    /// Returns the string representation of the log level.
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(LogLevel::Error),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(format!("Unknown log level: {}", s)),
        }
    }
}

/// Global configuration for ShadowFS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowConfig {
    /// Logging level
    pub log_level: LogLevel,
    
    /// Optional log file path (logs to stderr if None)
    pub log_file: Option<PathBuf>,
    
    /// Whether to run as a daemon process
    pub daemon_mode: bool,
    
    /// Optional PID file path for daemon mode
    pub pid_file: Option<PathBuf>,
    
    /// Path to the mount registry database
    pub mount_registry_path: PathBuf,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
            log_file: None,
            daemon_mode: false,
            pid_file: None,
            mount_registry_path: PathBuf::from("/var/lib/shadowfs/mounts.db"),
        }
    }
}

impl ShadowConfig {
    /// Creates a new ShadowConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates a config suitable for development/testing.
    pub fn development() -> Self {
        Self {
            log_level: LogLevel::Debug,
            log_file: None,
            daemon_mode: false,
            pid_file: None,
            mount_registry_path: PathBuf::from("./shadowfs-mounts.db"),
        }
    }
    
    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check PID file is specified if daemon mode is enabled
        if self.daemon_mode && self.pid_file.is_none() {
            errors.push("PID file must be specified when daemon_mode is enabled".to_string());
        }
        
        // Check log file parent directory exists if specified
        if let Some(log_file) = &self.log_file {
            if let Some(parent) = log_file.parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    errors.push(format!("Log file directory does not exist: {:?}", parent));
                }
            }
        }
        
        // Check mount registry parent directory exists
        if let Some(parent) = self.mount_registry_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                errors.push(format!("Mount registry directory does not exist: {:?}", parent));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A record of an active mount for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountRecord {
    /// Unique identifier for this mount
    pub id: Uuid,
    
    /// Source path being mounted
    pub source: String,
    
    /// Target mount point
    pub target: String,
    
    /// Mount options used
    pub options: MountOptions,
    
    /// When this mount was created
    pub created_at: SystemTime,
    
    /// Process ID that created this mount
    pub process_id: u32,
}

impl MountRecord {
    /// Creates a new mount record.
    pub fn new(
        source: String,
        target: String,
        options: MountOptions,
        process_id: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            options,
            created_at: SystemTime::now(),
            process_id,
        }
    }
    
    /// Creates a mount record with a specific ID (for restoration).
    pub fn with_id(
        id: Uuid,
        source: String,
        target: String,
        options: MountOptions,
        created_at: SystemTime,
        process_id: u32,
    ) -> Self {
        Self {
            id,
            source,
            target,
            options,
            created_at,
            process_id,
        }
    }
    
    /// Checks if the process that created this mount is still alive.
    pub fn is_process_alive(&self) -> bool {
        // Platform-specific implementation would go here
        // For now, return true as a placeholder
        true
    }
}

/// Trait for managing mount records persistently.
#[async_trait::async_trait]
pub trait MountRegistry: Send + Sync {
    /// Registers a new mount record.
    async fn register(&mut self, record: MountRecord) -> Result<(), ShadowError>;
    
    /// Unregisters a mount by its ID.
    async fn unregister(&mut self, id: Uuid) -> Result<(), ShadowError>;
    
    /// Gets a mount record by its ID.
    async fn get(&self, id: Uuid) -> Option<MountRecord>;
    
    /// Lists all mount records.
    async fn list(&self) -> Vec<MountRecord>;
    
    /// Cleans up stale mount records (where process is no longer alive).
    /// Returns the IDs of cleaned up mounts.
    async fn cleanup_stale(&mut self) -> Result<Vec<Uuid>, ShadowError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }
    
    #[test]
    fn test_log_level_from_str() {
        assert_eq!("error".parse::<LogLevel>().unwrap(), LogLevel::Error);
        assert_eq!("warn".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("warning".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("trace".parse::<LogLevel>().unwrap(), LogLevel::Trace);
        
        assert!("invalid".parse::<LogLevel>().is_err());
    }
    
    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Error.to_string(), "error");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
        assert_eq!(LogLevel::Trace.to_string(), "trace");
    }
    
    #[test]
    fn test_shadow_config_default() {
        let config = ShadowConfig::default();
        assert_eq!(config.log_level, LogLevel::Info);
        assert!(config.log_file.is_none());
        assert!(!config.daemon_mode);
        assert!(config.pid_file.is_none());
        assert_eq!(config.mount_registry_path, PathBuf::from("/var/lib/shadowfs/mounts.db"));
    }
    
    #[test]
    fn test_shadow_config_development() {
        let config = ShadowConfig::development();
        assert_eq!(config.log_level, LogLevel::Debug);
        assert!(!config.daemon_mode);
        assert_eq!(config.mount_registry_path, PathBuf::from("./shadowfs-mounts.db"));
    }
    
    #[test]
    fn test_shadow_config_validate() {
        // Valid config - use development config which has a relative path
        let config = ShadowConfig::development();
        assert!(config.validate().is_ok());
        
        // Invalid: daemon mode without PID file
        let mut config = ShadowConfig::development();
        config.daemon_mode = true;
        let err = config.validate().unwrap_err();
        assert!(err[0].contains("PID file must be specified"));
        
        // Valid: daemon mode with PID file
        config.pid_file = Some(PathBuf::from("./shadowfs.pid"));
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_mount_record_new() {
        let options = MountOptions::default();
        let record = MountRecord::new(
            "/source".to_string(),
            "/target".to_string(),
            options.clone(),
            1234,
        );
        
        assert!(!record.id.is_nil());
        assert_eq!(record.source, "/source");
        assert_eq!(record.target, "/target");
        assert_eq!(record.process_id, 1234);
        
        // Verify created_at is recent
        let now = SystemTime::now();
        let duration = now.duration_since(record.created_at).unwrap();
        assert!(duration.as_secs() < 1);
    }
    
    #[test]
    fn test_mount_record_with_id() {
        let id = Uuid::new_v4();
        let created_at = SystemTime::UNIX_EPOCH;
        let options = MountOptions::default();
        
        let record = MountRecord::with_id(
            id,
            "/source".to_string(),
            "/target".to_string(),
            options,
            created_at,
            5678,
        );
        
        assert_eq!(record.id, id);
        assert_eq!(record.created_at, created_at);
        assert_eq!(record.process_id, 5678);
    }
}