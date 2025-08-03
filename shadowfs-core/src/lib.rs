//! # ShadowFS Core
//! 
//! The core library for ShadowFS - a cross-platform virtual filesystem that provides
//! in-memory overrides for real filesystem operations without modifying the underlying files.
//! 
//! ## Overview
//! 
//! ShadowFS allows you to create a virtual layer over existing filesystems where modifications
//! are kept in memory. This is useful for:
//! 
//! - Testing file operations without affecting real files
//! - Creating temporary workspaces
//! - Implementing undo/redo functionality for file operations
//! - Sandboxing applications
//! 
//! ## Basic Usage
//! 
//! ```rust,ignore
//! use shadowfs_core::traits::FileSystemProvider;
//! use std::path::Path;
//! 
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // Platform-specific provider would be used here
//!     let provider = get_platform_provider();
//!     
//!     // Mount a shadow filesystem
//!     provider.mount(
//!         Path::new("/source/directory"),
//!         Path::new("/mount/point")
//!     ).await?;
//!     
//!     // All operations on /mount/point now go through ShadowFS
//!     Ok(())
//! }
//! ```
//! 
//! ## Architecture
//! 
//! This crate provides the core abstractions used by platform-specific implementations:
//! 
//! - [`traits`]: Core traits that platform implementations must provide
//! - [`types`]: Common types used across the system
//! - [`error`]: Error types and handling
//! - [`override_store`]: In-memory storage for file overrides
//! - [`stats`]: Performance statistics collection
//! 
//! ## Platform Support
//! 
//! ShadowFS supports multiple platforms through separate crates:
//! 
//! - `shadowfs-windows`: Windows implementation using ProjFS
//! - `shadowfs-macos`: macOS implementation using FSKit
//! - `shadowfs-linux`: Linux implementation using FUSE
//! 
//! ## More Information
//! 
//! - [Architecture Documentation](https://github.com/aslitaser/shadowfs/blob/main/docs/architecture.md)
//! - [API Reference](https://github.com/aslitaser/shadowfs/blob/main/docs/api-reference.md)
//! - [Platform Guide](https://github.com/aslitaser/shadowfs/blob/main/docs/platform-guide.md)
//! - [Contributing](https://github.com/aslitaser/shadowfs/blob/main/docs/contributing.md)

pub mod traits;
pub mod types;
pub mod error;
pub mod override_store;
pub mod stats;
pub mod platform;