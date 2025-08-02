# ShadowFS API Reference

## Core Traits

### FileSystemProvider
The main trait that platform implementations must provide.

```rust
#[async_trait]
pub trait FileSystemProvider {
    async fn mount(&self, source: &Path, mount_point: &Path) -> Result<()>;
    async fn unmount(&self, mount_point: &Path) -> Result<()>;
    // ... more methods
}
```

### OverrideStore
Manages in-memory file overrides.

```rust
pub trait OverrideStore {
    fn add_override(&mut self, path: &Path, content: Vec<u8>);
    fn get_override(&self, path: &Path) -> Option<&[u8]>;
    fn remove_override(&mut self, path: &Path) -> Option<Vec<u8>>;
}
```

## Platform-Specific APIs

### Windows (ProjFS)
[TODO: Document Windows-specific APIs]

### macOS (FSKit)
[TODO: Document macOS-specific APIs]

### Linux (FUSE)
[TODO: Document Linux-specific APIs]

## FFI Interface
[TODO: Document C API for bindings]