//! Platform-specific installation helpers for ShadowFS
//! 
//! This module provides utilities to help users install the necessary
//! platform-specific components required for ShadowFS to function.

pub mod types;
pub mod windows;
pub mod macos;
pub mod linux;
pub mod interactive;

// Re-export commonly used types
pub use types::{
    InstallHelper, Prerequisite, InstallProgress, 
    InstallMethod, InstallResult
};

pub use windows::WindowsInstallHelper;
pub use macos::MacOSInstallHelper;
pub use linux::{LinuxInstallHelper, LinuxDistro};
pub use interactive::{InteractiveInstaller, PlatformInstallHelper};

/// Get the appropriate install helper for the current platform
pub fn get_platform_installer() -> PlatformInstallHelper {
    use crate::types::mount::Platform;
    use crate::traits::PlatformExt;
    
    match Platform::current() {
        Platform::Windows => PlatformInstallHelper::Windows(WindowsInstallHelper::new()),
        Platform::MacOS => PlatformInstallHelper::MacOS(MacOSInstallHelper::new()),
        Platform::Linux => PlatformInstallHelper::Linux(LinuxInstallHelper::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::types::mount::Platform;
    use crate::traits::PlatformExt;
    
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
        assert_eq!(progress.percentage(), 25.0);
        
        progress.advance("Step 2");
        assert_eq!(progress.percentage(), 50.0);
    }
    
    #[test]
    fn test_install_result() {
        let result = InstallResult::success()
            .with_restart()
            .with_step("Additional configuration needed");
        
        assert!(result.success);
        assert!(result.restart_required);
        assert_eq!(result.additional_steps.len(), 1);
    }
    
    #[test]
    fn test_get_platform_installer() {
        let installer = get_platform_installer();
        
        match Platform::current() {
            Platform::Windows => {
                assert!(matches!(installer, PlatformInstallHelper::Windows(_)));
            }
            Platform::MacOS => {
                assert!(matches!(installer, PlatformInstallHelper::MacOS(_)));
            }
            Platform::Linux => {
                assert!(matches!(installer, PlatformInstallHelper::Linux(_)));
            }
        }
    }
}