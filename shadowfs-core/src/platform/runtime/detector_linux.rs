//! Linux-specific feature detection

use std::fs;
use std::path::Path;
use std::time::SystemTime;
use crate::platform::runtime::types::{FeatureStatus, FeatureType, PerformanceMetrics};

/// Detect FUSE availability on Linux
pub fn detect_fuse() -> FeatureStatus {
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
    let version = get_fuse_version();
    
    FeatureStatus {
        available: true,
        details: "FUSE is available and loaded".to_string(),
        last_checked: SystemTime::now(),
        version,
        performance: None,
    }
}

/// Get FUSE version on Linux
pub fn get_fuse_version() -> Option<String> {
    use std::process::Command;
    
    if let Ok(output) = Command::new("fusermount").arg("--version").output() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        if let Some(version) = version_str.split(':').nth(1) {
            return Some(version.trim().to_string());
        }
    }
    None
}

/// Detect extended attributes support on Linux
pub fn detect_xattr_support() -> FeatureStatus {
    use std::env;
    
    let temp_file = env::temp_dir().join("shadowfs_xattr_test.tmp");
    if let Ok(_) = fs::write(&temp_file, "test") {
        // Try to set an extended attribute
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
    
    FeatureStatus {
        available: false,
        details: "Cannot determine extended attributes support".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

/// Check admin privileges on Linux
pub fn detect_admin_privileges() -> FeatureStatus {
    let is_admin = unsafe { libc::geteuid() } == 0;
    
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