use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::io;
use super::macos_xattr::{FinderInfo, finder_flags};

/// Finder color labels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum FinderLabel {
    None = 0,
    Gray = 2,
    Green = 4,
    Purple = 6,
    Blue = 8,
    Yellow = 10,
    Red = 12,
    Orange = 14,
}

impl FinderLabel {
    /// Convert from color index (0-7)
    pub fn from_index(index: u8) -> Self {
        match index {
            1 => FinderLabel::Gray,
            2 => FinderLabel::Green,
            3 => FinderLabel::Purple,
            4 => FinderLabel::Blue,
            5 => FinderLabel::Yellow,
            6 => FinderLabel::Red,
            7 => FinderLabel::Orange,
            _ => FinderLabel::None,
        }
    }
    
    /// Get the color index (0-7)
    pub fn to_index(self) -> u8 {
        match self {
            FinderLabel::None => 0,
            FinderLabel::Gray => 1,
            FinderLabel::Green => 2,
            FinderLabel::Purple => 3,
            FinderLabel::Blue => 4,
            FinderLabel::Yellow => 5,
            FinderLabel::Red => 6,
            FinderLabel::Orange => 7,
        }
    }
    
    /// Get the label bits for FinderInfo flags
    pub fn to_bits(self) -> u16 {
        self as u16
    }
}

/// Finder tag structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderTag {
    pub name: String,
    pub color: FinderLabel,
}

impl FinderTag {
    pub fn new(name: String, color: FinderLabel) -> Self {
        Self { name, color }
    }
    
    pub fn with_name(name: String) -> Self {
        Self {
            name,
            color: FinderLabel::None,
        }
    }
}

/// Finder integration handler
pub struct FinderIntegration {
    /// Custom icon resource data cache
    icon_cache: std::collections::HashMap<std::path::PathBuf, Vec<u8>>,
}

impl FinderIntegration {
    pub fn new() -> Self {
        Self {
            icon_cache: std::collections::HashMap::new(),
        }
    }
    
    /// Set custom icon for a file
    pub fn set_custom_icon(&mut self, finder_info: &mut FinderInfo, icon_data: Vec<u8>) {
        // Set the custom icon flag
        finder_info.finder_flags |= finder_flags::HAS_CUSTOM_ICON;
        
        // Store icon data for later use with resource fork
        // In real implementation, this would write to com.apple.ResourceFork
        self.icon_cache.insert(std::path::PathBuf::new(), icon_data);
    }
    
    /// Remove custom icon
    pub fn remove_custom_icon(&mut self, finder_info: &mut FinderInfo) {
        finder_info.finder_flags &= !finder_flags::HAS_CUSTOM_ICON;
        self.icon_cache.clear();
    }
    
    /// Check if file has custom icon
    pub fn has_custom_icon(finder_info: &FinderInfo) -> bool {
        finder_info.finder_flags & finder_flags::HAS_CUSTOM_ICON != 0
    }
    
    /// Set color label
    pub fn set_color_label(finder_info: &mut FinderInfo, label: FinderLabel) {
        // Clear existing color bits (bits 1-3)
        finder_info.finder_flags &= !finder_flags::COLOR_MASK;
        // Set new color
        finder_info.finder_flags |= label.to_bits();
    }
    
    /// Get color label
    pub fn get_color_label(finder_info: &FinderInfo) -> FinderLabel {
        let color_bits = finder_info.finder_flags & finder_flags::COLOR_MASK;
        match color_bits {
            2 => FinderLabel::Gray,
            4 => FinderLabel::Green,
            6 => FinderLabel::Purple,
            8 => FinderLabel::Blue,
            10 => FinderLabel::Yellow,
            12 => FinderLabel::Red,
            14 => FinderLabel::Orange,
            _ => FinderLabel::None,
        }
    }
    
    /// Set Finder comment (Spotlight comment)
    pub fn set_comment(path: &Path, comment: &str) -> (OsString, Vec<u8>) {
        // Finder comments are stored in com.apple.metadata:kMDItemFinderComment
        let attr_name = OsString::from("com.apple.metadata:kMDItemFinderComment");
        let value = Self::encode_comment(comment);
        (attr_name, value)
    }
    
    /// Get Finder comment
    pub fn get_comment(data: &[u8]) -> io::Result<String> {
        Self::decode_comment(data)
    }
    
    /// Encode comment to binary plist format
    fn encode_comment(comment: &str) -> Vec<u8> {
        // Simplified binary plist encoding for strings
        // Real implementation would use proper plist encoding
        let mut result = Vec::new();
        
        // Binary plist header
        result.extend_from_slice(b"bplist00");
        
        // String object (simplified)
        result.push(0x50 | (comment.len() as u8 & 0x0F)); // String type with length
        result.extend_from_slice(comment.as_bytes());
        
        // Offset table (simplified)
        result.push(0x08); // Offset to string object
        
        // Trailer (simplified)
        result.extend_from_slice(&[0u8; 6]); // unused
        result.push(1); // offset int size
        result.push(1); // object ref size
        result.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]); // number of objects
        result.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // top object
        result.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 8]); // offset table offset
        
        result
    }
    
    /// Decode comment from binary plist format
    fn decode_comment(data: &[u8]) -> io::Result<String> {
        // Simplified binary plist decoding
        if data.len() < 8 || &data[0..8] != b"bplist00" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid binary plist format"
            ));
        }
        
        // Find string object (simplified - assumes string at position 8)
        if data.len() > 8 {
            let type_byte = data[8];
            if (type_byte & 0xF0) == 0x50 {
                // ASCII string
                let length = (type_byte & 0x0F) as usize;
                if data.len() >= 9 + length {
                    let string_data = &data[9..9 + length];
                    return String::from_utf8(string_data.to_vec())
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
                }
            }
        }
        
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Could not decode comment"
        ))
    }
    
    /// Set Finder tags
    pub fn set_tags(tags: &[FinderTag]) -> (OsString, Vec<u8>) {
        // Tags are stored in com.apple.metadata:_kMDItemUserTags
        let attr_name = OsString::from("com.apple.metadata:_kMDItemUserTags");
        let value = Self::encode_tags(tags);
        (attr_name, value)
    }
    
    /// Get Finder tags
    pub fn get_tags(data: &[u8]) -> io::Result<Vec<FinderTag>> {
        Self::decode_tags(data)
    }
    
    /// Encode tags to binary plist format
    fn encode_tags(tags: &[FinderTag]) -> Vec<u8> {
        // Simplified binary plist encoding for tag array
        let mut result = Vec::new();
        
        // Binary plist header
        result.extend_from_slice(b"bplist00");
        
        // Array object (simplified)
        result.push(0xA0 | (tags.len() as u8 & 0x0F)); // Array type with count
        
        // Add each tag as a string
        for tag in tags {
            let tag_str = if tag.color != FinderLabel::None {
                format!("{}\n{}", tag.name, tag.color.to_index())
            } else {
                tag.name.clone()
            };
            
            result.push(0x50 | (tag_str.len() as u8 & 0x0F));
            result.extend_from_slice(tag_str.as_bytes());
        }
        
        // Add simplified trailer
        result.extend_from_slice(&[0u8; 32]); // Simplified trailer
        
        result
    }
    
    /// Decode tags from binary plist format
    fn decode_tags(data: &[u8]) -> io::Result<Vec<FinderTag>> {
        // Simplified binary plist decoding
        if data.len() < 8 || &data[0..8] != b"bplist00" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid binary plist format"
            ));
        }
        
        let mut tags = Vec::new();
        let mut pos = 8;
        
        if pos < data.len() {
            let type_byte = data[pos];
            if (type_byte & 0xF0) == 0xA0 {
                // Array
                let count = (type_byte & 0x0F) as usize;
                pos += 1;
                
                for _ in 0..count {
                    if pos >= data.len() {
                        break;
                    }
                    
                    let str_type = data[pos];
                    if (str_type & 0xF0) == 0x50 {
                        let length = (str_type & 0x0F) as usize;
                        pos += 1;
                        
                        if pos + length <= data.len() {
                            let tag_data = &data[pos..pos + length];
                            if let Ok(tag_str) = String::from_utf8(tag_data.to_vec()) {
                                // Parse tag string (name\ncolor_index)
                                let parts: Vec<&str> = tag_str.split('\n').collect();
                                let tag = if parts.len() == 2 {
                                    if let Ok(color_idx) = parts[1].parse::<u8>() {
                                        FinderTag::new(
                                            parts[0].to_string(),
                                            FinderLabel::from_index(color_idx)
                                        )
                                    } else {
                                        FinderTag::with_name(tag_str)
                                    }
                                } else {
                                    FinderTag::with_name(tag_str)
                                };
                                tags.push(tag);
                            }
                            pos += length;
                        }
                    }
                }
            }
        }
        
        Ok(tags)
    }
    
    /// Set file as stationery
    pub fn set_stationery(finder_info: &mut FinderInfo, is_stationery: bool) {
        if is_stationery {
            finder_info.finder_flags |= finder_flags::IS_STATIONERY;
        } else {
            finder_info.finder_flags &= !finder_flags::IS_STATIONERY;
        }
    }
    
    /// Check if file is stationery
    pub fn is_stationery(finder_info: &FinderInfo) -> bool {
        finder_info.finder_flags & finder_flags::IS_STATIONERY != 0
    }
    
    /// Set file as alias
    pub fn set_alias(finder_info: &mut FinderInfo, is_alias: bool) {
        if is_alias {
            finder_info.finder_flags |= finder_flags::IS_ALIAS;
        } else {
            finder_info.finder_flags &= !finder_flags::IS_ALIAS;
        }
    }
    
    /// Check if file is alias
    pub fn is_alias(finder_info: &FinderInfo) -> bool {
        finder_info.finder_flags & finder_flags::IS_ALIAS != 0
    }
    
    /// Set file as bundle
    pub fn set_bundle(finder_info: &mut FinderInfo, has_bundle: bool) {
        if has_bundle {
            finder_info.finder_flags |= finder_flags::HAS_BUNDLE;
        } else {
            finder_info.finder_flags &= !finder_flags::HAS_BUNDLE;
        }
    }
    
    /// Check if file has bundle bit set
    pub fn has_bundle(finder_info: &FinderInfo) -> bool {
        finder_info.finder_flags & finder_flags::HAS_BUNDLE != 0
    }
    
    /// Set file location in Finder window
    pub fn set_location(finder_info: &mut FinderInfo, x: i16, y: i16) {
        finder_info.location = (y, x); // Note: Finder uses (v, h) = (y, x)
    }
    
    /// Get file location in Finder window
    pub fn get_location(finder_info: &FinderInfo) -> (i16, i16) {
        (finder_info.location.1, finder_info.location.0) // Return as (x, y)
    }
}

impl Default for FinderIntegration {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for working with Finder metadata
pub mod helpers {
    use super::*;
    
    /// Create a complete set of Finder metadata for a file
    pub fn create_finder_metadata(
        label: FinderLabel,
        comment: Option<&str>,
        tags: Option<&[FinderTag]>,
        custom_icon: bool,
    ) -> Vec<(OsString, Vec<u8>)> {
        let mut attrs = Vec::new();
        
        // Create FinderInfo with label and icon flag
        let mut finder_info = FinderInfo::default();
        FinderIntegration::set_color_label(&mut finder_info, label);
        if custom_icon {
            finder_info.finder_flags |= finder_flags::HAS_CUSTOM_ICON;
        }
        
        attrs.push((
            OsString::from("com.apple.FinderInfo"),
            finder_info.to_bytes()
        ));
        
        // Add comment if provided
        if let Some(comment) = comment {
            attrs.push(FinderIntegration::set_comment(Path::new(""), comment));
        }
        
        // Add tags if provided
        if let Some(tags) = tags {
            attrs.push(FinderIntegration::set_tags(tags));
        }
        
        attrs
    }
    
    /// Parse all Finder metadata from attributes
    pub fn parse_finder_metadata(
        attrs: &[(OsString, Vec<u8>)]
    ) -> io::Result<(Option<FinderInfo>, Option<String>, Vec<FinderTag>)> {
        let mut finder_info = None;
        let mut comment = None;
        let mut tags = Vec::new();
        
        for (name, value) in attrs {
            let name_str = name.to_string_lossy();
            
            if name_str == "com.apple.FinderInfo" {
                finder_info = Some(FinderInfo::from_bytes(value)?);
            } else if name_str == "com.apple.metadata:kMDItemFinderComment" {
                comment = Some(FinderIntegration::get_comment(value)?);
            } else if name_str == "com.apple.metadata:_kMDItemUserTags" {
                tags = FinderIntegration::get_tags(value)?;
            }
        }
        
        Ok((finder_info, comment, tags))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_color_labels() {
        let mut finder_info = FinderInfo::default();
        
        // Set and get different color labels
        FinderIntegration::set_color_label(&mut finder_info, FinderLabel::Red);
        assert_eq!(FinderIntegration::get_color_label(&finder_info), FinderLabel::Red);
        
        FinderIntegration::set_color_label(&mut finder_info, FinderLabel::Blue);
        assert_eq!(FinderIntegration::get_color_label(&finder_info), FinderLabel::Blue);
        
        FinderIntegration::set_color_label(&mut finder_info, FinderLabel::None);
        assert_eq!(FinderIntegration::get_color_label(&finder_info), FinderLabel::None);
    }
    
    #[test]
    fn test_custom_icon_flag() {
        let mut finder_info = FinderInfo::default();
        let mut integration = FinderIntegration::new();
        
        assert!(!FinderIntegration::has_custom_icon(&finder_info));
        
        integration.set_custom_icon(&mut finder_info, vec![1, 2, 3]);
        assert!(FinderIntegration::has_custom_icon(&finder_info));
        
        integration.remove_custom_icon(&mut finder_info);
        assert!(!FinderIntegration::has_custom_icon(&finder_info));
    }
    
    #[test]
    fn test_finder_comment() {
        let comment = "This is a test comment";
        let (attr_name, value) = FinderIntegration::set_comment(Path::new(""), comment);
        
        assert_eq!(attr_name, OsString::from("com.apple.metadata:kMDItemFinderComment"));
        
        // For now, just check that we can encode/decode
        // Real test would verify proper plist format
        assert!(!value.is_empty());
    }
    
    #[test]
    fn test_finder_tags() {
        let tags = vec![
            FinderTag::new("Important".to_string(), FinderLabel::Red),
            FinderTag::new("Work".to_string(), FinderLabel::Blue),
            FinderTag::with_name("Project".to_string()),
        ];
        
        let (attr_name, value) = FinderIntegration::set_tags(&tags);
        
        assert_eq!(attr_name, OsString::from("com.apple.metadata:_kMDItemUserTags"));
        assert!(!value.is_empty());
    }
    
    #[test]
    fn test_file_location() {
        let mut finder_info = FinderInfo::default();
        
        FinderIntegration::set_location(&mut finder_info, 100, 200);
        let (x, y) = FinderIntegration::get_location(&finder_info);
        
        assert_eq!(x, 100);
        assert_eq!(y, 200);
    }
    
    #[test]
    fn test_stationery_flag() {
        let mut finder_info = FinderInfo::default();
        
        assert!(!FinderIntegration::is_stationery(&finder_info));
        
        FinderIntegration::set_stationery(&mut finder_info, true);
        assert!(FinderIntegration::is_stationery(&finder_info));
        
        FinderIntegration::set_stationery(&mut finder_info, false);
        assert!(!FinderIntegration::is_stationery(&finder_info));
    }
    
    #[test]
    fn test_alias_flag() {
        let mut finder_info = FinderInfo::default();
        
        assert!(!FinderIntegration::is_alias(&finder_info));
        
        FinderIntegration::set_alias(&mut finder_info, true);
        assert!(FinderIntegration::is_alias(&finder_info));
        
        FinderIntegration::set_alias(&mut finder_info, false);
        assert!(!FinderIntegration::is_alias(&finder_info));
    }
    
    #[test]
    fn test_bundle_flag() {
        let mut finder_info = FinderInfo::default();
        
        assert!(!FinderIntegration::has_bundle(&finder_info));
        
        FinderIntegration::set_bundle(&mut finder_info, true);
        assert!(FinderIntegration::has_bundle(&finder_info));
        
        FinderIntegration::set_bundle(&mut finder_info, false);
        assert!(!FinderIntegration::has_bundle(&finder_info));
    }
    
    #[test]
    fn test_create_finder_metadata() {
        let tags = vec![
            FinderTag::new("Test".to_string(), FinderLabel::Green),
        ];
        
        let attrs = helpers::create_finder_metadata(
            FinderLabel::Blue,
            Some("Test comment"),
            Some(&tags),
            true,
        );
        
        assert_eq!(attrs.len(), 3); // FinderInfo, comment, tags
        
        // Check attribute names
        assert_eq!(attrs[0].0, OsString::from("com.apple.FinderInfo"));
        assert_eq!(attrs[1].0, OsString::from("com.apple.metadata:kMDItemFinderComment"));
        assert_eq!(attrs[2].0, OsString::from("com.apple.metadata:_kMDItemUserTags"));
    }
}