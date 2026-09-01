//! Descriptor-bound, optional organization metadata for the private library.
//!
//! A bad record is not repaired or interpreted: callers receive an unavailable
//! state and retain no label authority.  Its identity is private and
//! content-free, but changes whenever absence, safety, bytes, or validity
//! change.
//!
//! # It had no writer until 2026-08-08, and that was load-bearing
//!
//! This file opened with "this module deliberately has no writer" from the day
//! it was written.  The reader was built first on purpose — a record that can
//! only be read cannot be corrupted by this program — and the consequence was
//! that every meeting in the library read `Untitled meeting`, because the only
//! title source was a field nothing could set.  Auto-titling covered that on
//! 2026-08-07 with a derived label, and in doing so shipped a three-way
//! precedence whose top branch, the operator's own title, was still unreachable.
//!
//! The writer below is the reachable branch.  It is not a relaxation of the
//! reader: **every mutation is serialized, parsed back through this module's own
//! `MetadataWire` deserializer, and run through the same `validate` the reader
//! uses, before a byte is replaced.**  A draft that this file's reader would
//! refuse is an internal error here, refused without writing, rather than a
//! record the next read quarantines.  One module owns the record's shape, and
//! the writer proves it agrees with the reader on every call instead of being
//! reviewed for agreeing with it.
//!
//! Mutation requires the process writer lock, through
//! [`crate::retention::AppDataWriterLock::library_organization_authority`].
//! The raw entry points are crate-private for the same reason every other
//! destructive path here is.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::meeting::valid_opaque_id;
use crate::storage::StorageRoot;

const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const METADATA_NAME: &[u8] = b"metadata.json\0";
/// Darwin's `<sys/fcntl.h>` O_UNIQUE. libc 0.2.189 does not expose it.
#[cfg(target_os = "macos")]
const O_UNIQUE: libc::c_int = 0x2000;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum MetadataState {
    Missing { identity: MetadataIdentity },
    Valid(MetadataDocument),
    Unavailable { identity: MetadataIdentity },
}

impl MetadataState {
    pub(crate) fn identity(&self) -> MetadataIdentity {
        match self {
            Self::Missing { identity } | Self::Unavailable { identity } => identity.clone(),
            Self::Valid(document) => document.identity.clone(),
        }
    }
    pub(crate) fn document(&self) -> Option<&MetadataDocument> {
        match self {
            Self::Valid(document) => Some(document),
            _ => None,
        }
    }
    pub(crate) fn unavailable_after_relative_validation(&self) -> Self {
        let mut bytes = b"library-metadata/1:relative-validation-failed:".to_vec();
        bytes.extend_from_slice(&self.identity().0);
        Self::Unavailable {
            identity: identity(&bytes),
        }
    }
}

/// Never exposed through a library result.  It includes a digest so equal
/// revisions with different canonical bytes still invalidate old handles.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MetadataIdentity([u8; 32]);

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MetadataDocument {
    pub(crate) revision: u64,
    pub(crate) folders: Vec<Folder>,
    pub(crate) meetings: Vec<MeetingMetadata>,
    identity: MetadataIdentity,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MeetingMetadata {
    pub(crate) meeting_id: String,
    pub(crate) title: Option<String>,
    pub(crate) folder_id: Option<String>,
}

pub(crate) fn read_library_metadata(storage: &StorageRoot) -> MetadataState {
    #[cfg(target_os = "macos")]
    {
        read_macos(storage.path())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = storage;
        MetadataState::Unavailable {
            identity: identity(b"library-metadata/1:unsupported"),
        }
    }
}

fn identity(bytes: &[u8]) -> MetadataIdentity {
    MetadataIdentity(Sha256::digest(bytes).into())
}

#[cfg(target_os = "macos")]
fn read_macos(root: &Path) -> MetadataState {
    let library = root.join("library");
    let directory = match open_library_dir(&library) {
        Ok(Some(directory)) => directory,
        Ok(None) => {
            return MetadataState::Missing {
                identity: identity(b"library-metadata/1:absent"),
            };
        }
        Err(()) => return unavailable_identity(root, b"unsafe-library"),
    };
    let directory_before = match safe_directory(&directory) {
        Ok(identity) => identity,
        Err(()) => return unavailable_identity(root, b"unsafe-library"),
    };
    let mut file = match open_metadata(&directory) {
        Ok(Some(file)) => file,
        Ok(None) => {
            if same_path_fd(&library, &directory)
                && safe_directory(&directory)
                    .map(|after| after == directory_before)
                    .unwrap_or(false)
                && metadata_child_is_missing(&directory).unwrap_or(false)
            {
                return MetadataState::Missing {
                    identity: identity(b"library-metadata/1:absent"),
                };
            }
            return unavailable_identity(root, b"missing-state-changed");
        }
        Err(()) => return unavailable_identity(root, b"unsafe-file"),
    };
    let metadata = match safe_file(&file) {
        Ok(metadata) => metadata,
        Err(()) => return unavailable_identity(root, b"unsafe-file"),
    };
    let before = fd_identity(&metadata);
    if before.size > MAX_METADATA_BYTES {
        return unavailable_identity(root, b"oversize");
    }
    let mut bytes = Vec::with_capacity(before.size as usize);
    if file
        .by_ref()
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_METADATA_BYTES
    {
        return unavailable_identity(root, b"read-failed");
    }
    // Both descriptor/path bindings are repeated after the bounded read; no
    // authority survives a name swap or a changed directory binding.
    if !same_path_fd(&library, &directory)
        || safe_directory(&directory)
            .map(|after| after != directory_before)
            .unwrap_or(true)
        || !same_child_fd(&library.join("metadata.json"), &file)
        || safe_file(&file)
            .map(|after| fd_identity(&after) != before)
            .unwrap_or(true)
    {
        return unavailable_identity(root, b"binding-changed");
    }
    let byte_identity = identity(&bytes);
    match serde_json::from_slice::<MetadataWire>(&bytes)
        .and_then(|wire| validate(wire).map_err(de::Error::custom))
    {
        Ok(wire) => MetadataState::Valid(MetadataDocument {
            revision: wire.revision,
            folders: wire.folders,
            meetings: wire.meetings,
            identity: byte_identity,
        }),
        Err(_) => MetadataState::Unavailable {
            identity: byte_identity,
        },
    }
}

#[cfg(target_os = "macos")]
fn unavailable_identity(root: &Path, reason: &[u8]) -> MetadataState {
    use std::os::darwin::fs::MetadataExt as _;
    let mut fingerprint = Vec::from(b"library-metadata/1:".as_slice());
    fingerprint.extend_from_slice(reason);
    for path in [root.join("library"), root.join("library/metadata.json")] {
        match fs::symlink_metadata(path) {
            Ok(metadata) => fingerprint.extend_from_slice(
                format!(
                    ":{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    metadata.dev(),
                    metadata.ino(),
                    metadata.st_gen(),
                    metadata.uid(),
                    metadata.mode(),
                    metadata.nlink(),
                    metadata.len(),
                    metadata.st_flags(),
                    metadata.mtime(),
                    metadata.mtime_nsec(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
                .as_bytes(),
            ),
            Err(error) => fingerprint.extend_from_slice(
                format!(":error:{}", error.raw_os_error().unwrap_or_default()).as_bytes(),
            ),
        }
    }
    MetadataState::Unavailable {
        identity: identity(&fingerprint),
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
struct FdIdentity {
    dev: u64,
    ino: u64,
    generation: u64,
    mode: u32,
    uid: u32,
    nlink: u64,
    flags: u32,
    size: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    dev: u64,
    ino: u64,
    generation: u64,
    mode: u32,
    uid: u32,
    flags: u32,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}
#[cfg(target_os = "macos")]
fn fd_identity(metadata: &fs::Metadata) -> FdIdentity {
    use std::os::darwin::fs::MetadataExt as _;
    FdIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        generation: u64::from(metadata.st_gen()),
        mode: metadata.mode(),
        uid: metadata.uid(),
        nlink: metadata.nlink(),
        flags: metadata.st_flags(),
        size: metadata.len(),
        mtime: metadata.mtime(),
        mtime_nsec: metadata.mtime_nsec(),
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
    }
}

#[cfg(target_os = "macos")]
fn safe_directory(file: &File) -> Result<DirectoryIdentity, ()> {
    use std::os::darwin::fs::MetadataExt as _;
    let metadata = file.metadata().map_err(|_| ())?;
    if !safe_directory_metadata(&metadata) {
        return Err(());
    }
    Ok(DirectoryIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        generation: u64::from(metadata.st_gen()),
        mode: metadata.mode(),
        uid: metadata.uid(),
        flags: metadata.st_flags(),
        mtime: metadata.mtime(),
        mtime_nsec: metadata.mtime_nsec(),
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
    })
}

#[cfg(target_os = "macos")]
fn open_library_dir(path: &Path) -> Result<Option<File>, ()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(metadata) if !safe_directory_metadata(&metadata) => return Err(()),
        Ok(_) => {}
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| ())?;
    if !same_path_fd(path, &file) || !safe_directory_metadata(&file.metadata().map_err(|_| ())?) {
        return Err(());
    }
    Ok(Some(file))
}

#[cfg(target_os = "macos")]
fn open_metadata(directory: &File) -> Result<Option<File>, ()> {
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            METADATA_NAME.as_ptr().cast(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | O_UNIQUE,
        )
    };
    if fd < 0 {
        return match io::Error::last_os_error().kind() {
            io::ErrorKind::NotFound => Ok(None),
            _ => Err(()),
        };
    }
    // SAFETY: openat returned a distinct owned descriptor.
    Ok(Some(unsafe { File::from_raw_fd(fd) }))
}

#[cfg(target_os = "macos")]
fn metadata_child_is_missing(directory: &File) -> Result<bool, ()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            METADATA_NAME.as_ptr().cast(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        return Ok(false);
    }
    match io::Error::last_os_error().raw_os_error() {
        Some(code) if code == libc::ENOENT => Ok(true),
        _ => Err(()),
    }
}

#[cfg(target_os = "macos")]
fn safe_directory_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == unsafe { libc::geteuid() }
        && (metadata.mode() & 0o7777) == 0o700
}

#[cfg(target_os = "macos")]
fn safe_file(file: &File) -> Result<fs::Metadata, ()> {
    use std::os::darwin::fs::MetadataExt as _;
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || (metadata.mode() & 0o7777) != 0o600
        || metadata.nlink() != 1
        || metadata.st_flags() != 0
    {
        return Err(());
    }
    if !xattrs_empty(file)? || !acl_empty(file)? {
        return Err(());
    }
    Ok(metadata)
}

#[cfg(target_os = "macos")]
fn same_path_fd(path: &Path, file: &File) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .zip(file.metadata().ok())
        .is_some_and(|(path, fd)| path.dev() == fd.dev() && path.ino() == fd.ino())
}
#[cfg(target_os = "macos")]
fn same_child_fd(path: &Path, file: &File) -> bool {
    same_path_fd(path, file)
}

#[cfg(target_os = "macos")]
fn xattrs_empty(file: &File) -> Result<bool, ()> {
    let fd = file.as_raw_fd();
    let length =
        unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0, libc::XATTR_SHOWCOMPRESSION) };
    if length < 0 {
        return Err(());
    }
    if length == 0 {
        return Ok(true);
    }
    let mut names = vec![0_u8; length as usize];
    let copied = unsafe {
        libc::flistxattr(
            fd,
            names.as_mut_ptr().cast(),
            names.len(),
            libc::XATTR_SHOWCOMPRESSION,
        )
    };
    if copied != length {
        return Err(());
    }
    let names: Vec<_> = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .collect();
    // macOS attaches this platform marker to app-created private storage on
    // the supported systems. It carries no application authority. Every other
    // attribute, including resource forks, remains outside the safe shape.
    Ok(names.iter().all(|name| *name == b"com.apple.provenance"))
}

#[cfg(target_os = "macos")]
fn acl_empty(file: &File) -> Result<bool, ()> {
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
        return Ok(
            matches!(io::Error::last_os_error().raw_os_error(), Some(code) if code == libc::ENOATTR || code == libc::ENOENT),
        );
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) };
    let entry_errno = io::Error::last_os_error().raw_os_error();
    let _ = unsafe { acl_free(acl) };
    // 0 means an entry was returned.  An empty allocated ACL ends with -1 and
    // EINVAL; any other result/error remains unsafe.
    Ok(result == -1 && entry_errno == Some(libc::EINVAL))
}

struct MetadataWire {
    revision: u64,
    folders: Vec<Folder>,
    meetings: Vec<MeetingMetadata>,
}
impl<'de> Deserialize<'de> for MetadataWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MetadataVisitor)
    }
}
struct MetadataVisitor;
impl<'de> Visitor<'de> for MetadataVisitor {
    type Value = MetadataWire;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("library-metadata/1 in canonical key order")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        expect(&mut map, "schema")?;
        let schema: String = map.next_value()?;
        if schema != "library-metadata/1" {
            return Err(de::Error::custom("wrong schema"));
        }
        expect(&mut map, "revision")?;
        let revision = map.next_value()?;
        expect(&mut map, "folders")?;
        let folders = map.next_value()?;
        expect(&mut map, "meetings")?;
        let meetings = map.next_value()?;
        if map.next_key::<String>()?.is_some() {
            return Err(de::Error::custom("unknown or duplicate field"));
        }
        Ok(MetadataWire {
            revision,
            folders,
            meetings,
        })
    }
}
impl<'de> Deserialize<'de> for Folder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(FolderVisitor)
    }
}
struct FolderVisitor;
impl<'de> Visitor<'de> for FolderVisitor {
    type Value = Folder;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("folder in canonical key order")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Folder, A::Error> {
        expect(&mut map, "id")?;
        let id = map.next_value()?;
        expect(&mut map, "name")?;
        let name = map.next_value()?;
        if map.next_key::<String>()?.is_some() {
            return Err(de::Error::custom("unknown or duplicate field"));
        }
        Ok(Folder { id, name })
    }
}
impl<'de> Deserialize<'de> for MeetingMetadata {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MeetingMetadataVisitor)
    }
}
struct MeetingMetadataVisitor;
impl<'de> Visitor<'de> for MeetingMetadataVisitor {
    type Value = MeetingMetadata;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("meeting metadata in canonical key order")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MeetingMetadata, A::Error> {
        expect(&mut map, "meeting_id")?;
        let meeting_id = map.next_value()?;
        expect(&mut map, "title")?;
        let title = map.next_value()?;
        expect(&mut map, "folder_id")?;
        let folder_id = map.next_value()?;
        if map.next_key::<String>()?.is_some() {
            return Err(de::Error::custom("unknown or duplicate field"));
        }
        Ok(MeetingMetadata {
            meeting_id,
            title,
            folder_id,
        })
    }
}
fn expect<'de, A: MapAccess<'de>>(map: &mut A, expected: &str) -> Result<(), A::Error> {
    match map.next_key::<String>()? {
        Some(key) if key == expected => Ok(()),
        _ => Err(de::Error::custom(
            "missing, duplicate, unknown, or out-of-order field",
        )),
    }
}

fn validate(wire: MetadataWire) -> Result<MetadataWire, &'static str> {
    let mut prior = None;
    for folder in &wire.folders {
        if !canonical_uuid(&folder.id)
            || !valid_label(&folder.name)
            || prior
                .as_ref()
                .is_some_and(|previous: &String| previous >= &folder.id)
        {
            return Err("invalid folders");
        }
        prior = Some(folder.id.clone());
    }
    let mut prior_meeting = None;
    for row in &wire.meetings {
        if !valid_opaque_id(&row.meeting_id)
            || prior_meeting
                .as_ref()
                .is_some_and(|previous: &String| previous >= &row.meeting_id)
            || row.title.as_ref().is_some_and(|name| !valid_label(name))
            || row
                .folder_id
                .as_ref()
                .is_some_and(|id| !wire.folders.iter().any(|folder| &folder.id == id))
        {
            return Err("invalid meetings");
        }
        prior_meeting = Some(row.meeting_id.clone());
    }
    Ok(wire)
}
// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// What one mutation did. The exact four values the runtime contract names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationOutcome {
    pub revision: u64,
    /// False for a semantic no-op, which writes nothing and leaves the revision
    /// where it was. Renaming a folder to the name it already has is not an
    /// event, and spending a revision on it would invalidate every outstanding
    /// snapshot handle for no change.
    pub changed: bool,
    /// The affected folder, or `None` for a title-only mutation. A changed
    /// `create_folder` returns the ID it generated.
    pub folder_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrganizationError {
    #[error("library organization request is invalid")]
    InvalidRequest,
    #[error("library organization metadata is unavailable")]
    MetadataUnavailable,
    #[error("library organization metadata revision conflict")]
    RevisionConflict { current_revision: u64 },
    #[error("library organization internal error")]
    Internal,
}

/// A folder or title name as this record stores it, or nothing.
///
/// **NFC normalization and trimming are applied; everything else is refused.**
/// The split is deliberate. Canonical composition and surrounding whitespace
/// are invisible to the person typing — the glyphs are identical either way, and
/// a trailing space is not a decision they made — so silently fixing them costs
/// nothing they can perceive. A control character, a slash or a line separator
/// *is* something they typed, and quietly deleting it would hand back a name
/// that is not the one on screen. That is refused instead.
fn normalized_label(value: &str) -> Option<String> {
    let candidate = unicode_normalization(value)
        .trim_matches(char::is_whitespace)
        .to_owned();
    valid_label(&candidate).then_some(candidate)
}

#[derive(Serialize)]
struct FolderOut<'a> {
    id: &'a str,
    name: &'a str,
}

#[derive(Serialize)]
struct MeetingOut<'a> {
    meeting_id: &'a str,
    title: Option<&'a str>,
    folder_id: Option<&'a str>,
}

/// Field order is declaration order in `serde_json`, and this record's reader
/// refuses any other order. That coupling is why these types exist rather than
/// a `serde_json::Value`, whose map does not preserve insertion order without a
/// feature flag this crate does not enable.
#[derive(Serialize)]
struct DocumentOut<'a> {
    schema: &'a str,
    revision: u64,
    folders: Vec<FolderOut<'a>>,
    meetings: Vec<MeetingOut<'a>>,
}

/// The mutable working copy one edit sees.
struct Draft {
    folders: Vec<Folder>,
    meetings: Vec<MeetingMetadata>,
}

impl Draft {
    /// Drops rows carrying neither a title nor a folder.
    ///
    /// Such a row is legal and means nothing: the reader treats an absent row
    /// and a row of two nulls identically. Keeping them would grow the record
    /// with every unfiled meeting and, because the reader refuses the whole
    /// document when any row names a meeting that is not safely projected, would
    /// widen the surface on which a later deletion can quarantine everything.
    fn prune(&mut self) {
        self.meetings
            .retain(|row| row.title.is_some() || row.folder_id.is_some());
    }

    fn row_mut(&mut self, meeting_id: &str) -> Option<&mut MeetingMetadata> {
        self.meetings
            .iter_mut()
            .find(|row| row.meeting_id == meeting_id)
    }

    fn insert_row(&mut self, row: MeetingMetadata) {
        let at = self
            .meetings
            .partition_point(|existing| existing.meeting_id < row.meeting_id);
        self.meetings.insert(at, row);
    }
}

/// Reads, edits, validates and replaces — or returns without writing.
///
/// The edit closure returns the affected folder ID. It never decides whether
/// anything changed: that is compared structurally against the record as read,
/// so an edit that believes it changed something and did not cannot spend a
/// revision.
fn mutate(
    storage: &StorageRoot,
    expected_revision: u64,
    edit: impl FnOnce(&mut Draft) -> Result<Option<String>, OrganizationError>,
) -> Result<OrganizationOutcome, OrganizationError> {
    let state = read_library_metadata(storage);
    let (current_revision, folders, meetings) = match &state {
        MetadataState::Missing { .. } => (0, Vec::new(), Vec::new()),
        MetadataState::Valid(document) => (
            document.revision,
            document.folders.clone(),
            document.meetings.clone(),
        ),
        // A malformed record is left exactly as it is. Rewriting it would
        // destroy whatever a person might still recover from it by hand, and
        // this program cannot tell a corrupted record from one written by
        // something it does not know about.
        MetadataState::Unavailable { .. } => return Err(OrganizationError::MetadataUnavailable),
    };
    if expected_revision != current_revision {
        return Err(OrganizationError::RevisionConflict { current_revision });
    }

    let mut draft = Draft {
        folders: folders.clone(),
        meetings: meetings.clone(),
    };
    let folder_id = edit(&mut draft)?;
    draft.prune();

    if draft.folders == folders && draft.meetings == meetings {
        return Ok(OrganizationOutcome {
            revision: current_revision,
            changed: false,
            folder_id,
        });
    }

    let revision = current_revision
        .checked_add(1)
        .ok_or(OrganizationError::InvalidRequest)?;
    let bytes = serialize(revision, &draft)?;
    write_document(storage, &bytes)?;
    Ok(OrganizationOutcome {
        revision,
        changed: true,
        folder_id,
    })
}

/// Canonical bytes, then this module's own reader run over them.
///
/// The round trip is the point. Field order, sort order, the folder a meeting
/// names, label validity and the size ceiling are all enforced by `validate`
/// and the `MetadataWire` visitor, and re-running them here means the writer
/// cannot emit a document the reader would quarantine — not because the two
/// were reviewed against each other, but because the same code ran.
fn serialize(revision: u64, draft: &Draft) -> Result<Vec<u8>, OrganizationError> {
    let document = DocumentOut {
        schema: "library-metadata/1",
        revision,
        folders: draft
            .folders
            .iter()
            .map(|folder| FolderOut {
                id: &folder.id,
                name: &folder.name,
            })
            .collect(),
        meetings: draft
            .meetings
            .iter()
            .map(|row| MeetingOut {
                meeting_id: &row.meeting_id,
                title: row.title.as_deref(),
                folder_id: row.folder_id.as_deref(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&document).map_err(|_| OrganizationError::Internal)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(OrganizationError::InvalidRequest);
    }
    serde_json::from_slice::<MetadataWire>(&bytes)
        .map_err(|_| OrganizationError::Internal)
        .and_then(|wire| validate(wire).map_err(|_| OrganizationError::Internal))?;
    Ok(bytes)
}

/// Creates `library/` at 0700 if it is absent, then replaces the record.
///
/// The directory can legitimately not exist: a missing record is revision zero
/// with no rows, which is the state every storage root starts in.
fn write_document(storage: &StorageRoot, bytes: &[u8]) -> Result<(), OrganizationError> {
    let library = storage
        .resolve(Path::new("library"))
        .map_err(|_| OrganizationError::Internal)?;
    if !library.exists() {
        crate::storage::create_private_dir(&library).map_err(|_| OrganizationError::Internal)?;
    }
    crate::storage::durable_replace(&library.join("metadata.json"), bytes)
        .map_err(|_| OrganizationError::Internal)
}

/// True when this meeting has a record the library projection can reach.
///
/// The writer refuses to create a row for anything else, because
/// `library_read` grants the whole document authority only while **every** row
/// targets a safely projected meeting. One row for a meeting that is not there
/// does not lose one title; it makes every title and folder in the record
/// disappear at once. Refusing at the writer is the cheap end of that.
fn meeting_is_present(storage: &StorageRoot, meeting_id: &str) -> bool {
    valid_opaque_id(meeting_id)
        && storage
            .resolve(&Path::new("meetings").join(meeting_id).join("meeting.json"))
            .map(|path| path.is_file())
            .unwrap_or(false)
}

pub(crate) fn create_folder(
    storage: &StorageRoot,
    expected_revision: u64,
    name: &str,
) -> Result<OrganizationOutcome, OrganizationError> {
    let name = normalized_label(name).ok_or(OrganizationError::InvalidRequest)?;
    let id = Uuid::new_v4().to_string();
    let created = id.clone();
    mutate(storage, expected_revision, move |draft| {
        // Duplicate names are allowed. Two folders called "Clients" are the
        // operator's business, and the ID is what anything here binds to.
        let at = draft.folders.partition_point(|folder| folder.id < id);
        draft.folders.insert(
            at,
            Folder {
                id: id.clone(),
                name,
            },
        );
        Ok(Some(id))
    })
    .map(|mut outcome| {
        // A create that changed nothing is impossible — a fresh v4 is never
        // already present — so this only guards against a future edit making it
        // possible without noticing.
        if !outcome.changed {
            outcome.folder_id = None;
        }
        let _ = &created;
        outcome
    })
}

pub(crate) fn rename_folder(
    storage: &StorageRoot,
    expected_revision: u64,
    folder_id: &str,
    name: &str,
) -> Result<OrganizationOutcome, OrganizationError> {
    if !canonical_uuid(folder_id) {
        return Err(OrganizationError::InvalidRequest);
    }
    let name = normalized_label(name).ok_or(OrganizationError::InvalidRequest)?;
    mutate(storage, expected_revision, |draft| {
        let folder = draft
            .folders
            .iter_mut()
            .find(|folder| folder.id == folder_id)
            .ok_or(OrganizationError::InvalidRequest)?;
        folder.name = name;
        Ok(Some(folder_id.to_owned()))
    })
}

pub(crate) fn delete_folder(
    storage: &StorageRoot,
    expected_revision: u64,
    folder_id: &str,
) -> Result<OrganizationOutcome, OrganizationError> {
    if !canonical_uuid(folder_id) {
        return Err(OrganizationError::InvalidRequest);
    }
    mutate(storage, expected_revision, |draft| {
        if !draft.folders.iter().any(|folder| folder.id == folder_id) {
            return Err(OrganizationError::InvalidRequest);
        }
        draft.folders.retain(|folder| folder.id != folder_id);
        // Atomically, in the same replacement: a row still naming a folder that
        // is gone fails `validate`, so this is not tidying, it is the only way
        // the deletion can be written at all.
        for row in &mut draft.meetings {
            if row.folder_id.as_deref() == Some(folder_id) {
                row.folder_id = None;
            }
        }
        Ok(Some(folder_id.to_owned()))
    })
}

pub(crate) fn assign_meeting_folder(
    storage: &StorageRoot,
    expected_revision: u64,
    meeting_id: &str,
    folder_id: Option<&str>,
) -> Result<OrganizationOutcome, OrganizationError> {
    if !meeting_is_present(storage, meeting_id) {
        return Err(OrganizationError::InvalidRequest);
    }
    if folder_id.is_some_and(|id| !canonical_uuid(id)) {
        return Err(OrganizationError::InvalidRequest);
    }
    mutate(storage, expected_revision, |draft| {
        if let Some(id) = folder_id
            && !draft.folders.iter().any(|folder| folder.id == id)
        {
            return Err(OrganizationError::InvalidRequest);
        }
        match draft.row_mut(meeting_id) {
            Some(row) => row.folder_id = folder_id.map(str::to_owned),
            None => draft.insert_row(MeetingMetadata {
                meeting_id: meeting_id.to_owned(),
                title: None,
                folder_id: folder_id.map(str::to_owned),
            }),
        }
        Ok(folder_id.map(str::to_owned))
    })
}

pub(crate) fn set_meeting_title(
    storage: &StorageRoot,
    expected_revision: u64,
    meeting_id: &str,
    title: Option<&str>,
) -> Result<OrganizationOutcome, OrganizationError> {
    if !meeting_is_present(storage, meeting_id) {
        return Err(OrganizationError::InvalidRequest);
    }
    let title = match title {
        Some(value) => Some(normalized_label(value).ok_or(OrganizationError::InvalidRequest)?),
        None => None,
    };
    mutate(storage, expected_revision, |draft| {
        match draft.row_mut(meeting_id) {
            Some(row) => row.title = title,
            None => draft.insert_row(MeetingMetadata {
                meeting_id: meeting_id.to_owned(),
                title,
                folder_id: None,
            }),
        }
        // Null for a title-only mutation, per the contract, even when the row
        // happens to sit in a folder.
        Ok(None)
    })
}

/// Removes one meeting's row, for the staged whole-meeting deletion.
///
/// **Ordered before `meeting.json` is removed, and that ordering is the whole
/// point.** `library_read` grants the record authority only while every row
/// targets a safely projected meeting, so a row outliving its meeting does not
/// leave one stale title behind — it makes every title and folder in the library
/// unavailable at once, and disables organization mutation with them.
///
/// It carries no `expected_revision`. The caller already holds the process
/// writer lock and the meeting's lease, and a conflict here would strand a
/// deletion mid-sequence with no one to resolve it.
///
/// Idempotent by construction: no row is a no-op that writes nothing, so a crash
/// between this and the `staged` transition resumes cleanly. A malformed record
/// is left untouched and reported as removed, because it was already refused by
/// every reader before this deletion began and rewriting it would destroy what a
/// person might still recover by hand.
pub(crate) fn forget_meeting(storage: &StorageRoot, meeting_id: &str) -> Result<(), ()> {
    let current = match read_library_metadata(storage) {
        MetadataState::Valid(document) => document.revision,
        MetadataState::Missing { .. } | MetadataState::Unavailable { .. } => return Ok(()),
    };
    match mutate(storage, current, |draft| {
        draft.meetings.retain(|row| row.meeting_id != meeting_id);
        Ok(None)
    }) {
        Ok(_) => Ok(()),
        Err(_) => Err(()),
    }
}

/// Writes back a title and folder remembered from before a meeting was
/// trashed, for the restore path only.
///
/// Mirrors `forget_meeting`'s self-contained revision handling: the caller
/// already holds the process writer lock and the meeting's lease, so there is
/// no concurrent writer to conflict with, and a conflict here would strand a
/// restore with no one to resolve it. It differs from `forget_meeting` in one
/// way that matters — **it must run after the meeting directory is back on
/// disk**, because `meeting_is_present` (which both `set_meeting_title` and
/// `assign_meeting_folder` require) can only see a meeting that has already
/// been moved back to `meetings/<id>`.
///
/// A folder the operator deleted while the meeting sat in trash is dropped
/// silently rather than refusing the whole restore: losing a folder
/// assignment is a smaller failure than a meeting that cannot come back.
///
/// **Guarded on `meeting_is_present`, same as `set_meeting_title` and
/// `assign_meeting_folder` above.** Skipping that guard was the actual bug: a
/// caller that reinstates before confirming the meeting is really back on
/// disk (for instance, after a purge finished mid-crash and left a `Trashed`
/// receipt pointing at nothing) would otherwise write a row for a meeting
/// that isn't there — which does not strand one title, it makes
/// `library_read` quarantine every title and folder in the record at once.
/// A caller-side check is not a substitute for this one; this is the only
/// place that can refuse atomically with the write.
pub(crate) fn reinstate_meeting_organization(
    storage: &StorageRoot,
    meeting_id: &str,
    title: Option<&str>,
    folder_id: Option<&str>,
) -> Result<(), ()> {
    if title.is_none() && folder_id.is_none() {
        return Ok(());
    }
    if !meeting_is_present(storage, meeting_id) {
        return Err(());
    }
    let current = match read_library_metadata(storage) {
        MetadataState::Valid(document) => document.revision,
        MetadataState::Missing { .. } => 0,
        // An unreadable record cannot be safely rewritten; the restore itself
        // already succeeded by the time this runs, so the meeting comes back
        // without its remembered name rather than not coming back at all.
        MetadataState::Unavailable { .. } => return Ok(()),
    };
    let title_owned = title.map(str::to_owned);
    let folder_owned = folder_id.map(str::to_owned);
    match mutate(storage, current, move |draft| {
        let resolved_folder = folder_owned
            .as_deref()
            .filter(|id| draft.folders.iter().any(|folder| folder.id == *id))
            .map(str::to_owned);
        match draft.row_mut(meeting_id) {
            Some(row) => {
                row.title = title_owned.clone();
                row.folder_id = resolved_folder;
            }
            None => draft.insert_row(MeetingMetadata {
                meeting_id: meeting_id.to_owned(),
                title: title_owned.clone(),
                folder_id: resolved_folder,
            }),
        }
        Ok(None)
    }) {
        Ok(_) => Ok(()),
        Err(_) => Err(()),
    }
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|id| {
            id.to_string() == value
                && id.get_version_num() == 4
                && id.get_variant() == uuid::Variant::RFC4122
        })
        .unwrap_or(false)
}
fn valid_label(value: &str) -> bool {
    let nfc: String = unicode_normalization(value);
    value == nfc
        && value == value.trim_matches(char::is_whitespace)
        && (1..=120).contains(&value.chars().count())
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | '\u{2028}' | '\u{2029}'))
}
fn unicode_normalization(value: &str) -> String {
    use icu_normalizer::ComposingNormalizer;
    let normalizer = ComposingNormalizer::new_nfc();
    let mut out = String::new();
    normalizer
        .normalize_to(value, &mut out)
        .expect("string write");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_wire() -> MetadataWire {
        MetadataWire {
            revision: 7,
            folders: vec![Folder {
                id: "11111111-1111-4111-8111-111111111111".into(),
                name: "Folder".into(),
            }],
            meetings: vec![MeetingMetadata {
                meeting_id: "meeting-a".into(),
                title: Some("Title".into()),
                folder_id: Some("11111111-1111-4111-8111-111111111111".into()),
            }],
        }
    }

    fn assert_invalid(wire: MetadataWire) {
        assert!(validate(wire).is_err());
    }
    #[test]
    fn parser_refuses_order_duplicates_and_noncanonical_rows() {
        let good = br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[]}"#;
        assert!(
            serde_json::from_slice::<MetadataWire>(good)
                .and_then(|v| validate(v).map_err(de::Error::custom))
                .is_ok()
        );
        for bad in [
            br#"{"revision":0,"schema":"library-metadata/1","folders":[],"meetings":[]}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[],"x":0}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[],"meetings":[]}"#.as_slice(),
        ] { assert!(serde_json::from_slice::<MetadataWire>(bad).is_err()); }
    }

    #[test]
    fn parser_refuses_every_label_authority_violation() {
        let rejected: &[&[u8]] = &[
            // Nested order, unknown fields, duplicate nested keys, and required nullable fields.
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"name":"needle","id":"11111111-1111-4111-8111-111111111111"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"needle","extra":true}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","id":"11111111-1111-4111-8111-111111111111","name":"needle"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"needle"}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","folder_id":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"needle","folder_id":null,"extra":true}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"needle","title":"needle","folder_id":null}]}"#,
            // Sorting/uniqueness, canonical identifiers, and every label constraint.
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"22222222-2222-4222-8222-222222222222","name":"needle"},{"id":"11111111-1111-4111-8111-111111111111","name":"later"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-11111111111A","name":"needle"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":" e\u0301 "}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"needle/unsafe"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-b","title":"needle","folder_id":null},{"meeting_id":"meeting-a","title":"later","folder_id":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"needle","folder_id":"11111111-1111-4111-8111-111111111111"}]}"#,
        ];
        for bytes in rejected {
            assert!(
                serde_json::from_slice::<MetadataWire>(bytes)
                    .and_then(|wire| validate(wire).map_err(de::Error::custom))
                    .is_err()
            );
        }
        let too_long = format!(
            "{{\"schema\":\"library-metadata/1\",\"revision\":0,\"folders\":[{{\"id\":\"11111111-1111-4111-8111-111111111111\",\"name\":\"{}\"}}],\"meetings\":[]}}",
            "n".repeat(121)
        );
        assert!(
            serde_json::from_str::<MetadataWire>(&too_long)
                .and_then(|wire| validate(wire).map_err(de::Error::custom))
                .is_err()
        );
    }

    #[test]
    fn validator_isolates_uuid_row_and_label_rules() {
        assert!(validate(valid_wire()).is_ok());
        assert!(canonical_uuid("11111111-1111-4111-8111-111111111111"));
        for invalid in [
            "00000000-0000-0000-0000-000000000000", // nil / non-v4
            "11111111-1111-1111-8111-111111111111", // v1
            "11111111-1111-4111-0111-111111111111", // NCS variant
            "11111111-1111-4111-8111-11111111111A", // non-lowercase
        ] {
            assert!(!canonical_uuid(invalid));
        }

        let mut duplicate_folder = valid_wire();
        duplicate_folder
            .folders
            .push(duplicate_folder.folders[0].clone());
        assert_invalid(duplicate_folder);
        let mut unsorted_folders = valid_wire();
        unsorted_folders.folders.insert(
            0,
            Folder {
                id: "22222222-2222-4222-8222-222222222222".into(),
                name: "Later".into(),
            },
        );
        assert_invalid(unsorted_folders);
        let mut duplicate_meeting = valid_wire();
        duplicate_meeting
            .meetings
            .push(duplicate_meeting.meetings[0].clone());
        assert_invalid(duplicate_meeting);
        let mut unsorted_meetings = valid_wire();
        unsorted_meetings.meetings.insert(
            0,
            MeetingMetadata {
                meeting_id: "meeting-b".into(),
                title: Some("Other".into()),
                folder_id: None,
            },
        );
        assert_invalid(unsorted_meetings);
        let mut opaque_id = valid_wire();
        opaque_id.meetings[0].meeting_id = "../unsafe".into();
        assert_invalid(opaque_id);
        let mut bad_reference = valid_wire();
        bad_reference.meetings[0].folder_id = Some("22222222-2222-4222-8222-222222222222".into());
        assert_invalid(bad_reference);

        for bad_label in [
            "",
            "e\u{301}",
            " title",
            "title\\path",
            "line\u{2028}break",
            "line\u{2029}break",
            "control\u{0001}",
        ] {
            let mut title = valid_wire();
            title.meetings[0].title = Some(bad_label.into());
            assert_invalid(title);
        }
        let mut too_long = valid_wire();
        too_long.folders[0].name = "n".repeat(121);
        assert_invalid(too_long);
    }

    #[test]
    fn parser_isolates_missing_and_nested_field_rules() {
        for bytes in [
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","folder_id":null}]}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":null}]}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":null,"folder_id":null,"extra":true}]}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":null,"title":null,"folder_id":null}]}"#.as_slice(),
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"Folder","name":"Again"}],"meetings":[]}"#.as_slice(),
        ] {
            assert!(serde_json::from_slice::<MetadataWire>(bytes).is_err());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn arbitrary_xattrs_resource_forks_and_flags_are_never_safe() {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let temp = tempfile::TempDir::new().unwrap();
        for (number, name) in [
            b"com.example.lmn\0".as_slice(),
            b"com.apple.ResourceFork\0".as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            let path = temp.path().join(format!("xattr-{number}"));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .unwrap();
            assert_eq!(
                unsafe {
                    libc::fsetxattr(
                        file.as_raw_fd(),
                        name.as_ptr().cast(),
                        b"x".as_ptr().cast(),
                        1,
                        0,
                        0,
                    )
                },
                0
            );
            assert!(safe_file(&file).is_err());
        }
        let path = temp.path().join("flags");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert_eq!(
            unsafe { libc::fchflags(file.as_raw_fd(), libc::UF_NODUMP) },
            0
        );
        assert!(safe_file(&file).is_err());
        assert_eq!(unsafe { libc::fchflags(file.as_raw_fd(), 0) }, 0);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn directory_identity_detects_metadata_namespace_changes_and_unsafe_modes() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let temp = tempfile::TempDir::new().unwrap();
        let directory = temp.path().join("library");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let fd = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&directory)
            .unwrap();
        let before = safe_directory(&fd).unwrap();
        fs::write(directory.join("metadata.json"), b"{}").unwrap();
        assert!(safe_directory(&fd).unwrap() != before);
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(safe_directory(&fd).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn directory_path_binding_rejects_a_replaced_missing_namespace() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let temp = tempfile::TempDir::new().unwrap();
        let library = temp.path().join("library");
        fs::create_dir(&library).unwrap();
        fs::set_permissions(&library, fs::Permissions::from_mode(0o700)).unwrap();
        let fd = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&library)
            .unwrap();
        fs::rename(&library, temp.path().join("old-library")).unwrap();
        fs::create_dir(&library).unwrap();
        fs::set_permissions(&library, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!same_path_fd(&library, &fd));
        assert!(metadata_child_is_missing(&fd).unwrap());
    }

    // ------------------------------------------------------------------
    // Writer
    // ------------------------------------------------------------------

    use crate::storage::{create_private_dir, durable_create_new};
    use tempfile::TempDir;

    struct WriteFixture {
        _temporary: TempDir,
        storage: StorageRoot,
    }

    impl WriteFixture {
        fn new() -> Self {
            let temporary = TempDir::new().unwrap();
            let protected = temporary.path().join("protected");
            create_private_dir(&protected).unwrap();
            let storage =
                StorageRoot::create(&temporary.path().join("app-data"), &protected).unwrap();
            Self {
                _temporary: temporary,
                storage,
            }
        }

        /// The writer refuses a row for a meeting it cannot see, so a test that
        /// sets a title has to put one there. Only `meeting.json` matters to
        /// that check; nothing here loads it.
        fn meeting(&self, id: &str) -> &Self {
            let directory = self.storage.path().join("meetings").join(id);
            create_private_dir(&directory).unwrap();
            durable_create_new(&directory.join("meeting.json"), b"{}\n").unwrap();
            self
        }

        fn record_bytes(&self) -> Option<Vec<u8>> {
            fs::read(self.storage.path().join("library/metadata.json")).ok()
        }

        fn document(&self) -> MetadataDocument {
            match read_library_metadata(&self.storage) {
                MetadataState::Valid(document) => document,
                _ => panic!("record is not valid"),
            }
        }
    }

    const FOLDER_NAME: &str = "Clients";

    #[test]
    fn a_created_folder_is_readable_by_this_modules_own_reader() {
        let fixture = WriteFixture::new();
        let outcome = create_folder(&fixture.storage, 0, FOLDER_NAME).unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.revision, 1);
        let id = outcome.folder_id.clone().unwrap();
        assert!(canonical_uuid(&id));

        let document = fixture.document();
        assert_eq!(document.revision, 1);
        assert_eq!(document.folders.len(), 1);
        assert_eq!(document.folders[0].name, FOLDER_NAME);
        assert_eq!(document.folders[0].id, id);
    }

    #[test]
    fn the_first_write_creates_the_library_directory_that_did_not_exist() {
        let fixture = WriteFixture::new();
        assert!(fixture.record_bytes().is_none());
        create_folder(&fixture.storage, 0, FOLDER_NAME).unwrap();
        let library = fixture.storage.path().join("library");
        let mode = fs::metadata(&library).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700, "library directory is not owner-only");
        let file = fs::metadata(library.join("metadata.json")).unwrap();
        assert_eq!(file.mode() & 0o777, 0o600);
        assert_eq!(file.nlink(), 1);
    }

    #[test]
    fn a_stale_expected_revision_is_refused_and_reports_the_current_one() {
        let fixture = WriteFixture::new();
        create_folder(&fixture.storage, 0, FOLDER_NAME).unwrap();
        let before = fixture.record_bytes().unwrap();
        assert_eq!(
            create_folder(&fixture.storage, 0, "Other"),
            Err(OrganizationError::RevisionConflict {
                current_revision: 1
            })
        );
        assert_eq!(
            fixture.record_bytes().unwrap(),
            before,
            "a refused mutation wrote bytes"
        );
    }

    #[test]
    fn a_semantic_no_op_writes_nothing_and_keeps_the_revision() {
        let fixture = WriteFixture::new();
        let id = create_folder(&fixture.storage, 0, FOLDER_NAME)
            .unwrap()
            .folder_id
            .unwrap();
        let before = fixture.record_bytes().unwrap();

        let outcome = rename_folder(&fixture.storage, 1, &id, FOLDER_NAME).unwrap();
        assert!(!outcome.changed);
        assert_eq!(outcome.revision, 1, "a no-op spent a revision");
        assert_eq!(
            fixture.record_bytes().unwrap(),
            before,
            "a no-op replaced the record"
        );

        // And an unfile of a meeting that was never filed.
        fixture.meeting("meeting-a");
        let outcome = assign_meeting_folder(&fixture.storage, 1, "meeting-a", None).unwrap();
        assert!(!outcome.changed);
        assert_eq!(fixture.record_bytes().unwrap(), before);
    }

    #[test]
    fn an_operator_title_survives_the_round_trip_and_clears_back_to_nothing() {
        let fixture = WriteFixture::new();
        fixture.meeting("meeting-a");

        let outcome =
            set_meeting_title(&fixture.storage, 0, "meeting-a", Some("Renewal call")).unwrap();
        assert!(outcome.changed);
        assert_eq!(
            outcome.folder_id, None,
            "a title-only mutation reports no folder"
        );
        let document = fixture.document();
        assert_eq!(document.meetings[0].title.as_deref(), Some("Renewal call"));

        let outcome = set_meeting_title(&fixture.storage, 1, "meeting-a", None).unwrap();
        assert!(outcome.changed);
        assert!(
            fixture.document().meetings.is_empty(),
            "a row carrying neither title nor folder was kept"
        );
    }

    #[test]
    fn names_are_trimmed_and_composed_but_never_stripped_of_what_was_typed() {
        let fixture = WriteFixture::new();
        // Decomposed e-acute plus surrounding whitespace: both invisible to the
        // person typing, so both are fixed rather than refused.
        let id = create_folder(&fixture.storage, 0, "  Cafe\u{0301} notes  ")
            .unwrap()
            .folder_id
            .unwrap();
        assert_eq!(fixture.document().folders[0].name, "Caf\u{00e9} notes");
        let _ = id;

        // Everything below is something the operator can see on screen, so it
        // is refused rather than silently removed.
        for hostile in [
            "with/slash",
            "with\\backslash",
            "with\u{2028}separator",
            "with\u{7}control",
            "",
            "   ",
        ] {
            assert_eq!(
                create_folder(&fixture.storage, 1, hostile),
                Err(OrganizationError::InvalidRequest),
                "{hostile:?} was accepted"
            );
        }
        let too_long = "a".repeat(121);
        assert_eq!(
            create_folder(&fixture.storage, 1, &too_long),
            Err(OrganizationError::InvalidRequest)
        );
        assert!(create_folder(&fixture.storage, 1, &"a".repeat(120)).is_ok());
    }

    #[test]
    fn deleting_a_folder_unfiles_every_meeting_in_it_in_the_same_replacement() {
        let fixture = WriteFixture::new();
        fixture.meeting("meeting-a").meeting("meeting-b");
        let id = create_folder(&fixture.storage, 0, FOLDER_NAME)
            .unwrap()
            .folder_id
            .unwrap();
        assign_meeting_folder(&fixture.storage, 1, "meeting-a", Some(&id)).unwrap();
        assign_meeting_folder(&fixture.storage, 2, "meeting-b", Some(&id)).unwrap();
        set_meeting_title(&fixture.storage, 3, "meeting-a", Some("Kept")).unwrap();

        let outcome = delete_folder(&fixture.storage, 4, &id).unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.folder_id.as_deref(), Some(id.as_str()));

        let document = fixture.document();
        assert!(document.folders.is_empty());
        // meeting-b held nothing but the folder, so its row goes with it;
        // meeting-a keeps its title and loses only the folder.
        assert_eq!(document.meetings.len(), 1);
        assert_eq!(document.meetings[0].meeting_id, "meeting-a");
        assert_eq!(document.meetings[0].title.as_deref(), Some("Kept"));
        assert_eq!(document.meetings[0].folder_id, None);
    }

    #[test]
    fn the_writer_refuses_a_row_the_reader_would_quarantine() {
        let fixture = WriteFixture::new();
        // No meeting directory exists, so a row naming it would make the whole
        // record unavailable on the next read.
        assert_eq!(
            set_meeting_title(&fixture.storage, 0, "meeting-a", Some("Ghost")),
            Err(OrganizationError::InvalidRequest)
        );
        assert!(fixture.record_bytes().is_none());

        fixture.meeting("meeting-a");
        assert_eq!(
            assign_meeting_folder(
                &fixture.storage,
                0,
                "meeting-a",
                Some("11111111-1111-4111-8111-111111111111")
            ),
            Err(OrganizationError::InvalidRequest),
            "a meeting was filed into a folder that does not exist"
        );
    }

    #[test]
    fn rows_and_folders_are_written_in_the_canonical_order_the_reader_demands() {
        let fixture = WriteFixture::new();
        for id in ["meeting-c", "meeting-a", "meeting-b"] {
            fixture.meeting(id);
        }
        let mut revision = 0;
        for id in ["meeting-c", "meeting-a", "meeting-b"] {
            revision = set_meeting_title(&fixture.storage, revision, id, Some("Title"))
                .unwrap()
                .revision;
        }
        for _ in 0..3 {
            revision = create_folder(&fixture.storage, revision, FOLDER_NAME)
                .unwrap()
                .revision;
        }
        let document = fixture.document();
        let meetings: Vec<_> = document
            .meetings
            .iter()
            .map(|row| row.meeting_id.as_str())
            .collect();
        assert_eq!(meetings, ["meeting-a", "meeting-b", "meeting-c"]);
        let mut folders: Vec<_> = document.folders.iter().map(|f| f.id.clone()).collect();
        let sorted = {
            let mut copy = folders.clone();
            copy.sort();
            copy
        };
        assert_eq!(folders, sorted, "folders are not sorted by id");
        folders.dedup();
        assert_eq!(folders.len(), 3);
    }

    #[test]
    fn a_malformed_record_is_refused_and_left_exactly_as_it_was() {
        let fixture = WriteFixture::new();
        fixture.meeting("meeting-a");
        let library = fixture.storage.path().join("library");
        create_private_dir(&library).unwrap();
        let corrupt =
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[],"x":1}"#;
        durable_create_new(&library.join("metadata.json"), corrupt).unwrap();

        assert_eq!(
            set_meeting_title(&fixture.storage, 0, "meeting-a", Some("Anything")),
            Err(OrganizationError::MetadataUnavailable)
        );
        assert_eq!(
            fixture.record_bytes().unwrap(),
            corrupt,
            "a malformed record was rewritten"
        );
    }

    #[test]
    fn forgetting_a_meeting_removes_only_its_row_and_tolerates_every_other_state() {
        let fixture = WriteFixture::new();
        fixture.meeting("meeting-a").meeting("meeting-b");

        // No record at all: nothing to do, and nothing created.
        forget_meeting(&fixture.storage, "meeting-a").unwrap();
        assert!(fixture.record_bytes().is_none());

        set_meeting_title(&fixture.storage, 0, "meeting-a", Some("Going")).unwrap();
        set_meeting_title(&fixture.storage, 1, "meeting-b", Some("Staying")).unwrap();

        forget_meeting(&fixture.storage, "meeting-a").unwrap();
        let document = fixture.document();
        assert_eq!(document.meetings.len(), 1);
        assert_eq!(document.meetings[0].title.as_deref(), Some("Staying"));
        assert_eq!(document.revision, 3);

        // Idempotent: a crash between this and the staged transition replays it.
        let before = fixture.record_bytes().unwrap();
        forget_meeting(&fixture.storage, "meeting-a").unwrap();
        assert_eq!(fixture.record_bytes().unwrap(), before);
    }

    #[test]
    fn every_mutation_is_parsed_back_through_the_readers_own_validator() {
        // `serialize` is the only path to a byte on disk, so proving it refuses
        // an invalid draft proves the writer cannot publish one. The draft below
        // names a folder that is not in the document, which `validate` rejects.
        let draft = Draft {
            folders: Vec::new(),
            meetings: vec![MeetingMetadata {
                meeting_id: "meeting-a".into(),
                title: None,
                folder_id: Some("11111111-1111-4111-8111-111111111111".into()),
            }],
        };
        assert_eq!(serialize(1, &draft), Err(OrganizationError::Internal));

        let draft = Draft {
            folders: Vec::new(),
            meetings: vec![
                MeetingMetadata {
                    meeting_id: "meeting-b".into(),
                    title: Some("B".into()),
                    folder_id: None,
                },
                MeetingMetadata {
                    meeting_id: "meeting-a".into(),
                    title: Some("A".into()),
                    folder_id: None,
                },
            ],
        };
        assert_eq!(
            serialize(1, &draft),
            Err(OrganizationError::Internal),
            "an out-of-order draft serialized"
        );
    }

    #[test]
    fn a_revision_at_the_ceiling_refuses_rather_than_wrapping_to_zero() {
        let fixture = WriteFixture::new();
        fixture.meeting("meeting-a");
        let library = fixture.storage.path().join("library");
        create_private_dir(&library).unwrap();
        let at_ceiling = format!(
            r#"{{"schema":"library-metadata/1","revision":{},"folders":[],"meetings":[]}}"#,
            u64::MAX
        );
        durable_create_new(&library.join("metadata.json"), at_ceiling.as_bytes()).unwrap();
        assert_eq!(
            set_meeting_title(&fixture.storage, u64::MAX, "meeting-a", Some("Overflow")),
            Err(OrganizationError::InvalidRequest)
        );
        assert_eq!(fixture.record_bytes().unwrap(), at_ceiling.as_bytes());
    }
}
