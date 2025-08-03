//! Runtime feature detection and monitoring for ShadowFS
//! 
//! This module provides dynamic detection of platform features and
//! monitors for changes in system capabilities during runtime.

pub mod types;
pub mod detector;
pub mod monitor;
pub mod fallback;

// Platform-specific detector modules
#[cfg(target_os = "linux")]
pub mod detector_linux;
#[cfg(target_os = "macos")]
pub mod detector_macos;
#[cfg(target_os = "windows")]
pub mod detector_windows;
pub mod detector_common;

// Re-export commonly used types
pub use types::{FeatureType, FeatureStatus, PerformanceMetrics, FeatureChange};
pub use detector::RuntimeDetector;
pub use monitor::FeatureMonitor;
pub use fallback::FallbackMechanism;