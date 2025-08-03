//! Common types and traits for installation helpers

use std::time::Duration;
use std::fmt;

/// A prerequisite that must be satisfied before installation
#[derive(Debug, Clone)]
pub struct Prerequisite {
    /// Name of the prerequisite
    pub name: String,
    /// Description of what this prerequisite is
    pub description: String,
    /// Whether this prerequisite is currently satisfied
    pub satisfied: bool,
    /// How to satisfy this prerequisite if not met
    pub resolution: String,
    /// Whether this is a hard requirement (blocks installation)
    pub required: bool,
}

impl Prerequisite {
    pub fn new(name: impl Into<String>, description: impl Into<String>, satisfied: bool) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            satisfied,
            resolution: String::new(),
            required: true,
        }
    }
    
    pub fn with_resolution(mut self, resolution: impl Into<String>) -> Self {
        self.resolution = resolution.into();
        self
    }
    
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// Installation progress indicator
pub struct InstallProgress {
    current_step: usize,
    total_steps: usize,
    current_task: String,
}

impl InstallProgress {
    pub fn new(total_steps: usize) -> Self {
        Self {
            current_step: 0,
            total_steps,
            current_task: String::new(),
        }
    }
    
    pub fn advance(&mut self, task: impl Into<String>) {
        self.current_step += 1;
        self.current_task = task.into();
    }
    
    pub fn percentage(&self) -> f32 {
        (self.current_step as f32 / self.total_steps as f32) * 100.0
    }
}

impl fmt::Display for InstallProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, 
            "[{}/{}] {:.0}% - {}", 
            self.current_step, 
            self.total_steps, 
            self.percentage(),
            self.current_task
        )
    }
}

/// Core trait for platform-specific installation helpers
pub trait InstallHelper: Send + Sync {
    /// Generate platform-specific installation script
    fn generate_install_script(&self) -> String;
    
    /// Check prerequisites for installation
    fn check_prerequisites(&self) -> Vec<Prerequisite>;
    
    /// Estimate time required for installation
    fn estimate_install_time(&self) -> Duration;
    
    /// Whether system restart is required after installation
    fn requires_restart(&self) -> bool;
    
    /// Get supported filesystem drivers for the platform
    fn supported_drivers(&self) -> Vec<&'static str>;
    
    /// Get installation instructions as markdown
    fn get_install_instructions(&self) -> String;
    
    /// Verify installation was successful
    fn verify_installation(&self) -> Result<(), String>;
    
    /// Get uninstall instructions
    fn get_uninstall_instructions(&self) -> String;
    
    /// Execute the installation with progress callback
    fn execute_with_progress(&self, _progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
        // Default implementation - platforms can override
        Err("Direct installation not supported. Please use the generated script.".to_string())
    }
    
    /// Check if running with sufficient privileges
    fn has_required_privileges(&self) -> bool {
        #[cfg(unix)]
        {
            unsafe { libc::geteuid() == 0 }
        }
        #[cfg(windows)]
        {
            // Check for admin on Windows
            false // Simplified - actual implementation would check properly
        }
    }
}

/// Installation method
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InstallMethod {
    /// Use system package manager
    PackageManager,
    /// Download and run installer
    Installer,
    /// Build from source
    Source,
    /// Manual installation
    Manual,
}

/// Installation result
#[derive(Debug)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
    pub restart_required: bool,
    pub additional_steps: Vec<String>,
}

impl InstallResult {
    pub fn success() -> Self {
        Self {
            success: true,
            message: "Installation completed successfully".to_string(),
            restart_required: false,
            additional_steps: Vec::new(),
        }
    }
    
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            restart_required: false,
            additional_steps: Vec::new(),
        }
    }
    
    pub fn with_restart(mut self) -> Self {
        self.restart_required = true;
        self
    }
    
    pub fn with_step(mut self, step: impl Into<String>) -> Self {
        self.additional_steps.push(step.into());
        self
    }
}