use std::path::{Path, PathBuf};
use std::fs;
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{S_OK, ERROR_ALREADY_EXISTS, WIN32_ERROR};
use windows::Win32::Storage::ProjectedFileSystem::{
    PrjMarkDirectoryAsPlaceholder,
    PrjUpdateFileIfNeeded,
    PRJ_UPDATE_TYPES,
};
use windows::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOEXW};
use crate::error::WindowsError;
use log::{info, warn, error, debug};

/// Options for initializing a virtualization root
#[derive(Debug, Clone)]
pub struct InitializationOptions {
    /// Whether to clean existing content in the directory
    pub clean_existing: bool,
    
    /// Whether to force clean (remove directories too)
    pub force_clean: bool,
    
    /// Whether to validate the directory before initialization
    pub validate_directory: bool,
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self {
            clean_existing: false,
            force_clean: false,
            validate_directory: true,
        }
    }
}

/// Manages the virtualization root directory for ProjFS
pub struct VirtualizationRoot {
    /// Path to the virtualization root
    path: PathBuf,
    
    /// Whether the root has been initialized
    initialized: bool,
}

impl VirtualizationRoot {
    /// Creates a new VirtualizationRoot instance
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            initialized: false,
        }
    }
    
    /// Gets the path of the virtualization root
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Checks if the virtualization root is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    
    /// Verifies that ProjFS is available on the system
    ///
    /// # Returns
    /// * `Ok(())` - ProjFS is available
    /// * `Err(WindowsError)` - ProjFS is not available or check failed
    pub fn verify_projfs_available() -> Result<(), WindowsError> {
        info!("Verifying ProjFS availability...");
        
        // Check Windows version (ProjFS requires Windows 10 1809 or later)
        if !Self::is_windows_version_supported()? {
            return Err(WindowsError::Unsupported {
                message: "ProjFS requires Windows 10 version 1809 or later".to_string(),
            });
        }
        
        // Check if ProjFS feature is enabled
        // In Windows 10, ProjFS is an optional feature that needs to be enabled
        // We try to use a ProjFS function to verify it's available
        Self::test_projfs_api()?;
        
        info!("ProjFS is available");
        Ok(())
    }
    
    /// Checks if the Windows version supports ProjFS
    fn is_windows_version_supported() -> Result<bool, WindowsError> {
        unsafe {
            let mut version_info: OSVERSIONINFOEXW = std::mem::zeroed();
            version_info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOEXW>() as u32;
            
            if GetVersionExW(&mut version_info as *mut _ as *mut _).as_bool() {
                // Windows 10 is version 10.0
                // Build 17763 is version 1809 (minimum for ProjFS)
                if version_info.dwMajorVersion > 10 {
                    return Ok(true);
                }
                if version_info.dwMajorVersion == 10 && version_info.dwBuildNumber >= 17763 {
                    return Ok(true);
                }
                
                debug!(
                    "Windows version {}.{} build {} detected",
                    version_info.dwMajorVersion,
                    version_info.dwMinorVersion,
                    version_info.dwBuildNumber
                );
                Ok(false)
            } else {
                Err(WindowsError::InvalidOperation {
                    message: "Failed to get Windows version".to_string(),
                })
            }
        }
    }
    
    /// Tests if ProjFS API is accessible
    fn test_projfs_api() -> Result<(), WindowsError> {
        // We'll try to mark a non-existent path as placeholder
        // This should fail with a specific error if ProjFS is available
        let test_path = r"C:\__projfs_test_path_that_should_not_exist__";
        let wide_path: Vec<u16> = test_path.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let result = PrjMarkDirectoryAsPlaceholder(
                PCWSTR::from_raw(wide_path.as_ptr()),
                None,
                None,
                None,
            );
            
            // We expect this to fail because the path doesn't exist
            // But if ProjFS is not available, we'll get a different error
            if result == HRESULT::from(WIN32_ERROR(2)) {  // ERROR_FILE_NOT_FOUND
                // This is expected - ProjFS is available but path doesn't exist
                return Ok(());
            }
            
            // If we get ERROR_NOT_SUPPORTED or similar, ProjFS is not available
            if result == HRESULT::from(WIN32_ERROR(50)) {  // ERROR_NOT_SUPPORTED
                return Err(WindowsError::Unsupported {
                    message: "ProjFS is not enabled. Please enable Windows Projected File System feature.".to_string(),
                });
            }
            
            // For other errors, assume ProjFS is available
            // (the actual operation failed for other reasons)
            Ok(())
        }
    }
    
    /// Performs the complete initialization sequence for a virtualization root
    ///
    /// This includes:
    /// 1. Verifying ProjFS is available
    /// 2. Creating the root directory
    /// 3. Converting it to a placeholder
    ///
    /// # Arguments
    /// * `path` - Path where the virtualization root should be initialized
    /// * `options` - Initialization options
    ///
    /// # Returns
    /// * `Ok(VirtualizationRoot)` - Successfully initialized
    /// * `Err(WindowsError)` - Initialization failed
    pub fn initialize(
        path: PathBuf,
        options: InitializationOptions,
    ) -> Result<Self, WindowsError> {
        info!("Starting virtualization root initialization at: {}", path.display());
        
        // Step 1: Verify ProjFS is available
        Self::verify_projfs_available()?;
        
        // Step 2: Handle existing content if needed
        if path.exists() && options.clean_existing {
            warn!("Cleaning existing content at {}", path.display());
            Self::cleanup_existing_content(&path, options.force_clean)?;
        }
        
        // Step 3: Create and prepare the virtualization root
        let root = Self::create_root(path)?;
        
        info!("Virtualization root initialization complete");
        Ok(root)
    }
    
    /// Creates and prepares a virtualization root directory
    ///
    /// This function:
    /// 1. Creates the directory if it doesn't exist
    /// 2. Validates the directory is suitable for virtualization
    /// 3. Marks it as a placeholder for ProjFS
    ///
    /// # Arguments
    /// * `path` - Path where the virtualization root should be created
    ///
    /// # Returns
    /// * `Ok(VirtualizationRoot)` - Successfully created virtualization root
    /// * `Err(WindowsError)` - If creation or validation failed
    pub fn create_root(path: PathBuf) -> Result<Self, WindowsError> {
        info!("Creating virtualization root at: {}", path.display());
        
        // Create the directory if it doesn't exist
        if !path.exists() {
            debug!("Creating directory: {}", path.display());
            fs::create_dir_all(&path).map_err(|e| {
                error!("Failed to create directory {}: {}", path.display(), e);
                WindowsError::IoError {
                    message: format!("Failed to create virtualization root directory: {}", e),
                    code: e.raw_os_error().unwrap_or(0) as u32,
                }
            })?;
        }
        
        // Check if the directory is suitable for virtualization
        Self::validate_directory(&path)?;
        
        // Mark the directory as a placeholder
        Self::mark_as_placeholder(&path)?;
        
        let mut root = Self::new(path);
        root.initialized = true;
        
        info!("Virtualization root created successfully");
        Ok(root)
    }
    
    /// Validates that a directory is suitable for use as a virtualization root
    ///
    /// # Arguments
    /// * `path` - Path to validate
    ///
    /// # Returns
    /// * `Ok(())` - Directory is suitable
    /// * `Err(WindowsError)` - Directory is not suitable
    fn validate_directory(path: &Path) -> Result<(), WindowsError> {
        debug!("Validating directory: {}", path.display());
        
        // Check if path exists and is a directory
        let metadata = fs::metadata(path).map_err(|e| {
            WindowsError::IoError {
                message: format!("Failed to get metadata for {}: {}", path.display(), e),
                code: e.raw_os_error().unwrap_or(0) as u32,
            }
        })?;
        
        if !metadata.is_dir() {
            return Err(WindowsError::InvalidOperation {
                message: format!("Path {} is not a directory", path.display()),
            });
        }
        
        // Check if directory is empty or only contains compatible content
        let entries = fs::read_dir(path).map_err(|e| {
            WindowsError::IoError {
                message: format!("Failed to read directory {}: {}", path.display(), e),
                code: e.raw_os_error().unwrap_or(0) as u32,
            }
        })?;
        
        let entry_count = entries.count();
        if entry_count > 0 {
            warn!(
                "Directory {} is not empty ({} entries). Existing content may interfere with virtualization.",
                path.display(),
                entry_count
            );
            
            // We'll allow non-empty directories but warn about them
            // In production, you might want to be more strict
        }
        
        debug!("Directory validation successful");
        Ok(())
    }
    
    /// Marks a directory as a ProjFS placeholder
    ///
    /// # Arguments
    /// * `path` - Path to mark as placeholder
    ///
    /// # Returns
    /// * `Ok(())` - Successfully marked as placeholder
    /// * `Err(WindowsError)` - Failed to mark as placeholder
    pub fn mark_as_placeholder(path: &Path) -> Result<(), WindowsError> {
        debug!("Marking directory as placeholder: {}", path.display());
        
        // Convert path to wide string for Windows API
        let path_str = path.to_str().ok_or_else(|| WindowsError::InvalidOperation {
            message: format!("Path contains invalid UTF-8: {}", path.display()),
        })?;
        
        let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let result = PrjMarkDirectoryAsPlaceholder(
                PCWSTR::from_raw(wide_path.as_ptr()),
                None,  // No specific target path
                None,  // No specific version info
                None,  // No specific virtualization instance ID
            );
            
            if result.is_err() {
                // Special handling for already initialized directories
                if result == HRESULT::from(WIN32_ERROR(ERROR_ALREADY_EXISTS.0)) {
                    debug!("Directory is already marked as placeholder");
                    return Ok(());
                }
                
                error!("Failed to mark directory as placeholder: {:?}", result);
                return Err(WindowsError::ProjFSError {
                    message: format!("Failed to mark directory as placeholder"),
                    hresult: result.0,
                });
            }
        }
        
        info!("Successfully marked {} as ProjFS placeholder", path.display());
        Ok(())
    }
    
    /// Cleans up existing content in a directory to prepare it for virtualization
    ///
    /// # Arguments
    /// * `path` - Path to clean
    /// * `force` - If true, forcefully removes all content
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of items cleaned
    /// * `Err(WindowsError)` - If cleanup failed
    pub fn cleanup_existing_content(path: &Path, force: bool) -> Result<usize, WindowsError> {
        info!("Cleaning up existing content in: {} (force={})", path.display(), force);
        
        let entries = fs::read_dir(path).map_err(|e| {
            WindowsError::IoError {
                message: format!("Failed to read directory for cleanup: {}", e),
                code: e.raw_os_error().unwrap_or(0) as u32,
            }
        })?;
        
        let mut cleaned_count = 0;
        let mut errors = Vec::new();
        
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    errors.push(format!("Failed to read entry: {}", e));
                    continue;
                }
            };
            
            let entry_path = entry.path();
            let entry_name = entry.file_name();
            
            // Skip special directories
            if entry_name == "." || entry_name == ".." {
                continue;
            }
            
            // Skip .git directories unless force is specified
            if !force && entry_name == ".git" {
                debug!("Skipping .git directory");
                continue;
            }
            
            // Try to remove the entry
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => {
                    if force {
                        match fs::remove_dir_all(&entry_path) {
                            Ok(_) => {
                                debug!("Removed directory: {}", entry_path.display());
                                cleaned_count += 1;
                            }
                            Err(e) => {
                                errors.push(format!("Failed to remove directory {}: {}", entry_path.display(), e));
                            }
                        }
                    } else {
                        warn!("Skipping non-empty directory: {} (use force to remove)", entry_path.display());
                    }
                }
                Ok(_) => {
                    // It's a file
                    match fs::remove_file(&entry_path) {
                        Ok(_) => {
                            debug!("Removed file: {}", entry_path.display());
                            cleaned_count += 1;
                        }
                        Err(e) => {
                            errors.push(format!("Failed to remove file {}: {}", entry_path.display(), e));
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("Failed to get file type for {}: {}", entry_path.display(), e));
                }
            }
        }
        
        if !errors.is_empty() {
            warn!("Cleanup completed with {} errors:", errors.len());
            for error in &errors {
                warn!("  - {}", error);
            }
        }
        
        info!("Cleaned up {} items", cleaned_count);
        Ok(cleaned_count)
    }
    
    /// Checks if a path can be virtualized
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// * `Ok(true)` - Path can be virtualized
    /// * `Ok(false)` - Path cannot be virtualized
    /// * `Err(WindowsError)` - Error checking path
    pub fn can_virtualize(path: &Path) -> Result<bool, WindowsError> {
        // Check if path exists
        if !path.exists() {
            return Ok(true); // Non-existent paths can be created and virtualized
        }
        
        // Check if it's a directory
        let metadata = fs::metadata(path).map_err(|e| {
            WindowsError::IoError {
                message: format!("Failed to get metadata: {}", e),
                code: e.raw_os_error().unwrap_or(0) as u32,
            }
        })?;
        
        if !metadata.is_dir() {
            return Ok(false); // Only directories can be virtualization roots
        }
        
        // Check for problematic content
        // In a real implementation, you might check for:
        // - Junction points or symbolic links
        // - System files
        // - Files with special attributes
        
        Ok(true)
    }
    
    /// Updates file state in the virtualization root
    ///
    /// # Arguments
    /// * `path` - Path to the file to update
    /// * `update_flags` - Flags controlling what to update
    ///
    /// # Returns
    /// * `Ok(())` - Successfully updated
    /// * `Err(WindowsError)` - Update failed
    pub fn update_file_state(&self, path: &Path, update_flags: PRJ_UPDATE_TYPES) -> Result<(), WindowsError> {
        if !self.initialized {
            return Err(WindowsError::InvalidOperation {
                message: "Virtualization root not initialized".to_string(),
            });
        }
        
        let full_path = self.path.join(path);
        let path_str = full_path.to_str().ok_or_else(|| WindowsError::InvalidOperation {
            message: format!("Path contains invalid UTF-8: {}", full_path.display()),
        })?;
        
        let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let result = PrjUpdateFileIfNeeded(
                PCWSTR::from_raw(wide_path.as_ptr()),
                None,  // No placeholder info
                0,     // Placeholder info size
                update_flags,
                None,  // No failure reason
            );
            
            if result.is_err() {
                let err_code = unsafe { windows::core::Error::from_win32() };
                return Err(WindowsError::ProjFSError {
                    message: format!("Failed to update file state for {}", path.display()),
                    hresult: err_code.code().0,
                });
            }
        }
        
        Ok(())
    }
    
    /// Deletes a file from the virtualization root
    ///
    /// NOTE: PrjDeleteFile API requires a namespace virtualization context
    /// which is only available within callback context. This is a limitation
    /// of the ProjFS API. Use standard file system operations instead.
    ///
    /// # Arguments
    /// * `path` - Path to the file to delete
    ///
    /// # Returns
    /// * `Ok(())` - Successfully deleted
    /// * `Err(WindowsError)` - Deletion failed
    pub fn delete_file(&self, path: &Path) -> Result<(), WindowsError> {
        if !self.initialized {
            return Err(WindowsError::InvalidOperation {
                message: "Virtualization root not initialized".to_string(),
            });
        }
        
        let full_path = self.path.join(path);
        
        // Use standard file system delete instead of PrjDeleteFile
        // since PrjDeleteFile requires a namespace context from callbacks
        fs::remove_file(&full_path).map_err(|e| {
            WindowsError::IoError {
                message: format!("Failed to delete file {}: {}", full_path.display(), e),
                code: e.raw_os_error().unwrap_or(0) as u32,
            }
        })?;
        
        Ok(())
    }
}

impl Drop for VirtualizationRoot {
    fn drop(&mut self) {
        if self.initialized {
            debug!("Dropping VirtualizationRoot for {}", self.path.display());
            // In a real implementation, you might want to perform cleanup here
            // such as stopping any active virtualization on this root
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_virtualization_root_creation() {
        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path().join("virt_root");
        
        let root = VirtualizationRoot::create_root(root_path.clone());
        assert!(root.is_ok());
        
        let root = root.unwrap();
        assert_eq!(root.path(), &root_path);
        assert!(root.is_initialized());
    }
    
    #[test]
    fn test_can_virtualize() {
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path().join("test_dir");
        fs::create_dir(&dir_path).unwrap();
        
        assert!(VirtualizationRoot::can_virtualize(&dir_path).unwrap());
        
        let file_path = temp_dir.path().join("test_file.txt");
        fs::write(&file_path, "test content").unwrap();
        
        assert!(!VirtualizationRoot::can_virtualize(&file_path).unwrap());
    }
    
    #[test]
    fn test_cleanup_existing_content() {
        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path().join("to_clean");
        fs::create_dir(&root_path).unwrap();
        
        // Create some test content
        fs::write(root_path.join("file1.txt"), "content1").unwrap();
        fs::write(root_path.join("file2.txt"), "content2").unwrap();
        fs::create_dir(root_path.join("subdir")).unwrap();
        
        let cleaned = VirtualizationRoot::cleanup_existing_content(&root_path, false).unwrap();
        assert_eq!(cleaned, 2); // Should clean files but not directories without force
        
        let cleaned = VirtualizationRoot::cleanup_existing_content(&root_path, true).unwrap();
        assert_eq!(cleaned, 1); // Should clean the remaining directory with force
    }
}
