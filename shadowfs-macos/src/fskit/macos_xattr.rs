use std::ffi::{OsStr, OsString};
use std::collections::HashMap;
use std::io;

/// Special macOS extended attributes that require special handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOSXattrType {
    /// com.apple.quarantine - Gatekeeper quarantine flag
    Quarantine,
    /// com.apple.metadata:* - Spotlight metadata
    Metadata,
    /// com.apple.FinderInfo - Finder file info and metadata
    FinderInfo,
    /// com.apple.ResourceFork - Classic Mac resource fork
    ResourceFork,
    /// com.apple.system.* - System attributes
    System,
    /// com.apple.security.* - Security attributes
    Security,
    /// Regular user attribute
    Regular,
}

/// Quarantine attribute data structure
#[derive(Debug, Clone)]
pub struct QuarantineData {
    /// Quarantine event ID (hex string)
    pub event_id: String,
    /// Timestamp when file was quarantined
    pub timestamp: u64,
    /// Application that downloaded the file
    pub agent_name: String,
    /// Origin URL or application bundle ID
    pub data_url: Option<String>,
}

impl QuarantineData {
    /// Parse quarantine data from raw bytes
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let s = String::from_utf8(data.to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        let parts: Vec<&str> = s.split(';').collect();
        if parts.len() < 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid quarantine data format"
            ));
        }
        
        Ok(QuarantineData {
            event_id: parts[0].to_string(),
            timestamp: parts[1].parse().unwrap_or(0),
            agent_name: parts[2].to_string(),
            data_url: parts.get(3).map(|s| s.to_string()),
        })
    }
    
    /// Convert quarantine data to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = format!("{};{};{}", 
            self.event_id, 
            self.timestamp, 
            self.agent_name
        );
        
        if let Some(ref url) = self.data_url {
            result.push(';');
            result.push_str(url);
        }
        
        result.into_bytes()
    }
    
    /// Create a new quarantine entry
    pub fn new(agent_name: String, data_url: Option<String>) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let event_id = format!("{:016x}", timestamp);
        
        QuarantineData {
            event_id,
            timestamp,
            agent_name,
            data_url,
        }
    }
}

/// Finder info structure (32 bytes)
#[derive(Debug, Clone)]
pub struct FinderInfo {
    /// File type code (4 bytes)
    pub file_type: [u8; 4],
    /// File creator code (4 bytes)
    pub file_creator: [u8; 4],
    /// Finder flags (2 bytes)
    pub finder_flags: u16,
    /// Location in window (4 bytes: v, h)
    pub location: (i16, i16),
    /// Folder window (2 bytes)
    pub folder: i16,
    /// Extended finder flags (2 bytes)
    pub extended_flags: u16,
    /// Reserved (4 bytes)
    pub reserved1: [u8; 4],
    /// Put away folder ID (4 bytes)
    pub put_away_folder_id: i32,
    /// Additional data (variable)
    pub additional_data: Vec<u8>,
}

impl FinderInfo {
    /// Create default FinderInfo
    pub fn default() -> Self {
        FinderInfo {
            file_type: [0; 4],
            file_creator: [0; 4],
            finder_flags: 0,
            location: (0, 0),
            folder: 0,
            extended_flags: 0,
            reserved1: [0; 4],
            put_away_folder_id: 0,
            additional_data: Vec::new(),
        }
    }
    
    /// Parse FinderInfo from raw bytes
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "FinderInfo must be at least 32 bytes"
            ));
        }
        
        let mut info = FinderInfo::default();
        info.file_type.copy_from_slice(&data[0..4]);
        info.file_creator.copy_from_slice(&data[4..8]);
        info.finder_flags = u16::from_be_bytes([data[8], data[9]]);
        info.location.0 = i16::from_be_bytes([data[10], data[11]]);
        info.location.1 = i16::from_be_bytes([data[12], data[13]]);
        info.folder = i16::from_be_bytes([data[14], data[15]]);
        info.extended_flags = u16::from_be_bytes([data[16], data[17]]);
        info.reserved1.copy_from_slice(&data[18..22]);
        info.put_away_folder_id = i32::from_be_bytes([data[22], data[23], data[24], data[25]]);
        
        if data.len() > 32 {
            info.additional_data = data[32..].to_vec();
        }
        
        Ok(info)
    }
    
    /// Convert FinderInfo to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(32 + self.additional_data.len());
        
        result.extend_from_slice(&self.file_type);
        result.extend_from_slice(&self.file_creator);
        result.extend_from_slice(&self.finder_flags.to_be_bytes());
        result.extend_from_slice(&self.location.0.to_be_bytes());
        result.extend_from_slice(&self.location.1.to_be_bytes());
        result.extend_from_slice(&self.folder.to_be_bytes());
        result.extend_from_slice(&self.extended_flags.to_be_bytes());
        result.extend_from_slice(&self.reserved1);
        result.extend_from_slice(&self.put_away_folder_id.to_be_bytes());
        
        // Pad to 32 bytes if needed
        while result.len() < 32 {
            result.push(0);
        }
        
        result.extend_from_slice(&self.additional_data);
        result
    }
}

/// Finder flags bits
pub mod finder_flags {
    pub const IS_ALIAS: u16 = 0x8000;
    pub const IS_INVISIBLE: u16 = 0x4000;
    pub const HAS_BUNDLE: u16 = 0x2000;
    pub const NAME_LOCKED: u16 = 0x1000;
    pub const IS_STATIONERY: u16 = 0x0800;
    pub const HAS_CUSTOM_ICON: u16 = 0x0400;
    pub const HAS_BEEN_INITED: u16 = 0x0100;
    pub const HAS_NO_INITS: u16 = 0x0080;
    pub const IS_SHARED: u16 = 0x0040;
    pub const COLOR_MASK: u16 = 0x000E;
    pub const IS_ON_DESKTOP: u16 = 0x0001;
}

/// Handler for special macOS extended attributes
pub struct MacOSXattrHandler {
    /// Whether to preserve quarantine attributes
    preserve_quarantine: bool,
    /// Whether to preserve resource forks
    preserve_resource_forks: bool,
    /// Whether to filter system attributes
    filter_system_attrs: bool,
    /// Custom handlers for specific attributes
    handlers: HashMap<OsString, Box<dyn Fn(&[u8]) -> io::Result<Vec<u8>> + Send + Sync>>,
}

impl MacOSXattrHandler {
    pub fn new() -> Self {
        Self {
            preserve_quarantine: false,
            preserve_resource_forks: true,
            filter_system_attrs: false,
            handlers: HashMap::new(),
        }
    }
    
    pub fn with_options(
        preserve_quarantine: bool,
        preserve_resource_forks: bool,
        filter_system_attrs: bool,
    ) -> Self {
        Self {
            preserve_quarantine,
            preserve_resource_forks,
            filter_system_attrs,
            handlers: HashMap::new(),
        }
    }
    
    /// Identify the type of a macOS extended attribute
    pub fn identify_xattr_type(name: &OsStr) -> MacOSXattrType {
        let name_str = name.to_string_lossy();
        
        if name_str == "com.apple.quarantine" {
            MacOSXattrType::Quarantine
        } else if name_str.starts_with("com.apple.metadata:") {
            MacOSXattrType::Metadata
        } else if name_str == "com.apple.FinderInfo" {
            MacOSXattrType::FinderInfo
        } else if name_str == "com.apple.ResourceFork" {
            MacOSXattrType::ResourceFork
        } else if name_str.starts_with("com.apple.system.") {
            MacOSXattrType::System
        } else if name_str.starts_with("com.apple.security.") {
            MacOSXattrType::Security
        } else {
            MacOSXattrType::Regular
        }
    }
    
    /// Check if an attribute should be filtered
    pub fn should_filter(&self, name: &OsStr) -> bool {
        let attr_type = Self::identify_xattr_type(name);
        
        match attr_type {
            MacOSXattrType::Quarantine => !self.preserve_quarantine,
            MacOSXattrType::ResourceFork => !self.preserve_resource_forks,
            MacOSXattrType::System => self.filter_system_attrs,
            MacOSXattrType::Security => self.filter_system_attrs,
            _ => false,
        }
    }
    
    /// Process an attribute value based on its type
    pub fn process_xattr(&self, name: &OsStr, value: &[u8]) -> io::Result<Option<Vec<u8>>> {
        if self.should_filter(name) {
            return Ok(None);
        }
        
        if let Some(handler) = self.handlers.get(name) {
            return handler(value).map(Some);
        }
        
        let attr_type = Self::identify_xattr_type(name);
        
        match attr_type {
            MacOSXattrType::Quarantine => {
                if self.preserve_quarantine {
                    let data = QuarantineData::from_bytes(value)?;
                    Ok(Some(data.to_bytes()))
                } else {
                    Ok(None)
                }
            },
            MacOSXattrType::FinderInfo => {
                let info = FinderInfo::from_bytes(value)?;
                Ok(Some(info.to_bytes()))
            },
            _ => Ok(Some(value.to_vec())),
        }
    }
    
    /// Remove quarantine from an attribute list
    pub fn remove_quarantine(&self, attrs: &mut Vec<OsString>) {
        attrs.retain(|name| {
            name.to_string_lossy() != "com.apple.quarantine"
        });
    }
    
    /// Check if a file has quarantine attribute
    pub fn has_quarantine(attrs: &[OsString]) -> bool {
        attrs.iter().any(|name| {
            name.to_string_lossy() == "com.apple.quarantine"
        })
    }
    
    /// Create a safe quarantine attribute
    pub fn create_safe_quarantine() -> (OsString, Vec<u8>) {
        let data = QuarantineData::new(
            "ShadowFS".to_string(),
            Some("shadowfs://local".to_string())
        );
        
        (
            OsString::from("com.apple.quarantine"),
            data.to_bytes()
        )
    }
    
    /// Check if an attribute is a resource fork
    pub fn is_resource_fork(name: &OsStr) -> bool {
        let name_str = name.to_string_lossy();
        name_str == "com.apple.ResourceFork" || name_str.ends_with("/..namedfork/rsrc")
    }
    
    /// Filter a list of attributes based on settings
    pub fn filter_attributes(&self, attrs: Vec<OsString>) -> Vec<OsString> {
        attrs.into_iter()
            .filter(|name| !self.should_filter(name))
            .collect()
    }
    
    /// Register a custom handler for a specific attribute
    pub fn register_handler<F>(&mut self, name: OsString, handler: F)
    where
        F: Fn(&[u8]) -> io::Result<Vec<u8>> + Send + Sync + 'static,
    {
        self.handlers.insert(name, Box::new(handler));
    }
    
    /// Get metadata attributes from a list
    pub fn get_metadata_attrs(attrs: &[OsString]) -> Vec<OsString> {
        attrs.iter()
            .filter(|name| {
                let name_str = name.to_string_lossy();
                name_str.starts_with("com.apple.metadata:")
            })
            .cloned()
            .collect()
    }
    
    /// Check if file has custom icon
    pub fn has_custom_icon(finder_info: &FinderInfo) -> bool {
        finder_info.finder_flags & finder_flags::HAS_CUSTOM_ICON != 0
    }
    
    /// Set invisibility flag in FinderInfo
    pub fn set_invisible(finder_info: &mut FinderInfo, invisible: bool) {
        if invisible {
            finder_info.finder_flags |= finder_flags::IS_INVISIBLE;
        } else {
            finder_info.finder_flags &= !finder_flags::IS_INVISIBLE;
        }
    }
}

impl Default for MacOSXattrHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quarantine_data() {
        let data = QuarantineData::new(
            "Safari".to_string(),
            Some("https://example.com".to_string())
        );
        
        let bytes = data.to_bytes();
        let parsed = QuarantineData::from_bytes(&bytes).unwrap();
        
        assert_eq!(parsed.agent_name, "Safari");
        assert_eq!(parsed.data_url, Some("https://example.com".to_string()));
    }
    
    #[test]
    fn test_finder_info() {
        let mut info = FinderInfo::default();
        info.file_type = *b"TEXT";
        info.file_creator = *b"ttxt";
        info.finder_flags = finder_flags::IS_INVISIBLE | finder_flags::HAS_CUSTOM_ICON;
        
        let bytes = info.to_bytes();
        assert_eq!(bytes.len(), 32);
        
        let parsed = FinderInfo::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.file_type, *b"TEXT");
        assert_eq!(parsed.file_creator, *b"ttxt");
        assert_eq!(parsed.finder_flags, finder_flags::IS_INVISIBLE | finder_flags::HAS_CUSTOM_ICON);
    }
    
    #[test]
    fn test_xattr_type_identification() {
        assert_eq!(
            MacOSXattrHandler::identify_xattr_type(OsStr::new("com.apple.quarantine")),
            MacOSXattrType::Quarantine
        );
        
        assert_eq!(
            MacOSXattrHandler::identify_xattr_type(OsStr::new("com.apple.metadata:kMDItemDownloadedDate")),
            MacOSXattrType::Metadata
        );
        
        assert_eq!(
            MacOSXattrHandler::identify_xattr_type(OsStr::new("com.apple.FinderInfo")),
            MacOSXattrType::FinderInfo
        );
        
        assert_eq!(
            MacOSXattrHandler::identify_xattr_type(OsStr::new("user.custom")),
            MacOSXattrType::Regular
        );
    }
    
    #[test]
    fn test_attribute_filtering() {
        let handler = MacOSXattrHandler::with_options(false, true, true);
        
        assert!(handler.should_filter(OsStr::new("com.apple.quarantine")));
        assert!(!handler.should_filter(OsStr::new("com.apple.ResourceFork")));
        assert!(handler.should_filter(OsStr::new("com.apple.system.foo")));
        assert!(!handler.should_filter(OsStr::new("user.custom")));
    }
    
    #[test]
    fn test_filter_attributes_list() {
        let handler = MacOSXattrHandler::with_options(false, true, false);
        
        let attrs = vec![
            OsString::from("com.apple.quarantine"),
            OsString::from("com.apple.ResourceFork"),
            OsString::from("user.custom"),
            OsString::from("com.apple.FinderInfo"),
        ];
        
        let filtered = handler.filter_attributes(attrs);
        
        assert_eq!(filtered.len(), 3);
        assert!(!filtered.contains(&OsString::from("com.apple.quarantine")));
        assert!(filtered.contains(&OsString::from("com.apple.ResourceFork")));
        assert!(filtered.contains(&OsString::from("user.custom")));
        assert!(filtered.contains(&OsString::from("com.apple.FinderInfo")));
    }
    
    #[test]
    fn test_invisibility_flag() {
        let mut info = FinderInfo::default();
        
        MacOSXattrHandler::set_invisible(&mut info, true);
        assert!(info.finder_flags & finder_flags::IS_INVISIBLE != 0);
        
        MacOSXattrHandler::set_invisible(&mut info, false);
        assert!(info.finder_flags & finder_flags::IS_INVISIBLE == 0);
    }
}