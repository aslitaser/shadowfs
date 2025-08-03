//! Common feature detection functions for all platforms

use std::fs;
use std::env;
use std::time::SystemTime;
use crate::platform::runtime::types::FeatureStatus;

/// Detect case sensitivity behavior
pub fn detect_case_sensitivity() -> FeatureStatus {
    let temp_dir = env::temp_dir();
    let test_file_lower = temp_dir.join("shadowfs_case_test.tmp");
    let test_file_upper = temp_dir.join("SHADOWFS_CASE_TEST.tmp");
    
    // Create lowercase file
    if let Ok(_) = fs::write(&test_file_lower, "lowercase") {
        // Try to create uppercase file
        if let Ok(_) = fs::write(&test_file_upper, "uppercase") {
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

/// Detect symbolic link support
pub fn detect_symlink_support() -> FeatureStatus {
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

/// Detect large file support
pub fn detect_large_file_support() -> FeatureStatus {
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

/// Detect long path support (Unix always supports)
#[cfg(not(target_os = "windows"))]
pub fn detect_long_path_support() -> FeatureStatus {
    FeatureStatus {
        available: true,
        details: "Long paths are supported".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

/// Platform-specific stub functions for cross-platform compilation
#[cfg(not(target_os = "linux"))]
pub fn detect_fuse() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "FUSE is Linux-specific".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn detect_projfs() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "ProjFS is Windows-specific".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn detect_developer_mode() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "Developer mode is Windows-specific".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_macfuse() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "macFUSE is macOS-specific".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_fskit() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "FSKit is macOS-specific".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn detect_xattr_support() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "Extended attributes not supported on this platform".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

#[cfg(not(unix))]
pub fn detect_admin_privileges() -> FeatureStatus {
    FeatureStatus {
        available: false,
        details: "Cannot detect admin privileges on this platform".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}