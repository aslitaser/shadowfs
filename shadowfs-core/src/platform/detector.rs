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