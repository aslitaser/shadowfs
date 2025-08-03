//! Interactive installer for ShadowFS

use std::io::{self, Write};
use crate::types::mount::Platform;
use crate::traits::PlatformExt;
use crate::platform::install::types::{InstallHelper, InstallProgress};
use crate::platform::install::windows::WindowsInstallHelper;
use crate::platform::install::macos::MacOSInstallHelper;
use crate::platform::install::linux::LinuxInstallHelper;

/// Platform-specific install helper enum
pub enum PlatformInstallHelper {
    Windows(WindowsInstallHelper),
    MacOS(MacOSInstallHelper),
    Linux(LinuxInstallHelper),
}

impl InstallHelper for PlatformInstallHelper {
    fn generate_install_script(&self) -> String {
        match self {
            Self::Windows(h) => h.generate_install_script(),
            Self::MacOS(h) => h.generate_install_script(),
            Self::Linux(h) => h.generate_install_script(),
        }
    }
    
    fn check_prerequisites(&self) -> Vec<crate::platform::install::types::Prerequisite> {
        match self {
            Self::Windows(h) => h.check_prerequisites(),
            Self::MacOS(h) => h.check_prerequisites(),
            Self::Linux(h) => h.check_prerequisites(),
        }
    }
    
    fn estimate_install_time(&self) -> std::time::Duration {
        match self {
            Self::Windows(h) => h.estimate_install_time(),
            Self::MacOS(h) => h.estimate_install_time(),
            Self::Linux(h) => h.estimate_install_time(),
        }
    }
    
    fn requires_restart(&self) -> bool {
        match self {
            Self::Windows(h) => h.requires_restart(),
            Self::MacOS(h) => h.requires_restart(),
            Self::Linux(h) => h.requires_restart(),
        }
    }
    
    fn supported_drivers(&self) -> Vec<&'static str> {
        match self {
            Self::Windows(h) => h.supported_drivers(),
            Self::MacOS(h) => h.supported_drivers(),
            Self::Linux(h) => h.supported_drivers(),
        }
    }
    
    fn get_install_instructions(&self) -> String {
        match self {
            Self::Windows(h) => h.get_install_instructions(),
            Self::MacOS(h) => h.get_install_instructions(),
            Self::Linux(h) => h.get_install_instructions(),
        }
    }
    
    fn verify_installation(&self) -> Result<(), String> {
        match self {
            Self::Windows(h) => h.verify_installation(),
            Self::MacOS(h) => h.verify_installation(),
            Self::Linux(h) => h.verify_installation(),
        }
    }
    
    fn get_uninstall_instructions(&self) -> String {
        match self {
            Self::Windows(h) => h.get_uninstall_instructions(),
            Self::MacOS(h) => h.get_uninstall_instructions(),
            Self::Linux(h) => h.get_uninstall_instructions(),
        }
    }
    
    fn execute_with_progress(&self, progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
        match self {
            Self::Windows(h) => h.execute_with_progress(progress_callback),
            Self::MacOS(h) => h.execute_with_progress(progress_callback),
            Self::Linux(h) => h.execute_with_progress(progress_callback),
        }
    }
}

impl PlatformInstallHelper {
    /// Get the current platform
    fn platform(&self) -> Platform {
        match self {
            Self::Windows(_) => Platform::Windows,
            Self::MacOS(_) => Platform::MacOS,
            Self::Linux(_) => Platform::Linux,
        }
    }
}

/// Interactive installer that uses the appropriate platform helper
pub struct InteractiveInstaller {
    helper: PlatformInstallHelper,
}

impl InteractiveInstaller {
    pub fn new() -> Self {
        let platform = Platform::current();
        let helper = match platform {
            Platform::Windows => PlatformInstallHelper::Windows(WindowsInstallHelper::new()),
            Platform::MacOS => PlatformInstallHelper::MacOS(MacOSInstallHelper::new()),
            Platform::Linux => PlatformInstallHelper::Linux(LinuxInstallHelper::new()),
        };
        
        Self { helper }
    }
    
    /// Run the interactive installer
    pub fn run(&self) -> Result<(), String> {
        println!("🚀 ShadowFS Interactive Installer");
        println!("=================================\n");
        
        // Print requirements
        self.pretty_print_requirements();
        
        // Check if we should proceed
        let prereqs = self.helper.check_prerequisites();
        let has_required_issues = prereqs.iter()
            .any(|p| p.required && !p.satisfied);
        
        if has_required_issues {
            return Err("Required prerequisites not met. Please resolve issues and try again.".to_string());
        }
        
        // Prompt for confirmation
        if !self.prompt_for_confirmation() {
            println!("Installation cancelled by user.");
            return Ok(());
        }
        
        println!("\n📦 Starting installation...\n");
        
        // Execute with progress
        match self.helper.execute_with_progress(&|progress| {
            println!("{}", progress);
        }) {
            Ok(()) => {
                println!("\n✅ Installation completed successfully!");
                if self.helper.requires_restart() {
                    println!("🔄 Please restart your system to complete the installation.");
                }
                Ok(())
            }
            Err(e) => {
                self.handle_error_gracefully(&e);
                Err(e)
            }
        }
    }
    
    /// Generate installation script without running it
    pub fn generate_script_only(&self) -> String {
        self.helper.generate_install_script()
    }
    
    /// Pretty print requirements and prerequisites
    fn pretty_print_requirements(&self) {
        println!("📋 Platform: {}", self.helper.platform().name());
        println!("⏱️  Estimated time: {} minutes", 
            self.helper.estimate_install_time().as_secs() / 60);
        println!("🔧 Supported drivers: {}", 
            self.helper.supported_drivers().join(", "));
        
        println!("\n📋 Prerequisites:");
        println!("==================");
        
        let prereqs = self.helper.check_prerequisites();
        for prereq in &prereqs {
            let status = if prereq.satisfied { "✅" } else { "❌" };
            let required = if prereq.required { "[Required]" } else { "[Optional]" };
            
            println!("{} {} {} - {}", status, required, prereq.name, prereq.description);
            
            if !prereq.satisfied && !prereq.resolution.is_empty() {
                println!("   └─ 💡 {}", prereq.resolution);
            }
        }
        println!();
    }
    
    /// Prompt user for confirmation
    fn prompt_for_confirmation(&self) -> bool {
        print!("Do you want to proceed with installation? [y/N]: ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    }
    
    /// Handle installation errors gracefully
    fn handle_error_gracefully(&self, error: &str) {
        println!("\n❌ Installation failed: {}", error);
        println!("\n💡 Troubleshooting tips:");
        
        match self.helper.platform() {
            Platform::Windows => {
                println!("  - Ensure you're running as Administrator");
                println!("  - Check Windows version (needs 1809 or later)");
                println!("  - Try running: Get-WindowsOptionalFeature -Online -FeatureName Client-ProjFS");
            }
            Platform::MacOS => {
                println!("  - For macFUSE: Check Security & Privacy settings");
                println!("  - For FSKit: Ensure macOS 15.0 or later");
                println!("  - Try manual installation from https://osxfuse.github.io");
            }
            Platform::Linux => {
                println!("  - Check if FUSE kernel module is loaded: lsmod | grep fuse");
                println!("  - Ensure you have sudo privileges");
                println!("  - Try manual package installation");
            }
        }
        
        println!("\nFor more help, see the installation guide in the documentation.");
    }
}

impl Default for InteractiveInstaller {
    fn default() -> Self {
        Self::new()
    }
}