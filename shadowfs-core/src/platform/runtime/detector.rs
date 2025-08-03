//! Runtime feature detector

use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::thread;
use crate::types::mount::Platform;
use crate::platform::runtime::types::*;

// Import common detection functions
use crate::platform::runtime::detector_common::*;

// Import platform-specific functions with explicit names
#[cfg(target_os = "linux")]
use crate::platform::runtime::detector_linux::{
    detect_fuse as platform_detect_fuse,
    detect_xattr_support as platform_detect_xattr,
    detect_admin_privileges as platform_detect_admin,
};

#[cfg(target_os = "macos")]
use crate::platform::runtime::detector_macos::{
    detect_macfuse,
    detect_fskit,
    detect_xattr_support as platform_detect_xattr,
    detect_admin_privileges as platform_detect_admin,
};

#[cfg(target_os = "windows")]
use crate::platform::runtime::detector_windows::{
    detect_projfs as platform_detect_projfs,
    detect_developer_mode as platform_detect_dev_mode,
    detect_long_path_support as platform_detect_long_paths,
    detect_admin_privileges as platform_detect_admin,
};

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
                #[cfg(target_os = "windows")]
                {
                    results.insert(FeatureType::ProjFSAvailable, platform_detect_projfs());
                    results.insert(FeatureType::DeveloperMode, platform_detect_dev_mode());
                    results.insert(FeatureType::LongPaths, platform_detect_long_paths());
                }
                results.insert(FeatureType::SymbolicLinks, detect_symlink_support());
            }
            Platform::MacOS => {
                #[cfg(target_os = "macos")]
                {
                    results.insert(FeatureType::MacFuseAvailable, detect_macfuse());
                    results.insert(FeatureType::FSKitAvailable, detect_fskit());
                    results.insert(FeatureType::ExtendedAttributes, platform_detect_xattr());
                }
            }
            Platform::Linux => {
                #[cfg(target_os = "linux")]
                {
                    results.insert(FeatureType::FuseAvailable, platform_detect_fuse());
                    results.insert(FeatureType::ExtendedAttributes, platform_detect_xattr());
                }
            }
        }
        
        // Common features
        results.insert(FeatureType::AdminPrivileges, self.detect_admin_privileges());
        results.insert(FeatureType::CaseSensitivity, detect_case_sensitivity());
        results.insert(FeatureType::LargeFiles, detect_large_file_support());
        
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
            FeatureType::FuseAvailable => {
                #[cfg(target_os = "linux")]
                { platform_detect_fuse() }
                #[cfg(not(target_os = "linux"))]
                { detect_fuse() }
            }
            FeatureType::ProjFSAvailable => {
                #[cfg(target_os = "windows")]
                { platform_detect_projfs() }
                #[cfg(not(target_os = "windows"))]
                { detect_projfs() }
            }
            FeatureType::MacFuseAvailable => {
                #[cfg(target_os = "macos")]
                { detect_macfuse() }
                #[cfg(not(target_os = "macos"))]
                { detect_macfuse() }
            }
            FeatureType::FSKitAvailable => {
                #[cfg(target_os = "macos")]
                { detect_fskit() }
                #[cfg(not(target_os = "macos"))]
                { detect_fskit() }
            }
            FeatureType::AdminPrivileges => self.detect_admin_privileges(),
            FeatureType::DeveloperMode => {
                #[cfg(target_os = "windows")]
                { platform_detect_dev_mode() }
                #[cfg(not(target_os = "windows"))]
                { detect_developer_mode() }
            }
            FeatureType::CaseSensitivity => detect_case_sensitivity(),
            FeatureType::ExtendedAttributes => {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                { platform_detect_xattr() }
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                { detect_xattr_support() }
            }
            FeatureType::SymbolicLinks => detect_symlink_support(),
            FeatureType::LargeFiles => detect_large_file_support(),
            FeatureType::LongPaths => {
                #[cfg(target_os = "windows")]
                { platform_detect_long_paths() }
                #[cfg(not(target_os = "windows"))]
                { detect_long_path_support() }
            }
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
    
    /// Detect admin privileges (platform-specific)
    fn detect_admin_privileges(&self) -> FeatureStatus {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        return platform_detect_admin();
        
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        return detect_admin_privileges();
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