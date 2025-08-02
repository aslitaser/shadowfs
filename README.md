# ShadowFS

[![Build Status](https://img.shields.io/github/workflow/status/aslitaser/shadowfs/CI)](https://github.com/aslitaser/shadowfs/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/shadowfs.svg)](https://crates.io/crates/shadowfs)

A high-performance, cross-platform virtual filesystem that provides in-memory overrides for real filesystem operations without modifying the underlying files.

## Features

- **Cross-platform support**: Native implementations for Windows (ProjFS), macOS (FSKit), and Linux (FUSE)
- **In-memory overrides**: Modify files virtually without touching the real filesystem
- **Transparent operation**: Applications see modified content while originals remain unchanged
- **High performance**: Optimized for minimal overhead
- **Async/await**: Built on Tokio for efficient concurrent operations

## Quick Start

```bash
# Install shadowfs
cargo install shadowfs-cli

# Mount a directory with shadowfs
shadowfs mount --source /path/to/source --mount /path/to/mount

# Check status
shadowfs status

# Unmount when done
shadowfs unmount /path/to/mount
```

## Architecture Overview

ShadowFS consists of several components:

- **shadowfs-core**: Core abstractions and traits shared across platforms
- **shadowfs-windows**: Windows implementation using Projected File System (ProjFS)
- **shadowfs-macos**: macOS implementation using File System Kit (FSKit)
- **shadowfs-linux**: Linux implementation using FUSE
- **shadowfs-ffi**: C API for language bindings
- **shadowfs-cli**: Command-line interface

## Platform Requirements

### Windows
- Windows 10 version 1809 or later
- Projected File System feature enabled

### macOS
- macOS 15.0 or later (for FSKit support)
- System Integrity Protection may need configuration

### Linux
- FUSE 3.0 or later
- Kernel support for FUSE filesystems

## Building from Source

```bash
# Clone the repository
git clone https://github.com/aslitaser/shadowfs.git
cd shadowfs

# Build for your platform
cargo build --release

# Run tests
cargo test --workspace
```

## Contributing

We welcome contributions! Please see our [Contributing Guidelines](CONTRIBUTING.md) for details.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.