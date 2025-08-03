use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlatform {
    Windows,
    MacOS,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X64,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub build: Option<u32>,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => match self.patch.cmp(&other.patch) {
                    Ordering::Equal => self.build.cmp(&other.build),
                    ord => ord,
                },
                ord => ord,
            },
            ord => ord,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub platform: RuntimePlatform,
    pub version: Version,
    pub kernel_version: Option<Version>,
    pub architecture: Architecture,
    pub hostname: String,
    pub cpu_count: usize,
    pub total_memory_mb: u64,
}

pub type DetectionResult<T> = Result<T, DetectionError>;

#[derive(Debug, Clone)]
pub enum DetectionError {
    UnsupportedPlatform(String),
    CommandFailed { command: String, error: String },
    ParseError { data: String, expected: String },
    PermissionDenied { feature: String },
}

pub trait PlatformDetector {
    fn detect_platform(&self) -> DetectionResult<SystemInfo>;
}

#[derive(Debug, Clone)]
pub enum FileSystemSupport {
    ProjFS(crate::platform::windows_detector::ProjFSInfo),
    FSKit(crate::platform::macos_detector::FSKitInfo),
    MacFUSE(crate::platform::macos_detector::MacFUSEInfo),
    FUSE(crate::platform::linux_detector::FUSEInfo),
    Multiple(Vec<FileSystemSupport>),
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub name: String,
    pub description: String,
    pub severity: RequirementSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementSeverity {
    Critical,
    Important,
    Optional,
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub title: String,
    pub description: String,
    pub action: String,
}

#[derive(Debug, Clone)]
pub struct Warning {
    pub message: String,
    pub impact: String,
}

#[derive(Debug, Clone)]
pub struct ComprehensiveReport {
    pub system_info: SystemInfo,
    pub filesystem_support: FileSystemSupport,
    pub missing_requirements: Vec<Requirement>,
    pub recommendations: Vec<Recommendation>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone)]
pub struct MissingRequirement {
    pub requirement: String,
    pub reason: String,
    pub solution: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PerformanceEstimate {
    Excellent,
    Good,
    Fair,
    Poor,
}

pub struct Detector {
    platform: RuntimePlatform,
}

impl Detector {
    pub fn new() -> Self {
        let platform = if cfg!(target_os = "windows") {
            RuntimePlatform::Windows
        } else if cfg!(target_os = "macos") {
            RuntimePlatform::MacOS
        } else if cfg!(target_os = "linux") {
            RuntimePlatform::Linux
        } else {
            RuntimePlatform::Unknown
        };

        Self { platform }
    }

    pub fn detect_all(&self) -> DetectionResult<ComprehensiveReport> {
        match self.platform {
            RuntimePlatform::Windows => self.detect_windows(),
            RuntimePlatform::MacOS => self.detect_macos(),
            RuntimePlatform::Linux => self.detect_linux(),
            RuntimePlatform::Unknown => Err(DetectionError::UnsupportedPlatform(
                "Unknown platform".to_string(),
            )),
        }
    }

    pub fn check_requirements(&self) -> Result<(), Vec<MissingRequirement>> {
        let report = self.detect_all().map_err(|_| vec![MissingRequirement {
            requirement: "Platform detection".to_string(),
            reason: "Failed to detect platform capabilities".to_string(),
            solution: None,
        }])?;

        if report.missing_requirements.is_empty() {
            Ok(())
        } else {
            let missing: Vec<MissingRequirement> = report
                .missing_requirements
                .into_iter()
                .filter(|req| req.severity == RequirementSeverity::Critical)
                .map(|req| MissingRequirement {
                    requirement: req.name,
                    reason: req.description,
                    solution: None,
                })
                .collect();

            if missing.is_empty() {
                Ok(())
            } else {
                Err(missing)
            }
        }
    }

    pub fn can_mount_without_admin(&self) -> bool {
        match self.platform {
            RuntimePlatform::Windows => {
                use crate::platform::windows_detector::WindowsDetector;
                let detector = WindowsDetector::new();
                !detector.is_admin()
            }
            RuntimePlatform::MacOS => true, // macFUSE typically allows user mounts
            RuntimePlatform::Linux => {
                // Check if user_allow_other is set in /etc/fuse.conf
                std::fs::read_to_string("/etc/fuse.conf")
                    .map(|content| content.contains("user_allow_other"))
                    .unwrap_or(false)
            }
            RuntimePlatform::Unknown => false,
        }
    }

    pub fn estimate_performance(&self) -> PerformanceEstimate {
        match self.platform {
            RuntimePlatform::Windows => {
                // ProjFS provides excellent performance on Windows
                PerformanceEstimate::Excellent
            }
            RuntimePlatform::MacOS => {
                // FSKit would be excellent, macFUSE is good
                PerformanceEstimate::Good
            }
            RuntimePlatform::Linux => {
                // FUSE3 is good, FUSE2 is fair
                PerformanceEstimate::Good
            }
            RuntimePlatform::Unknown => PerformanceEstimate::Poor,
        }
    }

    fn detect_windows(&self) -> DetectionResult<ComprehensiveReport> {
        use crate::platform::windows_detector::WindowsDetector;

        let detector = WindowsDetector::new();
        let system_info = detector.detect_system_info()?;
        let projfs_info = detector.detect_projfs()?;

        let mut missing_requirements = Vec::new();
        let mut recommendations = Vec::new();
        let mut warnings = Vec::new();

        if !projfs_info.available {
            missing_requirements.push(Requirement {
                name: "ProjFS".to_string(),
                description: "Windows Projected File System is not available".to_string(),
                severity: RequirementSeverity::Critical,
            });

            recommendations.push(Recommendation {
                title: "Enable ProjFS".to_string(),
                description: "ProjFS is required for optimal performance on Windows".to_string(),
                action: detector.enable_projfs()?,
            });
        }

        if !detector.is_admin() {
            warnings.push(Warning {
                message: "Not running as administrator".to_string(),
                impact: "Some features may be limited".to_string(),
            });
        }

        if !detector.supports_long_paths() {
            recommendations.push(Recommendation {
                title: "Enable long path support".to_string(),
                description: "Enable Windows long path support for better compatibility".to_string(),
                action: "Set LongPathsEnabled registry key to 1".to_string(),
            });
        }

        Ok(ComprehensiveReport {
            system_info,
            filesystem_support: FileSystemSupport::ProjFS(projfs_info),
            missing_requirements,
            recommendations,
            warnings,
        })
    }

    fn detect_macos(&self) -> DetectionResult<ComprehensiveReport> {
        use crate::platform::macos_detector::{MacOSDetector, CodeSignStatus};

        let detector = MacOSDetector::new();
        let system_info = detector.detect_system_info()?;
        let fskit_info = detector.detect_fskit()?;
        let macfuse_info = detector.detect_macfuse()?;

        let mut missing_requirements = Vec::new();
        let mut recommendations = Vec::new();
        let mut warnings = Vec::new();

        // Determine best filesystem support
        let filesystem_support = if fskit_info.available {
            FileSystemSupport::FSKit(fskit_info)
        } else if macfuse_info.installed {
            FileSystemSupport::MacFUSE(macfuse_info)
        } else {
            missing_requirements.push(Requirement {
                name: "Filesystem backend".to_string(),
                description: "No filesystem backend available (FSKit or macFUSE)".to_string(),
                severity: RequirementSeverity::Critical,
            });

            recommendations.push(Recommendation {
                title: "Install macFUSE".to_string(),
                description: "macFUSE is required for filesystem operations".to_string(),
                action: "Download and install macFUSE from https://osxfuse.github.io/".to_string(),
            });

            FileSystemSupport::Multiple(vec![])
        };

        if detector.is_sip_enabled() {
            warnings.push(Warning {
                message: "System Integrity Protection is enabled".to_string(),
                impact: "Some operations may require additional permissions".to_string(),
            });
        }

        if !detector.has_full_disk_access() {
            recommendations.push(Recommendation {
                title: "Grant Full Disk Access".to_string(),
                description: "Full Disk Access may be required for some operations".to_string(),
                action: "Add this application in System Preferences > Security & Privacy > Full Disk Access".to_string(),
            });
        }

        if detector.get_code_signing_status() != CodeSignStatus::Valid {
            warnings.push(Warning {
                message: "Application is not properly code signed".to_string(),
                impact: "May trigger security warnings".to_string(),
            });
        }

        Ok(ComprehensiveReport {
            system_info,
            filesystem_support,
            missing_requirements,
            recommendations,
            warnings,
        })
    }

    fn detect_linux(&self) -> DetectionResult<ComprehensiveReport> {
        use crate::platform::linux_detector::{LinuxDetector, SELinuxStatus};

        let detector = LinuxDetector::new();
        let system_info = detector.detect_system_info()?;
        let fuse_info = detector.detect_fuse()?;

        let mut missing_requirements = Vec::new();
        let mut recommendations = Vec::new();
        let mut warnings = Vec::new();

        if !fuse_info.installed {
            missing_requirements.push(Requirement {
                name: "FUSE".to_string(),
                description: "FUSE is not installed or /dev/fuse is not accessible".to_string(),
                severity: RequirementSeverity::Critical,
            });

            recommendations.push(Recommendation {
                title: "Install FUSE".to_string(),
                description: "FUSE is required for filesystem operations".to_string(),
                action: "Install fuse3 package using your distribution's package manager".to_string(),
            });
        }

        if !fuse_info.user_in_fuse_group {
            recommendations.push(Recommendation {
                title: "Add user to fuse group".to_string(),
                description: "Adding your user to the fuse group may improve permissions".to_string(),
                action: format!("sudo usermod -a -G fuse {}", std::env::var("USER").unwrap_or_else(|_| "$USER".to_string())),
            });
        }

        match detector.get_selinux_status() {
            SELinuxStatus::Enforcing => {
                warnings.push(Warning {
                    message: "SELinux is in enforcing mode".to_string(),
                    impact: "May prevent some filesystem operations".to_string(),
                });
            }
            SELinuxStatus::Permissive => {
                warnings.push(Warning {
                    message: "SELinux is in permissive mode".to_string(),
                    impact: "SELinux violations will be logged but not enforced".to_string(),
                });
            }
            _ => {}
        }

        Ok(ComprehensiveReport {
            system_info,
            filesystem_support: FileSystemSupport::FUSE(fuse_info),
            missing_requirements,
            recommendations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        let v1 = Version {
            major: 1,
            minor: 2,
            patch: 3,
            build: None,
        };
        let v2 = Version {
            major: 1,
            minor: 2,
            patch: 4,
            build: None,
        };
        assert!(v1 < v2);
        assert!(v2 > v1);
        assert_eq!(v1, v1.clone());
    }

    #[test]
    fn test_version_display() {
        let version = Version {
            major: 10,
            minor: 0,
            patch: 19041,
            build: Some(1234),
        };
        assert_eq!(version.to_string(), "10.0.19041");
    }

    #[test]
    fn test_detector_platform_detection() {
        let detector = Detector::new();
        let expected_platform = if cfg!(target_os = "windows") {
            RuntimePlatform::Windows
        } else if cfg!(target_os = "macos") {
            RuntimePlatform::MacOS
        } else if cfg!(target_os = "linux") {
            RuntimePlatform::Linux
        } else {
            RuntimePlatform::Unknown
        };
        assert_eq!(detector.platform, expected_platform);
    }

    #[test]
    fn test_performance_estimate() {
        let detector = Detector::new();
        let estimate = detector.estimate_performance();
        match detector.platform {
            RuntimePlatform::Windows => assert!(matches!(estimate, PerformanceEstimate::Excellent)),
            RuntimePlatform::MacOS | RuntimePlatform::Linux => assert!(matches!(estimate, PerformanceEstimate::Good)),
            RuntimePlatform::Unknown => assert!(matches!(estimate, PerformanceEstimate::Poor)),
        }
    }
}