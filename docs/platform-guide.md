# Platform-Specific Guide

## Windows

### Requirements
- Windows 10 version 1809 or later
- Windows SDK with ProjFS headers
- Visual Studio 2019 or later

### Building
```bash
cargo build --target x86_64-pc-windows-msvc
```

### Special Considerations
- ProjFS requires elevated permissions for some operations
- Antivirus software may interfere with virtual filesystem operations

## macOS

### Requirements
- macOS 15.0 or later
- Xcode 15 or later
- FSKit framework

### Building
```bash
cargo build --target x86_64-apple-darwin
```

### Special Considerations
- System Integrity Protection (SIP) may need configuration
- Requires notarization for distribution

## Linux

### Requirements
- Linux kernel 4.18 or later
- FUSE 3.0 or later
- Development headers for FUSE

### Installation
```bash
# Ubuntu/Debian
sudo apt-get install fuse3 libfuse3-dev

# Fedora/RHEL
sudo dnf install fuse3 fuse3-devel

# Arch
sudo pacman -S fuse3
```

### Building
```bash
cargo build --target x86_64-unknown-linux-gnu
```

### Special Considerations
- User must be in the `fuse` group or have appropriate permissions
- Some distributions require explicit FUSE module loading