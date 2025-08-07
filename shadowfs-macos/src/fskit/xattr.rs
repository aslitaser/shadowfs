use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::io;
use std::ffi::{OsStr, OsString};

#[cfg(target_os = "macos")]
use libc::{c_char, c_void, ssize_t};

#[derive(Debug, Clone)]
pub struct ExtendedAttribute {
    pub name: OsString,
    pub value: Vec<u8>,
    pub flags: XattrFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XattrFlags {
    pub create: bool,
    pub replace: bool,
    pub no_follow: bool,
}

impl Default for XattrFlags {
    fn default() -> Self {
        Self {
            create: false,
            replace: false,
            no_follow: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    UseOverride,
    UseSource,
    Merge,
}

pub struct ExtendedAttributesHandler {
    conflict_resolution: ConflictResolution,
    override_attributes: HashMap<PathBuf, HashMap<OsString, Vec<u8>>>,
    deleted_attributes: HashMap<PathBuf, HashSet<OsString>>,
}

impl ExtendedAttributesHandler {
    pub fn new(conflict_resolution: ConflictResolution) -> Self {
        Self {
            conflict_resolution,
            override_attributes: HashMap::new(),
            deleted_attributes: HashMap::new(),
        }
    }

    pub fn list_xattrs(&self, path: &Path, include_source: bool) -> io::Result<Vec<OsString>> {
        let mut attrs = HashSet::new();
        
        if include_source {
            let source_attrs = self.list_source_xattrs(path)?;
            attrs.extend(source_attrs);
        }
        
        if let Some(deleted) = self.deleted_attributes.get(path) {
            for attr in deleted {
                attrs.remove(attr);
            }
        }
        
        if let Some(override_attrs) = self.override_attributes.get(path) {
            attrs.extend(override_attrs.keys().cloned());
        }
        
        Ok(attrs.into_iter().collect())
    }

    pub fn get_xattr(&self, path: &Path, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        if let Some(deleted) = self.deleted_attributes.get(path) {
            if deleted.contains(name) {
                return Ok(None);
            }
        }
        
        if let Some(override_attrs) = self.override_attributes.get(path) {
            if let Some(value) = override_attrs.get(name) {
                return Ok(Some(value.clone()));
            }
        }
        
        self.get_source_xattr(path, name)
    }

    pub fn set_xattr(&mut self, path: &Path, name: OsString, value: Vec<u8>, flags: XattrFlags) -> io::Result<()> {
        let path_buf = path.to_path_buf();
        
        if let Some(deleted) = self.deleted_attributes.get_mut(&path_buf) {
            deleted.remove(&name);
        }
        
        let attrs = self.override_attributes.entry(path_buf.clone()).or_insert_with(HashMap::new);
        
        if flags.create && attrs.contains_key(&name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Extended attribute already exists"
            ));
        }
        
        if flags.replace && !attrs.contains_key(&name) {
            let has_source = self.get_source_xattr(path, &name)?.is_some();
            if !has_source {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Extended attribute not found"
                ));
            }
        }
        
        attrs.insert(name, value);
        Ok(())
    }

    pub fn remove_xattr(&mut self, path: &Path, name: OsString) -> io::Result<()> {
        let path_buf = path.to_path_buf();
        
        if let Some(override_attrs) = self.override_attributes.get_mut(&path_buf) {
            override_attrs.remove(&name);
        }
        
        let deleted = self.deleted_attributes.entry(path_buf.clone()).or_insert_with(HashSet::new);
        deleted.insert(name.clone());
        
        let has_attr = self.get_source_xattr(path, &name)?.is_some() ||
            self.override_attributes.get(&path_buf)
                .map(|attrs| attrs.contains_key(&name))
                .unwrap_or(false);
        
        if !has_attr {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Extended attribute not found"
            ));
        }
        
        Ok(())
    }

    pub fn merge_attributes(&self, path: &Path) -> io::Result<HashMap<OsString, Vec<u8>>> {
        let mut merged = HashMap::new();
        
        let source_attrs = self.list_source_xattrs(path)?;
        for attr_name in source_attrs {
            if let Some(deleted) = self.deleted_attributes.get(path) {
                if deleted.contains(&attr_name) {
                    continue;
                }
            }
            
            if let Some(value) = self.get_source_xattr(path, &attr_name)? {
                merged.insert(attr_name.clone(), value);
            }
        }
        
        if let Some(override_attrs) = self.override_attributes.get(path) {
            for (name, value) in override_attrs {
                match self.conflict_resolution {
                    ConflictResolution::UseOverride => {
                        merged.insert(name.clone(), value.clone());
                    },
                    ConflictResolution::UseSource => {
                        if !merged.contains_key(name) {
                            merged.insert(name.clone(), value.clone());
                        }
                    },
                    ConflictResolution::Merge => {
                        if let Some(source_value) = merged.get(name) {
                            let merged_value = self.merge_values(source_value, value);
                            merged.insert(name.clone(), merged_value);
                        } else {
                            merged.insert(name.clone(), value.clone());
                        }
                    },
                }
            }
        }
        
        Ok(merged)
    }

    fn merge_values(&self, source: &[u8], override_val: &[u8]) -> Vec<u8> {
        if source == override_val {
            return source.to_vec();
        }
        override_val.to_vec()
    }

    #[cfg(target_os = "macos")]
    fn list_source_xattrs(&self, path: &Path) -> io::Result<Vec<OsString>> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStringExt;
        
        let c_path = CString::new(path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid path")
        })?)?;
        
        let size = unsafe {
            libc::listxattr(
                c_path.as_ptr(),
                std::ptr::null_mut(),
                0,
                0
            )
        };
        
        if size < 0 {
            return if errno::errno().0 == libc::ENOATTR {
                Ok(Vec::new())
            } else {
                Err(io::Error::last_os_error())
            };
        }
        
        if size == 0 {
            return Ok(Vec::new());
        }
        
        let mut buffer = vec![0u8; size as usize];
        let actual_size = unsafe {
            libc::listxattr(
                c_path.as_ptr(),
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len(),
                0
            )
        };
        
        if actual_size < 0 {
            return Err(io::Error::last_os_error());
        }
        
        buffer.truncate(actual_size as usize);
        
        let mut attrs = Vec::new();
        let mut start = 0;
        
        for i in 0..buffer.len() {
            if buffer[i] == 0 {
                if i > start {
                    let name = OsString::from_vec(buffer[start..i].to_vec());
                    attrs.push(name);
                }
                start = i + 1;
            }
        }
        
        Ok(attrs)
    }

    #[cfg(not(target_os = "macos"))]
    fn list_source_xattrs(&self, _path: &Path) -> io::Result<Vec<OsString>> {
        Ok(Vec::new())
    }

    #[cfg(target_os = "macos")]
    fn get_source_xattr(&self, path: &Path, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        
        let c_path = CString::new(path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid path")
        })?)?;
        
        let c_name = CString::new(name.as_bytes())?;
        
        let size = unsafe {
            libc::getxattr(
                c_path.as_ptr(),
                c_name.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                0
            )
        };
        
        if size < 0 {
            return if errno::errno().0 == libc::ENOATTR {
                Ok(None)
            } else {
                Err(io::Error::last_os_error())
            };
        }
        
        let mut buffer = vec![0u8; size as usize];
        let actual_size = unsafe {
            libc::getxattr(
                c_path.as_ptr(),
                c_name.as_ptr(),
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len(),
                0,
                0
            )
        };
        
        if actual_size < 0 {
            return Err(io::Error::last_os_error());
        }
        
        buffer.truncate(actual_size as usize);
        Ok(Some(buffer))
    }

    #[cfg(not(target_os = "macos"))]
    fn get_source_xattr(&self, _path: &Path, _name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    pub fn handle_conflict(&self, path: &Path, name: &OsStr) -> io::Result<ConflictResolution> {
        let has_source = self.get_source_xattr(path, name)?.is_some();
        let has_override = self.override_attributes.get(path)
            .map(|attrs| attrs.contains_key(name))
            .unwrap_or(false);
        
        if has_source && has_override {
            Ok(self.conflict_resolution)
        } else if has_override {
            Ok(ConflictResolution::UseOverride)
        } else {
            Ok(ConflictResolution::UseSource)
        }
    }

    pub fn clear_overrides(&mut self, path: &Path) {
        self.override_attributes.remove(path);
        self.deleted_attributes.remove(path);
    }

    pub fn copy_attributes(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let attrs = self.merge_attributes(from)?;
        
        let to_path = to.to_path_buf();
        self.override_attributes.insert(to_path.clone(), attrs);
        
        if let Some(deleted) = self.deleted_attributes.get(from) {
            self.deleted_attributes.insert(to_path, deleted.clone());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xattr_flags_default() {
        let flags = XattrFlags::default();
        assert!(!flags.create);
        assert!(!flags.replace);
        assert!(!flags.no_follow);
    }

    #[test]
    fn test_extended_attributes_handler_new() {
        let handler = ExtendedAttributesHandler::new(ConflictResolution::UseOverride);
        assert!(handler.override_attributes.is_empty());
        assert!(handler.deleted_attributes.is_empty());
    }

    #[test]
    fn test_set_and_get_xattr() {
        let mut handler = ExtendedAttributesHandler::new(ConflictResolution::UseOverride);
        let path = Path::new("/test/file");
        let name = OsString::from("test.attr");
        let value = b"test value".to_vec();
        
        handler.set_xattr(path, name.clone(), value.clone(), XattrFlags::default()).unwrap();
        
        let result = handler.get_xattr(path, &name).unwrap();
        assert_eq!(result, Some(value));
    }

    #[test]
    fn test_remove_xattr() {
        let mut handler = ExtendedAttributesHandler::new(ConflictResolution::UseOverride);
        let path = Path::new("/test/file");
        let name = OsString::from("test.attr");
        let value = b"test value".to_vec();
        
        handler.set_xattr(path, name.clone(), value, XattrFlags::default()).unwrap();
        handler.remove_xattr(path, name.clone()).unwrap();
        
        let result = handler.get_xattr(path, &name).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_conflict_resolution() {
        let handler = ExtendedAttributesHandler::new(ConflictResolution::UseOverride);
        let path = Path::new("/test/file");
        let name = OsStr::new("test.attr");
        
        let resolution = handler.handle_conflict(path, name).unwrap();
        assert_eq!(resolution, ConflictResolution::UseSource);
    }

    #[test]
    fn test_clear_overrides() {
        let mut handler = ExtendedAttributesHandler::new(ConflictResolution::UseOverride);
        let path = Path::new("/test/file");
        let name = OsString::from("test.attr");
        let value = b"test value".to_vec();
        
        handler.set_xattr(path, name.clone(), value, XattrFlags::default()).unwrap();
        handler.clear_overrides(path);
        
        assert!(!handler.override_attributes.contains_key(path));
        assert!(!handler.deleted_attributes.contains_key(path));
    }
}