use super::{
    Architecture, DetectionError, DetectionResult, RuntimePlatform, SystemInfo, Version,
};
use std::path::PathBuf;

pub struct WindowsDetector;

#[derive(Debug, Clone)]
pub struct ProjFSInfo {
    pub available: bool,
    pub enabled: bool,
    pub version: Option<Version>,
    pub dll_path: Option<PathBuf>,
}

impl WindowsDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_system_info(&self) -> DetectionResult<SystemInfo> {
        if !cfg!(target_os = "windows") {
            return Err(DetectionError::UnsupportedPlatform(
                "Not running on Windows".to_string(),
            ));
        }

        let version = self.detect_windows_version()?;
        let kernel_version = Some(version.clone());
        let architecture = self.detect_architecture();
        let hostname = self.detect_hostname()?;
        let cpu_count = self.detect_cpu_count();
        let total_memory_mb = self.detect_memory()?;

        Ok(SystemInfo {
            platform: RuntimePlatform::Windows,
            version,
            kernel_version,
            architecture,
            hostname,
            cpu_count,
            total_memory_mb,
        })
    }

    pub fn detect_projfs(&self) -> DetectionResult<ProjFSInfo> {
        #[cfg(target_os = "windows")]
        {
            // First check Windows version - ProjFS requires Windows 10 1809+ or Windows 11
            let version = self.detect_windows_version()?;
            
            // Windows 10 is version 10.0, build 17763 is 1809
            // Windows 11 is version 10.0, build 22000+
            let is_supported_version = version.major == 10 && version.patch >= 17763;
            
            if !is_supported_version {
                return Ok(ProjFSInfo {
                    available: false,
                    enabled: false,
                    version: None,
                    dll_path: None,
                });
            }

            // Check if ProjFS is enabled using PowerShell
            let enabled = self.check_projfs_enabled()?;
            
            // Check for ProjectedFSLib.dll
            let dll_path = self.find_projfs_dll();
            let available = dll_path.is_some();
            
            // Try to determine ProjFS version
            let projfs_version = if available {
                self.get_projfs_version()
            } else {
                None
            };
            
            Ok(ProjFSInfo {
                available,
                enabled,
                version: projfs_version,
                dll_path,
            })
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Err(DetectionError::UnsupportedPlatform(
                "ProjFS detection only available on Windows".to_string(),
            ))
        }
    }

    pub fn is_admin(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
            use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
            
            unsafe {
                let mut token_handle = HANDLE::default();
                if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle).is_ok() {
                    let mut elevation = TOKEN_ELEVATION::default();
                    let mut return_length = 0u32;
                    
                    if GetTokenInformation(
                        token_handle,
                        TokenElevation,
                        Some(&mut elevation as *mut _ as *mut _),
                        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                        &mut return_length,
                    ).is_ok() {
                        return elevation.TokenIsElevated != 0;
                    }
                }
            }
        }
        
        false
    }

    #[cfg(target_os = "windows")]
    fn detect_windows_version(&self) -> DetectionResult<Version> {
        use windows::Win32::System::SystemInformation::{RtlGetVersion, OSVERSIONINFOEXW};
        use windows::Win32::Foundation::STATUS_SUCCESS;
        
        unsafe {
            let mut version_info = OSVERSIONINFOEXW::default();
            version_info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOEXW>() as u32;
            
            let status = RtlGetVersion(&mut version_info as *mut _ as *mut _);
            
            if status == STATUS_SUCCESS {
                Ok(Version {
                    major: version_info.dwMajorVersion,
                    minor: version_info.dwMinorVersion,
                    patch: version_info.dwBuildNumber,
                    build: Some(version_info.wServicePackMajor as u32),
                })
            } else {
                Err(DetectionError::CommandFailed {
                    command: "RtlGetVersion".to_string(),
                    error: format!("Failed with status: {:?}", status),
                })
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_windows_version(&self) -> DetectionResult<Version> {
        Err(DetectionError::UnsupportedPlatform(
            "Windows version detection only available on Windows".to_string(),
        ))
    }

    fn detect_architecture(&self) -> Architecture {
        match std::env::consts::ARCH {
            "x86_64" => Architecture::X64,
            "aarch64" => Architecture::Arm64,
            _ => Architecture::Unknown,
        }
    }

    fn detect_hostname(&self) -> DetectionResult<String> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::SystemInformation::{GetComputerNameExW, ComputerNamePhysicalDnsHostname};
            use windows::core::PWSTR;
            
            unsafe {
                let mut size = 0u32;
                // First call to get required size
                let _ = GetComputerNameExW(ComputerNamePhysicalDnsHostname, PWSTR::null(), &mut size);
                
                let mut buffer = vec![0u16; size as usize];
                if GetComputerNameExW(
                    ComputerNamePhysicalDnsHostname,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size
                ).is_ok() {
                    let hostname = String::from_utf16_lossy(&buffer);
                    Ok(hostname.trim_end_matches('\0').to_string())
                } else {
                    Err(DetectionError::CommandFailed {
                        command: "GetComputerNameExW".to_string(),
                        error: "Failed to get hostname".to_string(),
                    })
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Ok("unknown".to_string())
        }
    }

    fn detect_cpu_count(&self) -> usize {
        num_cpus::get()
    }

    fn detect_memory(&self) -> DetectionResult<u64> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::SystemInformation::{GetPhysicallyInstalledSystemMemory};
            
            unsafe {
                let mut memory_kb = 0u64;
                if GetPhysicallyInstalledSystemMemory(&mut memory_kb).is_ok() {
                    Ok(memory_kb / 1024) // Convert KB to MB
                } else {
                    Err(DetectionError::CommandFailed {
                        command: "GetPhysicallyInstalledSystemMemory".to_string(),
                        error: "Failed to get system memory".to_string(),
                    })
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Ok(0)
        }
    }

    #[cfg(target_os = "windows")]
    fn check_projfs_enabled(&self) -> DetectionResult<bool> {
        use std::process::Command;
        
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                "Get-WindowsOptionalFeature -Online -FeatureName Client-ProjFS | Select-Object -ExpandProperty State"
            ])
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "PowerShell Get-WindowsOptionalFeature".to_string(),
                error: e.to_string(),
            })?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(output_str.trim().eq_ignore_ascii_case("Enabled"))
    }

    #[cfg(target_os = "windows")]
    fn find_projfs_dll(&self) -> Option<PathBuf> {
        let system32 = std::env::var("WINDIR")
            .map(|windir| PathBuf::from(windir).join("System32"))
            .ok()?;
        
        let dll_path = system32.join("ProjectedFSLib.dll");
        if dll_path.exists() {
            Some(dll_path)
        } else {
            None
        }
    }

    #[cfg(target_os = "windows")]
    fn get_projfs_version(&self) -> Option<Version> {
        // In a real implementation, we would read the DLL version info
        // For now, return a default version if ProjFS is available
        Some(Version {
            major: 1,
            minor: 0,
            patch: 0,
            build: None,
        })
    }

    pub fn enable_projfs(&self) -> DetectionResult<String> {
        #[cfg(target_os = "windows")]
        {
            if !self.is_admin() {
                return Ok(r#"ProjFS requires administrator privileges to enable.

To enable ProjFS manually:
1. Open PowerShell as Administrator
2. Run: Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
3. Restart your computer

Alternatively, enable via Windows Features:
1. Open "Turn Windows features on or off"
2. Check "Windows Projected File System"
3. Click OK and restart"#.to_string());
            }

            // Generate PowerShell script to enable ProjFS
            let script = r#"
Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
if ($?) {
    Write-Host "ProjFS enabled successfully. Please restart your computer."
} else {
    Write-Host "Failed to enable ProjFS."
    exit 1
}
"#;

            Ok(format!("PowerShell script to enable ProjFS:\n{}", script))
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Err(DetectionError::UnsupportedPlatform(
                "ProjFS is only available on Windows".to_string(),
            ))
        }
    }

    pub fn supports_long_paths(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Registry::*;
            use windows::Win32::Foundation::ERROR_SUCCESS;
            use windows::core::PCWSTR;
            
            unsafe {
                let mut key = HKEY::default();
                let subkey = "SYSTEM\\CurrentControlSet\\Control\\FileSystem";
                let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
                
                if RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    PCWSTR(subkey_wide.as_ptr()),
                    0,
                    KEY_READ,
                    &mut key
                ) == ERROR_SUCCESS {
                    let mut value: u32 = 0;
                    let mut size = std::mem::size_of::<u32>() as u32;
                    let value_name = "LongPathsEnabled";
                    let value_name_wide: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
                    
                    if RegQueryValueExW(
                        key,
                        PCWSTR(value_name_wide.as_ptr()),
                        None,
                        None,
                        Some(&mut value as *mut _ as *mut _),
                        Some(&mut size)
                    ) == ERROR_SUCCESS {
                        let _ = RegCloseKey(key);
                        return value != 0;
                    }
                    let _ = RegCloseKey(key);
                }
            }
        }
        
        false
    }

    pub fn has_developer_mode(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Registry::*;
            use windows::Win32::Foundation::ERROR_SUCCESS;
            use windows::core::PCWSTR;
            
            unsafe {
                let mut key = HKEY::default();
                let subkey = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock";
                let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
                
                if RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    PCWSTR(subkey_wide.as_ptr()),
                    0,
                    KEY_READ,
                    &mut key
                ) == ERROR_SUCCESS {
                    let mut value: u32 = 0;
                    let mut size = std::mem::size_of::<u32>() as u32;
                    let value_name = "AllowDevelopmentWithoutDevLicense";
                    let value_name_wide: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
                    
                    if RegQueryValueExW(
                        key,
                        PCWSTR(value_name_wide.as_ptr()),
                        None,
                        None,
                        Some(&mut value as *mut _ as *mut _),
                        Some(&mut size)
                    ) == ERROR_SUCCESS {
                        let _ = RegCloseKey(key);
                        return value != 0;
                    }
                    let _ = RegCloseKey(key);
                }
            }
        }
        
        false
    }

    pub fn get_max_path_length(&self) -> usize {
        if self.supports_long_paths() {
            32767 // Windows long path limit
        } else {
            260 // Traditional MAX_PATH
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_detection() {
        let detector = WindowsDetector::new();
        let arch = detector.detect_architecture();
        assert!(matches!(
            arch,
            Architecture::X64 | Architecture::Arm64 | Architecture::Unknown
        ));
    }

    #[test]
    fn test_max_path_length() {
        let detector = WindowsDetector::new();
        let max_path = detector.get_max_path_length();
        assert!(max_path == 260 || max_path == 32767);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_is_admin() {
        let detector = WindowsDetector::new();
        // Just verify it returns a boolean without error
        let _ = detector.is_admin();
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_is_admin_on_non_windows() {
        let detector = WindowsDetector::new();
        assert!(!detector.is_admin());
    }
}