//! Windows-specific feature detection

use std::time::SystemTime;
use std::process::Command;
use crate::platform::runtime::types::{FeatureStatus, FeatureType, PerformanceMetrics};

/// Detect ProjFS availability on Windows
pub fn detect_projfs() -> FeatureStatus {
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
            performance: None,
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

/// Detect developer mode on Windows
pub fn detect_developer_mode() -> FeatureStatus {
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

/// Detect long path support on Windows
pub fn detect_long_path_support() -> FeatureStatus {
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

/// Check admin privileges on Windows
pub fn detect_admin_privileges() -> FeatureStatus {
    use std::ptr;
    use winapi::um::securitybaseapi::GetTokenInformation;
    use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
    
    let is_admin = unsafe {
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