//! Platform-specific installation helpers for ShadowFS
//! 
//! This module provides utilities to help users install the necessary
//! platform-specific components required for ShadowFS to function.

use std::time::Duration;
use std::fmt;
use std::io::{self, Write};
use crate::types::mount::Platform;
use crate::traits::PlatformExt;

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
    
    /// Get the platform this helper is for
    fn platform(&self) -> Platform;
    
    /// Execute installation with progress tracking
    fn execute_with_progress(&self, progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String>;
    
    /// Pretty print requirements to console
    fn pretty_print_requirements(&self) {
        let prereqs = self.check_prerequisites();
        
        println!("\n🔍 Checking prerequisites for {}...\n", self.platform().name());
        
        let mut has_issues = false;
        
        for prereq in &prereqs {
            let status = if prereq.satisfied { "✅" } else { "❌" };
            let required = if prereq.required { " (Required)" } else { " (Optional)" };
            
            println!("{} {} {}", status, prereq.name, required);
            println!("   {}", prereq.description);
            
            if !prereq.satisfied {
                has_issues = true;
                if !prereq.resolution.is_empty() {
                    println!("   📋 To resolve: {}", prereq.resolution);
                }
            }
            println!();
        }
        
        if has_issues {
            println!("⚠️  Some prerequisites are not satisfied.");
            println!("   Please resolve the required items before proceeding.\n");
        } else {
            println!("✨ All prerequisites satisfied!\n");
        }
        
        println!("📊 Estimated installation time: {:?}", self.estimate_install_time());
        if self.requires_restart() {
            println!("🔄 System restart will be required after installation.");
        }
    }
    
    /// Prompt user for confirmation
    fn prompt_for_confirmation(&self) -> bool {
        print!("\nDo you want to proceed with installation? [y/N]: ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    }
    
    /// Handle errors gracefully with helpful messages
    fn handle_error_gracefully(&self, error: &str) {
        eprintln!("\n❌ Installation failed: {}", error);
        eprintln!("\n💡 Troubleshooting tips:");
        
        match self.platform() {
            Platform::Windows => {
                eprintln!("   - Ensure you're running as Administrator");
                eprintln!("   - Check Windows Update for pending updates");
                eprintln!("   - Verify Windows version is 1809 or later");
            }
            Platform::MacOS => {
                eprintln!("   - Check if System Integrity Protection is blocking installation");
                eprintln!("   - Ensure Xcode Command Line Tools are installed");
                eprintln!("   - Try running with sudo if permission denied");
            }
            Platform::Linux => {
                eprintln!("   - Update your package manager cache");
                eprintln!("   - Check if FUSE kernel module is loaded");
                eprintln!("   - Ensure you have sufficient permissions");
            }
        }
        
        eprintln!("\nFor more help, see: https://github.com/aslitaser/shadowfs/wiki/Installation");
    }
}

/// Windows-specific installation helper
pub struct WindowsInstallHelper;

impl WindowsInstallHelper {
    pub fn new() -> Self {
        Self
    }
    
    fn check_windows_version(&self) -> Result<(u32, u32), String> {
        // In a real implementation, this would use Windows APIs
        // For now, return a mock version
        Ok((10, 1809))
    }
    
    fn is_elevated(&self) -> bool {
        // Check if running as administrator
        // In real implementation, would use Windows APIs
        false
    }
}

impl InstallHelper for WindowsInstallHelper {
    fn generate_install_script(&self) -> String {
        r#"# ShadowFS Windows Installation Script
# This script enables Windows Projected File System (ProjFS)

# Check if running as Administrator
if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Host "This script requires Administrator privileges." -ForegroundColor Red
    Write-Host "Please run PowerShell as Administrator and try again." -ForegroundColor Yellow
    exit 1
}

# Check Windows version
$os = Get-WmiObject -Class Win32_OperatingSystem
$version = [System.Version]$os.Version
$build = $os.BuildNumber

Write-Host "Detected Windows version: $version (Build $build)" -ForegroundColor Cyan

if ($build -lt 17763) {
    Write-Host "ERROR: Windows 10 version 1809 (Build 17763) or later is required." -ForegroundColor Red
    Write-Host "Current build: $build" -ForegroundColor Red
    exit 1
}

# Enable Windows Projected File System
Write-Host "`nEnabling Windows Projected File System..." -ForegroundColor Green

try {
    Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
    Write-Host "ProjFS feature enabled successfully!" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Failed to enable ProjFS feature" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}

# Verify installation
$feature = Get-WindowsOptionalFeature -Online -FeatureName Client-ProjFS
if ($feature.State -eq "Enabled") {
    Write-Host "`n✅ ProjFS is now enabled!" -ForegroundColor Green
} else {
    Write-Host "`n❌ ProjFS installation verification failed" -ForegroundColor Red
    exit 1
}

# Create scheduled task for non-admin usage (optional)
$taskName = "ShadowFS-Helper"
$action = New-ScheduledTaskAction -Execute "shadowfs-service.exe" -Argument "--background"
$trigger = New-ScheduledTaskTrigger -AtLogon
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest

Write-Host "`nCreating scheduled task for background service..." -ForegroundColor Green

try {
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Description "ShadowFS Background Service" | Out-Null
    Write-Host "Scheduled task created successfully!" -ForegroundColor Green
} catch {
    Write-Host "Warning: Could not create scheduled task. This is optional." -ForegroundColor Yellow
}

Write-Host "`n🎉 Installation completed successfully!" -ForegroundColor Green
Write-Host "You may need to restart your system for changes to take full effect." -ForegroundColor Yellow
"#.to_string()
    }
    
    fn check_prerequisites(&self) -> Vec<Prerequisite> {
        let mut prereqs = vec![];
        
        // Check Windows version
        match self.check_windows_version() {
            Ok((major, build)) => {
                let satisfied = major >= 10 && build >= 1809;
                prereqs.push(
                    Prerequisite::new(
                        "Windows Version",
                        format!("Windows 10 version 1809 or later (current: {}.{})", major, build),
                        satisfied
                    )
                    .with_resolution("Update Windows to version 1809 or later")
                );
            }
            Err(_) => {
                prereqs.push(
                    Prerequisite::new(
                        "Windows Version",
                        "Unable to detect Windows version",
                        false
                    )
                    .with_resolution("Ensure Windows 10 version 1809 or later is installed")
                );
            }
        }
        
        // Check administrator privileges
        prereqs.push(
            Prerequisite::new(
                "Administrator Privileges",
                "Installation requires administrator access",
                self.is_elevated()
            )
            .with_resolution("Run PowerShell as Administrator")
        );
        
        // Check if ProjFS is already enabled
        prereqs.push(
            Prerequisite::new(
                "Projected File System",
                "Windows ProjFS feature must be enabled",
                false // Would check actual state in real implementation
            )
            .with_resolution("Will be enabled during installation")
            .optional()
        );
        
        prereqs
    }
    
    fn estimate_install_time(&self) -> Duration {
        Duration::from_secs(120) // 2 minutes
    }
    
    fn requires_restart(&self) -> bool {
        true // ProjFS may require restart
    }
    
    fn platform(&self) -> Platform {
        Platform::Windows
    }
    
    fn execute_with_progress(&self, progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
        let mut progress = InstallProgress::new(4);
        
        // Step 1: Check prerequisites
        progress.advance("Checking prerequisites");
        progress_callback(&progress);
        
        let prereqs = self.check_prerequisites();
        for prereq in &prereqs {
            if prereq.required && !prereq.satisfied {
                return Err(format!("Prerequisite not met: {}", prereq.name));
            }
        }
        
        // Step 2: Generate script
        progress.advance("Generating installation script");
        progress_callback(&progress);
        let _script = self.generate_install_script();
        
        // Step 3: Execute installation
        progress.advance("Installing ProjFS feature");
        progress_callback(&progress);
        // In real implementation, would execute PowerShell script
        
        // Step 4: Verify installation
        progress.advance("Verifying installation");
        progress_callback(&progress);
        
        Ok(())
    }
}

/// macOS-specific installation helper
pub struct MacOSInstallHelper {
    use_macfuse: bool,
}

impl MacOSInstallHelper {
    pub fn new() -> Self {
        Self { use_macfuse: false }
    }
    
    pub fn with_macfuse(mut self) -> Self {
        self.use_macfuse = true;
        self
    }
    
    fn check_macos_version(&self) -> Result<(u32, u32), String> {
        // In real implementation, would use system calls
        Ok((15, 0)) // macOS 15.0
    }
    
    fn has_homebrew(&self) -> bool {
        // Check if Homebrew is installed
        false
    }
    
    fn has_xcode_tools(&self) -> bool {
        // Check if Xcode Command Line Tools are installed
        false
    }
}

impl InstallHelper for MacOSInstallHelper {
    fn generate_install_script(&self) -> String {
        if self.use_macfuse {
            r#"#!/bin/bash
# ShadowFS macOS Installation Script (macFUSE)

echo "🍎 ShadowFS macOS Installation (macFUSE)"
echo "======================================="

# Check if Homebrew is installed
if ! command -v brew &> /dev/null; then
    echo "❌ Homebrew is not installed."
    echo "Please install Homebrew first: https://brew.sh"
    exit 1
fi

# Update Homebrew
echo "📦 Updating Homebrew..."
brew update

# Install macFUSE
echo "📦 Installing macFUSE..."
brew install --cask macfuse

# Verify installation
if [ -d "/Library/Filesystems/macfuse.fs" ]; then
    echo "✅ macFUSE installed successfully!"
else
    echo "❌ macFUSE installation failed"
    exit 1
fi

# Install pkg-config (needed for building)
brew install pkg-config

echo ""
echo "🎉 Installation completed!"
echo "⚠️  Note: You may need to allow the kernel extension in System Preferences > Security & Privacy"
echo "🔄 A system restart is recommended."
"#.to_string()
        } else {
            r#"#!/bin/bash
# ShadowFS macOS Installation Script (FSKit)

echo "🍎 ShadowFS macOS Installation (FSKit)"
echo "====================================="

# Check macOS version
os_version=$(sw_vers -productVersion)
major_version=$(echo $os_version | cut -d. -f1)

echo "Detected macOS version: $os_version"

if [ "$major_version" -lt 15 ]; then
    echo "❌ ERROR: macOS 15.0 or later is required for FSKit support."
    echo "Current version: $os_version"
    echo ""
    echo "💡 Tip: You can use macFUSE instead on older macOS versions."
    exit 1
fi

# Check Xcode Command Line Tools
if ! xcode-select -p &> /dev/null; then
    echo "📦 Installing Xcode Command Line Tools..."
    xcode-select --install
    echo "Please complete the installation and run this script again."
    exit 0
fi

echo "✅ FSKit is built into macOS 15.0+"
echo "✅ No additional installation required!"
echo ""
echo "🎉 Ready to use ShadowFS with FSKit!"
"#.to_string()
        }
    }
    
    fn check_prerequisites(&self) -> Vec<Prerequisite> {
        let mut prereqs = vec![];
        
        if self.use_macfuse {
            // Prerequisites for macFUSE
            prereqs.push(
                Prerequisite::new(
                    "Homebrew",
                    "Package manager for macOS",
                    self.has_homebrew()
                )
                .with_resolution("Install from https://brew.sh")
            );
            
            prereqs.push(
                Prerequisite::new(
                    "Kernel Extension Permission",
                    "macFUSE requires kernel extension approval",
                    false
                )
                .with_resolution("Allow in System Preferences > Security & Privacy after installation")
                .optional()
            );
        } else {
            // Prerequisites for FSKit
            match self.check_macos_version() {
                Ok((major, _minor)) => {
                    let satisfied = major >= 15;
                    prereqs.push(
                        Prerequisite::new(
                            "macOS Version",
                            format!("macOS 15.0 or later required for FSKit (current: {})", major),
                            satisfied
                        )
                        .with_resolution("Update to macOS 15.0 or use macFUSE instead")
                    );
                }
                Err(_) => {
                    prereqs.push(
                        Prerequisite::new(
                            "macOS Version",
                            "Unable to detect macOS version",
                            false
                        )
                        .with_resolution("Ensure macOS 15.0 or later is installed")
                    );
                }
            }
        }
        
        // Common prerequisites
        prereqs.push(
            Prerequisite::new(
                "Xcode Command Line Tools",
                "Required for building native extensions",
                self.has_xcode_tools()
            )
            .with_resolution("Run: xcode-select --install")
        );
        
        prereqs
    }
    
    fn estimate_install_time(&self) -> Duration {
        if self.use_macfuse {
            Duration::from_secs(300) // 5 minutes for macFUSE
        } else {
            Duration::from_secs(30) // 30 seconds for FSKit check
        }
    }
    
    fn requires_restart(&self) -> bool {
        self.use_macfuse // macFUSE requires restart, FSKit doesn't
    }
    
    fn platform(&self) -> Platform {
        Platform::MacOS
    }
    
    fn execute_with_progress(&self, progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
        let steps = if self.use_macfuse { 5 } else { 3 };
        let mut progress = InstallProgress::new(steps);
        
        progress.advance("Checking prerequisites");
        progress_callback(&progress);
        
        let prereqs = self.check_prerequisites();
        for prereq in &prereqs {
            if prereq.required && !prereq.satisfied {
                return Err(format!("Prerequisite not met: {}", prereq.name));
            }
        }
        
        if self.use_macfuse {
            progress.advance("Updating Homebrew");
            progress_callback(&progress);
            
            progress.advance("Installing macFUSE");
            progress_callback(&progress);
            
            progress.advance("Configuring kernel extension");
            progress_callback(&progress);
        } else {
            progress.advance("Verifying FSKit availability");
            progress_callback(&progress);
        }
        
        progress.advance("Completing installation");
        progress_callback(&progress);
        
        Ok(())
    }
}

/// Linux-specific installation helper
pub struct LinuxInstallHelper {
    distro: LinuxDistro,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinuxDistro {
    Ubuntu,
    Debian,
    Fedora,
    CentOS,
    Arch,
    OpenSUSE,
    Unknown,
}

impl LinuxInstallHelper {
    pub fn new() -> Self {
        Self {
            distro: Self::detect_distro(),
        }
    }
    
    fn detect_distro() -> LinuxDistro {
        // In real implementation, would read /etc/os-release
        LinuxDistro::Ubuntu
    }
    
    fn get_package_manager(&self) -> &'static str {
        match self.distro {
            LinuxDistro::Ubuntu | LinuxDistro::Debian => "apt",
            LinuxDistro::Fedora => "dnf",
            LinuxDistro::CentOS => "yum",
            LinuxDistro::Arch => "pacman",
            LinuxDistro::OpenSUSE => "zypper",
            LinuxDistro::Unknown => "unknown",
        }
    }
    
    fn get_fuse_package_name(&self) -> &'static str {
        match self.distro {
            LinuxDistro::Ubuntu | LinuxDistro::Debian => "fuse3",
            LinuxDistro::Fedora | LinuxDistro::CentOS => "fuse3 fuse3-devel",
            LinuxDistro::Arch => "fuse3",
            LinuxDistro::OpenSUSE => "fuse3 fuse3-devel",
            LinuxDistro::Unknown => "fuse3",
        }
    }
    
    fn check_fuse_installed(&self) -> bool {
        // Check if FUSE is installed
        false
    }
    
    fn check_user_in_fuse_group(&self) -> bool {
        // Check if current user is in fuse group
        false
    }
}

impl InstallHelper for LinuxInstallHelper {
    fn generate_install_script(&self) -> String {
        let pm = self.get_package_manager();
        let fuse_pkg = self.get_fuse_package_name();
        
        match self.distro {
            LinuxDistro::Ubuntu | LinuxDistro::Debian => {
                format!(r#"#!/bin/bash
# ShadowFS Linux Installation Script (Ubuntu/Debian)

echo "🐧 ShadowFS Linux Installation"
echo "============================="

# Update package list
echo "📦 Updating package list..."
sudo apt update

# Install FUSE 3
echo "📦 Installing FUSE 3..."
sudo apt install -y {}

# Install build dependencies
echo "📦 Installing build dependencies..."
sudo apt install -y build-essential pkg-config

# Add user to fuse group
echo "👤 Adding current user to fuse group..."
sudo usermod -aG fuse $USER

# Configure /etc/fuse.conf
echo "⚙️  Configuring FUSE..."
sudo sh -c 'echo "user_allow_other" >> /etc/fuse.conf'

# Verify installation
if command -v fusermount3 &> /dev/null; then
    echo "✅ FUSE 3 installed successfully!"
else
    echo "❌ FUSE 3 installation failed"
    exit 1
fi

echo ""
echo "🎉 Installation completed!"
echo "⚠️  Please log out and back in for group changes to take effect."
"#, fuse_pkg)
            }
            LinuxDistro::Fedora => {
                format!(r#"#!/bin/bash
# ShadowFS Linux Installation Script (Fedora)

echo "🐧 ShadowFS Linux Installation"
echo "============================="

# Install FUSE 3
echo "📦 Installing FUSE 3..."
sudo dnf install -y {}

# Install build dependencies
echo "📦 Installing build dependencies..."
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y pkg-config

# Add user to fuse group
echo "👤 Adding current user to fuse group..."
sudo usermod -aG fuse $USER

# Configure /etc/fuse.conf
echo "⚙️  Configuring FUSE..."
sudo sh -c 'echo "user_allow_other" >> /etc/fuse.conf'

echo ""
echo "🎉 Installation completed!"
echo "⚠️  Please log out and back in for group changes to take effect."
"#, fuse_pkg)
            }
            LinuxDistro::Arch => {
                r#"#!/bin/bash
# ShadowFS Linux Installation Script (Arch Linux)

echo "🐧 ShadowFS Linux Installation"
echo "============================="

# Install FUSE 3
echo "📦 Installing FUSE 3..."
sudo pacman -S --noconfirm fuse3

# Install build dependencies
echo "📦 Installing build dependencies..."
sudo pacman -S --noconfirm base-devel pkg-config

# Add user to fuse group
echo "👤 Adding current user to fuse group..."
sudo usermod -aG fuse $USER

# Configure /etc/fuse.conf
echo "⚙️  Configuring FUSE..."
sudo sh -c 'echo "user_allow_other" >> /etc/fuse.conf'

echo ""
echo "🎉 Installation completed!"
echo "⚠️  Please log out and back in for group changes to take effect."
"#.to_string()
            }
            _ => {
                format!(r#"#!/bin/bash
# ShadowFS Linux Installation Script (Generic)

echo "🐧 ShadowFS Linux Installation"
echo "============================="
echo "⚠️  Unknown distribution detected. Using generic instructions."
echo ""
echo "Please install the following packages using your package manager:"
echo "  - fuse3"
echo "  - fuse3-devel (or fuse3-dev)"
echo "  - pkg-config"
echo "  - build-essential (or equivalent)"
echo ""
echo "Then run:"
echo "  sudo usermod -aG fuse $USER"
echo "  echo 'user_allow_other' | sudo tee -a /etc/fuse.conf"
echo ""
echo "Package manager detected: {}"
"#, pm)
            }
        }
    }
    
    fn check_prerequisites(&self) -> Vec<Prerequisite> {
        let mut prereqs = vec![];
        
        // Check distro detection
        if self.distro == LinuxDistro::Unknown {
            prereqs.push(
                Prerequisite::new(
                    "Linux Distribution",
                    "Unable to detect Linux distribution",
                    false
                )
                .with_resolution("Manual installation may be required")
                .optional()
            );
        }
        
        // Check FUSE kernel module
        prereqs.push(
            Prerequisite::new(
                "FUSE Kernel Module",
                "FUSE kernel support is required",
                true // Assume present on modern Linux
            )
            .with_resolution("Install kernel headers and FUSE module")
        );
        
        // Check if FUSE is installed
        prereqs.push(
            Prerequisite::new(
                "FUSE 3",
                "FUSE 3 userspace tools",
                self.check_fuse_installed()
            )
            .with_resolution(format!("Install with: sudo {} install {}", 
                self.get_package_manager(), 
                self.get_fuse_package_name()
            ))
        );
        
        // Check user group membership
        prereqs.push(
            Prerequisite::new(
                "FUSE Group Membership",
                "User should be in 'fuse' group",
                self.check_user_in_fuse_group()
            )
            .with_resolution("Run: sudo usermod -aG fuse $USER")
            .optional()
        );
        
        prereqs
    }
    
    fn estimate_install_time(&self) -> Duration {
        Duration::from_secs(180) // 3 minutes
    }
    
    fn requires_restart(&self) -> bool {
        false // Just need to re-login for group changes
    }
    
    fn platform(&self) -> Platform {
        Platform::Linux
    }
    
    fn execute_with_progress(&self, progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
        let mut progress = InstallProgress::new(5);
        
        progress.advance("Checking prerequisites");
        progress_callback(&progress);
        
        let prereqs = self.check_prerequisites();
        for prereq in &prereqs {
            if prereq.required && !prereq.satisfied {
                return Err(format!("Prerequisite not met: {}", prereq.name));
            }
        }
        
        progress.advance("Updating package manager");
        progress_callback(&progress);
        
        progress.advance("Installing FUSE 3");
        progress_callback(&progress);
        
        progress.advance("Configuring permissions");
        progress_callback(&progress);
        
        progress.advance("Verifying installation");
        progress_callback(&progress);
        
        Ok(())
    }
}

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
    
    fn check_prerequisites(&self) -> Vec<Prerequisite> {
        match self {
            Self::Windows(h) => h.check_prerequisites(),
            Self::MacOS(h) => h.check_prerequisites(),
            Self::Linux(h) => h.check_prerequisites(),
        }
    }
    
    fn estimate_install_time(&self) -> Duration {
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
    
    fn platform(&self) -> Platform {
        match self {
            Self::Windows(h) => h.platform(),
            Self::MacOS(h) => h.platform(),
            Self::Linux(h) => h.platform(),
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
    
    pub fn run(&self) -> Result<(), String> {
        println!("🚀 ShadowFS Interactive Installer");
        println!("=================================\n");
        
        // Print requirements
        self.helper.pretty_print_requirements();
        
        // Check if we should proceed
        let prereqs = self.helper.check_prerequisites();
        let has_required_issues = prereqs.iter()
            .any(|p| p.required && !p.satisfied);
        
        if has_required_issues {
            return Err("Required prerequisites not met. Please resolve issues and try again.".to_string());
        }
        
        // Prompt for confirmation
        if !self.helper.prompt_for_confirmation() {
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
                self.helper.handle_error_gracefully(&e);
                Err(e)
            }
        }
    }
    
    pub fn generate_script_only(&self) -> String {
        self.helper.generate_install_script()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_prerequisite_builder() {
        let prereq = Prerequisite::new("Test", "Test description", true)
            .with_resolution("Do something")
            .optional();
        
        assert_eq!(prereq.name, "Test");
        assert_eq!(prereq.description, "Test description");
        assert!(prereq.satisfied);
        assert_eq!(prereq.resolution, "Do something");
        assert!(!prereq.required);
    }
    
    #[test]
    fn test_install_progress() {
        let mut progress = InstallProgress::new(4);
        assert_eq!(progress.percentage(), 0.0);
        
        progress.advance("Step 1");
        assert_eq!(progress.current_step, 1);
        assert_eq!(progress.percentage(), 25.0);
        
        progress.advance("Step 2");
        assert_eq!(progress.percentage(), 50.0);
    }
    
    #[test]
    fn test_windows_helper() {
        let helper = WindowsInstallHelper::new();
        assert_eq!(helper.platform(), Platform::Windows);
        assert!(helper.requires_restart());
        assert_eq!(helper.estimate_install_time(), Duration::from_secs(120));
        
        let script = helper.generate_install_script();
        assert!(script.contains("Enable-WindowsOptionalFeature"));
        assert!(script.contains("Client-ProjFS"));
    }
    
    #[test]
    fn test_macos_helper() {
        let helper = MacOSInstallHelper::new();
        assert_eq!(helper.platform(), Platform::MacOS);
        assert!(!helper.requires_restart());
        
        let macfuse_helper = MacOSInstallHelper::new().with_macfuse();
        assert!(macfuse_helper.requires_restart());
        
        let script = macfuse_helper.generate_install_script();
        assert!(script.contains("brew install --cask macfuse"));
    }
    
    #[test]
    fn test_linux_helper() {
        let helper = LinuxInstallHelper::new();
        assert_eq!(helper.platform(), Platform::Linux);
        assert!(!helper.requires_restart());
        assert_eq!(helper.estimate_install_time(), Duration::from_secs(180));
    }
}