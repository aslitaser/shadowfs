//! Core types for runtime feature detection

use std::time::SystemTime;
use serde::{Serialize, Deserialize};

/// Types of features that can be detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureType {
    /// FUSE availability
    FuseAvailable,
    /// ProjFS availability (Windows)
    ProjFSAvailable,
    /// macFUSE availability
    MacFuseAvailable,
    /// FSKit availability (macOS)
    FSKitAvailable,
    /// Administrator privileges
    AdminPrivileges,
    /// Developer mode (Windows)
    DeveloperMode,
    /// Case sensitivity support
    CaseSensitivity,
    /// Extended attributes support
    ExtendedAttributes,
    /// Symbolic links support
    SymbolicLinks,
    /// Large file support
    LargeFiles,
    /// Long path support
    LongPaths,
}

/// Result of a feature detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStatus {
    /// Whether the feature is available
    pub available: bool,
    /// Additional details about the feature
    pub details: String,
    /// When this status was last checked
    pub last_checked: SystemTime,
    /// Version information if applicable
    pub version: Option<String>,
    /// Performance metrics if applicable
    pub performance: Option<PerformanceMetrics>,
}

/// Performance metrics for features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Average operation latency in milliseconds
    pub avg_latency_ms: f64,
    /// Peak latency in milliseconds
    pub peak_latency_ms: f64,
    /// Operations per second
    pub ops_per_second: f64,
    /// Number of samples
    pub sample_count: u64,
}

/// Change event for features
#[derive(Debug, Clone)]
pub enum FeatureChange {
    /// Feature became available
    Available {
        feature: FeatureType,
        details: String,
    },
    /// Feature became unavailable
    Unavailable {
        feature: FeatureType,
        reason: String,
    },
    /// Feature performance changed significantly
    PerformanceChange {
        feature: FeatureType,
        old_metrics: PerformanceMetrics,
        new_metrics: PerformanceMetrics,
    },
}

/// Cache entry for feature detection results
#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    pub status: FeatureStatus,
    pub expires_at: std::time::Instant,
}

/// Performance tracker for feature operations
pub(crate) struct PerformanceTracker {
    pub samples: Vec<f64>,
    pub total_latency: f64,
    pub peak_latency: f64,
    pub start_time: std::time::Instant,
}

impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            total_latency: 0.0,
            peak_latency: 0.0,
            start_time: std::time::Instant::now(),
        }
    }
    
    pub fn add_sample(&mut self, latency_ms: f64) {
        self.samples.push(latency_ms);
        self.total_latency += latency_ms;
        if latency_ms > self.peak_latency {
            self.peak_latency = latency_ms;
        }
        
        // Keep only recent samples (last 1000)
        if self.samples.len() > 1000 {
            let removed = self.samples.remove(0);
            self.total_latency -= removed;
        }
    }
    
    pub fn get_metrics(&self) -> PerformanceMetrics {
        let sample_count = self.samples.len() as u64;
        let avg_latency_ms = if sample_count > 0 {
            self.total_latency / sample_count as f64
        } else {
            0.0
        };
        
        let duration = self.start_time.elapsed().as_secs_f64();
        let ops_per_second = if duration > 0.0 {
            sample_count as f64 / duration
        } else {
            0.0
        };
        
        PerformanceMetrics {
            avg_latency_ms,
            peak_latency_ms: self.peak_latency,
            ops_per_second,
            sample_count,
        }
    }
}