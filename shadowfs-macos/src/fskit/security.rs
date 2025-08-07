use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use libc::{gid_t, uid_t, mode_t};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::Result;

/// POSIX permission bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosixMode(mode_t);

impl PosixMode {
    pub const S_IRUSR: mode_t = 0o400;
    pub const S_IWUSR: mode_t = 0o200;
    pub const S_IXUSR: mode_t = 0o100;
    pub const S_IRGRP: mode_t = 0o040;
    pub const S_IWGRP: mode_t = 0o020;
    pub const S_IXGRP: mode_t = 0o010;
    pub const S_IROTH: mode_t = 0o004;
    pub const S_IWOTH: mode_t = 0o002;
    pub const S_IXOTH: mode_t = 0o001;
    
    pub const S_ISUID: mode_t = 0o4000;
    pub const S_ISGID: mode_t = 0o2000;
    pub const S_ISVTX: mode_t = 0o1000;
    
    pub fn new(mode: mode_t) -> Self {
        Self(mode)
    }
    
    pub fn as_raw(&self) -> mode_t {
        self.0
    }
    
    pub fn user_read(&self) -> bool {
        self.0 & Self::S_IRUSR != 0
    }
    
    pub fn user_write(&self) -> bool {
        self.0 & Self::S_IWUSR != 0
    }
    
    pub fn user_execute(&self) -> bool {
        self.0 & Self::S_IXUSR != 0
    }
    
    pub fn group_read(&self) -> bool {
        self.0 & Self::S_IRGRP != 0
    }
    
    pub fn group_write(&self) -> bool {
        self.0 & Self::S_IWGRP != 0
    }
    
    pub fn group_execute(&self) -> bool {
        self.0 & Self::S_IXGRP != 0
    }
    
    pub fn other_read(&self) -> bool {
        self.0 & Self::S_IROTH != 0
    }
    
    pub fn other_write(&self) -> bool {
        self.0 & Self::S_IWOTH != 0
    }
    
    pub fn other_execute(&self) -> bool {
        self.0 & Self::S_IXOTH != 0
    }
    
    pub fn setuid(&self) -> bool {
        self.0 & Self::S_ISUID != 0
    }
    
    pub fn setgid(&self) -> bool {
        self.0 & Self::S_ISGID != 0
    }
    
    pub fn sticky(&self) -> bool {
        self.0 & Self::S_ISVTX != 0
    }
}

/// FSKit permission flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSKitPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub delete: bool,
    pub append: bool,
    pub read_attributes: bool,
    pub write_attributes: bool,
    pub read_security: bool,
    pub write_security: bool,
}

impl Default for FSKitPermissions {
    fn default() -> Self {
        Self {
            read: false,
            write: false,
            execute: false,
            delete: false,
            append: false,
            read_attributes: true,
            write_attributes: false,
            read_security: true,
            write_security: false,
        }
    }
}

/// Permission translator between POSIX and FSKit
pub struct PermissionTranslator;

impl PermissionTranslator {
    /// Convert POSIX mode to FSKit permissions
    pub fn posix_to_fskit(
        mode: PosixMode,
        uid: uid_t,
        gid: gid_t,
        current_uid: uid_t,
        current_gid: gid_t,
        groups: &[gid_t],
    ) -> FSKitPermissions {
        let mut perms = FSKitPermissions::default();
        
        // Check owner permissions
        if current_uid == uid {
            perms.read = mode.user_read();
            perms.write = mode.user_write();
            perms.append = mode.user_write();
            perms.execute = mode.user_execute();
            perms.delete = mode.user_write();
            perms.write_attributes = mode.user_write();
            perms.write_security = true;
        }
        // Check group permissions
        else if current_gid == gid || groups.contains(&gid) {
            perms.read = mode.group_read();
            perms.write = mode.group_write();
            perms.append = mode.group_write();
            perms.execute = mode.group_execute();
            perms.delete = mode.group_write();
            perms.write_attributes = mode.group_write();
        }
        // Check other permissions
        else {
            perms.read = mode.other_read();
            perms.write = mode.other_write();
            perms.append = mode.other_write();
            perms.execute = mode.other_execute();
            perms.delete = mode.other_write();
            perms.write_attributes = mode.other_write();
        }
        
        // Root has all permissions
        if current_uid == 0 {
            perms.read = true;
            perms.write = true;
            perms.execute = true;
            perms.delete = true;
            perms.append = true;
            perms.read_attributes = true;
            perms.write_attributes = true;
            perms.read_security = true;
            perms.write_security = true;
        }
        
        perms
    }
    
    /// Convert FSKit permissions to POSIX mode
    pub fn fskit_to_posix(perms: &FSKitPermissions, base_mode: mode_t) -> mode_t {
        let mut mode = base_mode & 0o7000; // Preserve special bits
        
        if perms.read {
            mode |= PosixMode::S_IRUSR | PosixMode::S_IRGRP | PosixMode::S_IROTH;
        }
        if perms.write {
            mode |= PosixMode::S_IWUSR | PosixMode::S_IWGRP | PosixMode::S_IWOTH;
        }
        if perms.execute {
            mode |= PosixMode::S_IXUSR | PosixMode::S_IXGRP | PosixMode::S_IXOTH;
        }
        
        mode
    }
}

/// Access Control List entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACLEntry {
    pub principal: ACLPrincipal,
    pub permissions: FSKitPermissions,
    pub flags: ACLFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ACLPrincipal {
    User(uid_t),
    Group(gid_t),
    Everyone,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ACLFlags {
    pub inherited: bool,
    pub no_propagate: bool,
    pub inherit_only: bool,
}

/// ACL manager for extended permissions
pub struct ACLManager {
    acls: Arc<RwLock<HashMap<PathBuf, Vec<ACLEntry>>>>,
}

impl ACLManager {
    pub fn new() -> Self {
        Self {
            acls: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn set_acl(&self, path: PathBuf, entries: Vec<ACLEntry>) {
        self.acls.write().unwrap().insert(path, entries);
    }
    
    pub fn get_acl(&self, path: &Path) -> Option<Vec<ACLEntry>> {
        self.acls.read().unwrap().get(path).cloned()
    }
    
    pub fn check_acl_permission(
        &self,
        path: &Path,
        uid: uid_t,
        gid: gid_t,
        groups: &[gid_t],
    ) -> FSKitPermissions {
        let mut perms = FSKitPermissions::default();
        
        if let Some(entries) = self.get_acl(path) {
            for entry in entries {
                let matches = match entry.principal {
                    ACLPrincipal::User(acl_uid) => acl_uid == uid,
                    ACLPrincipal::Group(acl_gid) => gid == acl_gid || groups.contains(&acl_gid),
                    ACLPrincipal::Everyone => true,
                };
                
                if matches {
                    perms.read |= entry.permissions.read;
                    perms.write |= entry.permissions.write;
                    perms.execute |= entry.permissions.execute;
                    perms.delete |= entry.permissions.delete;
                    perms.append |= entry.permissions.append;
                    perms.read_attributes |= entry.permissions.read_attributes;
                    perms.write_attributes |= entry.permissions.write_attributes;
                    perms.read_security |= entry.permissions.read_security;
                    perms.write_security |= entry.permissions.write_security;
                }
            }
        }
        
        perms
    }
}

/// App Sandbox entitlements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEntitlements {
    pub com_apple_security_app_sandbox: bool,
    pub com_apple_security_files_user_selected_read_write: bool,
    pub com_apple_security_files_downloads_read_write: bool,
    pub com_apple_security_files_user_selected_executable: bool,
    pub com_apple_security_temporary_exception_files: Vec<String>,
    pub com_apple_security_network_client: bool,
    pub com_apple_security_network_server: bool,
}

impl Default for SandboxEntitlements {
    fn default() -> Self {
        Self {
            com_apple_security_app_sandbox: true,
            com_apple_security_files_user_selected_read_write: true,
            com_apple_security_files_downloads_read_write: false,
            com_apple_security_files_user_selected_executable: false,
            com_apple_security_temporary_exception_files: Vec::new(),
            com_apple_security_network_client: false,
            com_apple_security_network_server: false,
        }
    }
}

/// Security-scoped bookmark manager
pub struct BookmarkManager {
    bookmarks: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

impl BookmarkManager {
    pub fn new() -> Self {
        Self {
            bookmarks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn create_bookmark(&self, path: PathBuf) -> Result<Vec<u8>> {
        // In real implementation, this would call macOS Security framework
        let bookmark = format!("bookmark:{}", path.display()).into_bytes();
        self.bookmarks.write().unwrap().insert(path, bookmark.clone());
        Ok(bookmark)
    }
    
    pub fn resolve_bookmark(&self, bookmark: &[u8]) -> Result<PathBuf> {
        let bookmark_str = String::from_utf8_lossy(bookmark);
        if let Some(path_str) = bookmark_str.strip_prefix("bookmark:") {
            Ok(PathBuf::from(path_str))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid bookmark",
            ).into())
        }
    }
    
    pub fn start_accessing(&self, path: &Path) -> Result<ScopedAccess> {
        if self.bookmarks.read().unwrap().contains_key(path) {
            Ok(ScopedAccess::new(path.to_path_buf()))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "No bookmark for path",
            ).into())
        }
    }
}

/// Scoped access token for sandboxed resources
pub struct ScopedAccess {
    path: PathBuf,
    active: bool,
}

impl ScopedAccess {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            active: true,
        }
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
}

impl Drop for ScopedAccess {
    fn drop(&mut self) {
        if self.active {
            // In real implementation, would call stopAccessingSecurityScopedResource
            info!("Stopped accessing security-scoped resource: {:?}", self.path);
        }
    }
}

/// Code signature verifier
pub struct CodeSignatureVerifier;

impl CodeSignatureVerifier {
    /// Verify FSKit extension signature
    pub fn verify_extension(bundle_path: &Path) -> Result<SignatureInfo> {
        // In real implementation, would use codesign APIs
        let info = SignatureInfo {
            team_id: "EXAMPLE123".to_string(),
            bundle_id: "com.shadowfs.fskit-extension".to_string(),
            signing_certificate: "Developer ID Application".to_string(),
            notarized: true,
            hardened_runtime: true,
            timestamp: SystemTime::now(),
        };
        
        if !info.notarized {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Extension not notarized",
            ).into());
        }
        
        Ok(info)
    }
    
    /// Check Gatekeeper assessment
    pub fn check_gatekeeper(bundle_path: &Path) -> Result<bool> {
        // In real implementation, would use spctl or SecAssessment APIs
        Ok(true)
    }
    
    /// Verify provisioning profile
    pub fn verify_provisioning_profile(bundle_path: &Path) -> Result<ProvisioningProfile> {
        Ok(ProvisioningProfile {
            app_id: "com.shadowfs.fskit-extension".to_string(),
            team_id: "EXAMPLE123".to_string(),
            entitlements: SandboxEntitlements::default(),
            expiration: SystemTime::now() + std::time::Duration::from_secs(365 * 24 * 3600),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub team_id: String,
    pub bundle_id: String,
    pub signing_certificate: String,
    pub notarized: bool,
    pub hardened_runtime: bool,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone)]
pub struct ProvisioningProfile {
    pub app_id: String,
    pub team_id: String,
    pub entitlements: SandboxEntitlements,
    pub expiration: SystemTime,
}

/// User context manager
pub struct UserContext {
    current_uid: uid_t,
    current_gid: gid_t,
    effective_uid: uid_t,
    effective_gid: gid_t,
    groups: Vec<gid_t>,
    username: String,
}

impl UserContext {
    /// Get current user context
    pub fn current() -> Result<Self> {
        unsafe {
            let uid = libc::getuid();
            let gid = libc::getgid();
            let euid = libc::geteuid();
            let egid = libc::getegid();
            
            // Get username
            let passwd = libc::getpwuid(uid);
            let username = if !passwd.is_null() {
                CStr::from_ptr((*passwd).pw_name)
                    .to_string_lossy()
                    .to_string()
            } else {
                format!("uid:{}", uid)
            };
            
            // Get group list
            let mut groups = vec![0; 32];
            let mut ngroups = groups.len() as i32;
            
            if libc::getgroups(ngroups, groups.as_mut_ptr()) != -1 {
                groups.truncate(ngroups as usize);
            } else {
                groups.clear();
            }
            
            Ok(Self {
                current_uid: uid,
                current_gid: gid,
                effective_uid: euid,
                effective_gid: egid,
                groups,
                username,
            })
        }
    }
    
    /// Check if user has permission for path
    pub fn check_permission(&self, path: &Path, mode: PosixMode) -> FSKitPermissions {
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return FSKitPermissions::default(),
        };
        
        let file_uid = metadata.uid();
        let file_gid = metadata.gid();
        
        PermissionTranslator::posix_to_fskit(
            mode,
            file_uid,
            file_gid,
            self.effective_uid,
            self.effective_gid,
            &self.groups,
        )
    }
    
    /// Switch to user context (for testing)
    pub fn impersonate(&mut self, uid: uid_t, gid: gid_t) -> Result<()> {
        self.effective_uid = uid;
        self.effective_gid = gid;
        Ok(())
    }
    
    /// Get home directory
    pub fn home_directory(&self) -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    }
    
    /// Check if running as root
    pub fn is_root(&self) -> bool {
        self.effective_uid == 0
    }
}

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    FileAccess,
    FileModification,
    FileCreation,
    FileDeletion,
    PermissionChange,
    OwnershipChange,
    MountOperation,
    UnmountOperation,
    AuthenticationFailure,
    AuthorizationFailure,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: SystemTime,
    pub event_type: AuditEventType,
    pub user: String,
    pub uid: uid_t,
    pub path: Option<PathBuf>,
    pub source_ip: Option<String>,
    pub result: AuditResult,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure(String),
    Denied,
}

/// Audit logger for security events
pub struct AuditLogger {
    events: Arc<RwLock<Vec<AuditEvent>>>,
    tx: mpsc::UnboundedSender<AuditEvent>,
}

impl AuditLogger {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AuditEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        
        let logger = Self {
            events: Arc::new(RwLock::new(Vec::new())),
            tx,
        };
        
        (logger, rx)
    }
    
    pub fn log_event(&self, event: AuditEvent) {
        self.events.write().unwrap().push(event.clone());
        let _ = self.tx.send(event);
    }
    
    pub fn log_file_access(
        &self,
        user: &UserContext,
        path: &Path,
        success: bool,
    ) {
        let event = AuditEvent {
            timestamp: SystemTime::now(),
            event_type: AuditEventType::FileAccess,
            user: user.username.clone(),
            uid: user.effective_uid,
            path: Some(path.to_path_buf()),
            source_ip: None,
            result: if success {
                AuditResult::Success
            } else {
                AuditResult::Denied
            },
            details: HashMap::new(),
        };
        
        self.log_event(event);
    }
    
    pub fn log_permission_change(
        &self,
        user: &UserContext,
        path: &Path,
        old_mode: mode_t,
        new_mode: mode_t,
    ) {
        let mut details = HashMap::new();
        details.insert("old_mode".to_string(), format!("{:o}", old_mode));
        details.insert("new_mode".to_string(), format!("{:o}", new_mode));
        
        let event = AuditEvent {
            timestamp: SystemTime::now(),
            event_type: AuditEventType::PermissionChange,
            user: user.username.clone(),
            uid: user.effective_uid,
            path: Some(path.to_path_buf()),
            source_ip: None,
            result: AuditResult::Success,
            details,
        };
        
        self.log_event(event);
    }
    
    pub fn log_authentication_failure(
        &self,
        username: &str,
        reason: &str,
    ) {
        let mut details = HashMap::new();
        details.insert("reason".to_string(), reason.to_string());
        
        let event = AuditEvent {
            timestamp: SystemTime::now(),
            event_type: AuditEventType::AuthenticationFailure,
            user: username.to_string(),
            uid: 0,
            path: None,
            source_ip: None,
            result: AuditResult::Failure(reason.to_string()),
            details,
        };
        
        self.log_event(event);
    }
    
    pub fn get_events(&self, limit: Option<usize>) -> Vec<AuditEvent> {
        let events = self.events.read().unwrap();
        match limit {
            Some(n) => events.iter().rev().take(n).cloned().collect(),
            None => events.clone(),
        }
    }
    
    pub fn export_events(&self, format: ExportFormat) -> Result<String> {
        let events = self.get_events(None);
        
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(&events)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e).into())
            }
            ExportFormat::CSV => {
                let mut csv = String::from("timestamp,event_type,user,uid,path,result\n");
                for event in events {
                    csv.push_str(&format!(
                        "{},{:?},{},{},{:?},{:?}\n",
                        event.timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        event.event_type,
                        event.user,
                        event.uid,
                        event.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                        event.result,
                    ));
                }
                Ok(csv)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Json,
    CSV,
}

/// Security manager combining all security features
pub struct SecurityManager {
    acl_manager: ACLManager,
    bookmark_manager: BookmarkManager,
    audit_logger: AuditLogger,
    entitlements: SandboxEntitlements,
}

impl SecurityManager {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AuditEvent>) {
        let (audit_logger, rx) = AuditLogger::new();
        
        let manager = Self {
            acl_manager: ACLManager::new(),
            bookmark_manager: BookmarkManager::new(),
            audit_logger,
            entitlements: SandboxEntitlements::default(),
        };
        
        (manager, rx)
    }
    
    pub fn check_access(
        &self,
        path: &Path,
        user: &UserContext,
        required_perms: FSKitPermissions,
    ) -> Result<bool> {
        // Check sandbox restrictions
        if self.entitlements.com_apple_security_app_sandbox {
            // Check if path is accessible under sandbox
            let path_str = path.to_string_lossy();
            let allowed = self.entitlements.com_apple_security_temporary_exception_files
                .iter()
                .any(|exception| path_str.starts_with(exception));
            
            if !allowed && !path.starts_with(user.home_directory()) {
                self.audit_logger.log_file_access(user, path, false);
                return Ok(false);
            }
        }
        
        // Check ACLs
        let acl_perms = self.acl_manager.check_acl_permission(
            path,
            user.effective_uid,
            user.effective_gid,
            &user.groups,
        );
        
        // Check POSIX permissions
        let mode = PosixMode::new(
            std::fs::metadata(path)
                .map(|m| m.permissions().mode() as mode_t)
                .unwrap_or(0),
        );
        
        let posix_perms = PermissionTranslator::posix_to_fskit(
            mode,
            user.effective_uid,
            user.effective_gid,
            user.effective_uid,
            user.effective_gid,
            &user.groups,
        );
        
        // Combine permissions
        let has_access = (acl_perms.read || posix_perms.read) && required_perms.read ||
                         (acl_perms.write || posix_perms.write) && required_perms.write ||
                         (acl_perms.execute || posix_perms.execute) && required_perms.execute;
        
        self.audit_logger.log_file_access(user, path, has_access);
        
        Ok(has_access)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_posix_mode_parsing() {
        let mode = PosixMode::new(0o755);
        assert!(mode.user_read());
        assert!(mode.user_write());
        assert!(mode.user_execute());
        assert!(mode.group_read());
        assert!(!mode.group_write());
        assert!(mode.group_execute());
        assert!(mode.other_read());
        assert!(!mode.other_write());
        assert!(mode.other_execute());
    }
    
    #[test]
    fn test_permission_translation() {
        let mode = PosixMode::new(0o644);
        let perms = PermissionTranslator::posix_to_fskit(
            mode,
            1000, // file uid
            1000, // file gid
            1000, // current uid
            1000, // current gid
            &[],
        );
        
        assert!(perms.read);
        assert!(perms.write);
        assert!(!perms.execute);
    }
    
    #[test]
    fn test_acl_manager() {
        let manager = ACLManager::new();
        let path = PathBuf::from("/test/file");
        
        let entry = ACLEntry {
            principal: ACLPrincipal::User(1000),
            permissions: FSKitPermissions {
                read: true,
                write: true,
                ..Default::default()
            },
            flags: ACLFlags::default(),
        };
        
        manager.set_acl(path.clone(), vec![entry]);
        
        let perms = manager.check_acl_permission(&path, 1000, 1000, &[]);
        assert!(perms.read);
        assert!(perms.write);
    }
    
    #[test]
    fn test_bookmark_manager() {
        let manager = BookmarkManager::new();
        let path = PathBuf::from("/test/file");
        
        let bookmark = manager.create_bookmark(path.clone()).unwrap();
        let resolved = manager.resolve_bookmark(&bookmark).unwrap();
        
        assert_eq!(path, resolved);
    }
    
    #[test]
    fn test_audit_logger() {
        let (logger, mut rx) = AuditLogger::new();
        let user = UserContext {
            current_uid: 1000,
            current_gid: 1000,
            effective_uid: 1000,
            effective_gid: 1000,
            groups: vec![],
            username: "testuser".to_string(),
        };
        
        logger.log_file_access(&user, &PathBuf::from("/test"), true);
        
        let event = rx.try_recv().unwrap();
        assert_eq!(event.user, "testuser");
        assert!(matches!(event.result, AuditResult::Success));
    }
}