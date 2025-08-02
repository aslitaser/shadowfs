// Module declarations
pub mod path;
pub mod metadata;
pub mod operations;
pub mod directory;
pub mod error;

// Re-export all types from submodules
pub use path::ShadowPath;
pub use metadata::{FileType, FilePermissions, PlatformMetadata, FileMetadata};
pub use operations::{FileHandle, OpenFlags, Bytes, FileOperation};
pub use directory::DirectoryEntry;
pub use error::{ShadowError, OperationResult};