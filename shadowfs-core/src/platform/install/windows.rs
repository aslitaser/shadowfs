//! Windows-specific installation helper

use std::time::Duration;
use crate::platform::install::types::{InstallHelper, Prerequisite, InstallProgress};

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
    
    fn supported_drivers(&self) -> Vec<&'static str> {
        vec!["ProjFS"]
    }
    
    fn get_install_instructions(&self) -> String {
        r#"# Windows ProjFS Installation

## Prerequisites
- Windows 10 version 1809 (Build 17763) or later
- Administrator privileges

## Installation Steps

1. **Open PowerShell as Administrator**
   - Right-click on PowerShell
   - Select "Run as Administrator"

2. **Run the installation script**
   ```powershell
   # Save and run the generated script, or run this command directly:
   Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
   ```

3. **Verify installation**
   ```powershell
   Get-WindowsOptionalFeature -Online -FeatureName Client-ProjFS
   ```

4. **Restart if required**
   Some systems may require a restart for ProjFS to fully activate.

## Troubleshooting

- **Error: Feature not found**
  - Ensure Windows 10 version 1809 or later
  - Run Windows Update

- **Access denied**
  - Ensure running as Administrator
  - Check group policy restrictions
"#.to_string()
    }
    
    fn verify_installation(&self) -> Result<(), String> {
        // In real implementation, would check if ProjFS is enabled
        Ok(())
    }
    
    fn get_uninstall_instructions(&self) -> String {
        r#"# Uninstalling ProjFS

Run as Administrator:
```powershell
Disable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
```

Note: This will disable ProjFS for all applications, not just ShadowFS.
"#.to_string()
    }
    
    fn execute_with_progress(&self, _progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
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

impl Default for WindowsInstallHelper {
    fn default() -> Self {
        Self::new()
    }
}