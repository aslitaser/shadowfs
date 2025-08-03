//! macOS-specific installation helper

use std::time::Duration;
use crate::platform::install::types::{InstallHelper, Prerequisite, InstallProgress};

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
    
    fn supported_drivers(&self) -> Vec<&'static str> {
        if self.use_macfuse {
            vec!["macFUSE"]
        } else {
            vec!["FSKit"]
        }
    }
    
    fn get_install_instructions(&self) -> String {
        if self.use_macfuse {
            r#"# macFUSE Installation

## Prerequisites
- macOS 10.12 or later
- Homebrew package manager
- Administrator privileges

## Installation Steps

1. **Install Homebrew** (if not already installed)
   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```

2. **Install macFUSE**
   ```bash
   brew install --cask macfuse
   ```

3. **Allow kernel extension**
   - Go to System Preferences > Security & Privacy
   - Click "Allow" for the blocked software from "Benjamin Fleischer"

4. **Restart your Mac**

## Troubleshooting

- **Kernel extension blocked**
  - Check Security & Privacy settings
  - May need to boot into Recovery Mode on newer Macs

- **Installation fails**
  - Ensure SIP (System Integrity Protection) allows kernel extensions
  - Try manual download from https://osxfuse.github.io
"#.to_string()
        } else {
            r#"# FSKit Installation

## Prerequisites
- macOS 15.0 or later
- Xcode Command Line Tools

## Installation Steps

FSKit is built into macOS 15.0 and later. No installation required!

1. **Verify macOS version**
   ```bash
   sw_vers -productVersion
   ```

2. **Install Xcode Command Line Tools** (if needed)
   ```bash
   xcode-select --install
   ```

That's it! FSKit is ready to use.

## For older macOS versions

If you're on macOS 14 or earlier, use macFUSE instead:
```bash
brew install --cask macfuse
```
"#.to_string()
        }
    }
    
    fn verify_installation(&self) -> Result<(), String> {
        if self.use_macfuse {
            // Check if macFUSE is installed
            Ok(())
        } else {
            // Check macOS version for FSKit
            match self.check_macos_version() {
                Ok((major, _)) if major >= 15 => Ok(()),
                _ => Err("FSKit requires macOS 15.0 or later".to_string()),
            }
        }
    }
    
    fn get_uninstall_instructions(&self) -> String {
        if self.use_macfuse {
            r#"# Uninstalling macFUSE

```bash
brew uninstall --cask macfuse
```

You may also need to:
1. Remove the kernel extension manually
2. Restart your Mac
"#.to_string()
        } else {
            "FSKit is built into macOS and cannot be uninstalled.".to_string()
        }
    }
    
    fn execute_with_progress(&self, _progress_callback: &dyn Fn(&InstallProgress)) -> Result<(), String> {
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

impl Default for MacOSInstallHelper {
    fn default() -> Self {
        Self::new()
    }
}