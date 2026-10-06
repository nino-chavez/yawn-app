#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
#[cfg(target_os = "macos")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::UNIX_EPOCH;

use thiserror::Error;
use uuid::Uuid;

/// Darwin's `<sys/fcntl.h>` O_UNIQUE. libc 0.2.189 does not expose it.
#[cfg(target_os = "macos")]
const O_UNIQUE: libc::c_int = 0x2000;

/// Darwin's `<sys/stdio.h>` RENAME_NOFOLLOW_ANY. libc 0.2.189 does not expose it.
#[cfg(target_os = "macos")]
const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x0000_0010;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("application data root must be absolute")]
    RelativeRoot,
    #[error("application data root may not be inside the source repository")]
    RootInsideRepository,
    #[error("application data path contains traversal")]
    Traversal,
    #[error("application data path escaped its root")]
    EscapedRoot,
    #[error("application data path is a symlink")]
    Symlink,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct StorageRoot {
    root: PathBuf,
}

/// The filesystem identity fields used by the profile lifecycle receipts.
/// Content digests remain a separate concern owned by the lifecycle module.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrivateObjectIdentity {
    pub(crate) device: u64,
    pub(crate) inode: u64,
    pub(crate) generation: u64,
    pub(crate) birth_seconds: u64,
    pub(crate) birth_nanoseconds: u32,
    pub(crate) owner: u32,
    pub(crate) mode: u32,
    pub(crate) link_count: u64,
    pub(crate) flags: u32,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrivateObjectObservation {
    pub(crate) identity: PrivateObjectIdentity,
    pub(crate) size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct BoundPrivateDirectory {
    path: PathBuf,
    file: File,
    binding: StableObjectBinding,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct BoundPrivateFile {
    name: CString,
    file: File,
    binding: StableObjectBinding,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StableObjectBinding {
    device: u64,
    inode: u64,
    generation: u64,
    birth_seconds: u64,
    birth_nanoseconds: u32,
}

impl StorageRoot {
    pub fn create(root: &Path, repository_root: &Path) -> Result<Self, StorageError> {
        if !root.is_absolute() {
            return Err(StorageError::RelativeRoot);
        }
        if fs::symlink_metadata(root)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(StorageError::Symlink);
        }
        let normalized_repo = lexical_absolute(repository_root)?;
        let normalized_root = lexical_absolute(root)?;
        if normalized_root.starts_with(&normalized_repo) {
            return Err(StorageError::RootInsideRepository);
        }
        create_private_dir(&normalized_root)?;
        for child in ["diagnostics", "profile", "meetings", "enrollment", "models"] {
            create_private_dir(&normalized_root.join(child))?;
        }
        let canonical = normalized_root.canonicalize()?;
        if normalized_repo.exists() && canonical.starts_with(normalized_repo.canonicalize()?) {
            return Err(StorageError::RootInsideRepository);
        }
        Ok(Self { root: canonical })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, StorageError> {
        if relative.is_absolute()
            || relative.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(StorageError::Traversal);
        }
        let candidate = self.root.join(relative);
        let mut cursor = self.root.clone();
        for component in relative.components() {
            cursor.push(component);
            if let Ok(metadata) = fs::symlink_metadata(&cursor)
                && metadata.file_type().is_symlink()
            {
                return Err(StorageError::Symlink);
            }
        }
        if !candidate.starts_with(&self.root) {
            return Err(StorageError::EscapedRoot);
        }
        Ok(candidate)
    }
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, StorageError> {
    if !path.is_absolute() {
        return Err(StorageError::RelativeRoot);
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => result.push(Path::new("/")),
            Component::Normal(value) => result.push(value),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Prefix(_) => return Err(StorageError::Traversal),
        }
    }
    Ok(result)
}

/// Finder writes a `.DS_Store` file into any folder the operator opens. It is
/// view metadata, never meeting data, so a scan that refuses unexpected
/// entries skips it. Only a regular file with exactly this name qualifies; a
/// symlink or directory with the same name is still unexpected.
pub fn is_finder_metadata(name: &std::ffi::OsStr, file_type: &fs::FileType) -> bool {
    name == ".DS_Store" && file_type.is_file()
}

pub fn create_private_dir(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(io::Error::other("private directory may not be a symlink"));
    }
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
impl BoundPrivateDirectory {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        reject_unsafe_directory_path(path)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
            .open(path)?;
        let identity = require_private_directory(&file)?;
        if !path_binds_directory(path, &file)? {
            return Err(invalid_private_storage());
        }
        Ok(Self {
            path: path.to_path_buf(),
            file,
            binding: stable_binding(identity.identity),
        })
    }

    pub(crate) fn identity(&self) -> io::Result<PrivateObjectObservation> {
        self.revalidate()?;
        require_private_directory(&self.file)
    }

    pub(crate) fn sync_all(&self) -> io::Result<()> {
        self.revalidate()?;
        self.file.sync_all()
    }

    pub(crate) fn require_file_swap(&self) -> io::Result<()> {
        #[repr(C)]
        struct VolumeCapabilities {
            length: u32,
            capabilities: libc::vol_capabilities_attr_t,
        }

        const VOL_CAP_INT_RENAME_SWAP: u32 = 0x0004_0000;
        self.revalidate()?;
        let mut attributes = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: 0,
            volattr: libc::ATTR_VOL_CAPABILITIES,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        let mut capabilities: VolumeCapabilities = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::fgetattrlist(
                self.file.as_raw_fd(),
                (&mut attributes as *mut libc::attrlist).cast(),
                (&mut capabilities as *mut VolumeCapabilities).cast(),
                std::mem::size_of::<VolumeCapabilities>(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let interfaces = libc::VOL_CAPABILITIES_INTERFACES;
        if capabilities.capabilities.valid[interfaces] & VOL_CAP_INT_RENAME_SWAP == 0
            || capabilities.capabilities.capabilities[interfaces] & VOL_CAP_INT_RENAME_SWAP == 0
        {
            return Err(io::Error::other(
                "private storage volume does not support atomic file swap",
            ));
        }
        self.revalidate()
    }

    pub(crate) fn revalidate(&self) -> io::Result<()> {
        let identity = require_private_directory(&self.file)?;
        if stable_binding(identity.identity) != self.binding
            || !path_binds_directory(&self.path, &self.file)?
        {
            return Err(invalid_private_storage());
        }
        Ok(())
    }

    pub(crate) fn open_file(
        &self,
        name: &str,
        writable: bool,
        max_bytes: u64,
    ) -> io::Result<BoundPrivateFile> {
        self.revalidate()?;
        let name = private_child_name(name)?;
        let access = if writable {
            libc::O_RDWR
        } else {
            libc::O_RDONLY
        };
        let descriptor = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                access | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | O_UNIQUE,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned descriptor.
        let file = unsafe { File::from_raw_fd(descriptor) };
        let identity = require_private_file(&file, max_bytes)?;
        if !child_binds_file(&self.file, &name, &file)? {
            return Err(invalid_private_storage());
        }
        self.revalidate()?;
        Ok(BoundPrivateFile {
            name,
            file,
            binding: stable_binding(identity.identity),
        })
    }

    pub(crate) fn open_directory(&self, name: &str) -> io::Result<BoundPrivateDirectory> {
        self.revalidate()?;
        let name = private_child_name(name)?;
        let descriptor = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | O_UNIQUE,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned descriptor.
        let file = unsafe { File::from_raw_fd(descriptor) };
        let identity = require_private_directory(&file)?;
        if !child_binds_directory(&self.file, &name, &file)? {
            return Err(invalid_private_storage());
        }
        self.revalidate()?;
        Ok(BoundPrivateDirectory {
            path: self.path.join(Path::new(
                name.to_str().map_err(|_| invalid_private_storage())?,
            )),
            file,
            binding: stable_binding(identity.identity),
        })
    }

    pub(crate) fn create_directory(&self, name: &str) -> io::Result<BoundPrivateDirectory> {
        self.revalidate()?;
        let name = private_child_name(name)?;
        if unsafe { libc::mkdirat(self.file.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.file.sync_all()?;
        self.open_directory(name.to_str().map_err(|_| invalid_private_storage())?)
    }

    pub(crate) fn publish_directory_exclusive(
        &self,
        mut source: BoundPrivateDirectory,
        from: &str,
        to: &str,
    ) -> io::Result<BoundPrivateDirectory> {
        self.revalidate()?;
        let from = private_child_name(from)?;
        let to = private_child_name(to)?;
        let from_text = from.to_str().map_err(|_| invalid_private_storage())?;
        let to_text = to.to_str().map_err(|_| invalid_private_storage())?;
        source.revalidate()?;
        if source.path != self.path.join(from_text)
            || !child_binds_directory(&self.file, &from, &source.file)?
        {
            return Err(invalid_private_storage());
        }
        let result = unsafe {
            libc::renameatx_np(
                self.file.as_raw_fd(),
                from.as_ptr(),
                self.file.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_EXCL | RENAME_NOFOLLOW_ANY,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        let after = require_private_directory(&source.file)?;
        if stable_binding(after.identity) != source.binding
            || !child_binds_directory(&self.file, &to, &source.file)?
        {
            return Err(invalid_private_storage());
        }
        self.file.sync_all()?;
        self.revalidate()?;
        source.path = self.path.join(to_text);
        source.revalidate()?;
        Ok(source)
    }

    /// Atomically exchanges two already-bound private files, potentially
    /// across two private directories on the same volume. The returned first
    /// file is the original `left` inode now bound below `other_directory`;
    /// the returned second file is the original `right` inode now bound below
    /// `self`. No pathname is unlinked or overwritten.
    pub(crate) fn swap_files(
        &self,
        mut left: BoundPrivateFile,
        left_max_bytes: u64,
        other_directory: &BoundPrivateDirectory,
        mut right: BoundPrivateFile,
        right_max_bytes: u64,
    ) -> io::Result<(BoundPrivateFile, BoundPrivateFile)> {
        self.revalidate()?;
        other_directory.revalidate()?;
        left.revalidate(self, left_max_bytes)?;
        right.revalidate(other_directory, right_max_bytes)?;
        let left_name = left.name.clone();
        let right_name = right.name.clone();
        let left_identity = require_private_file(&left.file, left_max_bytes)?.identity;
        let right_identity = require_private_file(&right.file, right_max_bytes)?.identity;
        if left_identity.device != right_identity.device
            || stable_binding(left_identity) == stable_binding(right_identity)
        {
            return Err(invalid_private_storage());
        }

        let result = unsafe {
            libc::renameatx_np(
                self.file.as_raw_fd(),
                left_name.as_ptr(),
                other_directory.file.as_raw_fd(),
                right_name.as_ptr(),
                libc::RENAME_SWAP | RENAME_NOFOLLOW_ANY,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        if !child_binds_file(&other_directory.file, &right_name, &left.file)?
            || !child_binds_file(&self.file, &left_name, &right.file)?
        {
            return Err(invalid_private_storage());
        }
        self.file.sync_all()?;
        if stable_binding(require_private_directory(&self.file)?.identity)
            != stable_binding(require_private_directory(&other_directory.file)?.identity)
        {
            other_directory.file.sync_all()?;
        }
        self.revalidate()?;
        other_directory.revalidate()?;

        left.name = right_name;
        right.name = left_name;
        left.revalidate(other_directory, left_max_bytes)?;
        right.revalidate(self, right_max_bytes)?;
        Ok((left, right))
    }

    pub(crate) fn child_names(&self) -> io::Result<Vec<String>> {
        self.revalidate()?;
        let cursor = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if cursor < 0 {
            return Err(io::Error::last_os_error());
        }
        let directory = unsafe { libc::fdopendir(cursor) };
        if directory.is_null() {
            let error = io::Error::last_os_error();
            unsafe { libc::close(cursor) };
            return Err(error);
        }
        let mut names = Vec::new();
        loop {
            unsafe { *libc::__error() = 0 };
            let entry = unsafe { libc::readdir(directory) };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                unsafe { libc::closedir(directory) };
                if error.raw_os_error() == Some(0) {
                    break;
                }
                return Err(error);
            }
            let bytes = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." {
                let Ok(name) = std::str::from_utf8(bytes) else {
                    unsafe { libc::closedir(directory) };
                    return Err(invalid_private_storage());
                };
                names.push(name.to_owned());
            }
        }
        names.sort();
        self.revalidate()?;
        Ok(names)
    }

    pub(crate) fn create_zero_file(&self, name: &str) -> io::Result<BoundPrivateFile> {
        self.revalidate()?;
        let name = private_child_name(name)?;
        let descriptor = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | O_UNIQUE,
                0o600,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned descriptor.
        let file = unsafe { File::from_raw_fd(descriptor) };
        file.sync_all()?;
        let identity = require_private_file(&file, 0)?;
        if !child_binds_file(&self.file, &name, &file)? {
            return Err(invalid_private_storage());
        }
        self.file.sync_all()?;
        self.revalidate()?;
        Ok(BoundPrivateFile {
            name,
            file,
            binding: stable_binding(identity.identity),
        })
    }
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
impl BoundPrivateFile {
    pub(crate) fn raw_fd(&self) -> libc::c_int {
        self.file.as_raw_fd()
    }

    pub(crate) fn identity(
        &self,
        directory: &BoundPrivateDirectory,
        max_bytes: u64,
    ) -> io::Result<PrivateObjectObservation> {
        self.revalidate(directory, max_bytes)?;
        require_private_file(&self.file, max_bytes)
    }

    pub(crate) fn revalidate(
        &self,
        directory: &BoundPrivateDirectory,
        max_bytes: u64,
    ) -> io::Result<()> {
        directory.revalidate()?;
        let identity = require_private_file(&self.file, max_bytes)?;
        if stable_binding(identity.identity) != self.binding
            || !child_binds_file(&directory.file, &self.name, &self.file)?
        {
            return Err(invalid_private_storage());
        }
        directory.revalidate()?;
        Ok(())
    }

    pub(crate) fn sync_all(&self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub(crate) fn read_all(
        &self,
        directory: &BoundPrivateDirectory,
        max_bytes: u64,
    ) -> io::Result<Vec<u8>> {
        self.read_all_observed(directory, max_bytes, || {})
    }

    fn read_all_observed<F: FnOnce()>(
        &self,
        directory: &BoundPrivateDirectory,
        max_bytes: u64,
        after_read: F,
    ) -> io::Result<Vec<u8>> {
        let before = self.identity(directory, max_bytes)?;
        let mut reader = &self.file;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(before.size as usize);
        reader.take(max_bytes + 1).read_to_end(&mut bytes)?;
        after_read();
        if bytes.len() as u64 > max_bytes || self.identity(directory, max_bytes)? != before {
            return Err(invalid_private_storage());
        }
        Ok(bytes)
    }

    pub(crate) fn replace_bytes(
        &self,
        directory: &BoundPrivateDirectory,
        bytes: &[u8],
        max_bytes: u64,
    ) -> io::Result<()> {
        if bytes.len() as u64 > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "private file exceeds limit",
            ));
        }
        self.revalidate(directory, max_bytes)?;
        if unsafe { libc::ftruncate(self.file.as_raw_fd(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut written = 0;
        while written < bytes.len() {
            let count = unsafe {
                libc::pwrite(
                    self.file.as_raw_fd(),
                    bytes[written..].as_ptr().cast(),
                    bytes.len() - written,
                    written as libc::off_t,
                )
            };
            if count < 0 {
                return Err(io::Error::last_os_error());
            }
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "private file write stalled",
                ));
            }
            written += count as usize;
        }
        self.file.sync_all()?;
        self.revalidate(directory, max_bytes)
    }
}

#[cfg(target_os = "macos")]
fn reject_unsafe_directory_path(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid_private_storage());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn private_child_name(name: &str) -> io::Result<CString> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private child name is invalid",
        ));
    }
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "private child name contains a nul byte",
        )
    })
}

#[cfg(target_os = "macos")]
fn require_private_directory(file: &File) -> io::Result<PrivateObjectObservation> {
    use std::os::darwin::fs::MetadataExt as _;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || (metadata.mode() & 0o7777) != 0o700
        || metadata.st_flags() != 0
        || !xattrs_empty(file)?
        || !acl_empty(file)?
    {
        return Err(invalid_private_storage());
    }
    private_object_observation(&metadata)
}

#[cfg(target_os = "macos")]
fn require_private_file(file: &File, max_bytes: u64) -> io::Result<PrivateObjectObservation> {
    use std::os::darwin::fs::MetadataExt as _;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || (metadata.mode() & 0o7777) != 0o600
        || metadata.nlink() != 1
        || metadata.st_flags() != 0
        || metadata.len() > max_bytes
        || !xattrs_empty(file)?
        || !acl_empty(file)?
    {
        return Err(invalid_private_storage());
    }
    private_object_observation(&metadata)
}

#[cfg(target_os = "macos")]
fn private_object_observation(metadata: &fs::Metadata) -> io::Result<PrivateObjectObservation> {
    use std::os::darwin::fs::MetadataExt as _;
    let birth = metadata
        .created()?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid_private_storage())?;
    Ok(PrivateObjectObservation {
        identity: PrivateObjectIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            generation: u64::from(metadata.st_gen()),
            birth_seconds: birth.as_secs(),
            birth_nanoseconds: birth.subsec_nanos(),
            owner: metadata.uid(),
            mode: metadata.mode(),
            link_count: metadata.nlink(),
            flags: metadata.st_flags(),
        },
        size: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
    })
}

#[cfg(target_os = "macos")]
fn stable_binding(identity: PrivateObjectIdentity) -> StableObjectBinding {
    StableObjectBinding {
        device: identity.device,
        inode: identity.inode,
        generation: identity.generation,
        birth_seconds: identity.birth_seconds,
        birth_nanoseconds: identity.birth_nanoseconds,
    }
}

#[cfg(target_os = "macos")]
fn path_binds_directory(path: &Path, file: &File) -> io::Result<bool> {
    let reopened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
        .open(path)?;
    let path_identity = require_private_directory(&reopened)?.identity;
    let file_identity = require_private_directory(file)?.identity;
    Ok(stable_binding(path_identity) == stable_binding(file_identity))
}

#[cfg(target_os = "macos")]
fn child_binds_file(directory: &File, name: &CString, file: &File) -> io::Result<bool> {
    child_binds_object(directory, name, file)
}

#[cfg(target_os = "macos")]
fn child_binds_directory(directory: &File, name: &CString, file: &File) -> io::Result<bool> {
    child_binds_object(directory, name, file)
}

#[cfg(target_os = "macos")]
fn child_binds_object(directory: &File, name: &CString, file: &File) -> io::Result<bool> {
    let mut child = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            child.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatat initialized child after returning success.
    let child = unsafe { child.assume_init() };
    let file_metadata = file.metadata()?;
    use std::os::darwin::fs::MetadataExt as _;
    Ok(child.st_dev as u64 == file_metadata.dev()
        && child.st_ino == file_metadata.ino()
        && u64::from(child.st_gen) == u64::from(file_metadata.st_gen()))
}

#[cfg(target_os = "macos")]
fn xattrs_empty(file: &File) -> io::Result<bool> {
    let descriptor = file.as_raw_fd();
    let length = unsafe {
        libc::flistxattr(
            descriptor,
            std::ptr::null_mut(),
            0,
            libc::XATTR_SHOWCOMPRESSION,
        )
    };
    if length < 0 {
        return Err(io::Error::last_os_error());
    }
    if length == 0 {
        return Ok(true);
    }
    let mut names = vec![0_u8; length as usize];
    let copied = unsafe {
        libc::flistxattr(
            descriptor,
            names.as_mut_ptr().cast(),
            names.len(),
            libc::XATTR_SHOWCOMPRESSION,
        )
    };
    if copied != length {
        return Err(invalid_private_storage());
    }
    let names = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    // macOS attaches this platform marker to app-created private storage on
    // the supported systems. It carries no application authority. Every other
    // attribute, including resource forks, remains outside the safe shape.
    Ok(names.iter().all(|name| *name == b"com.apple.provenance"))
}

#[cfg(target_os = "macos")]
fn acl_empty(file: &File) -> io::Result<bool> {
    unsafe extern "C" {
        fn acl_get_fd_np(fd: i32, acl_type: i32) -> *mut libc::c_void;
        fn acl_get_entry(
            acl: *mut libc::c_void,
            entry_id: i32,
            entry: *mut *mut libc::c_void,
        ) -> i32;
        fn acl_free(object: *mut libc::c_void) -> i32;
    }
    const ACL_FIRST_ENTRY: i32 = 0;
    const ACL_TYPE_EXTENDED: i32 = 0x0000_0100;
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        return Ok(matches!(
            io::Error::last_os_error().raw_os_error(),
            Some(code) if code == libc::ENOATTR || code == libc::ENOENT
        ));
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) };
    let entry_errno = io::Error::last_os_error().raw_os_error();
    let _ = unsafe { acl_free(acl) };
    Ok(result == -1 && entry_errno == Some(libc::EINVAL))
}

#[cfg(target_os = "macos")]
fn invalid_private_storage() -> io::Error {
    io::Error::other("private storage object is unsafe or changed")
}

pub fn durable_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    create_private_dir(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::hard_link(&temporary, path)?;
        sync_directory(parent)?;
        fs::remove_file(&temporary)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn durable_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    create_private_dir(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn refuses_repository_root_and_traversal() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        assert!(matches!(
            StorageRoot::create(&repo.join("private"), &repo),
            Err(StorageError::RootInsideRepository)
        ));
        let app = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        assert!(matches!(
            app.resolve(Path::new("../escape")),
            Err(StorageError::Traversal)
        ));
    }

    #[test]
    fn private_modes_override_open_umask() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let app = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        let receipt = app.resolve(Path::new("meetings/a/attempt.json")).unwrap();
        durable_create_new(&receipt, b"{}\n").unwrap();
        assert_eq!(
            fs::metadata(app.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(receipt).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_objects_reject_aliases_and_namespace_swaps() {
        let temp = TempDir::new().unwrap();
        let temp_root = temp.path().canonicalize().unwrap();
        let directory_path = temp_root.join("profile");
        create_private_dir(&directory_path).unwrap();
        durable_create_new(&directory_path.join("voiceprint.json"), b"profile").unwrap();

        let directory = BoundPrivateDirectory::open(&directory_path).unwrap();
        let profile = directory.open_file("voiceprint.json", true, 64).unwrap();
        let identity = profile.identity(&directory, 64).unwrap();
        assert_eq!(identity.size, 7);
        assert_eq!(identity.identity.owner, unsafe { libc::geteuid() });
        assert_eq!(identity.identity.mode & 0o7777, 0o600);
        assert_eq!(identity.identity.link_count, 1);
        profile.sync_all().unwrap();

        assert!(
            directory
                .open_file("../voiceprint.json", false, 64)
                .is_err()
        );
        std::os::unix::fs::symlink(
            directory_path.join("voiceprint.json"),
            directory_path.join("alias.json"),
        )
        .unwrap();
        assert!(directory.open_file("alias.json", false, 64).is_err());

        let hard_link = directory_path.join("hard-link.json");
        fs::hard_link(directory_path.join("voiceprint.json"), &hard_link).unwrap();
        assert!(profile.revalidate(&directory, 64).is_err());
        fs::remove_file(hard_link).unwrap();

        let displaced = temp_root.join("profile-displaced");
        fs::rename(&directory_path, &displaced).unwrap();
        create_private_dir(&directory_path).unwrap();
        durable_create_new(&directory_path.join("voiceprint.json"), b"replacement").unwrap();
        assert!(directory.revalidate().is_err());
        assert!(profile.revalidate(&directory, 64).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_objects_reject_unsafe_ancestors_and_file_shapes() {
        let temp = TempDir::new().unwrap();
        let temp_root = temp.path().canonicalize().unwrap();
        let real_root = temp_root.join("real-root");
        let directory_path = real_root.join("profile");
        create_private_dir(&directory_path).unwrap();
        std::os::unix::fs::symlink(&real_root, temp_root.join("linked-root")).unwrap();
        assert!(BoundPrivateDirectory::open(&temp_root.join("linked-root/profile")).is_err());

        let directory = BoundPrivateDirectory::open(&directory_path).unwrap();
        durable_create_new(&directory_path.join("wrong-mode.json"), b"value").unwrap();
        fs::set_permissions(
            directory_path.join("wrong-mode.json"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(directory.open_file("wrong-mode.json", false, 64).is_err());

        durable_create_new(&directory_path.join("oversize.json"), b"12345").unwrap();
        assert!(directory.open_file("oversize.json", false, 4).is_err());

        durable_create_new(&directory_path.join("flagged.json"), b"value").unwrap();
        let flagged = File::open(directory_path.join("flagged.json")).unwrap();
        assert_eq!(
            unsafe { libc::fchflags(flagged.as_raw_fd(), libc::UF_NODUMP) },
            0
        );
        assert!(directory.open_file("flagged.json", false, 64).is_err());

        durable_create_new(&directory_path.join("attributed.json"), b"value").unwrap();
        let attributed = File::open(directory_path.join("attributed.json")).unwrap();
        let name = CString::new("user.local-meeting-notes-test").unwrap();
        let value = b"present";
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    attributed.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            },
            0
        );
        assert!(directory.open_file("attributed.json", false, 64).is_err());

        durable_create_new(&directory_path.join("acl.json"), b"value").unwrap();
        assert!(
            std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone deny read"])
                .arg(directory_path.join("acl.json"))
                .status()
                .unwrap()
                .success()
        );
        assert!(directory.open_file("acl.json", false, 64).is_err());
        assert!(
            std::process::Command::new("/bin/chmod")
                .arg("-N")
                .arg(directory_path.join("acl.json"))
                .status()
                .unwrap()
                .success()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_file_rejects_name_replacement() {
        let temp = TempDir::new().unwrap();
        let directory_path = temp.path().canonicalize().unwrap().join("profile");
        create_private_dir(&directory_path).unwrap();
        durable_create_new(&directory_path.join("receipt.json"), b"first").unwrap();
        let directory = BoundPrivateDirectory::open(&directory_path).unwrap();
        let receipt = directory.open_file("receipt.json", true, 64).unwrap();

        fs::rename(
            directory_path.join("receipt.json"),
            directory_path.join("receipt.displaced"),
        )
        .unwrap();
        durable_create_new(&directory_path.join("receipt.json"), b"second").unwrap();
        assert!(receipt.revalidate(&directory, 64).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_directory_creates_and_reopens_zero_slots() {
        let temp = TempDir::new().unwrap();
        let directory_path = temp.path().canonicalize().unwrap().join("profile");
        create_private_dir(&directory_path).unwrap();
        let directory = BoundPrivateDirectory::open(&directory_path).unwrap();

        let zero = directory.create_zero_file("reset.tombstone").unwrap();
        assert_eq!(zero.identity(&directory, 0).unwrap().size, 0);
        assert!(directory.create_zero_file("reset.tombstone").is_err());
        assert!(
            directory
                .create_zero_file("nested/reset.tombstone")
                .is_err()
        );

        let reopened = directory.open_file("reset.tombstone", true, 0).unwrap();
        assert_eq!(
            reopened.identity(&directory, 0).unwrap(),
            zero.identity(&directory, 0).unwrap()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_child_directory_rejects_replacement_and_publish_overwrite() {
        let temp = TempDir::new().unwrap();
        let parent_path = temp.path().canonicalize().unwrap().join("profile");
        create_private_dir(&parent_path).unwrap();
        let parent = BoundPrivateDirectory::open(&parent_path).unwrap();
        let child = parent.create_directory("initializing").unwrap();
        child.create_zero_file("receipt.a.json").unwrap();
        let lifecycle = parent
            .publish_directory_exclusive(child, "initializing", "lifecycle")
            .unwrap();
        assert_eq!(lifecycle.child_names().unwrap(), ["receipt.a.json"]);

        parent.create_directory("initializing").unwrap();
        assert!(
            parent
                .publish_directory_exclusive(
                    parent.open_directory("initializing").unwrap(),
                    "initializing",
                    "lifecycle",
                )
                .is_err()
        );

        fs::rename(
            parent_path.join("lifecycle"),
            parent_path.join("lifecycle.displaced"),
        )
        .unwrap();
        create_private_dir(&parent_path.join("lifecycle")).unwrap();
        assert!(lifecycle.revalidate().is_err());

        let candidate = parent.create_directory("candidate").unwrap();
        fs::rename(
            parent_path.join("candidate"),
            parent_path.join("candidate.displaced"),
        )
        .unwrap();
        create_private_dir(&parent_path.join("candidate")).unwrap();
        assert!(
            parent
                .publish_directory_exclusive(candidate, "candidate", "published")
                .is_err()
        );
        assert!(!parent_path.join("published").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_files_swap_across_retained_directories_without_unlinking() {
        let temp = TempDir::new().unwrap();
        let profile_path = temp.path().canonicalize().unwrap().join("profile");
        let lifecycle_path = profile_path.join("lifecycle");
        create_private_dir(&profile_path).unwrap();
        create_private_dir(&lifecycle_path).unwrap();
        durable_create_new(&profile_path.join("voiceprint.json"), b"profile-bytes").unwrap();
        durable_create_new(&lifecycle_path.join("reset.tombstone"), b"").unwrap();

        let profile = BoundPrivateDirectory::open(&profile_path).unwrap();
        let lifecycle = profile.open_directory("lifecycle").unwrap();
        let live = profile.open_file("voiceprint.json", true, 64).unwrap();
        let reset = lifecycle.open_file("reset.tombstone", true, 0).unwrap();
        let live_identity = live.identity(&profile, 64).unwrap().identity;
        let reset_identity = reset.identity(&lifecycle, 0).unwrap().identity;

        let (live_at_reset, reset_at_live) =
            profile.swap_files(live, 64, &lifecycle, reset, 0).unwrap();

        assert_eq!(fs::read(profile_path.join("voiceprint.json")).unwrap(), b"");
        assert_eq!(
            fs::read(lifecycle_path.join("reset.tombstone")).unwrap(),
            b"profile-bytes"
        );
        assert_eq!(
            live_at_reset.identity(&lifecycle, 64).unwrap().identity,
            live_identity
        );
        assert_eq!(
            reset_at_live.identity(&profile, 0).unwrap().identity,
            reset_identity
        );
        assert_eq!(fs::read_dir(&profile_path).unwrap().count(), 2);
        assert_eq!(fs::read_dir(&lifecycle_path).unwrap().count(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bound_private_read_detects_same_inode_same_length_mutation() {
        let temp = TempDir::new().unwrap();
        let directory_path = temp.path().canonicalize().unwrap().join("profile");
        create_private_dir(&directory_path).unwrap();
        durable_create_new(&directory_path.join("receipt.json"), b"first").unwrap();
        let directory = BoundPrivateDirectory::open(&directory_path).unwrap();
        let receipt = directory.open_file("receipt.json", false, 64).unwrap();

        assert!(
            receipt
                .read_all_observed(&directory, 64, || {
                    fs::write(directory_path.join("receipt.json"), b"other").unwrap();
                })
                .is_err()
        );
    }
}
