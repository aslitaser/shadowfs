//! Platform capability testing framework
//! 
//! This module provides a comprehensive testing framework to verify
//! platform-specific capabilities and identify potential issues before
//! attempting to use ShadowFS.

use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs;
use std::io::{self, Write};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use crate::types::mount::Platform;
use crate::traits::PlatformExt;

/// Result of a capability test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestResult {
    /// Test passed successfully
    Passed { 
        /// Additional details about the test
        details: String 
    },
    /// Test failed
    Failed { 
        /// Reason for failure
        reason: String, 
        /// Whether the issue can be fixed programmatically
        fixable: bool 
    },
    /// Test was skipped
    Skipped { 
        /// Reason for skipping
        reason: String 
    },
    /// Test passed with warnings
    Warning { 
        /// Warning message
        message: String 
    },
}

impl TestResult {
    /// Check if the test passed (including warnings)
    pub fn is_success(&self) -> bool {
        matches!(self, TestResult::Passed { .. } | TestResult::Warning { .. })
    }
    
    /// Check if the test failed
    pub fn is_failure(&self) -> bool {
        matches!(self, TestResult::Failed { .. })
    }
    
    /// Get a short status string
    pub fn status_emoji(&self) -> &'static str {
        match self {
            TestResult::Passed { .. } => "✅",
            TestResult::Failed { .. } => "❌",
            TestResult::Skipped { .. } => "⏭️",
            TestResult::Warning { .. } => "⚠️",
        }
    }
}

/// Core trait for capability tests
pub trait CapabilityTest: Send + Sync {
    /// Name of the test
    fn name(&self) -> &'static str;
    
    /// Description of what this test checks
    fn description(&self) -> &'static str;
    
    /// Run the test and return the result
    fn run(&self) -> TestResult;
    
    /// Whether this test is critical for basic functionality
    fn is_critical(&self) -> bool;
    
    /// Get remediation suggestions if the test fails
    fn remediation(&self) -> Option<Remediation> {
        None
    }
    
    /// Platform this test applies to (None means all platforms)
    fn platform(&self) -> Option<Platform> {
        None
    }
}

/// Remediation information for failed tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remediation {
    /// Instructions to fix the issue
    pub instructions: Vec<String>,
    /// Links to relevant documentation
    pub documentation_links: Vec<String>,
    /// Estimated difficulty (1-5, 1 being easiest)
    pub difficulty: u8,
    /// Whether admin/root privileges are required
    pub requires_admin: bool,
    /// Automated fix command if available
    pub fix_command: Option<String>,
}

/// Test for mount operations without admin privileges
pub struct MountWithoutAdminTest;

impl CapabilityTest for MountWithoutAdminTest {
    fn name(&self) -> &'static str {
        "Mount Without Admin"
    }
    
    fn description(&self) -> &'static str {
        "Check if filesystem can be mounted without administrator privileges"
    }
    
    fn run(&self) -> TestResult {
        let platform = Platform::current();
        
        match platform {
            Platform::Windows => {
                // ProjFS doesn't require admin on Windows
                TestResult::Passed {
                    details: "Windows ProjFS supports user-mode operation".to_string()
                }
            }
            Platform::MacOS => {
                // Check if we're running as root
                if unsafe { libc::geteuid() } == 0 {
                    TestResult::Warning {
                        message: "Running as root. FSKit operations may work but macFUSE typically allows user-mode".to_string()
                    }
                } else {
                    TestResult::Warning {
                        message: "FSKit requires admin privileges, but macFUSE can run in user mode".to_string()
                    }
                }
            }
            Platform::Linux => {
                // Check if user_allow_other is in /etc/fuse.conf
                match fs::read_to_string("/etc/fuse.conf") {
                    Ok(content) => {
                        if content.contains("user_allow_other") {
                            TestResult::Passed {
                                details: "FUSE configured for user-mode operation".to_string()
                            }
                        } else {
                            TestResult::Failed {
                                reason: "FUSE not configured for user-mode operation".to_string(),
                                fixable: true,
                            }
                        }
                    }
                    Err(_) => TestResult::Failed {
                        reason: "Cannot read /etc/fuse.conf".to_string(),
                        fixable: false,
                    }
                }
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false // Not critical, but improves usability
    }
    
    fn remediation(&self) -> Option<Remediation> {
        Some(Remediation {
            instructions: vec![
                "On Linux: Add 'user_allow_other' to /etc/fuse.conf".to_string(),
                "On macOS: Consider using macFUSE instead of FSKit for user-mode operation".to_string(),
            ],
            documentation_links: vec![
                "https://github.com/aslitaser/shadowfs/wiki/User-Mode-Operation".to_string(),
            ],
            difficulty: 2,
            requires_admin: true,
            fix_command: match Platform::current() {
                Platform::Linux => Some("echo 'user_allow_other' | sudo tee -a /etc/fuse.conf".to_string()),
                _ => None,
            },
        })
    }
}

/// Test for large file support (>4GB)
pub struct LargeFileTest {
    test_dir: PathBuf,
}

impl LargeFileTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for LargeFileTest {
    fn name(&self) -> &'static str {
        "Large File Support"
    }
    
    fn description(&self) -> &'static str {
        "Check if the filesystem supports files larger than 4GB"
    }
    
    fn run(&self) -> TestResult {
        // Create test directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&self.test_dir) {
            return TestResult::Failed {
                reason: format!("Cannot create test directory: {}", e),
                fixable: false,
            };
        }
        
        let test_file = self.test_dir.join("large_file_test.tmp");
        
        // Try to create a sparse file (doesn't actually use disk space)
        match fs::File::create(&test_file) {
            Ok(file) => {
                // Try to seek to 5GB position
                let five_gb = 5 * 1024 * 1024 * 1024u64;
                
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileExt;
                    match file.write_at(b"test", five_gb) {
                        Ok(_) => {
                            let _ = fs::remove_file(&test_file);
                            TestResult::Passed {
                                details: "Filesystem supports large files (>4GB)".to_string()
                            }
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&test_file);
                            TestResult::Failed {
                                reason: format!("Cannot write at 5GB offset: {}", e),
                                fixable: false,
                            }
                        }
                    }
                }
                
                #[cfg(windows)]
                {
                    use std::os::windows::fs::FileExt;
                    match file.seek_write(b"test", five_gb) {
                        Ok(_) => {
                            let _ = fs::remove_file(&test_file);
                            TestResult::Passed {
                                details: "Filesystem supports large files (>4GB)".to_string()
                            }
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&test_file);
                            TestResult::Failed {
                                reason: format!("Cannot write at 5GB offset: {}", e),
                                fixable: false,
                            }
                        }
                    }
                }
            }
            Err(e) => TestResult::Failed {
                reason: format!("Cannot create test file: {}", e),
                fixable: false,
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false // Not critical for basic functionality
    }
}

/// Test for long path support
pub struct LongPathTest {
    test_dir: PathBuf,
}

impl LongPathTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for LongPathTest {
    fn name(&self) -> &'static str {
        "Long Path Support"
    }
    
    fn description(&self) -> &'static str {
        "Check if the filesystem supports paths longer than traditional limits"
    }
    
    fn run(&self) -> TestResult {
        let platform = Platform::current();
        let _max_component = 255; // Max filename length on most systems
        
        // Build a long path
        let mut long_path = self.test_dir.clone();
        
        // Add directories to exceed platform limits
        let target_length = match platform {
            Platform::Windows => 300, // Exceed traditional 260 char limit
            Platform::MacOS => 1100,  // Exceed 1024 char limit
            Platform::Linux => 4200,  // Exceed 4096 char limit
        };
        
        // Create nested directories
        let component = "a".repeat(50); // Safe component length
        while long_path.as_os_str().len() < target_length {
            long_path = long_path.join(&component);
        }
        
        // Try to create the directory structure
        match fs::create_dir_all(&long_path) {
            Ok(_) => {
                // Clean up
                let _ = fs::remove_dir_all(&self.test_dir.join("a".repeat(50)));
                TestResult::Passed {
                    details: format!("Supports paths longer than {} characters", target_length)
                }
            }
            Err(e) => {
                TestResult::Warning {
                    message: format!("Cannot create paths longer than {} chars: {}", target_length, e)
                }
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false
    }
    
    fn platform(&self) -> Option<Platform> {
        Some(Platform::Windows) // Most relevant on Windows
    }
    
    fn remediation(&self) -> Option<Remediation> {
        Some(Remediation {
            instructions: vec![
                "On Windows: Enable long path support in Group Policy or Registry".to_string(),
                "Registry: Set LongPathsEnabled to 1 in HKLM\\SYSTEM\\CurrentControlSet\\Control\\FileSystem".to_string(),
            ],
            documentation_links: vec![
                "https://docs.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation".to_string(),
            ],
            difficulty: 2,
            requires_admin: true,
            fix_command: None,
        })
    }
}

/// Test for symbolic link support
pub struct SymlinkTest {
    test_dir: PathBuf,
}

impl SymlinkTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for SymlinkTest {
    fn name(&self) -> &'static str {
        "Symbolic Link Support"
    }
    
    fn description(&self) -> &'static str {
        "Check if the filesystem supports creating symbolic links"
    }
    
    fn run(&self) -> TestResult {
        let _ = fs::create_dir_all(&self.test_dir);
        
        let target = self.test_dir.join("symlink_target.txt");
        let link = self.test_dir.join("symlink.txt");
        
        // Create target file
        if let Err(e) = fs::write(&target, "test content") {
            return TestResult::Failed {
                reason: format!("Cannot create target file: {}", e),
                fixable: false,
            };
        }
        
        // Try to create symlink
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&target, &link);
        
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(&target, &link);
        
        // Clean up
        let _ = fs::remove_file(&target);
        let _ = fs::remove_file(&link);
        
        match result {
            Ok(_) => TestResult::Passed {
                details: "Symbolic links are supported".to_string()
            },
            Err(e) => {
                if cfg!(windows) && e.raw_os_error() == Some(1314) {
                    TestResult::Failed {
                        reason: "Symbolic links require elevated privileges on Windows".to_string(),
                        fixable: true,
                    }
                } else {
                    TestResult::Failed {
                        reason: format!("Cannot create symbolic link: {}", e),
                        fixable: false,
                    }
                }
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false
    }
    
    fn remediation(&self) -> Option<Remediation> {
        if cfg!(windows) {
            Some(Remediation {
                instructions: vec![
                    "Enable Developer Mode in Windows Settings".to_string(),
                    "Or grant SeCreateSymbolicLinkPrivilege to your user account".to_string(),
                ],
                documentation_links: vec![
                    "https://docs.microsoft.com/en-us/windows/security/threat-protection/security-policy-settings/create-symbolic-links".to_string(),
                ],
                difficulty: 2,
                requires_admin: true,
                fix_command: None,
            })
        } else {
            None
        }
    }
}

/// Test for case sensitivity behavior
pub struct CaseSensitivityTest {
    test_dir: PathBuf,
}

impl CaseSensitivityTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for CaseSensitivityTest {
    fn name(&self) -> &'static str {
        "Case Sensitivity Check"
    }
    
    fn description(&self) -> &'static str {
        "Determine if the filesystem is case-sensitive"
    }
    
    fn run(&self) -> TestResult {
        let _ = fs::create_dir_all(&self.test_dir);
        
        let file_lower = self.test_dir.join("case_test.txt");
        let file_upper = self.test_dir.join("CASE_TEST.txt");
        
        // Create lowercase file
        if let Err(e) = fs::write(&file_lower, "lowercase") {
            return TestResult::Failed {
                reason: format!("Cannot create test file: {}", e),
                fixable: false,
            };
        }
        
        // Try to create uppercase file
        match fs::write(&file_upper, "uppercase") {
            Ok(_) => {
                // Both files created - check if they're the same
                let lower_content = fs::read_to_string(&file_lower).unwrap_or_default();
                let _upper_content = fs::read_to_string(&file_upper).unwrap_or_default();
                
                let _ = fs::remove_file(&file_lower);
                let _ = fs::remove_file(&file_upper);
                
                if lower_content == "uppercase" {
                    TestResult::Passed {
                        details: "Filesystem is case-insensitive".to_string()
                    }
                } else {
                    TestResult::Passed {
                        details: "Filesystem is case-sensitive".to_string()
                    }
                }
            }
            Err(e) => {
                let _ = fs::remove_file(&file_lower);
                TestResult::Failed {
                    reason: format!("Cannot determine case sensitivity: {}", e),
                    fixable: false,
                }
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false
    }
}

/// Basic performance test
pub struct PerformanceTest {
    test_dir: PathBuf,
}

impl PerformanceTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for PerformanceTest {
    fn name(&self) -> &'static str {
        "Basic Performance"
    }
    
    fn description(&self) -> &'static str {
        "Measure basic read/write performance"
    }
    
    fn run(&self) -> TestResult {
        let _ = fs::create_dir_all(&self.test_dir);
        
        let test_file = self.test_dir.join("perf_test.tmp");
        let test_size = 10 * 1024 * 1024; // 10MB
        let test_data = vec![0xAA; test_size];
        
        // Write test
        let write_start = Instant::now();
        match fs::write(&test_file, &test_data) {
            Ok(_) => {
                let write_duration = write_start.elapsed();
                let write_speed = test_size as f64 / write_duration.as_secs_f64() / 1024.0 / 1024.0;
                
                // Read test
                let read_start = Instant::now();
                match fs::read(&test_file) {
                    Ok(data) => {
                        let read_duration = read_start.elapsed();
                        let read_speed = data.len() as f64 / read_duration.as_secs_f64() / 1024.0 / 1024.0;
                        
                        let _ = fs::remove_file(&test_file);
                        
                        if write_speed < 10.0 || read_speed < 10.0 {
                            TestResult::Warning {
                                message: format!(
                                    "Low performance detected. Write: {:.1} MB/s, Read: {:.1} MB/s",
                                    write_speed, read_speed
                                )
                            }
                        } else {
                            TestResult::Passed {
                                details: format!(
                                    "Write: {:.1} MB/s, Read: {:.1} MB/s",
                                    write_speed, read_speed
                                )
                            }
                        }
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&test_file);
                        TestResult::Failed {
                            reason: format!("Read test failed: {}", e),
                            fixable: false,
                        }
                    }
                }
            }
            Err(e) => TestResult::Failed {
                reason: format!("Write test failed: {}", e),
                fixable: false,
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false
    }
}

/// Test for concurrent access
pub struct ConcurrencyTest {
    test_dir: PathBuf,
}

impl ConcurrencyTest {
    pub fn new(test_dir: PathBuf) -> Self {
        Self { test_dir }
    }
}

impl CapabilityTest for ConcurrencyTest {
    fn name(&self) -> &'static str {
        "Concurrent Access"
    }
    
    fn description(&self) -> &'static str {
        "Test concurrent file operations"
    }
    
    fn run(&self) -> TestResult {
        use std::thread;
        
        let _ = fs::create_dir_all(&self.test_dir);
        
        let test_file = self.test_dir.join("concurrent_test.tmp");
        let counter = Arc::new(Mutex::new(0));
        let errors = Arc::new(Mutex::new(Vec::new()));
        
        // Create initial file
        if let Err(e) = fs::write(&test_file, "0") {
            return TestResult::Failed {
                reason: format!("Cannot create test file: {}", e),
                fixable: false,
            };
        }
        
        let num_threads = 4;
        let iterations = 100;
        let mut handles = vec![];
        
        for _ in 0..num_threads {
            let test_file = test_file.clone();
            let counter = Arc::clone(&counter);
            let errors = Arc::clone(&errors);
            
            let handle = thread::spawn(move || {
                for _ in 0..iterations {
                    // Read current value
                    match fs::read_to_string(&test_file) {
                        Ok(content) => {
                            let value: i32 = content.trim().parse().unwrap_or(0);
                            
                            // Increment and write back
                            let new_value = value + 1;
                            if let Err(e) = fs::write(&test_file, new_value.to_string()) {
                                errors.lock().unwrap().push(format!("Write error: {}", e));
                            } else {
                                *counter.lock().unwrap() += 1;
                            }
                        }
                        Err(e) => {
                            errors.lock().unwrap().push(format!("Read error: {}", e));
                        }
                    }
                    
                    // Small delay to increase contention
                    thread::sleep(Duration::from_micros(100));
                }
            });
            
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            let _ = handle.join();
        }
        
        let _ = fs::remove_file(&test_file);
        
        let final_count = *counter.lock().unwrap();
        let error_list = errors.lock().unwrap();
        
        if !error_list.is_empty() {
            TestResult::Failed {
                reason: format!("Concurrent access errors: {:?}", error_list),
                fixable: false,
            }
        } else if final_count < (num_threads * iterations) / 2 {
            TestResult::Warning {
                message: format!("High contention detected. Only {} of {} operations succeeded", 
                    final_count, num_threads * iterations)
            }
        } else {
            TestResult::Passed {
                details: format!("{} concurrent operations completed successfully", final_count)
            }
        }
    }
    
    fn is_critical(&self) -> bool {
        false
    }
}

/// Test results cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTestResults {
    pub platform: Platform,
    pub timestamp: std::time::SystemTime,
    pub results: HashMap<String, TestResult>,
}

/// Main test suite for running capability tests
pub struct TestSuite {
    tests: Vec<Box<dyn CapabilityTest>>,
    cache_path: Option<PathBuf>,
}

impl TestSuite {
    /// Create a new test suite with default tests
    pub fn new(test_dir: PathBuf) -> Self {
        let platform = Platform::current();
        let mut tests: Vec<Box<dyn CapabilityTest>> = vec![];
        
        // Add tests applicable to all platforms
        tests.push(Box::new(MountWithoutAdminTest));
        tests.push(Box::new(LargeFileTest::new(test_dir.clone())));
        tests.push(Box::new(SymlinkTest::new(test_dir.clone())));
        tests.push(Box::new(CaseSensitivityTest::new(test_dir.clone())));
        tests.push(Box::new(PerformanceTest::new(test_dir.clone())));
        tests.push(Box::new(ConcurrencyTest::new(test_dir.clone())));
        
        // Add platform-specific tests
        if platform == Platform::Windows {
            tests.push(Box::new(LongPathTest::new(test_dir)));
        }
        
        Self {
            tests,
            cache_path: None,
        }
    }
    
    /// Set cache path for storing test results
    pub fn with_cache(mut self, cache_path: PathBuf) -> Self {
        self.cache_path = Some(cache_path);
        self
    }
    
    /// Load cached results if available and recent
    pub fn load_cache(&self) -> Option<CachedTestResults> {
        let cache_path = self.cache_path.as_ref()?;
        let content = fs::read_to_string(cache_path).ok()?;
        let cached: CachedTestResults = serde_json::from_str(&content).ok()?;
        
        // Check if cache is recent (within 24 hours)
        let age = std::time::SystemTime::now()
            .duration_since(cached.timestamp)
            .unwrap_or(Duration::from_secs(u64::MAX));
        
        if age < Duration::from_secs(24 * 60 * 60) && cached.platform == Platform::current() {
            Some(cached)
        } else {
            None
        }
    }
    
    /// Save results to cache
    pub fn save_cache(&self, results: &HashMap<String, TestResult>) {
        if let Some(cache_path) = &self.cache_path {
            let cached = CachedTestResults {
                platform: Platform::current(),
                timestamp: std::time::SystemTime::now(),
                results: results.clone(),
            };
            
            if let Ok(json) = serde_json::to_string_pretty(&cached) {
                let _ = fs::write(cache_path, json);
            }
        }
    }
    
    /// Run all tests and return results
    pub fn run_all(&self, use_cache: bool) -> HashMap<String, TestResult> {
        // Check cache first
        if use_cache {
            if let Some(cached) = self.load_cache() {
                println!("Using cached test results from {:?}", cached.timestamp);
                return cached.results;
            }
        }
        
        let mut results = HashMap::new();
        let platform = Platform::current();
        
        println!("🧪 Running ShadowFS Capability Tests");
        println!("Platform: {}", platform.name());
        println!("=====================================\n");
        
        for test in &self.tests {
            // Skip tests for other platforms
            if let Some(test_platform) = test.platform() {
                if test_platform != platform {
                    continue;
                }
            }
            
            print!("Running {:<30} ", format!("{}...", test.name()));
            io::stdout().flush().unwrap();
            
            let start = Instant::now();
            let result = test.run();
            let duration = start.elapsed();
            
            println!("{} ({:.2}s)", result.status_emoji(), duration.as_secs_f64());
            
            if let TestResult::Failed { ref reason, .. } = result {
                println!("  └─ {}", reason);
                
                if let Some(remediation) = test.remediation() {
                    println!("  📋 How to fix:");
                    for instruction in &remediation.instructions {
                        println!("     • {}", instruction);
                    }
                    if let Some(cmd) = &remediation.fix_command {
                        println!("     💻 Command: {}", cmd);
                    }
                }
            }
            
            results.insert(test.name().to_string(), result);
        }
        
        // Save to cache
        self.save_cache(&results);
        
        results
    }
    
    /// Generate a comprehensive report
    pub fn generate_report(&self, results: &HashMap<String, TestResult>) -> TestReport {
        let total = results.len();
        let passed = results.values().filter(|r| matches!(r, TestResult::Passed { .. })).count();
        let warnings = results.values().filter(|r| matches!(r, TestResult::Warning { .. })).count();
        let failed = results.values().filter(|r| matches!(r, TestResult::Failed { .. })).count();
        let skipped = results.values().filter(|r| matches!(r, TestResult::Skipped { .. })).count();
        
        let critical_failures: Vec<String> = self.tests.iter()
            .filter(|test| test.is_critical())
            .filter(|test| {
                results.get(test.name())
                    .map(|r| r.is_failure())
                    .unwrap_or(false)
            })
            .map(|test| test.name().to_string())
            .collect();
        
        TestReport {
            platform: Platform::current(),
            total_tests: total,
            passed,
            warnings,
            failed,
            skipped,
            critical_failures: critical_failures.clone(),
            can_proceed: critical_failures.is_empty(),
        }
    }
}

/// Summary report of test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub platform: Platform,
    pub total_tests: usize,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
    pub critical_failures: Vec<String>,
    pub can_proceed: bool,
}

impl TestReport {
    /// Print a summary of the test results
    pub fn print_summary(&self) {
        println!("\n📊 Test Summary");
        println!("===============");
        println!("Platform: {}", self.platform.name());
        println!("Total Tests: {}", self.total_tests);
        println!("✅ Passed: {}", self.passed);
        println!("⚠️  Warnings: {}", self.warnings);
        println!("❌ Failed: {}", self.failed);
        println!("⏭️  Skipped: {}", self.skipped);
        
        if !self.critical_failures.is_empty() {
            println!("\n🚨 Critical Failures:");
            for failure in &self.critical_failures {
                println!("   • {}", failure);
            }
        }
        
        println!("\n{}", if self.can_proceed {
            "✨ System is ready for ShadowFS!"
        } else {
            "❌ Please fix critical issues before using ShadowFS."
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_result_status() {
        let passed = TestResult::Passed { details: "test".to_string() };
        assert!(passed.is_success());
        assert!(!passed.is_failure());
        assert_eq!(passed.status_emoji(), "✅");
        
        let failed = TestResult::Failed { reason: "test".to_string(), fixable: true };
        assert!(!failed.is_success());
        assert!(failed.is_failure());
        assert_eq!(failed.status_emoji(), "❌");
    }
    
    #[test]
    fn test_remediation() {
        let remediation = Remediation {
            instructions: vec!["Do this".to_string()],
            documentation_links: vec!["https://example.com".to_string()],
            difficulty: 3,
            requires_admin: true,
            fix_command: Some("fix-it".to_string()),
        };
        
        assert_eq!(remediation.difficulty, 3);
        assert!(remediation.requires_admin);
        assert_eq!(remediation.fix_command, Some("fix-it".to_string()));
    }
    
    #[test]
    fn test_mount_without_admin() {
        let test = MountWithoutAdminTest;
        assert_eq!(test.name(), "Mount Without Admin");
        assert!(!test.is_critical());
        assert!(test.remediation().is_some());
    }
    
    #[test]
    fn test_suite_creation() {
        let temp_dir = env::temp_dir().join("shadowfs_test");
        let suite = TestSuite::new(temp_dir);
        
        // Should have at least the basic tests
        assert!(suite.tests.len() >= 6);
    }
    
    #[test]
    fn test_report_generation() {
        let mut results = HashMap::new();
        results.insert("Test1".to_string(), TestResult::Passed { details: "ok".to_string() });
        results.insert("Test2".to_string(), TestResult::Failed { reason: "error".to_string(), fixable: false });
        results.insert("Test3".to_string(), TestResult::Warning { message: "warning".to_string() });
        
        let temp_dir = env::temp_dir().join("shadowfs_test");
        let suite = TestSuite::new(temp_dir);
        let report = suite.generate_report(&results);
        
        assert_eq!(report.total_tests, 3);
        assert_eq!(report.passed, 1);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 0);
    }
}