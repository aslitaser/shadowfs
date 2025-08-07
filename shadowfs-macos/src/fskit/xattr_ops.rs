use std::path::Path;
use std::ffi::{OsStr, OsString};
use super::xattr::{ExtendedAttributesHandler, XattrFlags, ConflictResolution};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct XattrOperations {
    handler: Arc<RwLock<ExtendedAttributesHandler>>,
}

impl XattrOperations {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(RwLock::new(ExtendedAttributesHandler::new(ConflictResolution::UseOverride))),
        }
    }

    pub fn new_with_resolution(resolution: ConflictResolution) -> Self {
        Self {
            handler: Arc::new(RwLock::new(ExtendedAttributesHandler::new(resolution))),
        }
    }

    pub fn getxattr(&self, path: &Path, name: &OsStr, buffer: Option<&mut [u8]>) -> Result<usize, String> {
        let handler = self.handler.read()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        match handler.get_xattr(path, name) {
            Ok(Some(value)) => {
                if let Some(buffer) = buffer {
                    if buffer.len() < value.len() {
                        return Err(format!("Buffer too small: need {} bytes, got {}", value.len(), buffer.len()));
                    }
                    buffer[..value.len()].copy_from_slice(&value);
                }
                Ok(value.len())
            },
            Ok(None) => Err("Extended attribute not found".to_string()),
            Err(e) => Err(format!("Failed to get extended attribute: {}", e))
        }
    }

    pub fn setxattr(&self, path: &Path, name: OsString, value: Vec<u8>, flags: XattrFlags) -> Result<(), String> {
        let mut handler = self.handler.write()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        handler.set_xattr(path, name, value, flags)
            .map_err(|e| format!("Failed to set extended attribute: {}", e))
    }

    pub fn removexattr(&self, path: &Path, name: OsString) -> Result<(), String> {
        let mut handler = self.handler.write()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        handler.remove_xattr(path, name)
            .map_err(|e| format!("Failed to remove extended attribute: {}", e))
    }

    pub fn listxattr(&self, path: &Path, buffer: Option<&mut [u8]>) -> Result<usize, String> {
        let handler = self.handler.read()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        let attrs = handler.list_xattrs(path, true)
            .map_err(|e| format!("Failed to list extended attributes: {}", e))?;
        
        let mut total_size = 0;
        for attr in &attrs {
            total_size += attr.len() + 1;
        }
        
        if let Some(buffer) = buffer {
            if buffer.len() < total_size {
                return Err(format!("Buffer too small: need {} bytes, got {}", total_size, buffer.len()));
            }
            
            let mut offset = 0;
            for attr in attrs {
                let attr_bytes = attr.as_encoded_bytes();
                buffer[offset..offset + attr_bytes.len()].copy_from_slice(attr_bytes);
                offset += attr_bytes.len();
                buffer[offset] = 0;
                offset += 1;
            }
        }
        
        Ok(total_size)
    }

    pub fn getxattr_size(&self, path: &Path, name: &OsStr) -> Result<usize, String> {
        self.getxattr(path, name, None)
    }

    pub fn listxattr_size(&self, path: &Path) -> Result<usize, String> {
        self.listxattr(path, None)
    }

    pub fn copy_xattrs(&self, from: &Path, to: &Path) -> Result<(), String> {
        let mut handler = self.handler.write()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        handler.copy_attributes(from, to)
            .map_err(|e| format!("Failed to copy extended attributes: {}", e))
    }

    pub fn clear_xattrs(&self, path: &Path) -> Result<(), String> {
        let mut handler = self.handler.write()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;
        
        handler.clear_overrides(path);
        Ok(())
    }
}

impl Default for XattrOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xattr_operations() {
        let ops = XattrOperations::new();
        let path = Path::new("/test/file.txt");
        let name = OsString::from("user.test");
        let value = b"test value".to_vec();
        
        let result = ops.setxattr(path, name.clone(), value.clone(), XattrFlags::default());
        assert!(result.is_ok());
        
        let mut buffer = vec![0u8; 100];
        let result = ops.getxattr(path, &name, Some(&mut buffer));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), value.len());
        assert_eq!(&buffer[..value.len()], &value[..]);
    }

    #[test]
    fn test_xattr_size_query() {
        let ops = XattrOperations::new();
        let path = Path::new("/test/file.txt");
        let name = OsString::from("user.test");
        let value = b"test value".to_vec();
        
        ops.setxattr(path, name.clone(), value.clone(), XattrFlags::default()).unwrap();
        
        let size = ops.getxattr_size(path, &name);
        assert!(size.is_ok());
        assert_eq!(size.unwrap(), value.len());
    }

    #[test]
    fn test_xattr_list() {
        let ops = XattrOperations::new();
        let path = Path::new("/test/file.txt");
        let attr1 = OsString::from("user.test1");
        let attr2 = OsString::from("user.test2");
        
        ops.setxattr(path, attr1.clone(), b"value1".to_vec(), XattrFlags::default()).unwrap();
        ops.setxattr(path, attr2.clone(), b"value2".to_vec(), XattrFlags::default()).unwrap();
        
        let size = ops.listxattr_size(path);
        assert!(size.is_ok());
        
        let mut buffer = vec![0u8; size.unwrap()];
        let result = ops.listxattr(path, Some(&mut buffer));
        assert!(result.is_ok());
    }

    #[test]
    fn test_xattr_remove() {
        let ops = XattrOperations::new();
        let path = Path::new("/test/file.txt");
        let name = OsString::from("user.test");
        
        ops.setxattr(path, name.clone(), b"value".to_vec(), XattrFlags::default()).unwrap();
        
        let result = ops.removexattr(path, name.clone());
        assert!(result.is_ok());
        
        let result = ops.getxattr(path, &name, None);
        assert!(result.is_err());
    }
}