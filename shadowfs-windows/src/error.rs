use std::fmt;
use std::error::Error;

/// Windows-specific error types for ShadowFS
#[derive(Debug)]
pub enum WindowsError {
    /// I/O operation failed
    IoError {
        message: String,
        code: u32,
    },
    
    /// ProjFS API error
    ProjFSError {
        message: String,
        hresult: i32,
    },
    
    /// Invalid operation
    InvalidOperation {
        message: String,
    },
    
    /// Access denied
    AccessDenied {
        message: String,
    },
    
    /// Not found
    NotFound {
        path: String,
    },
    
    /// Already exists
    AlreadyExists {
        path: String,
    },
    
    /// Unsupported operation
    Unsupported {
        message: String,
    },
    
    /// Thread creation failed
    ThreadCreation(String),
    
    /// Queue is full
    QueueFull(usize),
    
    /// Channel closed
    ChannelClosed,
    
    /// Async processing error
    AsyncProcessing(String),
    
    /// Service not running
    ServiceNotRunning,
}

impl fmt::Display for WindowsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowsError::IoError { message, code } => {
                write!(f, "I/O error (code {}): {}", code, message)
            }
            WindowsError::ProjFSError { message, hresult } => {
                write!(f, "ProjFS error (HRESULT 0x{:08X}): {}", hresult, message)
            }
            WindowsError::InvalidOperation { message } => {
                write!(f, "Invalid operation: {}", message)
            }
            WindowsError::AccessDenied { message } => {
                write!(f, "Access denied: {}", message)
            }
            WindowsError::NotFound { path } => {
                write!(f, "Not found: {}", path)
            }
            WindowsError::AlreadyExists { path } => {
                write!(f, "Already exists: {}", path)
            }
            WindowsError::Unsupported { message } => {
                write!(f, "Unsupported: {}", message)
            }
            WindowsError::ThreadCreation(msg) => {
                write!(f, "Thread creation failed: {}", msg)
            }
            WindowsError::QueueFull(size) => {
                write!(f, "Queue is full (size: {})", size)
            }
            WindowsError::ChannelClosed => {
                write!(f, "Channel closed")
            }
            WindowsError::AsyncProcessing(msg) => {
                write!(f, "Async processing error: {}", msg)
            }
            WindowsError::ServiceNotRunning => {
                write!(f, "Service is not running")
            }
        }
    }
}

impl Error for WindowsError {}

impl From<std::io::Error> for WindowsError {
    fn from(err: std::io::Error) -> Self {
        WindowsError::IoError {
            message: err.to_string(),
            code: err.raw_os_error().unwrap_or(0) as u32,
        }
    }
}

impl From<windows::core::Error> for WindowsError {
    fn from(err: windows::core::Error) -> Self {
        WindowsError::ProjFSError {
            message: err.message().to_string_lossy(),
            hresult: err.code().0,
        }
    }
}

impl From<WindowsError> for windows::core::Error {
    fn from(err: WindowsError) -> Self {
        match err {
            WindowsError::IoError { code, .. } => {
                windows::core::Error::from_win32(code)
            }
            WindowsError::ProjFSError { hresult, .. } => {
                windows::core::Error::from(windows::core::HRESULT(hresult))
            }
            _ => {
                // Default to generic error
                windows::core::Error::from_win32(5) // ERROR_ACCESS_DENIED
            }
        }
    }
}

/// Result type for Windows operations
pub type WindowsResult<T> = Result<T, WindowsError>;
