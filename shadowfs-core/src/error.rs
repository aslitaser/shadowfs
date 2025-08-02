//! Error types for the ShadowFS system.

use crate::types::ShadowPath;
use std::fmt;
use thiserror::Error;

/// Represents the platform where the error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Windows => write!(f, "Windows"),
            Platform::MacOS => write!(f, "macOS"),
            Platform::Linux => write!(f, "Linux"),
        }
    }
}

/// Comprehensive error type for all ShadowFS operations.
#[derive(Debug, Error)]
pub enum ShadowError {
    /// File or directory not found.
    #[error("Path not found: {path}")]
    NotFound { 
        path: ShadowPath 
    },

    /// Permission denied for the operation.
    #[error("Permission denied for operation '{operation}' on path: {path}")]
    PermissionDenied { 
        path: ShadowPath, 
        operation: String 
    },

    /// File or directory already exists.
    #[error("Path already exists: {path}")]
    AlreadyExists { 
        path: ShadowPath 
    },

    /// Expected a directory but found something else.
    #[error("Not a directory: {path}")]
    NotADirectory { 
        path: ShadowPath 
    },

    /// Expected a file but found a directory.
    #[error("Is a directory: {path}")]
    IsADirectory { 
        path: ShadowPath 
    },

    /// Invalid path provided.
    #[error("Invalid path '{path}': {reason}")]
    InvalidPath { 
        path: String, 
        reason: String 
    },

    /// I/O error from the underlying system.
    #[error("I/O error: {source}")]
    IoError {
        #[source]
        source: std::io::Error,
    },

    /// Platform-specific error.
    #[error("Platform error on {platform}: {message} (code: {code:?})")]
    PlatformError { 
        platform: Platform, 
        message: String, 
        code: Option<i32> 
    },

    /// Override store is full.
    #[error("Override store is full: current size {current_size} bytes, maximum {max_size} bytes")]
    OverrideStoreFull { 
        current_size: usize, 
        max_size: usize 
    },

    /// Mount point is not mounted.
    #[error("Mount point not mounted: {mount_point}")]
    NotMounted { 
        mount_point: ShadowPath 
    },

    /// Feature not supported.
    #[error("Unsupported feature: {feature}")]
    Unsupported { 
        feature: String 
    },
}

impl ShadowError {
    /// Creates a ShadowError from an io::Error with context about the path.
    /// This provides more specific error mapping than the generic From trait.
    pub fn from_io_error(error: std::io::Error, path: Option<&ShadowPath>) -> Self {
        use std::io::ErrorKind;
        
        match error.kind() {
            ErrorKind::NotFound => {
                if let Some(p) = path {
                    ShadowError::NotFound { path: p.clone() }
                } else {
                    ShadowError::IoError { source: error }
                }
            }
            ErrorKind::PermissionDenied => {
                if let Some(p) = path {
                    ShadowError::PermissionDenied { 
                        path: p.clone(), 
                        operation: "access".to_string() 
                    }
                } else {
                    ShadowError::IoError { source: error }
                }
            }
            ErrorKind::AlreadyExists => {
                if let Some(p) = path {
                    ShadowError::AlreadyExists { path: p.clone() }
                } else {
                    ShadowError::IoError { source: error }
                }
            }
            ErrorKind::InvalidInput | ErrorKind::InvalidData => {
                if let Some(p) = path {
                    ShadowError::InvalidPath { 
                        path: p.to_string(), 
                        reason: error.to_string() 
                    }
                } else {
                    ShadowError::InvalidPath {
                        path: String::new(),
                        reason: error.to_string()
                    }
                }
            }
            _ => ShadowError::IoError { source: error }
        }
    }

    /// Creates a ShadowError from an io::Error for a specific operation.
    pub fn from_io_error_with_operation(
        error: std::io::Error, 
        path: &ShadowPath, 
        operation: &str
    ) -> Self {
        use std::io::ErrorKind;
        
        match error.kind() {
            ErrorKind::PermissionDenied => {
                ShadowError::PermissionDenied { 
                    path: path.clone(), 
                    operation: operation.to_string() 
                }
            }
            _ => Self::from_io_error(error, Some(path))
        }
    }
}

impl From<std::io::Error> for ShadowError {
    fn from(error: std::io::Error) -> Self {
        Self::from_io_error(error, None)
    }
}

/// Result type alias for ShadowFS operations.
pub type Result<T> = std::result::Result<T, ShadowError>;

/// Trait for adding context to errors.
/// 
/// This trait provides methods to add additional context to errors,
/// making it easier to understand where and why an error occurred.
pub trait ErrorContext<T> {
    /// Adds context to an error.
    /// 
    /// # Example
    /// ```ignore
    /// use shadowfs_core::error::{ErrorContext, Result};
    /// 
    /// fn read_config() -> Result<String> {
    ///     std::fs::read_to_string("/etc/shadowfs.conf")
    ///         .context("Failed to read configuration file")
    /// }
    /// ```
    fn context<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static;

    /// Adds context to an error with a closure.
    /// 
    /// The closure is only evaluated if an error occurs, which can be useful
    /// for expensive context generation.
    /// 
    /// # Example
    /// ```ignore
    /// use shadowfs_core::error::{ErrorContext, Result};
    /// 
    /// fn process_file(path: &str) -> Result<()> {
    ///     std::fs::read(path)
    ///         .with_context(|| format!("Failed to process file: {}", path))?;
    ///     Ok(())
    /// }
    /// ```
    fn with_context<C, F>(self, f: F) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T> ErrorContext<T> for Result<T> {
    fn context<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|err| {
            match err {
                ShadowError::IoError { source } => {
                    let new_source = std::io::Error::new(
                        source.kind(),
                        format!("{}: {}", context, source)
                    );
                    ShadowError::IoError { source: new_source }
                }
                _ => {
                    let io_err = std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}: {}", context, err)
                    );
                    ShadowError::IoError { source: io_err }
                }
            }
        })
    }

    fn with_context<C, F>(self, f: F) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|err| {
            let context = f();
            match err {
                ShadowError::IoError { source } => {
                    let new_source = std::io::Error::new(
                        source.kind(),
                        format!("{}: {}", context, source)
                    );
                    ShadowError::IoError { source: new_source }
                }
                _ => {
                    let io_err = std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}: {}", context, err)
                    );
                    ShadowError::IoError { source: io_err }
                }
            }
        })
    }
}

impl<T> ErrorContext<T> for std::io::Result<T> {
    fn context<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|err| {
            let io_err = std::io::Error::new(
                err.kind(),
                format!("{}: {}", context, err)
            );
            ShadowError::IoError { source: io_err }
        })
    }

    fn with_context<C, F>(self, f: F) -> Result<T>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|err| {
            let context = f();
            let io_err = std::io::Error::new(
                err.kind(),
                format!("{}: {}", context, err)
            );
            ShadowError::IoError { source: io_err }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let path = ShadowPath::from("/test/file.txt");
        
        // Test NotFound
        let err = ShadowError::NotFound { path: path.clone() };
        assert_eq!(err.to_string(), "Path not found: /test/file.txt");

        // Test PermissionDenied
        let err = ShadowError::PermissionDenied { 
            path: path.clone(), 
            operation: "write".to_string() 
        };
        assert_eq!(err.to_string(), "Permission denied for operation 'write' on path: /test/file.txt");

        // Test AlreadyExists
        let err = ShadowError::AlreadyExists { path: path.clone() };
        assert_eq!(err.to_string(), "Path already exists: /test/file.txt");

        // Test NotADirectory
        let err = ShadowError::NotADirectory { path: path.clone() };
        assert_eq!(err.to_string(), "Not a directory: /test/file.txt");

        // Test IsADirectory
        let err = ShadowError::IsADirectory { path: path.clone() };
        assert_eq!(err.to_string(), "Is a directory: /test/file.txt");

        // Test InvalidPath
        let err = ShadowError::InvalidPath { 
            path: "//invalid//path".to_string(), 
            reason: "contains double slashes".to_string() 
        };
        assert_eq!(err.to_string(), "Invalid path '//invalid//path': contains double slashes");

        // Test PlatformError
        let err = ShadowError::PlatformError { 
            platform: Platform::Windows, 
            message: "Access denied".to_string(), 
            code: Some(5) 
        };
        assert_eq!(err.to_string(), "Platform error on Windows: Access denied (code: Some(5))");

        // Test OverrideStoreFull
        let err = ShadowError::OverrideStoreFull { 
            current_size: 1048576, 
            max_size: 1048576 
        };
        assert_eq!(err.to_string(), "Override store is full: current size 1048576 bytes, maximum 1048576 bytes");

        // Test NotMounted
        let err = ShadowError::NotMounted { 
            mount_point: ShadowPath::from("/mnt/shadow") 
        };
        assert_eq!(err.to_string(), "Mount point not mounted: /mnt/shadow");

        // Test Unsupported
        let err = ShadowError::Unsupported { 
            feature: "symbolic links".to_string() 
        };
        assert_eq!(err.to_string(), "Unsupported feature: symbolic links");
    }

    #[test]
    fn test_io_error_conversion() {
        // Test basic conversion without path
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "generic error");
        let shadow_err: ShadowError = io_err.into();
        assert!(matches!(shadow_err, ShadowError::IoError { .. }));
    }

    #[test]
    fn test_io_error_conversion_with_path() {
        let path = ShadowPath::from("/test/file.txt");

        // Test NotFound conversion
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let shadow_err = ShadowError::from_io_error(io_err, Some(&path));
        assert!(matches!(shadow_err, ShadowError::NotFound { path: p } if p == path));

        // Test PermissionDenied conversion
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let shadow_err = ShadowError::from_io_error(io_err, Some(&path));
        assert!(matches!(
            shadow_err, 
            ShadowError::PermissionDenied { path: p, operation } 
            if p == path && operation == "access"
        ));

        // Test AlreadyExists conversion
        let io_err = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists");
        let shadow_err = ShadowError::from_io_error(io_err, Some(&path));
        assert!(matches!(shadow_err, ShadowError::AlreadyExists { path: p } if p == path));

        // Test InvalidInput conversion
        let io_err = std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid");
        let shadow_err = ShadowError::from_io_error(io_err, Some(&path));
        assert!(matches!(
            shadow_err, 
            ShadowError::InvalidPath { path: p, .. } 
            if p == path.to_string()
        ));
    }

    #[test]
    fn test_io_error_with_operation() {
        let path = ShadowPath::from("/test/file.txt");
        
        // Test PermissionDenied with custom operation
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let shadow_err = ShadowError::from_io_error_with_operation(io_err, &path, "write");
        assert!(matches!(
            shadow_err, 
            ShadowError::PermissionDenied { path: p, operation } 
            if p == path && operation == "write"
        ));

        // Test other error kinds fallback to from_io_error
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let shadow_err = ShadowError::from_io_error_with_operation(io_err, &path, "read");
        assert!(matches!(shadow_err, ShadowError::NotFound { path: p } if p == path));
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Windows.to_string(), "Windows");
        assert_eq!(Platform::MacOS.to_string(), "macOS");
        assert_eq!(Platform::Linux.to_string(), "Linux");
    }

    #[test]
    fn test_error_context() {
        // Test context on io::Error
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file.txt");
        let result: std::result::Result<(), _> = Err(io_err);
        let shadow_result = result.context("Failed to open configuration");
        
        assert!(shadow_result.is_err());
        let err = shadow_result.unwrap_err();
        assert!(matches!(err, ShadowError::IoError { .. }));
        assert!(err.to_string().contains("Failed to open configuration"));
    }

    #[test]
    fn test_error_with_context() {
        // Test with_context on io::Error
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let result: std::result::Result<(), _> = Err(io_err);
        let path = "/etc/shadowfs.conf";
        let shadow_result = result.with_context(|| format!("Cannot read file: {}", path));
        
        assert!(shadow_result.is_err());
        let err = shadow_result.unwrap_err();
        assert!(matches!(err, ShadowError::IoError { .. }));
        assert!(err.to_string().contains("Cannot read file: /etc/shadowfs.conf"));
    }

    #[test]
    fn test_shadow_error_context() {
        // Test context on ShadowError
        let path = ShadowPath::from("/test/file.txt");
        let result: Result<()> = Err(ShadowError::NotFound { path });
        let contextualized = result.context("While processing user request");
        
        assert!(contextualized.is_err());
        let err = contextualized.unwrap_err();
        assert!(matches!(err, ShadowError::IoError { .. }));
        assert!(err.to_string().contains("While processing user request"));
    }

    #[test]
    fn test_shadow_error_with_context_io_error() {
        // Test with_context on ShadowError::IoError preserves error kind
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "original error");
        let result: Result<()> = Err(ShadowError::IoError { source: io_err });
        let contextualized = result.with_context(|| "Additional context");
        
        assert!(contextualized.is_err());
        if let Err(ShadowError::IoError { source }) = contextualized {
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            assert!(source.to_string().contains("Additional context"));
            assert!(source.to_string().contains("original error"));
        } else {
            panic!("Expected IoError variant");
        }
    }

    #[test]
    fn test_context_chain() {
        // Test chaining multiple contexts
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "base error");
        let result: std::result::Result<(), _> = Err(io_err);
        let contextualized = result
            .context("First context")
            .context("Second context");
        
        assert!(contextualized.is_err());
        let err_string = contextualized.unwrap_err().to_string();
        assert!(err_string.contains("Second context"));
        assert!(err_string.contains("First context"));
    }
}