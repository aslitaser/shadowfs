use std::fmt;
use crate::types::{ShadowPath, FileHandle};

/// Represents all possible errors in the ShadowFS system.
#[derive(Debug, Clone, PartialEq)]
pub enum ShadowError {
    /// File or directory not found
    NotFound(ShadowPath),
    /// Permission denied
    PermissionDenied(ShadowPath),
    /// File or directory already exists
    AlreadyExists(ShadowPath),
    /// Invalid file handle
    InvalidHandle(FileHandle),
    /// I/O error with description
    IoError(String),
    /// Invalid path
    InvalidPath(String),
    /// Operation not supported
    NotSupported(String),
    /// File system is full
    NoSpace,
    /// Directory not empty
    DirectoryNotEmpty(ShadowPath),
    /// Not a directory
    NotADirectory(ShadowPath),
    /// Is a directory (when expecting a file)
    IsADirectory(ShadowPath),
    /// Invalid argument
    InvalidArgument(String),
    /// Operation would block
    WouldBlock,
    /// Broken pipe
    BrokenPipe,
    /// Connection aborted
    ConnectionAborted,
    /// Connection reset
    ConnectionReset,
    /// Operation interrupted
    Interrupted,
    /// Other error with custom message
    Other(String),
}

impl fmt::Display for ShadowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShadowError::NotFound(path) => write!(f, "File not found: {}", path),
            ShadowError::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            ShadowError::AlreadyExists(path) => write!(f, "Already exists: {}", path),
            ShadowError::InvalidHandle(handle) => write!(f, "Invalid handle: {}", handle),
            ShadowError::IoError(msg) => write!(f, "I/O error: {}", msg),
            ShadowError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            ShadowError::NotSupported(op) => write!(f, "Operation not supported: {}", op),
            ShadowError::NoSpace => write!(f, "No space left on device"),
            ShadowError::DirectoryNotEmpty(path) => write!(f, "Directory not empty: {}", path),
            ShadowError::NotADirectory(path) => write!(f, "Not a directory: {}", path),
            ShadowError::IsADirectory(path) => write!(f, "Is a directory: {}", path),
            ShadowError::InvalidArgument(arg) => write!(f, "Invalid argument: {}", arg),
            ShadowError::WouldBlock => write!(f, "Operation would block"),
            ShadowError::BrokenPipe => write!(f, "Broken pipe"),
            ShadowError::ConnectionAborted => write!(f, "Connection aborted"),
            ShadowError::ConnectionReset => write!(f, "Connection reset"),
            ShadowError::Interrupted => write!(f, "Operation interrupted"),
            ShadowError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for ShadowError {}

/// Type alias for Results in the ShadowFS system.
/// This provides a convenient way to return results from operations.
pub type OperationResult<T> = Result<T, ShadowError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_error_display() {
        let path = ShadowPath::from("/test/file.txt");
        let handle = FileHandle::new(42);

        let errors = vec![
            (ShadowError::NotFound(path.clone()), "File not found: /test/file.txt"),
            (ShadowError::PermissionDenied(path.clone()), "Permission denied: /test/file.txt"),
            (ShadowError::AlreadyExists(path.clone()), "Already exists: /test/file.txt"),
            (ShadowError::InvalidHandle(handle), "Invalid handle: FileHandle(42)"),
            (ShadowError::IoError("disk error".to_string()), "I/O error: disk error"),
            (ShadowError::InvalidPath("/bad/path".to_string()), "Invalid path: /bad/path"),
            (ShadowError::NotSupported("symlinks".to_string()), "Operation not supported: symlinks"),
            (ShadowError::NoSpace, "No space left on device"),
            (ShadowError::DirectoryNotEmpty(path.clone()), "Directory not empty: /test/file.txt"),
            (ShadowError::NotADirectory(path.clone()), "Not a directory: /test/file.txt"),
            (ShadowError::IsADirectory(path.clone()), "Is a directory: /test/file.txt"),
            (ShadowError::InvalidArgument("bad arg".to_string()), "Invalid argument: bad arg"),
            (ShadowError::WouldBlock, "Operation would block"),
            (ShadowError::BrokenPipe, "Broken pipe"),
            (ShadowError::ConnectionAborted, "Connection aborted"),
            (ShadowError::ConnectionReset, "Connection reset"),
            (ShadowError::Interrupted, "Operation interrupted"),
            (ShadowError::Other("custom error".to_string()), "Error: custom error"),
        ];

        for (error, expected) in errors {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn test_operation_result() {
        fn test_op(success: bool) -> OperationResult<u32> {
            if success {
                Ok(42)
            } else {
                Err(ShadowError::Other("test error".to_string()))
            }
        }

        let ok_result = test_op(true);
        assert!(ok_result.is_ok());
        assert_eq!(ok_result.unwrap(), 42);

        let err_result = test_op(false);
        assert!(err_result.is_err());
        assert_eq!(
            err_result.unwrap_err(),
            ShadowError::Other("test error".to_string())
        );
    }
}