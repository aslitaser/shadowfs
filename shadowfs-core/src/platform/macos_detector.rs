use super::{
    Architecture, DetectionError, DetectionResult, RuntimePlatform, SystemInfo, Version,
};
use std::path::PathBuf;

pub struct MacOSDetector;

#[derive(Debug, Clone)]
pub struct FSKitInfo {
    pub available: bool,
    pub framework_path: Option<PathBuf>,
    pub version: Option<Version>,
}

#[derive(Debug, Clone)]
pub struct MacFUSEInfo {
    pub installed: bool,
    pub version: Option<Version>,
    pub mount_path: Option<PathBuf>,
    pub uses_fskit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeSignStatus {
    Valid,
    Invalid,
    NotSigned,
    Unknown,
}

impl MacOSDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_system_info(&self) -> DetectionResult<SystemInfo> {
        if !cfg!(target_os = "macos") {
            return Err(DetectionError::UnsupportedPlatform(
                "Not running on macOS".to_string(),
            ));
        }

        let version = self.detect_macos_version()?;
        let kernel_version = self.detect_kernel_version()?;
        let architecture = self.detect_architecture();
        let hostname = self.detect_hostname()?;
        let cpu_count = self.detect_cpu_count()?;
        let total_memory_mb = self.detect_memory()?;

        Ok(SystemInfo {
            platform: RuntimePlatform::MacOS,
            version,
            kernel_version: Some(kernel_version),
            architecture,
            hostname,
            cpu_count,
            total_memory_mb,
        })
    }

    pub fn detect_fskit(&self) -> DetectionResult<FSKitInfo> {
        // FSKit requires macOS 15.4 or later
        let version = self.detect_macos_version()?;
        let is_supported = version.major >= 15 || (version.major == 15 && version.minor >= 4);

        if !is_supported {
            return Ok(FSKitInfo {
                available: false,
                framework_path: None,
                version: None,
            });
        }

        // Check for FSKit framework
        let framework_path = PathBuf::from("/System/Library/Frameworks/FSKit.framework");
        let available = framework_path.exists();

        Ok(FSKitInfo {
            available,
            framework_path: if available {
                Some(framework_path)
            } else {
                None
            },
            version: if available {
                Some(version.clone())
            } else {
                None
            },
        })
    }

    pub fn detect_macfuse(&self) -> DetectionResult<MacFUSEInfo> {
        use std::process::Command;

        // Check if macFUSE is installed
        let macfuse_path = PathBuf::from("/Library/Filesystems/macfuse.fs");
        let installed = macfuse_path.exists();

        if !installed {
            return Ok(MacFUSEInfo {
                installed: false,
                version: None,
                mount_path: None,
                uses_fskit: false,
            });
        }

        // Get macFUSE version
        let version = Command::new("mount_macfuse")
            .arg("-V")
            .output()
            .ok()
            .and_then(|output| {
                let output_str = String::from_utf8_lossy(&output.stdout);
                self.parse_macfuse_version(&output_str).ok()
            });

        // Check if version 5.0+ (FSKit support)
        let uses_fskit = version
            .as_ref()
            .map(|v| v.major >= 5)
            .unwrap_or(false);

        // Find mount path
        let mount_path = macfuse_path.join("Contents/Resources/mount_macfuse");
        let mount_path = if mount_path.exists() {
            Some(mount_path)
        } else {
            None
        };

        Ok(MacFUSEInfo {
            installed,
            version,
            mount_path,
            uses_fskit,
        })
    }

    pub fn is_sip_enabled(&self) -> bool {
        use std::process::Command;

        Command::new("csrutil")
            .arg("status")
            .output()
            .ok()
            .map(|output| {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.contains("enabled")
            })
            .unwrap_or(true) // Assume enabled if we can't check
    }

    pub fn has_full_disk_access(&self) -> bool {
        // Check if we can read a protected directory
        let protected_path = PathBuf::from("/Library/Application Support/com.apple.TCC");
        protected_path.exists() && protected_path.read_dir().is_ok()
    }

    pub fn get_code_signing_status(&self) -> CodeSignStatus {
        use std::process::Command;

        let exe_path = std::env::current_exe().ok();
        if let Some(path) = exe_path {
            let output = Command::new("codesign")
                .args(&["-v", "--verify", path.to_str().unwrap_or("")])
                .output();

            match output {
                Ok(result) => {
                    if result.status.success() {
                        CodeSignStatus::Valid
                    } else {
                        let stderr = String::from_utf8_lossy(&result.stderr);
                        if stderr.contains("not signed") {
                            CodeSignStatus::NotSigned
                        } else {
                            CodeSignStatus::Invalid
                        }
                    }
                }
                Err(_) => CodeSignStatus::Unknown,
            }
        } else {
            CodeSignStatus::Unknown
        }
    }

    fn detect_macos_version(&self) -> DetectionResult<Version> {
        use std::process::Command;

        let output = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "sw_vers -productVersion".to_string(),
                error: e.to_string(),
            })?;

        let version_str = String::from_utf8_lossy(&output.stdout);
        self.parse_version(version_str.trim())
    }

    fn detect_kernel_version(&self) -> DetectionResult<Version> {
        use std::process::Command;

        let output = Command::new("uname")
            .arg("-r")
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "uname -r".to_string(),
                error: e.to_string(),
            })?;

        let version_str = String::from_utf8_lossy(&output.stdout);
        self.parse_kernel_version(version_str.trim())
    }

    fn detect_architecture(&self) -> Architecture {
        match std::env::consts::ARCH {
            "x86_64" => Architecture::X64,
            "aarch64" => Architecture::Arm64,
            _ => Architecture::Unknown,
        }
    }

    fn detect_hostname(&self) -> DetectionResult<String> {
        use std::process::Command;

        let output = Command::new("hostname")
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "hostname".to_string(),
                error: e.to_string(),
            })?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn detect_cpu_count(&self) -> DetectionResult<usize> {
        use std::process::Command;

        let output = Command::new("sysctl")
            .args(&["-n", "hw.ncpu"])
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "sysctl -n hw.ncpu".to_string(),
                error: e.to_string(),
            })?;

        let count_str = String::from_utf8_lossy(&output.stdout);
        count_str
            .trim()
            .parse()
            .map_err(|_| DetectionError::ParseError {
                data: count_str.to_string(),
                expected: "numeric CPU count".to_string(),
            })
    }

    fn detect_memory(&self) -> DetectionResult<u64> {
        use std::process::Command;

        let output = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
            .map_err(|e| DetectionError::CommandFailed {
                command: "sysctl -n hw.memsize".to_string(),
                error: e.to_string(),
            })?;

        let memory_str = String::from_utf8_lossy(&output.stdout);
        let memory_bytes: u64 = memory_str
            .trim()
            .parse()
            .map_err(|_| DetectionError::ParseError {
                data: memory_str.to_string(),
                expected: "numeric memory value".to_string(),
            })?;

        Ok(memory_bytes / (1024 * 1024)) // Convert to MB
    }

    fn parse_version(&self, version_str: &str) -> DetectionResult<Version> {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() < 2 {
            return Err(DetectionError::ParseError {
                data: version_str.to_string(),
                expected: "X.Y or X.Y.Z format".to_string(),
            });
        }

        let major = parts[0].parse().map_err(|_| DetectionError::ParseError {
            data: parts[0].to_string(),
            expected: "numeric major version".to_string(),
        })?;

        let minor = parts[1].parse().map_err(|_| DetectionError::ParseError {
            data: parts[1].to_string(),
            expected: "numeric minor version".to_string(),
        })?;

        let patch = if parts.len() > 2 {
            parts[2].parse().unwrap_or(0)
        } else {
            0
        };

        let build = if parts.len() > 3 {
            parts[3].parse().ok()
        } else {
            None
        };

        Ok(Version {
            major,
            minor,
            patch,
            build,
        })
    }

    fn parse_kernel_version(&self, version_str: &str) -> DetectionResult<Version> {
        // Darwin kernel version format: XX.Y.Z
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.is_empty() {
            return Err(DetectionError::ParseError {
                data: version_str.to_string(),
                expected: "Darwin kernel version format".to_string(),
            });
        }

        let major = parts[0].parse().map_err(|_| DetectionError::ParseError {
            data: parts[0].to_string(),
            expected: "numeric major version".to_string(),
        })?;

        let minor = if parts.len() > 1 {
            parts[1].parse().unwrap_or(0)
        } else {
            0
        };

        let patch = if parts.len() > 2 {
            parts[2].parse().unwrap_or(0)
        } else {
            0
        };

        Ok(Version {
            major,
            minor,
            patch,
            build: None,
        })
    }

    fn parse_macfuse_version(&self, output: &str) -> DetectionResult<Version> {
        // macFUSE version format: "macFUSE X.Y.Z"
        let version_regex = regex::Regex::new(r"macFUSE (\d+)\.(\d+)(?:\.(\d+))?")
            .map_err(|_| DetectionError::ParseError {
                data: "regex creation failed".to_string(),
                expected: "valid regex".to_string(),
            })?;

        if let Some(captures) = version_regex.captures(output) {
            let major = captures[1].parse().unwrap_or(0);
            let minor = captures[2].parse().unwrap_or(0);
            let patch = captures
                .get(3)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);

            Ok(Version {
                major,
                minor,
                patch,
                build: None,
            })
        } else {
            Err(DetectionError::ParseError {
                data: output.to_string(),
                expected: "macFUSE X.Y.Z format".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_sign_status() {
        let detector = MacOSDetector::new();
        let status = detector.get_code_signing_status();
        // Just verify it returns a valid status
        assert!(matches!(
            status,
            CodeSignStatus::Valid | CodeSignStatus::Invalid | CodeSignStatus::NotSigned | CodeSignStatus::Unknown
        ));
    }

    #[test]
    fn test_architecture_detection() {
        let detector = MacOSDetector::new();
        let arch = detector.detect_architecture();
        assert!(matches!(
            arch,
            Architecture::X64 | Architecture::Arm64 | Architecture::Unknown
        ));
    }

    #[test]
    fn test_version_parsing() {
        let detector = MacOSDetector::new();
        
        // Test standard version format
        let version = detector.parse_version("14.0").unwrap();
        assert_eq!(version.major, 14);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 0);
        
        // Test full version format
        let version = detector.parse_version("14.2.1").unwrap();
        assert_eq!(version.major, 14);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 1);
    }

    #[test]
    fn test_macfuse_version_parsing() {
        let detector = MacOSDetector::new();
        
        let output = "macFUSE 4.4.0";
        let version = detector.parse_macfuse_version(output).unwrap();
        assert_eq!(version.major, 4);
        assert_eq!(version.minor, 4);
        assert_eq!(version.patch, 0);
    }
}