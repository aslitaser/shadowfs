//! macOS-specific feature detection

use std::fs;
use std::path::Path;
use std::time::SystemTime;
use std::process::Command;
use crate::platform::runtime::types::FeatureStatus;

/// Detect macFUSE availability on macOS
pub fn detect_macfuse() -> FeatureStatus {
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
            performance: None,
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

/// Detect FSKit availability on macOS
pub fn detect_fskit() -> FeatureStatus {
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
                    performance: None,
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

/// Detect extended attributes support on macOS
pub fn detect_xattr_support() -> FeatureStatus {
    use std::env;
    
    let temp_file = env::temp_dir().join("shadowfs_xattr_test.tmp");
    if let Ok(_) = fs::write(&temp_file, "test") {
        // Try to set an extended attribute
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
    
    FeatureStatus {
        available: false,
        details: "Cannot determine extended attributes support".to_string(),
        last_checked: SystemTime::now(),
        version: None,
        performance: None,
    }
}

/// Check admin privileges on macOS
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