# ShadowFS Architecture

## Overview

ShadowFS is designed as a modular, cross-platform virtual filesystem that provides in-memory overrides for real filesystem operations.

## Core Components

### shadowfs-core
The foundation library containing:
- Common traits and abstractions
- Override store implementation
- Error handling
- Performance statistics

### Platform Implementations
- **shadowfs-windows**: Uses Windows Projected File System (ProjFS)
- **shadowfs-macos**: Uses macOS File System Kit (FSKit)
- **shadowfs-linux**: Uses FUSE (Filesystem in Userspace)

### Additional Components
- **shadowfs-ffi**: C API for language bindings
- **shadowfs-cli**: Command-line interface

## Data Flow

[TODO: Add architecture diagram]

## Design Principles

1. **Platform Independence**: Core logic is separated from platform-specific implementations
2. **Performance First**: Minimize overhead for non-overridden files
3. **Memory Efficiency**: Smart caching and lazy loading
4. **Type Safety**: Leverage Rust's type system for correctness

## Implementation Details

[TODO: Add detailed implementation notes]