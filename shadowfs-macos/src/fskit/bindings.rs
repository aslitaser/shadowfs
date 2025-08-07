use objc2::{
    declare_class, extern_protocol, msg_send, msg_send_id,
    mutability, rc::{Allocated, Id},
    ClassType, DeclaredClass, ProtocolType,
};
use objc2_foundation::{
    NSData, NSDate, NSDictionary, NSError, NSNumber, NSObject, NSString, NSURL,
};
use std::ffi::c_void;

#[link(name = "FSKit", kind = "framework")]
extern "C" {}

extern_protocol!(
    pub unsafe trait FSOperations: NSObject {
        #[method(lookupItemNamed:inDirectory:replyHandler:)]
        unsafe fn lookup_item_named(
            &self,
            name: &NSString,
            directory: &FSDirectory,
            reply: &block2::Block<dyn Fn(*mut FSItem, *mut NSError)>,
        );

        #[method(enumerateDirectory:startingAtOffset:replyHandler:)]
        unsafe fn enumerate_directory(
            &self,
            directory: &FSDirectory,
            offset: i64,
            reply: &block2::Block<dyn Fn(*mut NSArray<FSItem>, *mut NSError)>,
        );

        #[method(readContentsOfFile:atOffset:length:replyHandler:)]
        unsafe fn read_contents_of_file(
            &self,
            file: &FSFile,
            offset: i64,
            length: i64,
            reply: &block2::Block<dyn Fn(*mut NSData, *mut NSError)>,
        );

        #[method(writeContentsToFile:atOffset:data:replyHandler:)]
        unsafe fn write_contents_to_file(
            &self,
            file: &FSFile,
            offset: i64,
            data: &NSData,
            reply: &block2::Block<dyn Fn(i64, *mut NSError)>,
        );

        #[method(createItemNamed:type:inDirectory:attributes:replyHandler:)]
        unsafe fn create_item_named(
            &self,
            name: &NSString,
            item_type: FSItemType,
            directory: &FSDirectory,
            attributes: &NSDictionary,
            reply: &block2::Block<dyn Fn(*mut FSItem, *mut NSError)>,
        );

        #[method(deleteItem:replyHandler:)]
        unsafe fn delete_item(
            &self,
            item: &FSItem,
            reply: &block2::Block<dyn Fn(*mut NSError)>,
        );

        #[method(renameItem:toName:replyHandler:)]
        unsafe fn rename_item(
            &self,
            item: &FSItem,
            new_name: &NSString,
            reply: &block2::Block<dyn Fn(*mut FSItem, *mut NSError)>,
        );

        #[method(getAttributesOfItem:replyHandler:)]
        unsafe fn get_attributes_of_item(
            &self,
            item: &FSItem,
            reply: &block2::Block<dyn Fn(*mut NSDictionary, *mut NSError)>,
        );

        #[method(setAttributes:onItem:replyHandler:)]
        unsafe fn set_attributes_on_item(
            &self,
            attributes: &NSDictionary,
            item: &FSItem,
            reply: &block2::Block<dyn Fn(*mut NSError)>,
        );
    }
);

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSItemType(pub i32);

impl FSItemType {
    pub const File: Self = Self(0);
    pub const Directory: Self = Self(1);
    pub const SymbolicLink: Self = Self(2);
    pub const BlockDevice: Self = Self(3);
    pub const CharacterDevice: Self = Self(4);
    pub const NamedPipe: Self = Self(5);
    pub const Socket: Self = Self(6);
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSErrorCode(pub i64);

impl FSErrorCode {
    pub const Success: Self = Self(0);
    pub const NotFound: Self = Self(-2);
    pub const PermissionDenied: Self = Self(-13);
    pub const InvalidArgument: Self = Self(-22);
    pub const IOError: Self = Self(-5);
    pub const NoSpace: Self = Self(-28);
    pub const NotEmpty: Self = Self(-39);
    pub const NotDirectory: Self = Self(-20);
    pub const IsDirectory: Self = Self(-21);
    pub const FileExists: Self = Self(-17);
    pub const CrossDeviceLink: Self = Self(-18);
    pub const ReadOnlyFileSystem: Self = Self(-30);
    pub const NotSupported: Self = Self(-45);
    pub const Busy: Self = Self(-16);
    pub const QuotaExceeded: Self = Self(-122);
    pub const Stale: Self = Self(-116);
    pub const RemoteIO: Self = Self(-121);
    pub const Canceled: Self = Self(-125);
    pub const BadFileDescriptor: Self = Self(-9);
    pub const FileTooLarge: Self = Self(-27);
    pub const NameTooLong: Self = Self(-36);
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSOperationFlags(pub u32);

impl FSOperationFlags {
    pub const None: Self = Self(0);
    pub const Read: Self = Self(1 << 0);
    pub const Write: Self = Self(1 << 1);
    pub const Create: Self = Self(1 << 2);
    pub const Exclusive: Self = Self(1 << 3);
    pub const Truncate: Self = Self(1 << 4);
    pub const Append: Self = Self(1 << 5);
    pub const NonBlocking: Self = Self(1 << 6);
    pub const Directory: Self = Self(1 << 7);
    pub const NoFollow: Self = Self(1 << 8);
    pub const Sync: Self = Self(1 << 9);
    pub const DataSync: Self = Self(1 << 10);
    pub const Direct: Self = Self(1 << 11);
}

impl FSOperationFlags {
    pub fn contains(&self, flag: FSOperationFlags) -> bool {
        (self.0 & flag.0) != 0
    }
    
    pub fn insert(&mut self, flag: FSOperationFlags) {
        self.0 |= flag.0;
    }
    
    pub fn remove(&mut self, flag: FSOperationFlags) {
        self.0 &= !flag.0;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSVolumeCapabilities(pub u64);

impl FSVolumeCapabilities {
    pub const None: Self = Self(0);
    pub const CaseSensitive: Self = Self(1 << 0);
    pub const CasePreserving: Self = Self(1 << 1);
    pub const SupportsHardLinks: Self = Self(1 << 2);
    pub const SupportsSymLinks: Self = Self(1 << 3);
    pub const SupportsFileCloning: Self = Self(1 << 4);
    pub const SupportsSwapFile: Self = Self(1 << 5);
    pub const SupportsExclusiveLocks: Self = Self(1 << 6);
    pub const SupportsSharedLocks: Self = Self(1 << 7);
    pub const SupportsFileExtensions: Self = Self(1 << 8);
    pub const SupportsExtendedAttributes: Self = Self(1 << 9);
    pub const SupportsCompression: Self = Self(1 << 10);
    pub const SupportsEncryption: Self = Self(1 << 11);
    pub const SupportsNamedStreams: Self = Self(1 << 12);
    pub const SupportsObjectIDs: Self = Self(1 << 13);
    pub const SupportsReparse: Self = Self(1 << 14);
    pub const SupportsSparseFiles: Self = Self(1 << 15);
    pub const SupportsRemoteStorage: Self = Self(1 << 16);
    pub const SupportsVolumeQuotas: Self = Self(1 << 17);
    pub const SupportsPersistentHandles: Self = Self(1 << 18);
    pub const SupportsJournaling: Self = Self(1 << 19);
    pub const SupportsZeroData: Self = Self(1 << 20);
    pub const SupportsPunching: Self = Self(1 << 21);
    pub const SupportsSnapshots: Self = Self(1 << 22);
    pub const SupportsCloneRange: Self = Self(1 << 23);
    pub const SupportsBlockCloning: Self = Self(1 << 24);
}

impl FSVolumeCapabilities {
    pub fn contains(&self, cap: FSVolumeCapabilities) -> bool {
        (self.0 & cap.0) != 0
    }
    
    pub fn insert(&mut self, cap: FSVolumeCapabilities) {
        self.0 |= cap.0;
    }
    
    pub fn remove(&mut self, cap: FSVolumeCapabilities) {
        self.0 &= !cap.0;
    }
    
    pub fn default_capabilities() -> Self {
        let mut caps = Self::None;
        caps.insert(Self::CasePreserving);
        caps.insert(Self::SupportsHardLinks);
        caps.insert(Self::SupportsSymLinks);
        caps.insert(Self::SupportsFileExtensions);
        caps.insert(Self::SupportsExtendedAttributes);
        caps
    }
}

pub const FS_MAX_NAME_LENGTH: usize = 255;
pub const FS_MAX_PATH_LENGTH: usize = 1024;
pub const FS_DEFAULT_BLOCK_SIZE: usize = 4096;
pub const FS_DEFAULT_IO_SIZE: usize = 1048576;
pub const FS_MIN_DIO_SIZE: usize = 512;
pub const FS_MAX_READDIR_ENTRIES: usize = 1000;

declare_class!(
    pub struct FSExtensionPoint;

    unsafe impl ClassType for FSExtensionPoint {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "FSExtensionPoint";
    }

    impl DeclaredClass for FSExtensionPoint {}

    unsafe impl FSExtensionPoint {
        #[method_id(initWithDomain:identifier:)]
        pub fn init_with_domain_identifier(
            this: Allocated<Self>,
            domain: &NSString,
            identifier: &NSString,
        ) -> Id<Self>;

        #[method(activate)]
        pub fn activate(&self) -> bool;

        #[method(deactivate)]
        pub fn deactivate(&self);

        #[method_id(volumeWithName:atPath:)]
        pub fn volume_with_name_at_path(
            &self,
            name: &NSString,
            path: &NSURL,
        ) -> Option<Id<FSVolume>>;
    }
);

declare_class!(
    pub struct FSVolume;

    unsafe impl ClassType for FSVolume {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "FSVolume";
    }

    impl DeclaredClass for FSVolume {}

    unsafe impl FSVolume {
        #[method_id(volumeName)]
        pub fn volume_name(&self) -> Id<NSString>;

        #[method_id(volumeUUID)]
        pub fn volume_uuid(&self) -> Id<NSString>;

        #[method_id(mountPoint)]
        pub fn mount_point(&self) -> Id<NSURL>;

        #[method(mount)]
        pub fn mount(&self) -> bool;

        #[method(unmount)]
        pub fn unmount(&self) -> bool;

        #[method(setDelegate:)]
        pub fn set_delegate(&self, delegate: &NSObject);

        #[method_id(delegate)]
        pub fn delegate(&self) -> Option<Id<NSObject>>;

        #[method(isMounted)]
        pub fn is_mounted(&self) -> bool;
    }
);

declare_class!(
    pub struct FSItem;

    unsafe impl ClassType for FSItem {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "FSItem";
    }

    impl DeclaredClass for FSItem {}

    unsafe impl FSItem {
        #[method_id(initWithFileID:)]
        pub fn init_with_file_id(
            this: Allocated<Self>,
            file_id: u64,
        ) -> Id<Self>;

        #[method(fileID)]
        pub fn file_id(&self) -> u64;

        #[method_id(name)]
        pub fn name(&self) -> Id<NSString>;

        #[method(setName:)]
        pub fn set_name(&self, name: &NSString);

        #[method(type)]
        pub fn item_type(&self) -> FSItemType;

        #[method(setType:)]
        pub fn set_type(&self, item_type: FSItemType);

        #[method(size)]
        pub fn size(&self) -> u64;

        #[method(setSize:)]
        pub fn set_size(&self, size: u64);

        #[method_id(creationDate)]
        pub fn creation_date(&self) -> Id<NSDate>;

        #[method(setCreationDate:)]
        pub fn set_creation_date(&self, date: &NSDate);

        #[method_id(modificationDate)]
        pub fn modification_date(&self) -> Id<NSDate>;

        #[method(setModificationDate:)]
        pub fn set_modification_date(&self, date: &NSDate);

        #[method_id(accessDate)]
        pub fn access_date(&self) -> Id<NSDate>;

        #[method(setAccessDate:)]
        pub fn set_access_date(&self, date: &NSDate);

        #[method(permissions)]
        pub fn permissions(&self) -> u16;

        #[method(setPermissions:)]
        pub fn set_permissions(&self, permissions: u16);

        #[method(uid)]
        pub fn uid(&self) -> u32;

        #[method(setUid:)]
        pub fn set_uid(&self, uid: u32);

        #[method(gid)]
        pub fn gid(&self) -> u32;

        #[method(setGid:)]
        pub fn set_gid(&self, gid: u32);
    }
);

declare_class!(
    pub struct FSDirectory;

    unsafe impl ClassType for FSDirectory {
        type Super = FSItem;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "FSDirectory";
    }

    impl DeclaredClass for FSDirectory {}

    unsafe impl FSDirectory {
        #[method_id(initWithFileID:)]
        pub fn init_with_file_id(
            this: Allocated<Self>,
            file_id: u64,
        ) -> Id<Self>;

        #[method(childCount)]
        pub fn child_count(&self) -> u64;

        #[method(setChildCount:)]
        pub fn set_child_count(&self, count: u64);
    }
);

declare_class!(
    pub struct FSFile;

    unsafe impl ClassType for FSFile {
        type Super = FSItem;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "FSFile";
    }

    impl DeclaredClass for FSFile {}

    unsafe impl FSFile {
        #[method_id(initWithFileID:)]
        pub fn init_with_file_id(
            this: Allocated<Self>,
            file_id: u64,
        ) -> Id<Self>;

        #[method(dataSize)]
        pub fn data_size(&self) -> u64;

        #[method(setDataSize:)]
        pub fn set_data_size(&self, size: u64);
    }
);

use objc2_foundation::NSArray;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

pub type FSCompletionHandler = block2::Block<dyn Fn(*mut NSError)>;
pub type FSItemCompletionHandler = block2::Block<dyn Fn(*mut FSItem, *mut NSError)>;
pub type FSDataCompletionHandler = block2::Block<dyn Fn(*mut NSData, *mut NSError)>;
pub type FSWriteCompletionHandler = block2::Block<dyn Fn(i64, *mut NSError)>;
pub type FSEnumerationCompletionHandler = block2::Block<dyn Fn(*mut NSArray<FSItem>, *mut NSError)>;
pub type FSAttributesCompletionHandler = block2::Block<dyn Fn(*mut NSDictionary, *mut NSError)>;

#[derive(Debug)]
pub struct FSKitError {
    pub domain: String,
    pub code: i64,
    pub description: String,
}

impl fmt::Display for FSKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FSKit Error {}: {} ({})", self.code, self.description, self.domain)
    }
}

impl Error for FSKitError {}

impl From<&NSError> for FSKitError {
    fn from(error: &NSError) -> Self {
        unsafe {
            let domain = error.domain().to_string();
            let code = error.code();
            let description = error.localizedDescription().to_string();
            
            FSKitError {
                domain,
                code,
                description,
            }
        }
    }
}

pub struct SafeFSExtensionPoint {
    inner: Id<FSExtensionPoint>,
}

unsafe impl Send for SafeFSExtensionPoint {}
unsafe impl Sync for SafeFSExtensionPoint {}

impl SafeFSExtensionPoint {
    pub fn new(domain: &str, identifier: &str) -> Result<Self, FSKitError> {
        unsafe {
            let domain = NSString::from_str(domain);
            let identifier = NSString::from_str(identifier);
            let point = FSExtensionPoint::alloc().init_with_domain_identifier(&domain, &identifier);
            
            Ok(SafeFSExtensionPoint { inner: point })
        }
    }
    
    pub fn activate(&self) -> bool {
        unsafe { self.inner.activate() }
    }
    
    pub fn deactivate(&self) {
        unsafe { self.inner.deactivate() }
    }
    
    pub fn create_volume(&self, name: &str, path: &str) -> Option<SafeFSVolume> {
        unsafe {
            let name = NSString::from_str(name);
            let url = NSURL::from_file_path(&NSString::from_str(path));
            
            self.inner.volume_with_name_at_path(&name, &url)
                .map(|volume| SafeFSVolume { inner: volume })
        }
    }
    
    pub fn as_raw(&self) -> &FSExtensionPoint {
        &self.inner
    }
}

pub struct SafeFSVolume {
    inner: Id<FSVolume>,
}

unsafe impl Send for SafeFSVolume {}
unsafe impl Sync for SafeFSVolume {}

impl SafeFSVolume {
    pub fn volume_name(&self) -> String {
        unsafe { self.inner.volume_name().to_string() }
    }
    
    pub fn volume_uuid(&self) -> String {
        unsafe { self.inner.volume_uuid().to_string() }
    }
    
    pub fn mount_point(&self) -> String {
        unsafe {
            self.inner.mount_point().path()
                .map(|path| path.to_string())
                .unwrap_or_default()
        }
    }
    
    pub fn mount(&self) -> bool {
        unsafe { self.inner.mount() }
    }
    
    pub fn unmount(&self) -> bool {
        unsafe { self.inner.unmount() }
    }
    
    pub fn is_mounted(&self) -> bool {
        unsafe { self.inner.is_mounted() }
    }
    
    pub fn set_delegate(&self, delegate: &NSObject) {
        unsafe { self.inner.set_delegate(delegate) }
    }
    
    pub fn as_raw(&self) -> &FSVolume {
        &self.inner
    }
}

pub struct SafeFSItem {
    inner: Id<FSItem>,
}

unsafe impl Send for SafeFSItem {}
unsafe impl Sync for SafeFSItem {}

impl SafeFSItem {
    pub fn new(file_id: u64) -> Self {
        unsafe {
            let item = FSItem::alloc().init_with_file_id(file_id);
            SafeFSItem { inner: item }
        }
    }
    
    pub fn file_id(&self) -> u64 {
        unsafe { self.inner.file_id() }
    }
    
    pub fn name(&self) -> String {
        unsafe { self.inner.name().to_string() }
    }
    
    pub fn set_name(&self, name: &str) {
        unsafe {
            let ns_name = NSString::from_str(name);
            self.inner.set_name(&ns_name);
        }
    }
    
    pub fn item_type(&self) -> FSItemType {
        unsafe { self.inner.item_type() }
    }
    
    pub fn set_type(&self, item_type: FSItemType) {
        unsafe { self.inner.set_type(item_type) }
    }
    
    pub fn size(&self) -> u64 {
        unsafe { self.inner.size() }
    }
    
    pub fn set_size(&self, size: u64) {
        unsafe { self.inner.set_size(size) }
    }
    
    pub fn permissions(&self) -> u16 {
        unsafe { self.inner.permissions() }
    }
    
    pub fn set_permissions(&self, permissions: u16) {
        unsafe { self.inner.set_permissions(permissions) }
    }
    
    pub fn uid(&self) -> u32 {
        unsafe { self.inner.uid() }
    }
    
    pub fn set_uid(&self, uid: u32) {
        unsafe { self.inner.set_uid(uid) }
    }
    
    pub fn gid(&self) -> u32 {
        unsafe { self.inner.gid() }
    }
    
    pub fn set_gid(&self, gid: u32) {
        unsafe { self.inner.set_gid(gid) }
    }
    
    pub fn as_raw(&self) -> &FSItem {
        &self.inner
    }
    
    pub fn into_raw(self) -> Id<FSItem> {
        self.inner
    }
}

pub struct SafeFSDirectory {
    inner: Id<FSDirectory>,
}

unsafe impl Send for SafeFSDirectory {}
unsafe impl Sync for SafeFSDirectory {}

impl SafeFSDirectory {
    pub fn new(file_id: u64) -> Self {
        unsafe {
            let dir = FSDirectory::alloc().init_with_file_id(file_id);
            SafeFSDirectory { inner: dir }
        }
    }
    
    pub fn child_count(&self) -> u64 {
        unsafe { self.inner.child_count() }
    }
    
    pub fn set_child_count(&self, count: u64) {
        unsafe { self.inner.set_child_count(count) }
    }
    
    pub fn as_raw(&self) -> &FSDirectory {
        &self.inner
    }
    
    pub fn as_item(&self) -> &FSItem {
        unsafe { &*(self.inner.as_ref() as *const FSDirectory as *const FSItem) }
    }
    
    pub fn into_raw(self) -> Id<FSDirectory> {
        self.inner
    }
}

pub struct SafeFSFile {
    inner: Id<FSFile>,
}

unsafe impl Send for SafeFSFile {}
unsafe impl Sync for SafeFSFile {}

impl SafeFSFile {
    pub fn new(file_id: u64) -> Self {
        unsafe {
            let file = FSFile::alloc().init_with_file_id(file_id);
            SafeFSFile { inner: file }
        }
    }
    
    pub fn data_size(&self) -> u64 {
        unsafe { self.inner.data_size() }
    }
    
    pub fn set_data_size(&self, size: u64) {
        unsafe { self.inner.set_data_size(size) }
    }
    
    pub fn as_raw(&self) -> &FSFile {
        &self.inner
    }
    
    pub fn as_item(&self) -> &FSItem {
        unsafe { &*(self.inner.as_ref() as *const FSFile as *const FSItem) }
    }
    
    pub fn into_raw(self) -> Id<FSFile> {
        self.inner
    }
}

pub fn create_error(code: i64, description: &str) -> Id<NSError> {
    unsafe {
        let domain = NSString::from_str("com.shadowfs.fskit");
        let desc = NSString::from_str(description);
        let user_info = NSDictionary::from_keys_and_objects(
            &[&*NSString::from_str("NSLocalizedDescriptionKey")],
            vec![desc.as_ref()],
        );
        
        NSError::errorWithDomain_code_userInfo(&domain, code, Some(&user_info))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeNormalization {
    NFC,
    NFD,
    NFKC,
    NFKD,
}

pub trait NSStringConversion {
    fn to_nsstring(&self) -> Id<NSString>;
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString>;
}

impl NSStringConversion for Path {
    fn to_nsstring(&self) -> Id<NSString> {
        let bytes = self.as_os_str().as_bytes();
        unsafe {
            if let Ok(s) = std::str::from_utf8(bytes) {
                NSString::from_str(s)
            } else {
                let utf8_lossy = String::from_utf8_lossy(bytes);
                NSString::from_str(&utf8_lossy)
            }
        }
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        let nsstring = self.to_nsstring();
        normalize_nsstring(&nsstring, form)
    }
}

impl NSStringConversion for PathBuf {
    fn to_nsstring(&self) -> Id<NSString> {
        self.as_path().to_nsstring()
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        self.as_path().to_nsstring_normalized(form)
    }
}

impl NSStringConversion for str {
    fn to_nsstring(&self) -> Id<NSString> {
        unsafe { NSString::from_str(self) }
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        let nsstring = self.to_nsstring();
        normalize_nsstring(&nsstring, form)
    }
}

impl NSStringConversion for String {
    fn to_nsstring(&self) -> Id<NSString> {
        self.as_str().to_nsstring()
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        self.as_str().to_nsstring_normalized(form)
    }
}

impl NSStringConversion for OsStr {
    fn to_nsstring(&self) -> Id<NSString> {
        let bytes = self.as_bytes();
        unsafe {
            if let Ok(s) = std::str::from_utf8(bytes) {
                NSString::from_str(s)
            } else {
                let utf8_lossy = String::from_utf8_lossy(bytes);
                NSString::from_str(&utf8_lossy)
            }
        }
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        let nsstring = self.to_nsstring();
        normalize_nsstring(&nsstring, form)
    }
}

impl NSStringConversion for OsString {
    fn to_nsstring(&self) -> Id<NSString> {
        self.as_os_str().to_nsstring()
    }
    
    fn to_nsstring_normalized(&self, form: UnicodeNormalization) -> Id<NSString> {
        self.as_os_str().to_nsstring_normalized(form)
    }
}

pub trait PathFromNSString {
    fn to_path(&self) -> PathBuf;
    fn to_os_string(&self) -> OsString;
}

impl PathFromNSString for NSString {
    fn to_path(&self) -> PathBuf {
        let os_string = self.to_os_string();
        PathBuf::from(os_string)
    }
    
    fn to_os_string(&self) -> OsString {
        let string = self.to_string();
        OsString::from(string)
    }
}

fn normalize_nsstring(string: &NSString, form: UnicodeNormalization) -> Id<NSString> {
    unsafe {
        let selector = match form {
            UnicodeNormalization::NFC => sel!(precomposedStringWithCanonicalMapping),
            UnicodeNormalization::NFD => sel!(decomposedStringWithCanonicalMapping),
            UnicodeNormalization::NFKC => sel!(precomposedStringWithCompatibilityMapping),
            UnicodeNormalization::NFKD => sel!(decomposedStringWithCompatibilityMapping),
        };
        
        let normalized: Id<NSString> = msg_send_id![string, selector];
        normalized
    }
}

pub fn path_to_nsstring(path: &Path) -> Id<NSString> {
    path.to_nsstring_normalized(UnicodeNormalization::NFD)
}

pub fn nsstring_to_path(string: &NSString) -> PathBuf {
    let normalized = normalize_nsstring(string, UnicodeNormalization::NFC);
    normalized.to_path()
}

pub fn handle_utf8_edge_cases(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut i = 0;
        
        while i < bytes.len() {
            if bytes[i] < 0x80 {
                result.push(bytes[i] as char);
                i += 1;
            } else if bytes[i] < 0xC0 {
                result.push('\u{FFFD}');
                i += 1;
            } else if bytes[i] < 0xE0 {
                if i + 1 < bytes.len() {
                    if let Ok(s) = std::str::from_utf8(&bytes[i..i+2]) {
                        result.push_str(s);
                        i += 2;
                    } else {
                        result.push('\u{FFFD}');
                        i += 1;
                    }
                } else {
                    result.push('\u{FFFD}');
                    i += 1;
                }
            } else if bytes[i] < 0xF0 {
                if i + 2 < bytes.len() {
                    if let Ok(s) = std::str::from_utf8(&bytes[i..i+3]) {
                        result.push_str(s);
                        i += 3;
                    } else {
                        result.push('\u{FFFD}');
                        i += 1;
                    }
                } else {
                    result.push('\u{FFFD}');
                    i += 1;
                }
            } else if bytes[i] < 0xF8 {
                if i + 3 < bytes.len() {
                    if let Ok(s) = std::str::from_utf8(&bytes[i..i+4]) {
                        result.push_str(s);
                        i += 4;
                    } else {
                        result.push('\u{FFFD}');
                        i += 1;
                    }
                } else {
                    result.push('\u{FFFD}');
                    i += 1;
                }
            } else {
                result.push('\u{FFFD}');
                i += 1;
            }
        }
        
        result
    }
}

use objc2::sel;