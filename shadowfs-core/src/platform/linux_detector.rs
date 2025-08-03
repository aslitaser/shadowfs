use super::{
    Architecture, DetectionError, DetectionResult, RuntimePlatform, SystemInfo, Version,
};
use std::path::PathBuf;

pub struct LinuxDetector;

#[derive(Debug, Clone)]
pub struct FUSEInfo {
    pub installed: bool,
    pub version: FUSEVersion,
    pub version_string: String,
    pub fusermount_path: PathBuf,
    pub device_path: PathBuf,
    pub user_in_fuse_group: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FUSEVersion {
    FUSE2,
    FUSE3,
}

#[derive(Debug, Clone)]
pub enum Distribution {
    Ubuntu(Version),
    Debian(Version),
    Fedora(Version),
    Arch,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SELinuxStatus {
    Enforcing,
    Permissive,
    Disabled,
    NotInstalled,
}

#[derive(Debug, Clone)]
pub struct NamespaceSupport {
    pub user_ns: bool,
    pub pid_ns: bool,
    pub net_ns: bool,
    pub mount_ns: bool,
    pub uts_ns: bool,
    pub ipc_ns: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CGroupVersion {
    V1,
    V2,
    Hybrid,
    Unknown,
}

impl LinuxDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_system_info(&self) -> DetectionResult<SystemInfo> {
        if !cfg!(target_os = "linux") {
            return Err(DetectionError::UnsupportedPlatform(
                "Not running on Linux".to_string(),
            ));
        }

        let distribution = self.detect_distribution()?;
        let version = self.get_distribution_version(&distribution)?;
        let kernel_version = self.detect_kernel_version()?;
        let architecture = self.detect_architecture();
        let hostname = self.detect_hostname()?;
        let cpu_count = num_cpus::get();
        let total_memory_mb = self.detect_memory()?;

        Ok(SystemInfo {
            platform: RuntimePlatform::Linux,
            version,
            kernel_version: Some(kernel_version),
            architecture,
            hostname,
            cpu_count,
            total_memory_mb,
        })
    }

    pub fn detect_fuse(&self) -> DetectionResult<FUSEInfo> {
        use std::process::Command;

        // Check for FUSE3 first
        let (fusermount_path, version, version_string) = if let Ok(output) = Command::new("fusermount3")
            .arg("-V")
            .output()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            (
                PathBuf::from("/usr/bin/fusermount3"),
                FUSEVersion::FUSE3,
                output_str.trim().to_string(),
            )
        } else if let Ok(output) = Command::new("fusermount")
            .arg("-V")
            .output()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            (
                PathBuf::from("/usr/bin/fusermount"),
                FUSEVersion::FUSE2,
                output_str.trim().to_string(),
            )
        } else {
            return Ok(FUSEInfo {
                installed: false,
                version: FUSEVersion::FUSE2,
                version_string: String::new(),
                fusermount_path: PathBuf::new(),
                device_path: PathBuf::from("/dev/fuse"),
                user_in_fuse_group: false,
            });
        };

        // Check /dev/fuse
        let device_path = PathBuf::from("/dev/fuse");
        let device_accessible = device_path.exists() && {
            use std::fs::OpenOptions;
            OpenOptions::new()
                .read(true)
                .open(&device_path)
                .is_ok()
        };

        // Check if user is in fuse group
        let user_in_fuse_group = self.check_user_in_group("fuse");

        // Check user_allow_other in /etc/fuse.conf
        let _user_allow_other = std::fs::read_to_string("/etc/fuse.conf")
            .map(|content| content.contains("user_allow_other"))
            .unwrap_or(false);

        Ok(FUSEInfo {
            installed: device_accessible,
            version,
            version_string,
            fusermount_path,
            device_path,
            user_in_fuse_group,
        })
    }

    pub fn get_selinux_status(&self) -> SELinuxStatus {
        use std::process::Command;

        match Command::new("getenforce").output() {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
                match status.as_str() {
                    "enforcing" => SELinuxStatus::Enforcing,
                    "permissive" => SELinuxStatus::Permissive,
                    "disabled" => SELinuxStatus::Disabled,
                    _ => SELinuxStatus::NotInstalled,
                }
            }
            Err(_) => {
                // Try reading from /sys/fs/selinux/enforce
                match std::fs::read_to_string("/sys/fs/selinux/enforce") {
                    Ok(content) => match content.trim() {
                        "1" => SELinuxStatus::Enforcing,
                        "0" => SELinuxStatus::Permissive,
                        _ => SELinuxStatus::Disabled,
                    },
                    Err(_) => SELinuxStatus::NotInstalled,
                }
            }
        }
    }

    pub fn check_namespace_support(&self) -> NamespaceSupport {
        let check_ns = |ns_type: &str| -> bool {
            let path = format!("/proc/self/ns/{}", ns_type);
            PathBuf::from(path).exists()
        };

        NamespaceSupport {
            user_ns: check_ns("user"),
            pid_ns: check_ns("pid"),
            net_ns: check_ns("net"),
            mount_ns: check_ns("mnt"),
            uts_ns: check_ns("uts"),
            ipc_ns: check_ns("ipc"),
        }
    }

    pub fn get_cgroup_version(&self) -> CGroupVersion {
        // Check for cgroup v2
        if PathBuf::from("/sys/fs/cgroup/cgroup.controllers").exists() {
            return CGroupVersion::V2;
        }

        // Check for cgroup v1
        if PathBuf::from("/sys/fs/cgroup/memory").exists() {
            // Check if it's a hybrid setup
            if PathBuf::from("/sys/fs/cgroup/unified").exists() {
                return CGroupVersion::Hybrid;
            }
            return CGroupVersion::V1;
        }

        CGroupVersion::Unknown
    }

    fn detect_distribution(&self) -> DetectionResult<Distribution> {
        let os_release = std::fs::read_to_string("/etc/os-release")
            .map_err(|e| DetectionError::CommandFailed {
                command: "read /etc/os-release".to_string(),
                error: e.to_string(),
            })?;

        let mut id = String::new();
        let mut version_id = String::new();

        for line in os_release.lines() {
            if let Some(value) = line.strip_prefix("ID=") {
                id = value.trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
                version_id = value.trim_matches('"').to_string();
            }
        }

        match id.as_str() {
            "ubuntu" => {
                let version = self.parse_version(&version_id)?;
                Ok(Distribution::Ubuntu(version))
            }
            "debian" => {
                let version = self.parse_version(&version_id)?;
                Ok(Distribution::Debian(version))
            }
            "fedora" => {
                let version = self.parse_version(&version_id)?;
                Ok(Distribution::Fedora(version))
            }
            "arch" => Ok(Distribution::Arch),
            _ => Ok(Distribution::Other(id)),
        }
    }

    fn get_distribution_version(&self, distribution: &Distribution) -> DetectionResult<Version> {
        match distribution {
            Distribution::Ubuntu(v) | Distribution::Debian(v) | Distribution::Fedora(v) => Ok(v.clone()),
            Distribution::Arch => Ok(Version {
                major: 0,
                minor: 0,
                patch: 0,
                build: None,
            }),
            Distribution::Other(_) => Ok(Version {
                major: 0,
                minor: 0,
                patch: 0,
                build: None,
            }),
        }
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

    fn detect_memory(&self) -> DetectionResult<u64> {
        let meminfo = std::fs::read_to_string("/proc/meminfo")
            .map_err(|e| DetectionError::CommandFailed {
                command: "read /proc/meminfo".to_string(),
                error: e.to_string(),
            })?;

        for line in meminfo.lines() {
            if let Some(value) = line.strip_prefix("MemTotal:") {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if !parts.is_empty() {
                    let memory_kb: u64 = parts[0].parse().map_err(|_| {
                        DetectionError::ParseError {
                            data: parts[0].to_string(),
                            expected: "numeric memory value".to_string(),
                        }
                    })?;
                    return Ok(memory_kb / 1024); // Convert KB to MB
                }
            }
        }

        Err(DetectionError::ParseError {
            data: meminfo,
            expected: "MemTotal line in /proc/meminfo".to_string(),
        })
    }

    fn check_user_in_group(&self, group_name: &str) -> bool {
        use std::process::Command;

        if let Ok(output) = Command::new("groups").output() {
            let groups_str = String::from_utf8_lossy(&output.stdout);
            groups_str.split_whitespace().any(|g| g == group_name)
        } else {
            false
        }
    }

    fn parse_version(&self, version_str: &str) -> DetectionResult<Version> {
        let parts: Vec<&str> = version_str.split('.').collect();
        
        let major = if !parts.is_empty() {
            parts[0].parse().unwrap_or(0)
        } else {
            0
        };

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

    fn parse_kernel_version(&self, version_str: &str) -> DetectionResult<Version> {
        // Linux kernel version format: X.Y.Z-suffix
        let base_version = version_str.split('-').next().unwrap_or(version_str);
        self.parse_version(base_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selinux_status() {
        let detector = LinuxDetector::new();
        let status = detector.get_selinux_status();
        assert!(matches!(
            status,
            SELinuxStatus::Enforcing | SELinuxStatus::Permissive | SELinuxStatus::Disabled | SELinuxStatus::NotInstalled
        ));
    }

    #[test]
    fn test_version_parsing() {
        let detector = LinuxDetector::new();
        
        // Test Ubuntu version format
        let version = detector.parse_version("22.04").unwrap();
        assert_eq!(version.major, 22);
        assert_eq!(version.minor, 4);
        assert_eq!(version.patch, 0);
        
        // Test Fedora version format
        let version = detector.parse_version("38").unwrap();
        assert_eq!(version.major, 38);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 0);
    }

    #[test]
    fn test_kernel_version_parsing() {
        let detector = LinuxDetector::new();
        
        // Test standard kernel version
        let version = detector.parse_kernel_version("5.15.0-generic").unwrap();
        assert_eq!(version.major, 5);
        assert_eq!(version.minor, 15);
        assert_eq!(version.patch, 0);
        
        // Test with patch version
        let version = detector.parse_kernel_version("6.2.16-300.fc38.x86_64").unwrap();
        assert_eq!(version.major, 6);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 16);
    }

    #[test]
    fn test_fuse_version() {
        let detector = LinuxDetector::new();
        let fuse_info = detector.detect_fuse().unwrap();
        // Just verify it returns without error
        assert!(matches!(fuse_info.version, FUSEVersion::FUSE2 | FUSEVersion::FUSE3));
    }
}