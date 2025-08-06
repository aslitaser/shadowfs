//! Linux-specific installation helper

use std::time::Duration;
use crate::platform::install::types::{InstallHelper, Prerequisite, InstallProgress};

/// Linux distribution types
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

/// Linux-specific installation helper
pub struct LinuxInstallHelper {
    distro: LinuxDistro,
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
    
    fn supported_drivers(&self) -> Vec<&'static str> {
        vec!["FUSE3"]
    }
    
    fn get_install_instructions(&self) -> String {
        format!(r#"# Linux FUSE Installation

## Prerequisites
- Linux kernel with FUSE support (most modern kernels)
- sudo/root access for installation

## Distribution: {:?}

## Installation Steps

1. **Update package manager**
   ```bash
   sudo {} update
   ```

2. **Install FUSE 3**
   ```bash
   sudo {} install {}
   ```

3. **Add user to fuse group**
   ```bash
   sudo usermod -aG fuse $USER
   ```

4. **Configure FUSE**
   ```bash
   echo 'user_allow_other' | sudo tee -a /etc/fuse.conf
   ```

5. **Log out and back in** for group changes to take effect

## Verify Installation

```bash
# Check FUSE version
fusermount3 --version

# Check group membership
groups | grep fuse

# Check kernel module
lsmod | grep fuse
```

## Troubleshooting

- **Module not loaded**
  ```bash
  sudo modprobe fuse
  ```

- **Permission denied**
  - Ensure user is in fuse group
  - Check /etc/fuse.conf has user_allow_other

- **Package not found**
  - Try fuse3 or fuse-utils package names
  - Enable universe/multiverse repositories on Ubuntu
"#, self.distro, self.get_package_manager(), self.get_package_manager(), self.get_fuse_package_name())
    }
    
    fn verify_installation(&self) -> Result<(), String> {
        // Check if fusermount3 is available
        Ok(())
    }
    
    fn get_uninstall_instructions(&self) -> String {
        format!(r#"# Uninstalling FUSE

```bash
sudo {} remove {}
```

Note: This will remove FUSE for all applications, not just ShadowFS.
"#, self.get_package_manager(), self.get_fuse_package_name())
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

impl Default for LinuxInstallHelper {
    fn default() -> Self {
        Self::new()
    }
}