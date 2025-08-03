//! Cross-platform compatibility layer for ShadowFS
//! 
//! This module provides functions to handle platform-specific differences
//! in paths, permissions, timestamps, errors, and text encoding.

use std::path::PathBuf;
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::ffi::{OsStr, OsString};
use crate::types::{ShadowPath, FilePermissions};
use crate::error::{ShadowError, invalid_path, platform_error, Platform as ErrorPlatform};
use crate::types::mount::Platform;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

/// Windows file attributes
#[derive(Debug, Clone, Copy)]
pub struct WindowsAttributes {
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    pub archive: bool,
    pub temporary: bool,
    pub compressed: bool,
    pub encrypted: bool,
}

/// Path compatibility functions
pub struct PathCompat;

impl PathCompat {
    /// Normalize a path for cross-platform compatibility
    pub fn normalize_path(path: &str) -> Result<ShadowPath, ShadowError> {
        let path = path.trim();
        
        // Handle empty paths
        if path.is_empty() {
            return Err(invalid_path("", "Empty path provided"));
        }
        
        // Platform-specific normalization
        let normalized = match Platform::current() {
            Platform::Windows => Self::normalize_windows_path(path)?,
            Platform::MacOS => Self::normalize_macos_path(path)?,
            Platform::Linux => Self::normalize_linux_path(path)?,
        };
        
        Ok(ShadowPath::from(normalized))
    }
    
    /// Normalize Windows paths
    #[cfg(windows)]
    fn normalize_windows_path(path: &str) -> Result<PathBuf, ShadowError> {
        let mut normalized = String::new();
        
        // Handle UNC paths (\\server\share or //server/share)
        if path.starts_with("\\\\") || path.starts_with("//") {
            normalized.push_str("\\\\");
            let rest = &path[2..];
            
            // Ensure we have server and share components
            let parts: Vec<&str> = rest.split(&['\\', '/'][..]).collect();
            if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
                return Err(invalid_path(path, "Invalid UNC path: missing server or share"));
            }
            
            // Rebuild with consistent separators
            normalized.push_str(parts[0]);
            for part in &parts[1..] {
                if !part.is_empty() {
                    normalized.push('\\');
                    normalized.push_str(part);
                }
            }
        } else {
            // Handle regular paths
            // Convert forward slashes to backslashes
            normalized = path.replace('/', "\\");
            
            // Handle drive letters
            if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
                // Ensure drive letter is uppercase
                let drive = normalized.chars().next().unwrap().to_ascii_uppercase();
                normalized.replace_range(0..1, &drive.to_string());
            }
        }
        
        // Remove duplicate separators
        while normalized.contains("\\\\") && !normalized.starts_with("\\\\") {
            normalized = normalized.replace("\\\\", "\\");
        }
        
        // Remove trailing separator unless it's the root
        if normalized.len() > 1 && normalized.ends_with('\\') && !normalized.ends_with(":\\") {
            normalized.pop();
        }
        
        Ok(PathBuf::from(normalized))
    }
    
    /// Normalize Windows paths (non-Windows fallback)
    #[cfg(not(windows))]
    fn normalize_windows_path(path: &str) -> Result<PathBuf, ShadowError> {
        // On non-Windows, just convert backslashes to forward slashes
        let normalized = path.replace('\\', "/");
        Ok(PathBuf::from(normalized))
    }
    
    /// Normalize macOS paths with NFD/NFC considerations
    fn normalize_macos_path(path: &str) -> Result<PathBuf, ShadowError> {
        // Convert to NFC form (precomposed)
        let normalized = EncodingCompat::normalize_unicode(path, UnicodeNormalization::NFC);
        
        // Convert backslashes to forward slashes
        let normalized = normalized.replace('\\', "/");
        
        // Remove duplicate slashes
        let mut result = String::new();
        let mut prev_slash = false;
        for ch in normalized.chars() {
            if ch == '/' {
                if !prev_slash {
                    result.push(ch);
                }
                prev_slash = true;
            } else {
                result.push(ch);
                prev_slash = false;
            }
        }
        
        // Remove trailing slash unless it's the root
        if result.len() > 1 && result.ends_with('/') {
            result.pop();
        }
        
        Ok(PathBuf::from(result))
    }
    
    /// Normalize Linux paths
    fn normalize_linux_path(path: &str) -> Result<PathBuf, ShadowError> {
        // Convert backslashes to forward slashes
        let normalized = path.replace('\\', "/");
        
        // Remove duplicate slashes
        let mut result = String::new();
        let mut prev_slash = false;
        for ch in normalized.chars() {
            if ch == '/' {
                if !prev_slash {
                    result.push(ch);
                }
                prev_slash = true;
            } else {
                result.push(ch);
                prev_slash = false;
            }
        }
        
        // Remove trailing slash unless it's the root
        if result.len() > 1 && result.ends_with('/') {
            result.pop();
        }
        
        Ok(PathBuf::from(result))
    }
    
    /// Convert path separators for the current platform
    pub fn convert_separators(path: &str) -> String {
        match Platform::current() {
            Platform::Windows => path.replace('/', "\\"),
            _ => path.replace('\\', "/"),
        }
    }
    
    /// Check if a path is absolute
    pub fn is_absolute(path: &str) -> bool {
        match Platform::current() {
            Platform::Windows => {
                // Check for drive letter or UNC path
                (path.len() >= 3 && path.as_bytes()[1] == b':' && (path.as_bytes()[2] == b'\\' || path.as_bytes()[2] == b'/'))
                || path.starts_with("\\\\")
                || path.starts_with("//")
            }
            _ => path.starts_with('/'),
        }
    }
    
    /// Join two paths with proper handling of absolute paths
    pub fn join(base: &str, relative: &str) -> String {
        if Self::is_absolute(relative) {
            relative.to_string()
        } else {
            let separator = match Platform::current() {
                Platform::Windows => "\\",
                _ => "/",
            };
            
            let mut result = base.to_string();
            if !result.ends_with(separator) && !result.is_empty() {
                result.push_str(separator);
            }
            result.push_str(relative);
            result
        }
    }
}

/// Permission compatibility functions
pub struct PermissionCompat;

impl PermissionCompat {
    /// Convert Unix mode to Windows attributes
    #[cfg(windows)]
    pub fn unix_to_windows_attrs(mode: u32) -> WindowsAttributes {
        WindowsAttributes {
            readonly: (mode & 0o200) == 0, // No write permission
            hidden: false, // Unix doesn't have hidden attribute
            system: false,
            archive: true, // Default to archive
            temporary: false,
            compressed: false,
            encrypted: false,
        }
    }
    
    /// Convert Unix mode to Windows attributes (non-Windows stub)
    #[cfg(not(windows))]
    pub fn unix_to_windows_attrs(_mode: u32) -> u32 {
        0
    }
    
    /// Convert Windows attributes to Unix mode
    #[cfg(windows)]
    pub fn windows_to_unix_mode(attrs: WindowsAttributes, is_dir: bool) -> u32 {
        let mut mode = if is_dir { 0o755 } else { 0o644 };
        
        if attrs.readonly {
            // Remove write permissions
            mode &= !0o200; // Owner write
            mode &= !0o020; // Group write
            mode &= !0o002; // Other write
        }
        
        mode
    }
    
    /// Convert Windows attributes to Unix mode (non-Windows stub)
    #[cfg(not(windows))]
    pub fn windows_to_unix_mode(_attrs: u32, is_dir: bool) -> u32 {
        if is_dir { 0o755 } else { 0o644 }
    }
    
    /// Convert FilePermissions to platform-specific representation
    pub fn to_platform_perms(perms: &FilePermissions) -> u32 {
        match Platform::current() {
            Platform::Windows => {
                // Windows uses attributes, not Unix permissions
                let mut attrs = 0u32;
                if perms.readonly {
                    attrs |= 0x1; // FILE_ATTRIBUTE_READONLY
                }
                attrs
            }
            _ => perms.to_unix_mode(),
        }
    }
    
    /// Create FilePermissions from platform-specific representation
    pub fn from_platform_perms(value: u32, is_dir: bool) -> FilePermissions {
        match Platform::current() {
            Platform::Windows => {
                // Convert Windows attributes to Unix-like permissions
                let mut mode = if is_dir { 0o755 } else { 0o644 };
                if (value & 0x1) != 0 { // FILE_ATTRIBUTE_READONLY
                    mode &= !0o200; // Remove owner write
                }
                FilePermissions::from_unix_mode(mode)
            }
            _ => FilePermissions::from_unix_mode(value),
        }
    }
}

/// Timestamp compatibility functions
pub struct TimestampCompat;

impl TimestampCompat {
    /// Windows epoch (January 1, 1601)
    const WINDOWS_EPOCH_OFFSET: u64 = 11644473600;
    
    /// Convert Windows FILETIME to Unix timestamp
    pub fn windows_to_unix_timestamp(filetime: u64) -> SystemTime {
        // FILETIME is in 100-nanosecond intervals since 1601
        let seconds = filetime / 10_000_000;
        let nanos = (filetime % 10_000_000) * 100;
        
        if seconds >= Self::WINDOWS_EPOCH_OFFSET {
            let unix_seconds = seconds - Self::WINDOWS_EPOCH_OFFSET;
            UNIX_EPOCH + Duration::from_secs(unix_seconds) + Duration::from_nanos(nanos)
        } else {
            // Time before Unix epoch
            UNIX_EPOCH
        }
    }
    
    /// Convert Unix timestamp to Windows FILETIME
    pub fn unix_to_windows_timestamp(time: SystemTime) -> u64 {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let unix_seconds = duration.as_secs();
                let nanos = duration.subsec_nanos() as u64;
                let windows_seconds = unix_seconds + Self::WINDOWS_EPOCH_OFFSET;
                (windows_seconds * 10_000_000) + (nanos / 100)
            }
            Err(_) => 0, // Time before Unix epoch
        }
    }
    
    /// Normalize timestamp precision across platforms
    pub fn normalize_precision(time: SystemTime) -> SystemTime {
        match Platform::current() {
            Platform::Windows => {
                // Windows has 100ns precision
                match time.duration_since(UNIX_EPOCH) {
                    Ok(duration) => {
                        let nanos = duration.as_nanos();
                        let normalized_nanos = (nanos / 100) * 100;
                        UNIX_EPOCH + Duration::from_nanos(normalized_nanos as u64)
                    }
                    Err(_) => time,
                }
            }
            Platform::MacOS => {
                // macOS typically has microsecond precision
                match time.duration_since(UNIX_EPOCH) {
                    Ok(duration) => {
                        let nanos = duration.as_nanos();
                        let normalized_nanos = (nanos / 1000) * 1000;
                        UNIX_EPOCH + Duration::from_nanos(normalized_nanos as u64)
                    }
                    Err(_) => time,
                }
            }
            Platform::Linux => {
                // Linux typically has nanosecond precision
                time
            }
        }
    }
    
    /// Get current time with platform-appropriate precision
    pub fn now() -> SystemTime {
        Self::normalize_precision(SystemTime::now())
    }
}

/// Error mapping functions
pub struct ErrorCompat;

impl ErrorCompat {
    /// Map platform-specific errors to ShadowError
    pub fn from_io_error(error: std::io::Error) -> ShadowError {
        ShadowError::from(error)
    }
    
    /// Get user-friendly error message
    pub fn get_user_message(error: &ShadowError) -> String {
        use ShadowError::*;
        match error {
            NotFound { path } => {
                format!("File or directory not found: {}", path)
            }
            PermissionDenied { path, operation } => {
                let mut msg = format!("Permission denied for {} on: {}", operation, path);
                if Platform::current() == Platform::Windows {
                    msg.push_str("\nTip: Check if the file is in use or if you need administrator privileges.");
                } else {
                    msg.push_str("\nTip: Check file permissions or try with sudo.");
                }
                msg
            }
            AlreadyExists { path } => {
                format!("File or directory already exists: {}", path)
            }
            OverrideStoreFull { current_size, max_size } => {
                format!("Storage space is full. Current: {} bytes, Max: {} bytes", current_size, max_size)
            }
            InvalidPath { path, reason } => {
                let mut msg = format!("Invalid path '{}': {}", path, reason);
                match Platform::current() {
                    Platform::Windows => {
                        msg.push_str("\nTip: Check for invalid characters like < > : \" | ? *");
                    }
                    _ => {
                        msg.push_str("\nTip: Check for null bytes or invalid UTF-8");
                    }
                }
                msg
            }
            _ => error.to_string(),
        }
    }
    
    /// Map Windows error codes to ShadowError
    #[cfg(windows)]
    pub fn from_windows_error(code: u32) -> ShadowError {
        let message = match code {
            2 | 3 => "File or directory not found",
            5 => "Access denied",
            80 | 183 => "File already exists",
            39 | 112 => "Disk full",
            123 | 161 => "Invalid name or path",
            _ => "Unknown error",
        };
        platform_error(ErrorPlatform::Windows, message, Some(code as i32))
    }
    
    /// Map errno values to ShadowError
    #[cfg(unix)]
    pub fn from_errno(errno: i32) -> ShadowError {
        let (message, platform) = match errno {
            libc::ENOENT => ("No such file or directory", Platform::current()),
            libc::EACCES => ("Permission denied", Platform::current()),
            libc::EPERM => ("Operation not permitted", Platform::current()),
            libc::EEXIST => ("File exists", Platform::current()),
            libc::ENOSPC => ("No space left on device", Platform::current()),
            libc::EDQUOT => ("Disk quota exceeded", Platform::current()),
            libc::EINVAL => ("Invalid argument", Platform::current()),
            libc::ENAMETOOLONG => ("File name too long", Platform::current()),
            libc::EISDIR => ("Is a directory", Platform::current()),
            libc::ENOTDIR => ("Not a directory", Platform::current()),
            _ => ("Unknown error", Platform::current()),
        };
        let error_platform = match platform {
            Platform::Linux => ErrorPlatform::Linux,
            Platform::MacOS => ErrorPlatform::MacOS,
            _ => ErrorPlatform::Linux,
        };
        platform_error(error_platform, message, Some(errno))
    }
}

/// Text encoding compatibility
pub struct EncodingCompat;

/// Unicode normalization forms
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnicodeNormalization {
    /// Canonical Decomposition
    NFD,
    /// Canonical Decomposition, followed by Canonical Composition
    NFC,
}

impl EncodingCompat {
    /// Convert between UTF-16 (Windows) and UTF-8
    #[cfg(windows)]
    pub fn os_str_to_utf8(s: &OsStr) -> Result<String, ShadowError> {
        // Get UTF-16 representation
        let wide: Vec<u16> = s.encode_wide().collect();
        
        // Convert to UTF-8
        String::from_utf16(&wide).map_err(|e| {
            invalid_path(s.to_string_lossy(), format!("Invalid UTF-16 sequence: {}", e))
        })
    }
    
    /// Convert between UTF-16 (Windows) and UTF-8 (non-Windows)
    #[cfg(not(windows))]
    pub fn os_str_to_utf8(s: &OsStr) -> Result<String, ShadowError> {
        s.to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                invalid_path(s.to_string_lossy(), "Invalid UTF-8 sequence")
            })
    }
    
    /// Convert UTF-8 to OsString
    pub fn utf8_to_os_string(s: &str) -> OsString {
        OsString::from(s)
    }
    
    /// Handle invalid UTF-8 sequences on Unix
    #[cfg(unix)]
    pub fn handle_invalid_utf8(bytes: &[u8]) -> String {
        // Use Unicode replacement character for invalid sequences
        String::from_utf8_lossy(bytes).into_owned()
    }
    
    /// Handle invalid UTF-8 sequences (non-Unix)
    #[cfg(not(unix))]
    pub fn handle_invalid_utf8(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }
    
    /// Normalize Unicode representation
    pub fn normalize_unicode(s: &str, form: UnicodeNormalization) -> String {
        // Simplified normalization - in production, use unicode-normalization crate
        match form {
            UnicodeNormalization::NFC => {
                // For now, just return as-is
                // In production: unicode_normalization::nfc(s).collect()
                s.to_string()
            }
            UnicodeNormalization::NFD => {
                // For now, just return as-is
                // In production: unicode_normalization::nfd(s).collect()
                s.to_string()
            }
        }
    }
    
    /// Check if a string needs normalization
    pub fn needs_normalization(s: &str) -> bool {
        // Check for common combining characters
        s.chars().any(|c| {
            matches!(c, '\u{0300}'..='\u{036F}' | '\u{1DC0}'..='\u{1DFF}')
        })
    }
}

/// Platform-specific path utilities
pub struct PathUtils;

impl PathUtils {
    /// Get the path separator for the current platform
    pub fn separator() -> char {
        match Platform::current() {
            Platform::Windows => '\\',
            _ => '/',
        }
    }
    
    /// Check if a character is a path separator
    pub fn is_separator(c: char) -> bool {
        match Platform::current() {
            Platform::Windows => c == '\\' || c == '/',
            _ => c == '/',
        }
    }
    
    /// Split a path into components
    pub fn split_path(path: &str) -> Vec<&str> {
        path.split(|c| Self::is_separator(c))
            .filter(|s| !s.is_empty())
            .collect()
    }
    
    /// Get the file name from a path
    pub fn file_name(path: &str) -> Option<&str> {
        path.rsplit(|c| Self::is_separator(c)).next()
    }
    
    /// Get the parent directory of a path
    pub fn parent(path: &str) -> Option<&str> {
        let trimmed = path.trim_end_matches(|c| Self::is_separator(c));
        if let Some(pos) = trimmed.rfind(|c| Self::is_separator(c)) {
            Some(&trimmed[..pos])
        } else {
            None
        }
    }
    
    /// Check if a filename is valid for the platform
    pub fn is_valid_filename(name: &str) -> bool {
        if name.is_empty() || name == "." || name == ".." {
            return false;
        }
        
        match Platform::current() {
            Platform::Windows => {
                // Windows forbidden characters
                !name.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0'))
                    && !name.ends_with('.') 
                    && !name.ends_with(' ')
                    && !matches!(
                        name.to_uppercase().as_str(),
                        "CON" | "PRN" | "AUX" | "NUL" |
                        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | 
                        "COM6" | "COM7" | "COM8" | "COM9" |
                        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | 
                        "LPT6" | "LPT7" | "LPT8" | "LPT9"
                    )
            }
            _ => {
                // Unix-like: no null bytes or slashes
                !name.chars().any(|c| c == '\0' || Self::is_separator(c))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_path_normalization() {
        // Test basic normalization
        let result = PathCompat::normalize_path("/path/to/file").unwrap();
        assert!(result.to_string().contains("file"));
        
        // Test empty path
        assert!(PathCompat::normalize_path("").is_err());
        
        // Test path with mixed separators
        let result = PathCompat::normalize_path("/path\\to/file").unwrap();
        let normalized = result.to_string();
        assert!(!normalized.contains("\\\\"));
    }
    
    #[test]
    fn test_separator_conversion() {
        let path = "path/to/file";
        let converted = PathCompat::convert_separators(path);
        
        #[cfg(windows)]
        assert_eq!(converted, "path\\to\\file");
        
        #[cfg(not(windows))]
        assert_eq!(converted, "path/to/file");
    }
    
    #[test]
    fn test_absolute_path_detection() {
        #[cfg(windows)]
        {
            assert!(PathCompat::is_absolute("C:\\path"));
            assert!(PathCompat::is_absolute("\\\\server\\share"));
            assert!(!PathCompat::is_absolute("path\\to\\file"));
        }
        
        #[cfg(not(windows))]
        {
            assert!(PathCompat::is_absolute("/path"));
            assert!(!PathCompat::is_absolute("path/to/file"));
        }
    }
    
    #[test]
    fn test_path_join() {
        let base = "/base";
        let relative = "relative";
        let result = PathCompat::join(base, relative);
        
        #[cfg(not(windows))]
        assert_eq!(result, "/base/relative");
        
        // Test with absolute relative path
        let absolute = "/absolute";
        let result = PathCompat::join(base, absolute);
        assert_eq!(result, absolute);
    }
    
    #[test]
    fn test_permission_conversion() {
        // Test read-only conversion
        let perms = FilePermissions::from_unix_mode(0o444);
        let platform_perms = PermissionCompat::to_platform_perms(&perms);
        
        #[cfg(windows)]
        assert_eq!(platform_perms & 0x1, 0x1); // Should have readonly attribute
        
        #[cfg(not(windows))]
        assert_eq!(platform_perms, 0o444);
    }
    
    #[test]
    fn test_timestamp_precision() {
        let now = SystemTime::now();
        let normalized = TimestampCompat::normalize_precision(now);
        
        // Normalized time should be close to original
        let diff = now.duration_since(normalized).or_else(|_| normalized.duration_since(now));
        assert!(diff.unwrap().as_nanos() < 1_000_000); // Less than 1ms difference
    }
    
    #[test]
    fn test_error_mapping() {
        use std::io;
        
        let io_error = io::Error::new(io::ErrorKind::NotFound, "test");
        let shadow_error = ErrorCompat::from_io_error(io_error);
        assert!(matches!(shadow_error, ShadowError::IoError { .. }));
    }
    
    #[test]
    fn test_user_friendly_messages() {
        let path = ShadowPath::from("/test.txt");
        let error = ShadowError::PermissionDenied { path, operation: "write".to_string() };
        let message = ErrorCompat::get_user_message(&error);
        assert!(message.contains("Permission denied"));
        assert!(message.contains("Tip:"));
    }
    
    #[test]
    fn test_encoding_conversion() {
        let test_str = "Hello, 世界!";
        let os_string = EncodingCompat::utf8_to_os_string(test_str);
        let result = EncodingCompat::os_str_to_utf8(&os_string).unwrap();
        assert_eq!(result, test_str);
    }
    
    #[test]
    fn test_invalid_utf8_handling() {
        let invalid = vec![0xFF, 0xFE, 0x00];
        let result = EncodingCompat::handle_invalid_utf8(&invalid);
        assert!(result.contains('\u{FFFD}')); // Replacement character
    }
    
    #[test]
    fn test_path_utils() {
        let path = "path/to/file.txt";
        
        assert_eq!(PathUtils::file_name(path), Some("file.txt"));
        assert_eq!(PathUtils::parent(path), Some("path/to"));
        
        let components = PathUtils::split_path(path);
        assert_eq!(components, vec!["path", "to", "file.txt"]);
    }
    
    #[test]
    fn test_filename_validation() {
        assert!(PathUtils::is_valid_filename("file.txt"));
        assert!(!PathUtils::is_valid_filename(""));
        assert!(!PathUtils::is_valid_filename("."));
        assert!(!PathUtils::is_valid_filename(".."));
        
        #[cfg(windows)]
        {
            assert!(!PathUtils::is_valid_filename("file?.txt"));
            assert!(!PathUtils::is_valid_filename("CON"));
            assert!(!PathUtils::is_valid_filename("file."));
        }
        
        assert!(!PathUtils::is_valid_filename("file\0name"));
    }
    
    #[test]
    fn test_normalization_check() {
        assert!(!EncodingCompat::needs_normalization("simple"));
        assert!(EncodingCompat::needs_normalization("e\u{0301}")); // e with combining acute
    }
}