//! Runtime feature detection and monitoring for ShadowFS
//! 
//! This module provides dynamic detection of platform features and
//! monitors for changes in system capabilities during runtime.

use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::path::Path;
use std::thread;
use serde::{Serialize, Deserialize};
use crate::types::mount::Platform;
use crate::error::{ShadowError, Result};

/// Types of features that can be detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureType {
    /// FUSE availability
    FuseAvailable,
    /// ProjFS availability (Windows)
    ProjFSAvailable,
    /// macFUSE availability
    MacFuseAvailable,
    /// FSKit availability (macOS)
    FSKitAvailable,
    /// Administrator privileges
    AdminPrivileges,
    /// Developer mode (Windows)
    DeveloperMode,
    /// Case sensitivity support
    CaseSensitivity,
    /// Extended attributes support
    ExtendedAttributes,
    /// Symbolic links support
    SymbolicLinks,
    /// Large file support
    LargeFiles,
    /// Long path support
    LongPaths,
}

/// Result of a feature detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStatus {
    /// Whether the feature is available
    pub available: bool,
    /// Additional details about the feature
    pub details: String,
    /// When this status was last checked
    pub last_checked: SystemTime,
    /// Version information if applicable
    pub version: Option<String>,
    /// Performance metrics if applicable
    pub performance: Option<PerformanceMetrics>,
}

/// Performance metrics for features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Average operation latency in milliseconds
    pub avg_latency_ms: f64,
    /// Peak latency in milliseconds
    pub peak_latency_ms: f64,
    /// Operations per second
    pub ops_per_second: f64,
    /// Number of samples
    pub sample_count: u64,
}

/// Change event for features
#[derive(Debug, Clone)]
pub enum FeatureChange {
    /// Feature became available
    Available {
        feature: FeatureType,
        details: String,
    },
    /// Feature became unavailable
    Unavailable {
        feature: FeatureType,
        reason: String,
    },
    /// Feature performance changed significantly
    PerformanceChange {
        feature: FeatureType,
        old_metrics: PerformanceMetrics,
        new_metrics: PerformanceMetrics,
    },
}

/// Cache entry for feature detection results
#[derive(Debug, Clone)]
struct CacheEntry {
    status: FeatureStatus,
    expires_at: Instant,
}

/// Runtime feature detector
pub struct RuntimeDetector {
    /// Cached detection results
    cache: Arc<RwLock<HashMap<FeatureType, CacheEntry>>>,
    /// Default TTL for cache entries
    default_ttl: Duration,
    /// Performance trackers
    perf_trackers: Arc<Mutex<HashMap<FeatureType, PerformanceTracker>>>,
}

impl RuntimeDetector {
    /// Create a new runtime detector with default settings
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Duration::from_secs(300), // 5 minutes
            perf_trackers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Create with custom cache TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: ttl,
            perf_trackers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Detect all features at startup
    pub fn detect_at_startup(&self) -> HashMap<FeatureType, FeatureStatus> {
        let mut results = HashMap::new();
        
        // Detect features based on platform
        let platform = Platform::current();
        
        match platform {
            Platform::Windows => {
                results.insert(FeatureType::ProjFSAvailable, self.detect_projfs());
                results.insert(FeatureType::DeveloperMode, self.detect_developer_mode());
                results.insert(FeatureType::SymbolicLinks, self.detect_symlink_support());
                results.insert(FeatureType::LongPaths, self.detect_long_path_support());
            }
            Platform::MacOS => {
                results.insert(FeatureType::MacFuseAvailable, self.detect_macfuse());
                results.insert(FeatureType::FSKitAvailable, self.detect_fskit());
                results.insert(FeatureType::ExtendedAttributes, self.detect_xattr_support());
            }
            Platform::Linux => {
                results.insert(FeatureType::FuseAvailable, self.detect_fuse());
                results.insert(FeatureType::ExtendedAttributes, self.detect_xattr_support());
            }
        }
        
        // Common features
        results.insert(FeatureType::AdminPrivileges, self.detect_admin_privileges());
        results.insert(FeatureType::CaseSensitivity, self.detect_case_sensitivity());
        results.insert(FeatureType::LargeFiles, self.detect_large_file_support());
        
        // Update cache
        let mut cache = self.cache.write().unwrap();
        let expires_at = Instant::now() + self.default_ttl;
        for (feature, status) in &results {
            cache.insert(*feature, CacheEntry {
                status: status.clone(),
                expires_at,
            });
        }
        
        results
    }
    
    /// Detect a specific feature on demand
    pub fn detect_on_demand(&self, feature: FeatureType, force_refresh: bool) -> FeatureStatus {
        // Check cache first
        if !force_refresh {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(&feature) {
                if entry.expires_at > Instant::now() {
                    return entry.status.clone();
                }
            }
        }
        
        // Perform detection
        let status = match feature {
            FeatureType::FuseAvailable => self.detect_fuse(),
            FeatureType::ProjFSAvailable => self.detect_projfs(),
            FeatureType::MacFuseAvailable => self.detect_macfuse(),
            FeatureType::FSKitAvailable => self.detect_fskit(),
            FeatureType::AdminPrivileges => self.detect_admin_privileges(),
            FeatureType::DeveloperMode => self.detect_developer_mode(),
            FeatureType::CaseSensitivity => self.detect_case_sensitivity(),
            FeatureType::ExtendedAttributes => self.detect_xattr_support(),
            FeatureType::SymbolicLinks => self.detect_symlink_support(),
            FeatureType::LargeFiles => self.detect_large_file_support(),
            FeatureType::LongPaths => self.detect_long_path_support(),
        };
        
        // Update cache
        let mut cache = self.cache.write().unwrap();
        cache.insert(feature, CacheEntry {
            status: status.clone(),
            expires_at: Instant::now() + self.default_ttl,
        });
        
        status
    }
    
    /// Start background refresh of cached features
    pub fn start_background_refresh(&self, interval: Duration) -> thread::JoinHandle<()> {
        let cache = Arc::clone(&self.cache);
        let detector = Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: self.default_ttl,
            perf_trackers: Arc::clone(&self.perf_trackers),
        };
        
        thread::spawn(move || {
            loop {
                thread::sleep(interval);
                
                // Get features that need refresh
                let features_to_refresh: Vec<FeatureType> = {
                    let cache_read = cache.read().unwrap();
                    cache_read.iter()
                        .filter(|(_, entry)| entry.expires_at <= Instant::now())
                        .map(|(feature, _)| *feature)
                        .collect()
                };
                
                // Refresh each feature
                for feature in features_to_refresh {
                    let status = detector.detect_on_demand(feature, true);
                    
                    let mut cache_write = cache.write().unwrap();
                    cache_write.insert(feature, CacheEntry {
                        status,
                        expires_at: Instant::now() + detector.default_ttl,
                    });
                }
            }
        })
    }
    
    /// Track performance for a feature operation
    pub fn track_operation(&self, feature: FeatureType, latency_ms: f64) {
        let mut trackers = self.perf_trackers.lock().unwrap();
        let tracker = trackers.entry(feature).or_insert_with(PerformanceTracker::new);
        tracker.add_sample(latency_ms);
    }
    
    /// Get performance metrics for a feature
    pub fn get_performance_metrics(&self, feature: FeatureType) -> Option<PerformanceMetrics> {
        let trackers = self.perf_trackers.lock().unwrap();
        trackers.get(&feature).map(|t| t.get_metrics())
    }
    
    // Platform-specific detection methods
    
    #[cfg(target_os = "linux")]
    fn detect_fuse(&self) -> FeatureStatus {
        use std::fs;
        
        // Check if FUSE device exists
        let fuse_dev = Path::new("/dev/fuse");
        if !fuse_dev.exists() {
            return FeatureStatus {
                available: false,
                details: "FUSE device not found".to_string(),
                last_checked: SystemTime::now(),
                version: None,
                performance: None,
            };
        }
        
        // Check if FUSE module is loaded
        if let Ok(modules) = fs::read_to_string("/proc/modules") {
            if !modules.contains("fuse") {
                return FeatureStatus {
                    available: false,
                    details: "FUSE module not loaded".to_string(),
                    last_checked: SystemTime::now(),
                    version: None,
                    performance: None,
                };
            }
        }
        
        // Try to get FUSE version
        let version = self.get_fuse_version();
        
        FeatureStatus {
            available: true,
            details: "FUSE is available and loaded".to_string(),
            last_checked: SystemTime::now(),
            version,
            performance: self.get_performance_metrics(FeatureType::FuseAvailable),
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    fn detect_fuse(&self) -> FeatureStatus {
        FeatureStatus {
            available: false,
            details: "FUSE is Linux-specific".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "windows")]
    fn detect_projfs(&self) -> FeatureStatus {
        use std::process::Command;
        
        // Check Windows version (ProjFS requires Windows 10 1809+)
        if let Ok(output) = Command::new("cmd")
            .args(&["/C", "ver"])
            .output()
        {
            let version_str = String::from_utf8_lossy(&output.stdout);
            // Parse version...
        }
        
        // Check if ProjFS is enabled
        if let Ok(output) = Command::new("powershell")
            .args(&["-Command", "Get-WindowsOptionalFeature -Online -FeatureName Client-ProjFS"])
            .output()
        {
            let result = String::from_utf8_lossy(&output.stdout);
            let available = result.contains("Enabled");
            
            return FeatureStatus {
                available,
                details: if available {
                    "Windows Projected File System is enabled".to_string()
                } else {
                    "Windows Projected File System is not enabled".to_string()
                },
                last_checked: SystemTime::now(),
                version: None,
                performance: self.get_performance_metrics(FeatureType::ProjFSAvailable),
            };
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot determine ProjFS status".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    fn detect_projfs(&self) -> FeatureStatus {
        FeatureStatus {
            available: false,
            details: "ProjFS is Windows-specific".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "macos")]
    fn detect_macfuse(&self) -> FeatureStatus {
        use std::fs;
        
        // Check common macFUSE locations
        let macfuse_paths = vec![
            "/usr/local/lib/libfuse.dylib",
            "/Library/Filesystems/macfuse.fs",
            "/Library/PreferencePanes/macFUSE.prefPane",
        ];
        
        let installed = macfuse_paths.iter().any(|p| Path::new(p).exists());
        
        if installed {
            // Try to get version
            let version = if let Ok(_plist) = fs::read_to_string("/Library/Filesystems/macfuse.fs/Contents/Info.plist") {
                // Parse version from plist...
                None
            } else {
                None
            };
            
            FeatureStatus {
                available: true,
                details: "macFUSE is installed".to_string(),
                last_checked: SystemTime::now(),
                version,
                performance: self.get_performance_metrics(FeatureType::MacFuseAvailable),
            }
        } else {
            FeatureStatus {
                available: false,
                details: "macFUSE is not installed".to_string(),
                last_checked: SystemTime::now(),
                version: None,
                performance: None,
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    fn detect_macfuse(&self) -> FeatureStatus {
        FeatureStatus {
            available: false,
            details: "macFUSE is macOS-specific".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "macos")]
    fn detect_fskit(&self) -> FeatureStatus {
        use std::process::Command;
        
        // Check macOS version (FSKit requires macOS 15.0+)
        if let Ok(output) = Command::new("sw_vers").args(&["-productVersion"]).output() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if let Some(major) = version_str.trim().split('.').next().and_then(|s| s.parse::<u32>().ok()) {
                if major >= 15 {
                    return FeatureStatus {
                        available: true,
                        details: format!("FSKit available on macOS {}", version_str.trim()),
                        last_checked: SystemTime::now(),
                        version: Some(version_str.trim().to_string()),
                        performance: self.get_performance_metrics(FeatureType::FSKitAvailable),
                    };
                }
            }
        }
        
        FeatureStatus {
            available: false,
            details: "FSKit requires macOS 15.0 or later".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    fn detect_fskit(&self) -> FeatureStatus {
        FeatureStatus {
            available: false,
            details: "FSKit is macOS-specific".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    fn detect_admin_privileges(&self) -> FeatureStatus {
        #[cfg(unix)]
        let is_admin = unsafe { libc::geteuid() } == 0;
        
        #[cfg(windows)]
        let is_admin = {
            use std::ptr;
            use winapi::um::securitybaseapi::GetTokenInformation;
            use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
            use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
            
            unsafe {
                let mut token = ptr::null_mut();
                if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0 {
                    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
                    let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
                    let result = GetTokenInformation(
                        token,
                        TokenElevation,
                        &mut elevation as *mut _ as *mut _,
                        size,
                        &mut size,
                    );
                    result != 0 && elevation.TokenIsElevated != 0
                } else {
                    false
                }
            }
        };
        
        FeatureStatus {
            available: is_admin,
            details: if is_admin {
                "Running with administrator privileges".to_string()
            } else {
                "Running without administrator privileges".to_string()
            },
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "windows")]
    fn detect_developer_mode(&self) -> FeatureStatus {
        use winreg::enums::*;
        use winreg::RegKey;
        
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock") {
            if let Ok(value) = key.get_value::<u32, _>("AllowDevelopmentWithoutDevLicense") {
                return FeatureStatus {
                    available: value != 0,
                    details: if value != 0 {
                        "Developer mode is enabled".to_string()
                    } else {
                        "Developer mode is not enabled".to_string()
                    },
                    last_checked: SystemTime::now(),
                    version: None,
                    performance: None,
                };
            }
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot determine developer mode status".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    fn detect_developer_mode(&self) -> FeatureStatus {
        FeatureStatus {
            available: false,
            details: "Developer mode is Windows-specific".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    fn detect_case_sensitivity(&self) -> FeatureStatus {
        use std::fs;
        use std::env;
        
        let temp_dir = env::temp_dir();
        let test_file_lower = temp_dir.join("shadowfs_case_test.tmp");
        let test_file_upper = temp_dir.join("SHADOWFS_CASE_TEST.tmp");
        
        // Create lowercase file
        if let Ok(_) = fs::write(&test_file_lower, "lower") {
            // Try to create uppercase file
            if let Ok(_) = fs::write(&test_file_upper, "upper") {
                // Read both files
                let lower_content = fs::read_to_string(&test_file_lower).unwrap_or_default();
                let upper_content = fs::read_to_string(&test_file_upper).unwrap_or_default();
                
                let _ = fs::remove_file(&test_file_lower);
                let _ = fs::remove_file(&test_file_upper);
                
                let case_sensitive = lower_content != upper_content;
                
                return FeatureStatus {
                    available: case_sensitive,
                    details: if case_sensitive {
                        "Filesystem is case-sensitive".to_string()
                    } else {
                        "Filesystem is case-insensitive".to_string()
                    },
                    last_checked: SystemTime::now(),
                    version: None,
                    performance: None,
                };
            }
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot determine case sensitivity".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    fn detect_xattr_support(&self) -> FeatureStatus {
        #[cfg(unix)]
        {
            use std::fs;
            use std::env;
            
            let temp_file = env::temp_dir().join("shadowfs_xattr_test.tmp");
            if let Ok(_) = fs::write(&temp_file, "test") {
                // Try to set an extended attribute
                #[cfg(target_os = "linux")]
                let result = unsafe {
                    use std::ffi::CString;
                    let path = CString::new(temp_file.to_str().unwrap()).unwrap();
                    let name = CString::new("user.shadowfs.test").unwrap();
                    let value = b"test";
                    libc::setxattr(
                        path.as_ptr(),
                        name.as_ptr(),
                        value.as_ptr() as *const _,
                        value.len(),
                        0,
                    )
                };
                
                #[cfg(target_os = "macos")]
                let result = unsafe {
                    use std::ffi::CString;
                    let path = CString::new(temp_file.to_str().unwrap()).unwrap();
                    let name = CString::new("com.shadowfs.test").unwrap();
                    let value = b"test";
                    libc::setxattr(
                        path.as_ptr(),
                        name.as_ptr(),
                        value.as_ptr() as *const _,
                        value.len(),
                        0,
                        0,
                    )
                };
                
                let _ = fs::remove_file(&temp_file);
                
                return FeatureStatus {
                    available: result == 0,
                    details: if result == 0 {
                        "Extended attributes are supported".to_string()
                    } else {
                        "Extended attributes are not supported".to_string()
                    },
                    last_checked: SystemTime::now(),
                    version: None,
                    performance: None,
                };
            }
        }
        
        #[cfg(windows)]
        {
            // Windows doesn't have xattr in the same way
            return FeatureStatus {
                available: false,
                details: "Extended attributes not supported on Windows".to_string(),
                last_checked: SystemTime::now(),
                version: None,
                performance: None,
            };
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot determine extended attributes support".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    fn detect_symlink_support(&self) -> FeatureStatus {
        use std::fs;
        use std::env;
        
        let temp_dir = env::temp_dir();
        let target = temp_dir.join("shadowfs_symlink_target.tmp");
        let link = temp_dir.join("shadowfs_symlink.tmp");
        
        if let Ok(_) = fs::write(&target, "test") {
            #[cfg(unix)]
            let result = std::os::unix::fs::symlink(&target, &link);
            
            #[cfg(windows)]
            let result = std::os::windows::fs::symlink_file(&target, &link);
            
            let _ = fs::remove_file(&target);
            let _ = fs::remove_file(&link);
            
            return FeatureStatus {
                available: result.is_ok(),
                details: if result.is_ok() {
                    "Symbolic links are supported".to_string()
                } else {
                    format!("Symbolic links not supported: {:?}", result.err())
                },
                last_checked: SystemTime::now(),
                version: None,
                performance: None,
            };
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot test symbolic link support".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    fn detect_large_file_support(&self) -> FeatureStatus {
        // Most modern filesystems support large files
        // This is a simplified check
        FeatureStatus {
            available: true,
            details: "Large file support assumed available".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "windows")]
    fn detect_long_path_support(&self) -> FeatureStatus {
        use winreg::enums::*;
        use winreg::RegKey;
        
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = hklm.open_subkey("SYSTEM\\CurrentControlSet\\Control\\FileSystem") {
            if let Ok(value) = key.get_value::<u32, _>("LongPathsEnabled") {
                return FeatureStatus {
                    available: value != 0,
                    details: if value != 0 {
                        "Long paths are enabled".to_string()
                    } else {
                        "Long paths are not enabled".to_string()
                    },
                    last_checked: SystemTime::now(),
                    version: None,
                    performance: None,
                };
            }
        }
        
        FeatureStatus {
            available: false,
            details: "Cannot determine long path support".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    fn detect_long_path_support(&self) -> FeatureStatus {
        // Unix systems generally support long paths
        FeatureStatus {
            available: true,
            details: "Long paths are supported".to_string(),
            last_checked: SystemTime::now(),
            version: None,
            performance: None,
        }
    }
    
    #[cfg(target_os = "linux")]
    fn get_fuse_version(&self) -> Option<String> {
        use std::process::Command;
        
        if let Ok(output) = Command::new("fusermount").arg("--version").output() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if let Some(version) = version_str.split(':').nth(1) {
                return Some(version.trim().to_string());
            }
        }
        None
    }
    
    #[cfg(not(target_os = "linux"))]
    fn get_fuse_version(&self) -> Option<String> {
        None
    }
}

/// Performance tracker for feature operations
struct PerformanceTracker {
    samples: Vec<f64>,
    total_latency: f64,
    peak_latency: f64,
    start_time: Instant,
}

impl PerformanceTracker {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
            total_latency: 0.0,
            peak_latency: 0.0,
            start_time: Instant::now(),
        }
    }
    
    fn add_sample(&mut self, latency_ms: f64) {
        self.samples.push(latency_ms);
        self.total_latency += latency_ms;
        if latency_ms > self.peak_latency {
            self.peak_latency = latency_ms;
        }
        
        // Keep only recent samples (last 1000)
        if self.samples.len() > 1000 {
            let removed = self.samples.remove(0);
            self.total_latency -= removed;
        }
    }
    
    fn get_metrics(&self) -> PerformanceMetrics {
        let sample_count = self.samples.len() as u64;
        let avg_latency_ms = if sample_count > 0 {
            self.total_latency / sample_count as f64
        } else {
            0.0
        };
        
        let duration = self.start_time.elapsed().as_secs_f64();
        let ops_per_second = if duration > 0.0 {
            sample_count as f64 / duration
        } else {
            0.0
        };
        
        PerformanceMetrics {
            avg_latency_ms,
            peak_latency_ms: self.peak_latency,
            ops_per_second,
            sample_count,
        }
    }
}

/// Feature monitor for watching system changes
pub struct FeatureMonitor {
    detector: Arc<RuntimeDetector>,
    callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
    running: Arc<RwLock<bool>>,
}

impl FeatureMonitor {
    /// Create a new feature monitor
    pub fn new(detector: Arc<RuntimeDetector>) -> Self {
        Self {
            detector,
            callbacks: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Add a callback for feature changes
    pub fn watch_for_changes<F>(&self, callback: F)
    where
        F: Fn(FeatureChange) + Send + 'static,
    {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.push(Box::new(callback));
    }
    
    /// Start monitoring for changes
    pub fn start(&self) -> Result<thread::JoinHandle<()>> {
        let mut running = self.running.write().unwrap();
        if *running {
            return Err(ShadowError::InvalidConfiguration {
                message: "Monitor already running".to_string(),
            });
        }
        *running = true;
        
        let detector = Arc::clone(&self.detector);
        let callbacks = Arc::clone(&self.callbacks);
        let running_flag = Arc::clone(&self.running);
        let platform = Platform::current();
        
        let handle = thread::spawn(move || {
            #[cfg(target_os = "linux")]
            {
                if platform == Platform::Linux {
                    Self::monitor_linux(detector, callbacks, running_flag);
                    return;
                }
            }
            
            #[cfg(target_os = "macos")]
            {
                if platform == Platform::MacOS {
                    Self::monitor_macos(detector, callbacks, running_flag);
                    return;
                }
            }
            
            #[cfg(target_os = "windows")]
            {
                if platform == Platform::Windows {
                    Self::monitor_windows(detector, callbacks, running_flag);
                    return;
                }
            }
            
            // Fallback for any platform
            while *running_flag.read().unwrap() {
                thread::sleep(Duration::from_secs(1));
            }
        });
        
        Ok(handle)
    }
    
    /// Stop monitoring
    pub fn stop(&self) {
        let mut running = self.running.write().unwrap();
        *running = false;
    }
    
    #[cfg(target_os = "linux")]
    fn monitor_linux(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        use inotify::{Inotify, WatchMask};
        
        let mut inotify = match Inotify::init() {
            Ok(i) => i,
            Err(_) => return,
        };
        
        // Watch for FUSE module changes
        let _ = inotify.add_watch("/proc/modules", WatchMask::MODIFY);
        let _ = inotify.add_watch("/dev", WatchMask::CREATE | WatchMask::DELETE);
        
        let mut buffer = [0u8; 4096];
        let mut last_fuse_status = detector.detect_on_demand(FeatureType::FuseAvailable, false);
        
        while *running.read().unwrap() {
            if let Ok(events) = inotify.read_events(&mut buffer) {
                for _event in events {
                    // Check if FUSE status changed
                    let new_status = detector.detect_on_demand(FeatureType::FuseAvailable, true);
                    
                    if new_status.available != last_fuse_status.available {
                        let change = if new_status.available {
                            FeatureChange::Available {
                                feature: FeatureType::FuseAvailable,
                                details: new_status.details.clone(),
                            }
                        } else {
                            FeatureChange::Unavailable {
                                feature: FeatureType::FuseAvailable,
                                reason: new_status.details.clone(),
                            }
                        };
                        
                        let callbacks = callbacks.lock().unwrap();
                        for callback in callbacks.iter() {
                            callback(change.clone());
                        }
                        
                        last_fuse_status = new_status;
                    }
                }
            }
            
            thread::sleep(Duration::from_millis(100));
        }
    }
    
    #[cfg(target_os = "macos")]
    fn monitor_macos(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        // Use FSEvents to monitor filesystem changes
        // This is a simplified implementation
        let mut last_macfuse_status = detector.detect_on_demand(FeatureType::MacFuseAvailable, false);
        let mut last_fskit_status = detector.detect_on_demand(FeatureType::FSKitAvailable, false);
        
        while *running.read().unwrap() {
            thread::sleep(Duration::from_secs(5));
            
            // Check macFUSE status
            let new_macfuse = detector.detect_on_demand(FeatureType::MacFuseAvailable, true);
            if new_macfuse.available != last_macfuse_status.available {
                let change = if new_macfuse.available {
                    FeatureChange::Available {
                        feature: FeatureType::MacFuseAvailable,
                        details: new_macfuse.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::MacFuseAvailable,
                        reason: new_macfuse.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_macfuse_status = new_macfuse;
            }
            
            // Check FSKit status
            let new_fskit = detector.detect_on_demand(FeatureType::FSKitAvailable, true);
            if new_fskit.available != last_fskit_status.available {
                let change = if new_fskit.available {
                    FeatureChange::Available {
                        feature: FeatureType::FSKitAvailable,
                        details: new_fskit.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::FSKitAvailable,
                        reason: new_fskit.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_fskit_status = new_fskit;
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    fn monitor_windows(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        // Monitor Windows features using WMI or registry polling
        let mut last_projfs_status = detector.detect_on_demand(FeatureType::ProjFSAvailable, false);
        let mut last_dev_mode = detector.detect_on_demand(FeatureType::DeveloperMode, false);
        
        while *running.read().unwrap() {
            thread::sleep(Duration::from_secs(5));
            
            // Check ProjFS status
            let new_projfs = detector.detect_on_demand(FeatureType::ProjFSAvailable, true);
            if new_projfs.available != last_projfs_status.available {
                let change = if new_projfs.available {
                    FeatureChange::Available {
                        feature: FeatureType::ProjFSAvailable,
                        details: new_projfs.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::ProjFSAvailable,
                        reason: new_projfs.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_projfs_status = new_projfs;
            }
            
            // Check Developer Mode
            let new_dev_mode = detector.detect_on_demand(FeatureType::DeveloperMode, true);
            if new_dev_mode.available != last_dev_mode.available {
                let change = if new_dev_mode.available {
                    FeatureChange::Available {
                        feature: FeatureType::DeveloperMode,
                        details: new_dev_mode.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::DeveloperMode,
                        reason: new_dev_mode.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_dev_mode = new_dev_mode;
            }
        }
    }
}

/// Fallback mechanism for feature operations
pub struct FallbackMechanism {
    primary_method: String,
    fallback_methods: Vec<String>,
    notification_handler: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl FallbackMechanism {
    /// Create a new fallback mechanism
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            primary_method: primary.into(),
            fallback_methods: Vec::new(),
            notification_handler: None,
        }
    }
    
    /// Add a fallback method
    pub fn with_fallback(mut self, method: impl Into<String>) -> Self {
        self.fallback_methods.push(method.into());
        self
    }
    
    /// Set notification handler for fallback events
    pub fn with_notification<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.notification_handler = Some(Box::new(handler));
        self
    }
    
    /// Execute with fallback
    pub fn execute<F, T>(&self, operation: F) -> Result<T>
    where
        F: Fn(&str) -> Result<T>,
    {
        // Try primary method
        match operation(&self.primary_method) {
            Ok(result) => Ok(result),
            Err(primary_err) => {
                // Notify about primary failure
                if let Some(handler) = &self.notification_handler {
                    handler(&format!(
                        "Primary method '{}' failed: {}. Trying fallbacks...",
                        self.primary_method, primary_err
                    ));
                }
                
                // Try fallback methods
                for (i, method) in self.fallback_methods.iter().enumerate() {
                    match operation(method) {
                        Ok(result) => {
                            if let Some(handler) = &self.notification_handler {
                                handler(&format!(
                                    "Fallback method '{}' succeeded",
                                    method
                                ));
                            }
                            return Ok(result);
                        }
                        Err(err) => {
                            if let Some(handler) = &self.notification_handler {
                                handler(&format!(
                                    "Fallback method {} of {} failed: {}",
                                    i + 1,
                                    self.fallback_methods.len(),
                                    err
                                ));
                            }
                        }
                    }
                }
                
                // All methods failed
                Err(primary_err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_runtime_detector_creation() {
        let detector = RuntimeDetector::new();
        assert_eq!(detector.default_ttl, Duration::from_secs(300));
        
        let custom_detector = RuntimeDetector::with_ttl(Duration::from_secs(60));
        assert_eq!(custom_detector.default_ttl, Duration::from_secs(60));
    }
    
    #[test]
    fn test_feature_status() {
        let status = FeatureStatus {
            available: true,
            details: "Test feature".to_string(),
            last_checked: SystemTime::now(),
            version: Some("1.0".to_string()),
            performance: None,
        };
        
        assert!(status.available);
        assert_eq!(status.details, "Test feature");
        assert_eq!(status.version, Some("1.0".to_string()));
    }
    
    #[test]
    fn test_performance_tracker() {
        let mut tracker = PerformanceTracker::new();
        
        tracker.add_sample(10.0);
        tracker.add_sample(20.0);
        tracker.add_sample(15.0);
        
        let metrics = tracker.get_metrics();
        assert_eq!(metrics.avg_latency_ms, 15.0);
        assert_eq!(metrics.peak_latency_ms, 20.0);
        assert_eq!(metrics.sample_count, 3);
    }
    
    #[test]
    fn test_fallback_mechanism() {
        let fallback = FallbackMechanism::new("primary")
            .with_fallback("secondary")
            .with_fallback("tertiary");
        
        // Test successful primary
        let result = fallback.execute(|method| {
            if method == "primary" {
                Ok(42)
            } else {
                Err(ShadowError::InvalidConfiguration {
                    message: "Not primary".to_string(),
                })
            }
        });
        assert_eq!(result.unwrap(), 42);
        
        // Test fallback to secondary
        let result = fallback.execute(|method| {
            if method == "secondary" {
                Ok(24)
            } else {
                Err(ShadowError::InvalidConfiguration {
                    message: "Not secondary".to_string(),
                })
            }
        });
        assert_eq!(result.unwrap(), 24);
    }
    
    #[test]
    fn test_cache_expiry() {
        let detector = RuntimeDetector::with_ttl(Duration::from_millis(100));
        
        // Force a detection to populate cache
        let _status1 = detector.detect_on_demand(FeatureType::AdminPrivileges, false);
        
        // Immediate second call should use cache
        let _status2 = detector.detect_on_demand(FeatureType::AdminPrivileges, false);
        
        // Wait for cache to expire
        thread::sleep(Duration::from_millis(150));
        
        // This should trigger a new detection
        let _status3 = detector.detect_on_demand(FeatureType::AdminPrivileges, false);
    }
}