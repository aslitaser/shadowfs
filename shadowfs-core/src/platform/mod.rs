mod detector;
mod windows_detector;
mod macos_detector;
mod linux_detector;
pub mod install;
mod capability_test;
mod compatibility;
pub mod runtime;

pub use detector::*;
pub use install::*;
pub use capability_test::*;
pub use compatibility::*;
// Re-export specific items to avoid name conflicts
pub use runtime::{
    FeatureType, FeatureStatus, PerformanceMetrics, FeatureChange,
    RuntimeDetector, FeatureMonitor, FallbackMechanism
};