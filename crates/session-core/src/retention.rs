use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::library_metadata::{OrganizationError, OrganizationOutcome};
use crate::meeting::{
    AUDIO_ARTIFACT_PATHS, ArtifactRef, AudioState, MAX_RECEIPT_BYTES, MeetingError,
    MeetingLifecycle, MeetingRecord, PendingStorageOperation, artifact_ref, load_meeting,
    open_private_file, read_private_bytes, require_private_directory, resolve_artifact,
    valid_opaque_id, verify_artifact_ref, verify_record_artifacts, verify_record_static_artifacts,
    write_meeting,
};
use crate::meeting_coordination::{MeetingCoordinationError, MeetingStorageCoordination};
use crate::operation_store::{OperationStore, OperationStoreError, StoredOperationRequest};
#[cfg(target_os = "macos")]
use crate::profile_lifecycle::{
    ProfileEnrollmentOutcome, ProfileLifecycleBaseline, ProfileLifecycleError, ProfileResetOutcome,
    enroll_profile_lifecycle_bound, initialize_profile_lifecycle_bound,
    preserve_legacy_profile_bound, reset_profile_lifecycle_bound,
};
use crate::storage::{
    BoundPrivateDirectory, BoundPrivateFile, StorageRoot, create_private_dir, durable_create_new,
    durable_replace, sync_directory,
};
use crate::transcript_retry::TranscriptRetryAuthority;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioDeletionReceipt {
    schema: AudioDeletionSchema,
    capture_session_sha256: String,
    state: DeletionState,
    artifacts: Vec<DeletedArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum AudioDeletionSchema {
    #[serde(rename = "audio-deletion/1")]
    V1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum DeletionState {
    Deleting,
    Staged,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedArtifact {
    relative_name: String,
    byte_size: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetentionOutcome {
    DeferredActive(String),
    NotDue(String),
    AudioReleased(String),
    RecoveredRemoval(String),
    Quarantined(String),
}

/// Result of the application-private, per-meeting audio-release action.
///
/// This deliberately reports only storage authority states. It neither changes a
/// retention policy nor deletes the meeting record, transcript, note, profile,
/// or another meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualAudioDeletionOutcome {
    DeferredActive,
    AudioReleased,
    RecoveredRemoval,
    AlreadyReleased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingRetentionResult {
    NotDue,
    AudioReleased,
    RecoveredRemoval,
}

#[derive(Debug, Error)]
pub enum RetentionError {
    #[error("audio state is unexplained")]
    UnexplainedAudioLoss,
    #[error("audio deletion receipt is malformed")]
    MalformedReceipt,
    #[error("capture audio is not a regular private file")]
    InvalidAudio,
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum ManualAudioDeletionError {
    #[error("meeting storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Retention(#[from] RetentionError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    OperationStore(#[from] OperationStoreError),
    #[error("meeting has a nonterminal product operation")]
    NonterminalProductOperation,
}

/// The sole capability that may authorize immediate audio release.
///
/// A caller obtains it only by borrowing an [`AppDataWriterLock`] that already
/// holds the owner-only process lock for this storage root. Its lifetime keeps
/// that lock alive through the core's target lease, sequence gate, operation
/// scan, and staged deletion/recovery path.
///
/// ```compile_fail
/// use local_meeting_notes_session_core::retention::delete_meeting_audio_manually;
/// // The raw mutation entry point is crate-private. A dependent crate must
/// // acquire AppDataWriterLock and borrow its deletion_authority instead.
/// # let _ = delete_meeting_audio_manually;
/// ```
///
/// ```compile_fail
/// use std::sync::Arc;
/// use local_meeting_notes_session_core::meeting_coordination::MeetingStorageCoordination;
/// use local_meeting_notes_session_core::retention::AppDataWriterLock;
/// use local_meeting_notes_session_core::storage::StorageRoot;
/// fn inject_registry(root: &StorageRoot, registry: Arc<MeetingStorageCoordination>) {
///     let _ = AppDataWriterLock::acquire(root, registry);
/// }
/// ```
///
/// ```compile_fail
/// use local_meeting_notes_session_core::retention::AppDataWriterLock;
/// use local_meeting_notes_session_core::storage::StorageRoot;
/// fn redirect_root(lock: &AppDataWriterLock, other_root: &StorageRoot) {
///     let _ = lock.deletion_authority().delete_audio(other_root, "meeting");
/// }
/// ```
pub struct ManualAudioDeletionAuthority<'a> {
    lock: &'a AppDataWriterLock,
}

/// The sole capability that may initialize or reopen profile lifecycle state.
///
/// It borrows the process writer lock and admits no caller-provided storage
/// path or coordination registry. The operation holds the global storage
/// sequence and refuses while any meeting capture lease is active.
///
/// ```compile_fail
/// use local_meeting_notes_session_core::profile_lifecycle::initialize_profile_lifecycle_bound;
/// # let _ = initialize_profile_lifecycle_bound;
/// ```
#[cfg(target_os = "macos")]
pub struct ProfileLifecycleAuthority<'a> {
    lock: &'a AppDataWriterLock,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Error)]
pub enum ProfileLifecycleAdmissionError {
    #[error("profile lifecycle storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error("profile lifecycle writer authority was lost")]
    AuthorityLost,
    #[error("profile lifecycle is blocked by an active meeting")]
    ActiveMeeting,
    #[error(transparent)]
    Lifecycle(#[from] ProfileLifecycleError),
}

#[cfg(target_os = "macos")]
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ProfileEnrollmentWorkerError {
    #[error("profile enrollment worker refused the candidate")]
    Refused,
    #[error("profile enrollment worker is unavailable")]
    Unavailable,
}

#[cfg(target_os = "macos")]
pub trait ProfileEnrollmentWorker: Send + Sync {
    fn inspect_candidate(&self, operation_id: &str)
    -> Result<String, ProfileEnrollmentWorkerError>;

    fn discard_candidate(
        &self,
        operation_id: &str,
        profile_sha256: &str,
    ) -> Result<String, ProfileEnrollmentWorkerError>;
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileEnrollmentCompletion {
    Enrolled {
        operation_id: String,
        profile_sha256: String,
    },
    CleanupPending {
        operation_id: String,
        profile_sha256: String,
    },
}

#[cfg(target_os = "macos")]
#[derive(Debug, Error)]
pub enum ProfileEnrollmentAdmissionError {
    #[error("profile enrollment storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error("profile enrollment writer authority was lost")]
    AuthorityLost,
    #[error("profile enrollment is blocked by an active meeting")]
    ActiveMeeting,
    #[error("profile enrollment candidate is missing, changed, or unsafe")]
    CandidateUnsafe,
    #[error(transparent)]
    Worker(#[from] ProfileEnrollmentWorkerError),
    #[error(transparent)]
    Lifecycle(#[from] ProfileLifecycleError),
}

#[cfg(target_os = "macos")]
fn valid_sha256_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(target_os = "macos")]
impl ProfileLifecycleAuthority<'_> {
    pub fn initialize_or_open(
        &self,
    ) -> Result<ProfileLifecycleBaseline, ProfileLifecycleAdmissionError> {
        self.run(initialize_profile_lifecycle_bound)
    }

    /// Preserves an existing safe legacy live slot as the sequence-zero
    /// baseline. It does not validate, activate, replace, or delete its bytes.
    pub fn preserve_legacy_for_review(
        &self,
    ) -> Result<ProfileLifecycleBaseline, ProfileLifecycleAdmissionError> {
        self.run(preserve_legacy_profile_bound)
    }

    /// Removes only the account-level profile bytes through the fixed rolling
    /// lifecycle journal. Meetings are never opened by this action.
    pub fn reset_profile(
        &self,
        operation_id: &str,
        requested_at: u64,
    ) -> Result<ProfileResetOutcome, ProfileLifecycleAdmissionError> {
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(ProfileLifecycleAdmissionError::ActiveMeeting);
        }
        let outcome =
            reset_profile_lifecycle_bound(&self.lock.storage_root, operation_id, requested_at);
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        outcome.map_err(Into::into)
    }

    /// Publishes exact profile bytes that the caller has already passed through
    /// the canonical semantic loader. The supplied digest is re-derived before
    /// any lifecycle mutation; meetings are never opened by this action.
    #[allow(dead_code)] // exposed only after the strict-loader bridge owns admission
    pub(crate) fn enroll_admitted_profile(
        &self,
        operation_id: &str,
        requested_at: u64,
        admitted_profile: &[u8],
        admitted_sha256: &str,
    ) -> Result<ProfileEnrollmentOutcome, ProfileLifecycleAdmissionError> {
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(ProfileLifecycleAdmissionError::ActiveMeeting);
        }
        let outcome = enroll_profile_lifecycle_bound(
            &self.lock.storage_root,
            operation_id,
            requested_at,
            admitted_profile,
            admitted_sha256,
        );
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        outcome.map_err(Into::into)
    }

    /// Strictly admits one private candidate. The worker first validates the
    /// canonical profile semantics and returns its digest. Rust then reopens the
    /// exact private candidate through retained descriptors, re-derives that
    /// digest, publishes only those bytes, and asks the worker to remove the
    /// candidate. A cleanup failure never rolls back active profile authority.
    pub fn enroll_profile_candidate(
        &self,
        worker: &dyn ProfileEnrollmentWorker,
        operation_id: &str,
        requested_at: u64,
    ) -> Result<ProfileEnrollmentCompletion, ProfileEnrollmentAdmissionError> {
        const MAX_PROFILE_BYTES: u64 = 4 * 1024 * 1024;
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(ProfileEnrollmentAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(ProfileEnrollmentAdmissionError::ActiveMeeting);
        }

        let inspected_sha256 = worker.inspect_candidate(operation_id)?;
        if !valid_sha256_text(&inspected_sha256) {
            return Err(ProfileEnrollmentAdmissionError::CandidateUnsafe);
        }
        let candidates = self
            .lock
            .storage_root
            .open_directory("profile-candidates")
            .map_err(|_| ProfileEnrollmentAdmissionError::CandidateUnsafe)?;
        let candidate = candidates
            .open_directory(operation_id)
            .map_err(|_| ProfileEnrollmentAdmissionError::CandidateUnsafe)?;
        if candidate
            .child_names()
            .map_err(|_| ProfileEnrollmentAdmissionError::CandidateUnsafe)?
            != ["voiceprint.json"]
        {
            return Err(ProfileEnrollmentAdmissionError::CandidateUnsafe);
        }
        let profile = candidate
            .open_file("voiceprint.json", false, MAX_PROFILE_BYTES)
            .map_err(|_| ProfileEnrollmentAdmissionError::CandidateUnsafe)?;
        let profile_bytes = profile
            .read_all(&candidate, MAX_PROFILE_BYTES)
            .map_err(|_| ProfileEnrollmentAdmissionError::CandidateUnsafe)?;
        if profile_bytes.is_empty()
            || format!("{:x}", Sha256::digest(&profile_bytes)) != inspected_sha256
        {
            return Err(ProfileEnrollmentAdmissionError::CandidateUnsafe);
        }
        let outcome = enroll_profile_lifecycle_bound(
            &self.lock.storage_root,
            operation_id,
            requested_at,
            &profile_bytes,
            &inspected_sha256,
        )?;
        if !authority_is_current()
            || candidates.revalidate().is_err()
            || candidate.revalidate().is_err()
        {
            return Err(ProfileEnrollmentAdmissionError::AuthorityLost);
        }
        let (operation_id, profile_sha256) = match outcome {
            ProfileEnrollmentOutcome::Activated {
                operation_id,
                profile_sha256,
            }
            | ProfileEnrollmentOutcome::Recovered {
                operation_id,
                profile_sha256,
            }
            | ProfileEnrollmentOutcome::AlreadyActive {
                operation_id,
                profile_sha256,
            } => (operation_id, profile_sha256),
        };
        let cleanup = worker.discard_candidate(&operation_id, &profile_sha256);
        if matches!(cleanup, Ok(ref digest) if digest == &profile_sha256) {
            Ok(ProfileEnrollmentCompletion::Enrolled {
                operation_id,
                profile_sha256,
            })
        } else {
            Ok(ProfileEnrollmentCompletion::CleanupPending {
                operation_id,
                profile_sha256,
            })
        }
    }

    /// Retries only cleanup for an already-active enrollment receipt. It never
    /// opens or rewrites the live profile bytes.
    pub fn cleanup_active_enrollment_candidate(
        &self,
        worker: &dyn ProfileEnrollmentWorker,
    ) -> Result<ProfileEnrollmentCompletion, ProfileEnrollmentAdmissionError> {
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(ProfileEnrollmentAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(ProfileEnrollmentAdmissionError::ActiveMeeting);
        }
        let lifecycle = initialize_profile_lifecycle_bound(&self.lock.storage_root)?;
        let operation_id = lifecycle
            .active_enrollment_id()
            .ok_or(ProfileEnrollmentAdmissionError::CandidateUnsafe)?
            .to_string();
        let profile_sha256 = lifecycle.profile_sha256().to_string();
        let cleanup = worker.discard_candidate(&operation_id, &profile_sha256);
        if !authority_is_current() {
            return Err(ProfileEnrollmentAdmissionError::AuthorityLost);
        }
        if matches!(cleanup, Ok(ref digest) if digest == &profile_sha256) {
            Ok(ProfileEnrollmentCompletion::Enrolled {
                operation_id,
                profile_sha256,
            })
        } else {
            Ok(ProfileEnrollmentCompletion::CleanupPending {
                operation_id,
                profile_sha256,
            })
        }
    }

    fn run(
        &self,
        operation: impl FnOnce(
            &BoundPrivateDirectory,
        ) -> Result<ProfileLifecycleBaseline, ProfileLifecycleError>,
    ) -> Result<ProfileLifecycleBaseline, ProfileLifecycleAdmissionError> {
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(ProfileLifecycleAdmissionError::ActiveMeeting);
        }
        let lifecycle = operation(&self.lock.storage_root);
        if !authority_is_current() {
            return Err(ProfileLifecycleAdmissionError::AuthorityLost);
        }
        lifecycle.map_err(Into::into)
    }
}

impl ManualAudioDeletionAuthority<'_> {
    /// Releases only the bound audio for one exact, non-active meeting.
    pub fn delete_audio(
        &self,
        meeting_id: &str,
    ) -> Result<ManualAudioDeletionOutcome, ManualAudioDeletionError> {
        delete_meeting_audio_manually(
            &self.lock.storage,
            self.lock.coordination.as_ref(),
            meeting_id,
        )
    }
}

/// The sole capability that may write `library/metadata.json`.
///
/// It carries the operator's own organization — folder names and the titles
/// they typed — and nothing derived. Every method takes an `expected_revision`
/// and refuses on mismatch rather than merging, because two windows editing the
/// same record silently is how one of them loses a rename without being told.
///
/// ```compile_fail
/// use local_meeting_notes_session_core::library_metadata::set_meeting_title;
/// // The raw entry points are crate-private. A dependent crate must acquire
/// // AppDataWriterLock and borrow its library_organization_authority instead.
/// # let _ = set_meeting_title;
/// ```
///
/// ```compile_fail
/// use local_meeting_notes_session_core::retention::LibraryOrganizationAuthority;
/// use local_meeting_notes_session_core::storage::StorageRoot;
/// fn redirect_root(authority: &LibraryOrganizationAuthority, other: &StorageRoot) {
///     let _ = authority.set_meeting_title(other, 0, "meeting", None);
/// }
/// ```
pub struct LibraryOrganizationAuthority<'a> {
    storage: &'a StorageRoot,
}

impl LibraryOrganizationAuthority<'_> {
    pub fn create_folder(
        &self,
        expected_revision: u64,
        name: &str,
    ) -> Result<OrganizationOutcome, OrganizationError> {
        crate::library_metadata::create_folder(self.storage, expected_revision, name)
    }

    pub fn rename_folder(
        &self,
        expected_revision: u64,
        folder_id: &str,
        name: &str,
    ) -> Result<OrganizationOutcome, OrganizationError> {
        crate::library_metadata::rename_folder(self.storage, expected_revision, folder_id, name)
    }

    pub fn delete_folder(
        &self,
        expected_revision: u64,
        folder_id: &str,
    ) -> Result<OrganizationOutcome, OrganizationError> {
        crate::library_metadata::delete_folder(self.storage, expected_revision, folder_id)
    }

    pub fn assign_meeting_folder(
        &self,
        expected_revision: u64,
        meeting_id: &str,
        folder_id: Option<&str>,
    ) -> Result<OrganizationOutcome, OrganizationError> {
        crate::library_metadata::assign_meeting_folder(
            self.storage,
            expected_revision,
            meeting_id,
            folder_id,
        )
    }

    pub fn set_meeting_title(
        &self,
        expected_revision: u64,
        meeting_id: &str,
        title: Option<&str>,
    ) -> Result<OrganizationOutcome, OrganizationError> {
        crate::library_metadata::set_meeting_title(
            self.storage,
            expected_revision,
            meeting_id,
            title,
        )
    }
}

/// Process-lifetime owner-only lock for one app-data storage root.
///
/// The file descriptor is private and non-cloneable. Successful construction
/// means this process holds the canonical nonblocking exclusive `flock`; the
/// bound coordinator is the same registry used for capture and retention.
pub struct AppDataWriterLock {
    storage: StorageRoot,
    storage_root: BoundPrivateDirectory,
    writer_file: BoundPrivateFile,
    coordination: Arc<MeetingStorageCoordination>,
}

#[derive(Debug, Error)]
pub enum AppDataWriterLockError {
    #[error("app-data writer lock path is unavailable")]
    Path,
    #[error("app-data writer lock could not be opened safely")]
    Open,
    #[error("app-data writer lock could not be inspected")]
    Inspect,
    #[error("app-data writer lock is not a regular file")]
    NotRegular,
    #[error("app-data writer lock permissions could not be secured")]
    Permissions,
    #[error("another app process may already be using private meeting storage")]
    Contended,
}

impl AppDataWriterLock {
    /// Acquires the canonical app-data writer lock and creates the one
    /// process-local registry that serializes capture, recovery, and retention.
    pub fn acquire(storage: &StorageRoot) -> Result<Self, AppDataWriterLockError> {
        let storage_root = BoundPrivateDirectory::open(storage.path())
            .map_err(|_| AppDataWriterLockError::Path)?;
        let file = match storage_root.open_file(".writer.lock", true, 0) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => storage_root
                .create_zero_file(".writer.lock")
                .map_err(|_| AppDataWriterLockError::Open)?,
            Err(_) => return Err(AppDataWriterLockError::Open),
        };
        if unsafe { libc::flock(file.raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(AppDataWriterLockError::Contended);
        }
        file.revalidate(&storage_root, 0)
            .map_err(|_| AppDataWriterLockError::Open)?;
        Ok(Self {
            storage: storage.clone(),
            storage_root,
            writer_file: file,
            coordination: Arc::new(MeetingStorageCoordination::default()),
        })
    }

    /// Returns the sole process-local registry bound to this held lock.
    /// Callers may clone the handle for capture/recovery work but cannot replace
    /// the registry stored in this authority.
    pub fn coordination(&self) -> Arc<MeetingStorageCoordination> {
        self.coordination.clone()
    }

    pub fn deletion_authority(&self) -> ManualAudioDeletionAuthority<'_> {
        ManualAudioDeletionAuthority { lock: self }
    }

    /// Authority for `library-metadata/1` — folders and operator titles.
    ///
    /// Separate from every deletion authority above it, and deliberately so:
    /// naming a meeting is not the same permission as destroying one, and a
    /// caller that may rename should not thereby be able to remove.
    pub fn library_organization_authority(&self) -> LibraryOrganizationAuthority<'_> {
        LibraryOrganizationAuthority {
            storage: &self.storage,
        }
    }

    /// Authority for `meeting-deletion/1`, the whole-meeting removal.
    ///
    /// Deliberately separate from [`Self::deletion_authority`], which releases
    /// audio only and keeps the meeting. Holding one does not imply the other,
    /// because a caller that may free disk space is not thereby a caller that
    /// may destroy the retained evidence.
    pub fn whole_meeting_deletion_authority(
        &self,
    ) -> crate::meeting_deletion::WholeMeetingDeletionAuthority<'_> {
        crate::meeting_deletion::WholeMeetingDeletionAuthority {
            storage: &self.storage,
            coordination: self.coordination.as_ref(),
        }
    }

    /// Authority for `transcript-deletion/1`, which removes derived text while
    /// preserving the capture evidence and the operator's own note.
    pub fn transcript_deletion_authority(
        &self,
    ) -> crate::transcript_deletion::TranscriptDeletionAuthority<'_> {
        crate::transcript_deletion::TranscriptDeletionAuthority {
            storage: &self.storage,
            coordination: self.coordination.as_ref(),
        }
    }

    /// Authority for durable transcript retry candidates and operator decisions.
    ///
    /// This borrows the held writer lock just like the other storage
    /// authorities, so callers cannot substitute a different storage root or
    /// meeting coordination registry.
    pub fn transcript_retry_authority(&self) -> TranscriptRetryAuthority<'_> {
        TranscriptRetryAuthority {
            storage: &self.storage,
            coordination: self.coordination.as_ref(),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn profile_lifecycle_authority(&self) -> ProfileLifecycleAuthority<'_> {
        ProfileLifecycleAuthority { lock: self }
    }

    #[cfg(target_os = "macos")]
    pub fn sitting_evidence_authority(&self) -> SittingEvidenceAuthority<'_> {
        SittingEvidenceAuthority { lock: self }
    }
}

/// Process capability for the guided-enrolment sitting evidence store.
///
/// It borrows the process writer lock and admits no caller-provided storage
/// path or coordination registry. Every operation holds the global storage
/// sequence and refuses while any meeting capture lease is active: a dedicated
/// enrolment sitting never overlaps a meeting.
///
/// ```compile_fail
/// use local_meeting_notes_session_core::sitting_evidence::begin_sitting;
/// # let _ = begin_sitting;
/// ```
#[cfg(target_os = "macos")]
pub struct SittingEvidenceAuthority<'a> {
    lock: &'a AppDataWriterLock,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Error)]
pub enum SittingEvidenceAdmissionError {
    #[error("sitting evidence storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error("sitting evidence writer authority was lost")]
    AuthorityLost,
    #[error("sitting evidence work is blocked by an active meeting")]
    ActiveMeeting,
    #[error(transparent)]
    Evidence(#[from] crate::sitting_evidence::SittingEvidenceError),
}

#[cfg(target_os = "macos")]
impl SittingEvidenceAuthority<'_> {
    fn run<T>(
        &self,
        operation: impl FnOnce(&StorageRoot) -> Result<T, crate::sitting_evidence::SittingEvidenceError>,
    ) -> Result<T, SittingEvidenceAdmissionError> {
        let sequence = self.lock.coordination.lock_sequence()?;
        let authority_is_current = || {
            self.lock.storage_root.revalidate().is_ok()
                && self
                    .lock
                    .writer_file
                    .revalidate(&self.lock.storage_root, 0)
                    .is_ok()
        };
        if !authority_is_current() {
            return Err(SittingEvidenceAdmissionError::AuthorityLost);
        }
        if !sequence.active_meeting_ids()?.is_empty() {
            return Err(SittingEvidenceAdmissionError::ActiveMeeting);
        }
        let outcome = operation(&self.lock.storage);
        if !authority_is_current() {
            return Err(SittingEvidenceAdmissionError::AuthorityLost);
        }
        outcome.map_err(Into::into)
    }

    pub fn begin_sitting(
        &self,
        sitting_id: &str,
        kind: crate::sitting_evidence::SittingKind,
        source_class: Option<&str>,
        started_at_epoch_seconds: u64,
    ) -> Result<(), SittingEvidenceAdmissionError> {
        self.run(|storage| {
            crate::sitting_evidence::begin_sitting(
                storage,
                sitting_id,
                kind,
                source_class,
                started_at_epoch_seconds,
            )
        })
    }

    pub fn append_raw_audio(
        &self,
        sitting_id: &str,
        bytes: &[u8],
    ) -> Result<(), SittingEvidenceAdmissionError> {
        self.run(|storage| crate::sitting_evidence::append_raw_audio(storage, sitting_id, bytes))
    }

    pub fn finalize_capture(
        &self,
        sitting_id: &str,
        captured_at_epoch_seconds: u64,
    ) -> Result<(), SittingEvidenceAdmissionError> {
        self.run(|storage| {
            crate::sitting_evidence::finalize_capture(
                storage,
                sitting_id,
                captured_at_epoch_seconds,
            )
        })
    }

    pub fn record_segments(
        &self,
        sitting_id: &str,
        segments: &[crate::sitting_evidence::SegmentSpan],
    ) -> Result<(), SittingEvidenceAdmissionError> {
        self.run(|storage| crate::sitting_evidence::record_segments(storage, sitting_id, segments))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn store_derived_material(
        &self,
        sitting_id: &str,
        embeddings: &[u8],
        encoder_sha256: &str,
        onnx_artifact_sha256: Option<&str>,
        embedding_dim: u32,
        derived_at_epoch_seconds: u64,
    ) -> Result<crate::sitting_evidence::SittingLifecycleState, SittingEvidenceAdmissionError> {
        self.run(|storage| {
            crate::sitting_evidence::store_derived_material(
                storage,
                sitting_id,
                embeddings,
                encoder_sha256,
                onnx_artifact_sha256,
                embedding_dim,
                derived_at_epoch_seconds,
            )
        })
    }

    pub fn retry_sitting_cleanup(
        &self,
        sitting_id: &str,
    ) -> Result<crate::sitting_evidence::SittingLifecycleState, SittingEvidenceAdmissionError> {
        self.run(|storage| crate::sitting_evidence::retry_sitting_cleanup(storage, sitting_id))
    }

    /// Admits the worker's `sitting.derive` work-directory output into the
    /// store's durable rows, re-verifying the whole join against the capture
    /// record and the caller-supplied manifest encoder identity first. The
    /// store then owns segment and derived rows, the cleanup receipt, and the
    /// raw deletion exactly as if the rows had been written directly.
    pub fn admit_derived_material(
        &self,
        sitting_id: &str,
        expected_encoder_sha256: &str,
        derived_at_epoch_seconds: u64,
    ) -> Result<crate::sitting_evidence::SittingLifecycleState, SittingEvidenceAdmissionError> {
        self.run(|storage| {
            crate::sitting_evidence::admit_worker_derivation(
                storage,
                sitting_id,
                expected_encoder_sha256,
                derived_at_epoch_seconds,
            )
        })
    }

    pub fn abandon_sitting(
        &self,
        sitting_id: &str,
        labeled_at_epoch_seconds: u64,
    ) -> Result<(), SittingEvidenceAdmissionError> {
        self.run(|storage| {
            crate::sitting_evidence::abandon_sitting(storage, sitting_id, labeled_at_epoch_seconds)
        })
    }

    /// Startup entry: finishes every interrupted deletion or cleanup, labels
    /// crashed recordings as rehearsals, then projects the store.
    pub fn reconcile_and_read(
        &self,
        reconciled_at_epoch_seconds: u64,
    ) -> Result<
        (
            crate::enrollment_guidance::EnrollmentEvidence,
            Vec<crate::sitting_evidence::SittingRecordSummary>,
        ),
        SittingEvidenceAdmissionError,
    > {
        self.run(|storage| {
            crate::sitting_evidence::reconcile_sitting_evidence(
                storage,
                reconciled_at_epoch_seconds,
            )?;
            crate::sitting_evidence::read_sitting_evidence(storage)
        })
    }

    pub fn read_evidence(
        &self,
    ) -> Result<
        (
            crate::enrollment_guidance::EnrollmentEvidence,
            Vec<crate::sitting_evidence::SittingRecordSummary>,
        ),
        SittingEvidenceAdmissionError,
    > {
        self.run(crate::sitting_evidence::read_sitting_evidence)
    }
}

/// Raw storage state machine. It is deliberately crate-private: callers in a
/// dependent crate must hold `ManualAudioDeletionAuthority` instead.
///
/// Within the process, this action first claims the target's active-meeting
/// lease, then holds the storage sequence gate through the operation scan and
/// staged deletion/recovery path. It therefore cannot overlap an active capture
/// or a durable, nonterminal correction/note operation.
pub(crate) fn delete_meeting_audio_manually(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    meeting_id: &str,
) -> Result<ManualAudioDeletionOutcome, ManualAudioDeletionError> {
    if !valid_opaque_id(meeting_id) {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }
    let _lease = match coordination.acquire(meeting_id) {
        Ok(lease) => lease,
        Err(MeetingCoordinationError::AlreadyActive) => {
            return Ok(ManualAudioDeletionOutcome::DeferredActive);
        }
        Err(error) => return Err(error.into()),
    };
    let _sequence = coordination.lock_sequence()?;

    let directory = meeting_dir(storage, meeting_id)?;
    let mut meeting = load_meeting(&directory)?;
    if meeting.meeting_id != meeting_id {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }

    let operations = OperationStore::open(storage)?;
    let has_nonterminal = operations.scan()?.values().any(|receipt| {
        if receipt.commit.is_some() {
            return false;
        }
        match &receipt.request {
            StoredOperationRequest::Restoration(request) => {
                request.meeting_id.to_string() == meeting_id
            }
            StoredOperationRequest::NoteGeneration(request) => {
                request.meeting_id.to_string() == meeting_id
            }
        }
    });
    if has_nonterminal {
        return Err(ManualAudioDeletionError::NonterminalProductOperation);
    }

    if meeting.retention.state == AudioState::Released {
        // Re-validate the completed receipt before making the idempotent claim.
        // A changed receipt must remain a refusal, not be silently rebound.
        preflight_meeting_retention(&directory, &meeting)?;
        return Ok(ManualAudioDeletionOutcome::AlreadyReleased);
    }

    match reconcile_meeting_audio_deletion(&directory, &mut meeting, true)? {
        MeetingRetentionResult::NotDue => {
            // A retained meeting cannot reach this path because manual deletion
            // authorizes release independently of its policy or deadline.
            Err(RetentionError::UnexplainedAudioLoss.into())
        }
        MeetingRetentionResult::AudioReleased => Ok(ManualAudioDeletionOutcome::AudioReleased),
        MeetingRetentionResult::RecoveredRemoval => {
            Ok(ManualAudioDeletionOutcome::RecoveredRemoval)
        }
    }
}

pub fn execute_due_retention(
    storage: &StorageRoot,
    now_epoch_seconds: u64,
) -> Result<Vec<RetentionOutcome>, RetentionError> {
    execute_due_retention_excluding(storage, now_epoch_seconds, &HashSet::new())
}

pub fn execute_due_retention_excluding(
    storage: &StorageRoot,
    now_epoch_seconds: u64,
    active_meeting_ids: &HashSet<String>,
) -> Result<Vec<RetentionOutcome>, RetentionError> {
    let meetings = storage
        .resolve(Path::new("meetings"))
        .map_err(|error| io::Error::other(error.to_string()))?;
    // A meeting between the `staged` and `removed` transitions of
    // `meeting-deletion/1` exists as a directory without a `meeting.json`, so
    // loading it fails and it would be reported as quarantined. That would show
    // the operator a damaged meeting where they asked for an absent one, so
    // these are skipped and left to `reconcile_pending_meeting_deletions`.
    // Propagated rather than defaulted: if we cannot tell which meetings are
    // mid-deletion, we do not know that none are. Swallowing the error here
    // would fail open, and would produce exactly the quarantine report this
    // skip exists to prevent.
    let deleting = crate::meeting_deletion::pending_deletion_ids(storage)
        .map(|ids| ids.into_iter().collect::<HashSet<_>>())
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut outcomes = Vec::new();
    for entry in fs::read_dir(meetings)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if deleting.contains(&id) {
            continue;
        }
        if active_meeting_ids.contains(&id) {
            outcomes.push(RetentionOutcome::DeferredActive(id));
            continue;
        }
        if !entry.file_type()?.is_dir() {
            outcomes.push(RetentionOutcome::Quarantined(id));
            continue;
        }
        let meeting_dir = entry.path();
        let result = (|| {
            let mut meeting = load_meeting(&meeting_dir)?;
            reconcile_meeting_retention(&meeting_dir, &mut meeting, now_epoch_seconds, true)
        })();
        outcomes.push(match result {
            Ok(MeetingRetentionResult::NotDue) => RetentionOutcome::NotDue(id),
            Ok(MeetingRetentionResult::AudioReleased) => RetentionOutcome::AudioReleased(id),
            Ok(MeetingRetentionResult::RecoveredRemoval) => RetentionOutcome::RecoveredRemoval(id),
            Err(_) => RetentionOutcome::Quarantined(id),
        });
    }
    Ok(outcomes)
}

pub(crate) fn preflight_meeting_retention(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<(), RetentionError> {
    meeting.validate(&meeting.meeting_id)?;
    verify_record_static_artifacts(meeting_dir, meeting)?;
    validate_deletion_directory(meeting_dir)?;
    let receipt_path = meeting_dir.join("deletion/audio-deletion.json");
    if !deletion_receipt_present(&receipt_path)? {
        if matches!(
            meeting.retention.state,
            AudioState::Deleting | AudioState::Released
        ) {
            return Err(RetentionError::UnexplainedAudioLoss);
        }
        ensure_no_staged_audio_without_receipt(meeting_dir)?;
        if meeting.lifecycle != MeetingLifecycle::Incomplete {
            ensure_no_unbound_audio(meeting_dir, meeting)?;
        }
        verify_record_artifacts(meeting_dir, meeting)?;
        return Ok(());
    }

    let receipt = load_receipt(&receipt_path)?;
    validate_receipt_binding(meeting, &receipt)?;
    if meeting.retention.state == AudioState::Released {
        let stored_receipt = meeting
            .retention
            .deletion_receipt
            .as_ref()
            .ok_or(RetentionError::MalformedReceipt)?;
        verify_artifact_ref(meeting_dir, stored_receipt)?;
        if receipt.state != DeletionState::Removed {
            return Err(RetentionError::MalformedReceipt);
        }
    }
    validate_receipt_storage(meeting_dir, meeting, &receipt)
}

pub(crate) fn reconcile_meeting_retention(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
    now_epoch_seconds: u64,
    execute_due: bool,
) -> Result<MeetingRetentionResult, RetentionError> {
    let due = meeting
        .retention
        .next_deletion_at_epoch_seconds
        .is_some_and(|deadline| deadline <= now_epoch_seconds);
    reconcile_meeting_audio_deletion(meeting_dir, meeting, execute_due && due)
}

/// Runs the audited `audio-deletion/1` recovery and staged-removal state
/// machine. `release_retained_audio` controls only the authority to begin a
/// new deletion; receipts already on disk are always reconciled first.
fn reconcile_meeting_audio_deletion(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
    release_retained_audio: bool,
) -> Result<MeetingRetentionResult, RetentionError> {
    preflight_meeting_retention(meeting_dir, meeting)?;
    let receipt_path = meeting_dir.join("deletion/audio-deletion.json");
    let receipt_present = deletion_receipt_present(&receipt_path)?;

    if receipt_present {
        let receipt = load_receipt(&receipt_path)?;
        validate_receipt_binding(meeting, &receipt)?;
        let already_released = meeting.retention.state == AudioState::Released;
        if already_released {
            let stored_receipt = meeting
                .retention
                .deletion_receipt
                .as_ref()
                .ok_or(RetentionError::MalformedReceipt)?;
            verify_artifact_ref(meeting_dir, stored_receipt)?;
            if receipt.state != DeletionState::Removed {
                return Err(RetentionError::MalformedReceipt);
            }
        }
        finish_staged_removal(meeting_dir, &receipt_path, meeting, receipt)?;
        write_meeting(meeting_dir, meeting)?;
        return Ok(if already_released {
            MeetingRetentionResult::NotDue
        } else {
            MeetingRetentionResult::RecoveredRemoval
        });
    }

    if matches!(
        meeting.retention.state,
        AudioState::Deleting | AudioState::Released
    ) {
        return Err(RetentionError::UnexplainedAudioLoss);
    }
    verify_record_artifacts(meeting_dir, meeting)?;
    if meeting.retention.state == AudioState::NeverCreated {
        return Ok(MeetingRetentionResult::NotDue);
    }
    if !release_retained_audio {
        return Ok(MeetingRetentionResult::NotDue);
    }

    let capture_session = meeting
        .artifacts
        .capture_session
        .as_ref()
        .ok_or(RetentionError::UnexplainedAudioLoss)?;
    let mut artifacts = Vec::new();
    for reference in audio_references(meeting) {
        let inspected = inspect_audio(
            &resolve_artifact(meeting_dir, &reference.relative_path)?,
            &reference.relative_path,
        )?;
        if inspected.sha256 != reference.sha256 {
            return Err(RetentionError::UnexplainedAudioLoss);
        }
        artifacts.push(inspected);
    }
    if artifacts.is_empty() {
        return Err(RetentionError::UnexplainedAudioLoss);
    }
    artifacts.sort_by(|left, right| left.relative_name.cmp(&right.relative_name));
    let receipt = AudioDeletionReceipt {
        schema: AudioDeletionSchema::V1,
        capture_session_sha256: capture_session.sha256.clone(),
        state: DeletionState::Deleting,
        artifacts,
    };
    let deletion_dir = meeting_dir.join("deletion");
    create_private_dir(&deletion_dir)?;
    durable_create_new(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    meeting.retention.state = AudioState::Deleting;
    meeting.pending_storage_operation = Some(PendingStorageOperation::AudioDeletionV1);
    write_meeting(meeting_dir, meeting)?;

    finish_staged_removal(meeting_dir, &receipt_path, meeting, receipt)?;
    write_meeting(meeting_dir, meeting)?;
    Ok(MeetingRetentionResult::AudioReleased)
}

fn finish_staged_removal(
    meeting_dir: &Path,
    receipt_path: &Path,
    meeting: &mut MeetingRecord,
    mut receipt: AudioDeletionReceipt,
) -> Result<(), RetentionError> {
    validate_receipt_binding(meeting, &receipt)?;
    let deletion_dir = meeting_dir.join("deletion");
    let capture_dir = meeting_dir.join("capture");
    validate_receipt_storage(meeting_dir, meeting, &receipt)?;

    if receipt.state == DeletionState::Removed {
        finalize_released_record(meeting_dir, receipt_path, meeting)?;
        return Ok(());
    }

    if receipt.state == DeletionState::Deleting {
        let mut validated_locations = Vec::with_capacity(receipt.artifacts.len());
        for expected in &receipt.artifacts {
            let live = resolve_artifact(meeting_dir, &expected.relative_name)?;
            let staged = staged_path(&deletion_dir, &expected.relative_name)?;
            let live_present = path_present(&live)?;
            let staged_present = path_present(&staged)?;
            if live_present == staged_present {
                return Err(RetentionError::UnexplainedAudioLoss);
            }
            let present = if live_present { &live } else { &staged };
            let actual = inspect_audio(present, &expected.relative_name)?;
            if &actual != expected {
                return Err(RetentionError::UnexplainedAudioLoss);
            }
            validated_locations.push((live, staged, live_present));
        }

        for (live, staged, live_present) in validated_locations {
            if live_present {
                fs::rename(&live, &staged)?;
            }
        }
        sync_directory(&capture_dir)?;
        sync_directory(&deletion_dir)?;
        receipt.state = DeletionState::Staged;
        durable_replace(receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    }

    let mut staged_to_remove = Vec::with_capacity(receipt.artifacts.len());
    for expected in &receipt.artifacts {
        let live = resolve_artifact(meeting_dir, &expected.relative_name)?;
        if path_present(&live)? {
            return Err(RetentionError::UnexplainedAudioLoss);
        }
        let staged = staged_path(&deletion_dir, &expected.relative_name)?;
        if path_present(&staged)? {
            let actual = inspect_audio(&staged, &expected.relative_name)?;
            if &actual != expected {
                return Err(RetentionError::UnexplainedAudioLoss);
            }
            staged_to_remove.push(staged);
        }
    }
    for staged in staged_to_remove {
        fs::remove_file(staged)?;
    }
    sync_directory(&deletion_dir)?;
    receipt.state = DeletionState::Removed;
    durable_replace(receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    finalize_released_record(meeting_dir, receipt_path, meeting)?;
    Ok(())
}

fn deletion_receipt_present(receipt_path: &Path) -> Result<bool, RetentionError> {
    match fs::symlink_metadata(receipt_path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn validate_deletion_directory(meeting_dir: &Path) -> Result<(), RetentionError> {
    let deletion_dir = meeting_dir.join("deletion");
    match fs::symlink_metadata(&deletion_dir) {
        Ok(_) => {
            require_private_directory(&deletion_dir)?;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_receipt_storage(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
    receipt: &AudioDeletionReceipt,
) -> Result<(), RetentionError> {
    let deletion_dir = meeting_dir.join("deletion");
    ensure_no_unbound_audio(meeting_dir, meeting)?;
    for expected in &receipt.artifacts {
        let live = resolve_artifact(meeting_dir, &expected.relative_name)?;
        let staged = staged_path(&deletion_dir, &expected.relative_name)?;
        let live_present = path_present(&live)?;
        let staged_present = path_present(&staged)?;
        match receipt.state {
            DeletionState::Deleting => {
                if live_present == staged_present {
                    return Err(RetentionError::UnexplainedAudioLoss);
                }
                let present = if live_present { &live } else { &staged };
                if inspect_audio(present, &expected.relative_name)? != *expected {
                    return Err(RetentionError::UnexplainedAudioLoss);
                }
            }
            DeletionState::Staged => {
                if live_present {
                    return Err(RetentionError::UnexplainedAudioLoss);
                }
                if staged_present && inspect_audio(&staged, &expected.relative_name)? != *expected {
                    return Err(RetentionError::UnexplainedAudioLoss);
                }
            }
            DeletionState::Removed => {
                if live_present || staged_present {
                    return Err(RetentionError::UnexplainedAudioLoss);
                }
            }
        }
    }
    Ok(())
}

fn ensure_no_staged_audio_without_receipt(meeting_dir: &Path) -> Result<(), RetentionError> {
    let deletion_dir = meeting_dir.join("deletion");
    for relative in AUDIO_ARTIFACT_PATHS {
        if path_present(&staged_path(&deletion_dir, relative)?)? {
            return Err(RetentionError::UnexplainedAudioLoss);
        }
    }
    Ok(())
}

fn finalize_released_record(
    meeting_dir: &Path,
    receipt_path: &Path,
    meeting: &mut MeetingRecord,
) -> Result<(), RetentionError> {
    let receipt_relative = receipt_path
        .strip_prefix(meeting_dir)
        .map_err(|_| RetentionError::MalformedReceipt)?
        .to_str()
        .ok_or(RetentionError::MalformedReceipt)?;
    meeting.retention.state = AudioState::Released;
    meeting.retention.deletion_receipt = Some(artifact_ref(meeting_dir, receipt_relative)?);
    meeting.pending_storage_operation = None;
    Ok(())
}

fn validate_receipt_binding(
    meeting: &MeetingRecord,
    receipt: &AudioDeletionReceipt,
) -> Result<(), RetentionError> {
    let capture_session = meeting
        .artifacts
        .capture_session
        .as_ref()
        .ok_or(RetentionError::MalformedReceipt)?;
    if receipt.capture_session_sha256 != capture_session.sha256 {
        return Err(RetentionError::MalformedReceipt);
    }
    let references = audio_references(meeting);
    if references.is_empty() || receipt.artifacts.len() != references.len() {
        return Err(RetentionError::MalformedReceipt);
    }
    let mut names = HashSet::new();
    for artifact in &receipt.artifacts {
        if !names.insert(&artifact.relative_name) {
            return Err(RetentionError::MalformedReceipt);
        }
        let Some(reference) = references
            .iter()
            .find(|reference| reference.relative_path == artifact.relative_name)
        else {
            return Err(RetentionError::MalformedReceipt);
        };
        if artifact.sha256 != reference.sha256 {
            return Err(RetentionError::MalformedReceipt);
        }
    }
    Ok(())
}

fn ensure_no_unbound_audio(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<(), RetentionError> {
    let bound: HashSet<&str> = audio_references(meeting)
        .into_iter()
        .map(|reference| reference.relative_path.as_str())
        .collect();
    for relative in AUDIO_ARTIFACT_PATHS {
        if bound.contains(relative) {
            continue;
        }
        let live = meeting_dir.join(relative);
        let staged = staged_path(&meeting_dir.join("deletion"), relative)?;
        if path_present(&live)? || path_present(&staged)? {
            return Err(RetentionError::UnexplainedAudioLoss);
        }
    }
    Ok(())
}

fn audio_references(meeting: &MeetingRecord) -> Vec<&ArtifactRef> {
    [
        meeting.artifacts.microphone_audio.as_ref(),
        meeting.artifacts.system_audio.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn staged_path(deletion_dir: &Path, relative_name: &str) -> Result<PathBuf, RetentionError> {
    let file_name = Path::new(relative_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RetentionError::MalformedReceipt)?;
    if !AUDIO_ARTIFACT_PATHS.contains(&relative_name) {
        return Err(RetentionError::MalformedReceipt);
    }
    Ok(deletion_dir.join(format!("{file_name}.staged")))
}

fn path_present(path: &Path) -> Result<bool, RetentionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn load_receipt(path: &Path) -> Result<AudioDeletionReceipt, RetentionError> {
    let bytes = read_private_bytes(path, MAX_RECEIPT_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| RetentionError::MalformedReceipt)
}

fn inspect_audio(path: &Path, relative_name: &str) -> Result<DeletedArtifact, RetentionError> {
    let mut file = open_private_file(path).map_err(|_| RetentionError::InvalidAudio)?;
    let metadata = file.metadata()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(DeletedArtifact {
        relative_name: relative_name.into(),
        byte_size: metadata.len(),
        sha256: format!("{:x}", hasher.finalize()),
    })
}

pub fn meeting_dir(storage: &StorageRoot, meeting_id: &str) -> Result<PathBuf, io::Error> {
    storage
        .resolve(&Path::new("meetings").join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::meeting::{
        AudioRetention, AudioRetentionRule, MeetingArtifacts, MeetingLifecycle, MeetingSchema,
        artifact_ref, retention_policy_sha256,
    };
    use crate::meeting_coordination::MeetingStorageCoordination;
    use crate::operation_store::StoredOperationResult;
    use crate::storage::{create_private_dir, durable_create_new};
    use serde::de::DeserializeOwned;
    use serde_json::Value;
    use tempfile::TempDir;

    #[cfg(target_os = "macos")]
    struct EnrollmentWorkerFixture {
        root: PathBuf,
        discard_available: Mutex<bool>,
    }

    #[cfg(target_os = "macos")]
    impl EnrollmentWorkerFixture {
        fn new(root: PathBuf, discard_available: bool) -> Self {
            Self {
                root,
                discard_available: Mutex::new(discard_available),
            }
        }

        fn candidate(&self, operation_id: &str) -> PathBuf {
            self.root
                .join("profile-candidates")
                .join(operation_id)
                .join("voiceprint.json")
        }
    }

    #[cfg(target_os = "macos")]
    impl ProfileEnrollmentWorker for EnrollmentWorkerFixture {
        fn inspect_candidate(
            &self,
            operation_id: &str,
        ) -> Result<String, ProfileEnrollmentWorkerError> {
            let bytes = fs::read(self.candidate(operation_id))
                .map_err(|_| ProfileEnrollmentWorkerError::Refused)?;
            Ok(format!("{:x}", Sha256::digest(bytes)))
        }

        fn discard_candidate(
            &self,
            operation_id: &str,
            profile_sha256: &str,
        ) -> Result<String, ProfileEnrollmentWorkerError> {
            if !*self.discard_available.lock().unwrap() {
                return Err(ProfileEnrollmentWorkerError::Unavailable);
            }
            let candidate = self.candidate(operation_id);
            let bytes = fs::read(&candidate).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    ProfileEnrollmentWorkerError::Unavailable
                } else {
                    ProfileEnrollmentWorkerError::Refused
                }
            })?;
            if format!("{:x}", Sha256::digest(bytes)) != profile_sha256 {
                return Err(ProfileEnrollmentWorkerError::Refused);
            }
            fs::remove_file(&candidate).map_err(|_| ProfileEnrollmentWorkerError::Unavailable)?;
            fs::remove_dir(candidate.parent().unwrap())
                .map_err(|_| ProfileEnrollmentWorkerError::Unavailable)?;
            Ok(profile_sha256.into())
        }
    }

    fn fixture(storage: &StorageRoot, id: &str, deadline: Option<u64>) -> (PathBuf, MeetingRecord) {
        let directory = meeting_dir(storage, id).unwrap();
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        for (relative, bytes) in [
            ("attempt.json", b"attempt".as_slice()),
            ("ownership.json", b"ownership".as_slice()),
            ("capture/session.json", b"session".as_slice()),
            ("capture/mic.wav", b"mic".as_slice()),
            ("capture/system.wav", b"system".as_slice()),
        ] {
            durable_create_new(&directory.join(relative), bytes).unwrap();
        }
        let rule = match deadline {
            Some(_) => AudioRetentionRule::DeleteAfter { seconds: 10 },
            None => AudioRetentionRule::UntilManualDeletion,
        };
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::Captured,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: deadline,
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: Some(artifact_ref(&directory, "capture/mic.wav").unwrap()),
                system_audio: Some(artifact_ref(&directory, "capture/system.wav").unwrap()),
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        };
        durable_create_new(
            &directory.join("meeting.json"),
            &serde_json::to_vec_pretty(&meeting).unwrap(),
        )
        .unwrap();
        (directory, meeting)
    }

    fn partial_fixture(storage: &StorageRoot, id: &str) -> (PathBuf, MeetingRecord) {
        let directory = meeting_dir(storage, id).unwrap();
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("attempt.json"), b"attempt").unwrap();
        durable_create_new(&directory.join("ownership.json"), b"ownership").unwrap();
        let mic_bytes = vec![1_u8; 44];
        let system_bytes = vec![2_u8; 44];
        durable_create_new(&directory.join("capture/.mic.wav.partial"), &mic_bytes).unwrap();
        durable_create_new(
            &directory.join("capture/.system.wav.partial"),
            &system_bytes,
        )
        .unwrap();
        let mic = artifact_ref(&directory, "capture/.mic.wav.partial").unwrap();
        let system = artifact_ref(&directory, "capture/.system.wav.partial").unwrap();
        let interruption = serde_json::json!({
            "schema": "capture-interruption/1",
            "status": "interrupted",
            "reason": "fresh-process-recovery",
            "meeting_id": id,
            "artifacts": [
                {
                    "name": ".mic.wav.partial",
                    "bytes": mic_bytes.len(),
                    "sha256": mic.sha256.clone(),
                    "mode": "0600"
                },
                {
                    "name": ".system.wav.partial",
                    "bytes": system_bytes.len(),
                    "sha256": system.sha256.clone(),
                    "mode": "0600"
                }
            ]
        });
        durable_create_new(
            &directory.join("capture/session.json"),
            &serde_json::to_vec_pretty(&interruption).unwrap(),
        )
        .unwrap();
        let rule = AudioRetentionRule::DeleteAfter { seconds: 10 };
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::RecoveredInterrupted,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: Some(10),
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: Some(mic),
                system_audio: Some(system),
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        };
        durable_create_new(
            &directory.join("meeting.json"),
            &serde_json::to_vec_pretty(&meeting).unwrap(),
        )
        .unwrap();
        (directory, meeting)
    }

    fn storage() -> (TempDir, StorageRoot) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        (temp, storage)
    }

    fn product_operations_fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/product-operations-v1.json"
        ))
        .unwrap()
    }

    fn parse_operation_fixture<T: DeserializeOwned>(fixture: &Value, path: &[&str]) -> T {
        let mut value = fixture;
        for segment in path {
            value = &value[*segment];
        }
        serde_json::from_value(value.clone()).unwrap()
    }

    fn begin_audio_deletion(directory: &Path, meeting: &mut MeetingRecord) {
        create_private_dir(&directory.join("deletion")).unwrap();
        let receipt = AudioDeletionReceipt {
            schema: AudioDeletionSchema::V1,
            capture_session_sha256: meeting
                .artifacts
                .capture_session
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            state: DeletionState::Deleting,
            artifacts: vec![
                inspect_audio(&directory.join("capture/mic.wav"), "capture/mic.wav").unwrap(),
                inspect_audio(&directory.join("capture/system.wav"), "capture/system.wav").unwrap(),
            ],
        };
        durable_create_new(
            &directory.join("deletion/audio-deletion.json"),
            &serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        meeting.retention.state = AudioState::Deleting;
        meeting.pending_storage_operation = Some(PendingStorageOperation::AudioDeletionV1);
        write_meeting(directory, meeting).unwrap();
    }

    fn advance_interrupted_deletion_to_staged(directory: &Path) {
        fs::rename(
            directory.join("capture/mic.wav"),
            directory.join("deletion/mic.wav.staged"),
        )
        .unwrap();
        fs::rename(
            directory.join("capture/system.wav"),
            directory.join("deletion/system.wav.staged"),
        )
        .unwrap();
        sync_directory(&directory.join("capture")).unwrap();
        sync_directory(&directory.join("deletion")).unwrap();
        let receipt_path = directory.join("deletion/audio-deletion.json");
        let mut receipt: AudioDeletionReceipt =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        receipt.state = DeletionState::Staged;
        durable_replace(&receipt_path, &serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }

    fn advance_interrupted_deletion_to_removed(directory: &Path) {
        advance_interrupted_deletion_to_staged(directory);
        fs::remove_file(directory.join("deletion/mic.wav.staged")).unwrap();
        fs::remove_file(directory.join("deletion/system.wav.staged")).unwrap();
        sync_directory(&directory.join("deletion")).unwrap();
        let receipt_path = directory.join("deletion/audio-deletion.json");
        let mut receipt: AudioDeletionReceipt =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        receipt.state = DeletionState::Removed;
        durable_replace(&receipt_path, &serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }

    #[test]
    fn manual_deletion_releases_an_until_manual_meeting_and_preserves_other_private_content() {
        let (_temp, storage) = storage();
        let (target, _) = fixture(&storage, "manual-target", None);
        let (other, _) = fixture(
            &storage,
            "11111111-1111-4111-8111-111111111111",
            Some(1_000),
        );
        create_private_dir(&target.join("transcript")).unwrap();
        create_private_dir(&target.join("notes")).unwrap();
        create_private_dir(&storage.path().join("profile")).unwrap();
        durable_create_new(&target.join("transcript/retained.json"), b"transcript").unwrap();
        durable_create_new(&target.join("notes/retained.md"), b"note").unwrap();
        durable_create_new(&storage.path().join("profile/profile.json"), b"profile").unwrap();
        OperationStore::open(&storage)
            .unwrap()
            .write_request(&StoredOperationRequest::Restoration(
                parse_operation_fixture(&product_operations_fixture(), &["restoration", "request"]),
            ))
            .unwrap();
        let coordination = MeetingStorageCoordination::default();

        assert_eq!(
            delete_meeting_audio_manually(&storage, &coordination, "manual-target").unwrap(),
            ManualAudioDeletionOutcome::AudioReleased
        );

        assert!(!target.join("capture/mic.wav").exists());
        assert!(!target.join("capture/system.wav").exists());
        assert!(target.join("transcript/retained.json").exists());
        assert!(target.join("notes/retained.md").exists());
        assert!(storage.path().join("profile/profile.json").exists());
        assert!(other.join("capture/mic.wav").exists());
        assert!(other.join("capture/system.wav").exists());
        assert_eq!(
            load_meeting(&target).unwrap().retention.state,
            AudioState::Released
        );
    }

    #[test]
    fn manual_deletion_ignores_a_not_yet_due_deadline_and_is_idempotent() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "manual-before-deadline", Some(u64::MAX));
        let coordination = MeetingStorageCoordination::default();

        assert_eq!(
            delete_meeting_audio_manually(&storage, &coordination, "manual-before-deadline")
                .unwrap(),
            ManualAudioDeletionOutcome::AudioReleased
        );
        let committed = fs::read(directory.join("meeting.json")).unwrap();
        assert_eq!(
            delete_meeting_audio_manually(&storage, &coordination, "manual-before-deadline")
                .unwrap(),
            ManualAudioDeletionOutcome::AlreadyReleased
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), committed);
    }

    #[test]
    fn manual_deletion_defers_an_active_meeting_without_opening_its_directory() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "manual-active", None);
        durable_replace(&directory.join("meeting.json"), b"write-in-progress").unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let coordination = MeetingStorageCoordination::default();
        let _lease = coordination.acquire("manual-active").unwrap();

        assert_eq!(
            delete_meeting_audio_manually(&storage, &coordination, "manual-active").unwrap(),
            ManualAudioDeletionOutcome::DeferredActive
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
    }

    #[test]
    fn manual_deletion_recovers_each_interrupted_receipt_phase() {
        let (_temp, storage) = storage();
        let coordination = MeetingStorageCoordination::default();

        for (id, advance) in [
            ("manual-deleting", None),
            (
                "manual-staged",
                Some(advance_interrupted_deletion_to_staged as fn(&Path)),
            ),
            (
                "manual-removed",
                Some(advance_interrupted_deletion_to_removed as fn(&Path)),
            ),
        ] {
            let (directory, mut meeting) = fixture(&storage, id, None);
            begin_audio_deletion(&directory, &mut meeting);
            if let Some(advance) = advance {
                advance(&directory);
            }

            assert_eq!(
                delete_meeting_audio_manually(&storage, &coordination, id).unwrap(),
                ManualAudioDeletionOutcome::RecoveredRemoval
            );
            assert_eq!(
                load_meeting(&directory).unwrap().retention.state,
                AudioState::Released
            );
            assert!(!directory.join("capture/mic.wav").exists());
            assert!(!directory.join("capture/system.wav").exists());
            assert!(!directory.join("deletion/mic.wav.staged").exists());
            assert!(!directory.join("deletion/system.wav.staged").exists());
        }
    }

    #[test]
    fn manual_deletion_refuses_unbound_or_missing_audio_without_rewrite() {
        let (_temp, storage) = storage();
        let (directory, _) = partial_fixture(&storage, "manual-unbound");
        durable_create_new(&directory.join("capture/mic.wav"), &[7_u8; 44]).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let coordination = MeetingStorageCoordination::default();

        assert!(delete_meeting_audio_manually(&storage, &coordination, "manual-unbound").is_err());
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/.mic.wav.partial").exists());
        assert!(!directory.join("deletion/audio-deletion.json").exists());
    }

    #[derive(Debug, Clone, Copy)]
    enum NonterminalOperationCase {
        RestorationRequestOnly,
        RestorationResultStored,
        RestorationAlreadyApplied,
        NoteRequestOnly,
        NoteResultStored,
        NoteAlreadyApplied,
    }

    #[test]
    fn manual_deletion_refuses_every_nonterminal_operation_kind_and_durable_state() {
        const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";

        for case in [
            NonterminalOperationCase::RestorationRequestOnly,
            NonterminalOperationCase::RestorationResultStored,
            NonterminalOperationCase::RestorationAlreadyApplied,
            NonterminalOperationCase::NoteRequestOnly,
            NonterminalOperationCase::NoteResultStored,
            NonterminalOperationCase::NoteAlreadyApplied,
        ] {
            let (_temp, storage) = storage();
            let (directory, _) = fixture(&storage, MEETING_ID, None);
            let fixture = product_operations_fixture();
            let (request, result, applied_meeting): (
                StoredOperationRequest,
                Option<StoredOperationResult>,
                Option<MeetingRecord>,
            ) = match case {
                NonterminalOperationCase::RestorationRequestOnly => (
                    StoredOperationRequest::Restoration(parse_operation_fixture(
                        &fixture,
                        &["restoration", "request"],
                    )),
                    None,
                    None,
                ),
                NonterminalOperationCase::RestorationResultStored => (
                    StoredOperationRequest::Restoration(parse_operation_fixture(
                        &fixture,
                        &["restoration", "request"],
                    )),
                    Some(StoredOperationResult::Restoration(parse_operation_fixture(
                        &fixture,
                        &["restoration", "result"],
                    ))),
                    None,
                ),
                NonterminalOperationCase::RestorationAlreadyApplied => (
                    StoredOperationRequest::Restoration(parse_operation_fixture(
                        &fixture,
                        &["restoration", "request"],
                    )),
                    Some(StoredOperationResult::Restoration(parse_operation_fixture(
                        &fixture,
                        &["restoration", "result"],
                    ))),
                    Some(parse_operation_fixture(
                        &fixture,
                        &["committed_meetings", "restoration"],
                    )),
                ),
                NonterminalOperationCase::NoteRequestOnly => (
                    StoredOperationRequest::NoteGeneration(parse_operation_fixture(
                        &fixture,
                        &["accepted_note", "request"],
                    )),
                    None,
                    None,
                ),
                NonterminalOperationCase::NoteResultStored => (
                    StoredOperationRequest::NoteGeneration(parse_operation_fixture(
                        &fixture,
                        &["accepted_note", "request"],
                    )),
                    Some(StoredOperationResult::NoteGeneration(
                        parse_operation_fixture(&fixture, &["accepted_note", "result"]),
                    )),
                    None,
                ),
                NonterminalOperationCase::NoteAlreadyApplied => (
                    StoredOperationRequest::NoteGeneration(parse_operation_fixture(
                        &fixture,
                        &["accepted_note", "request"],
                    )),
                    Some(StoredOperationResult::NoteGeneration(
                        parse_operation_fixture(&fixture, &["accepted_note", "result"]),
                    )),
                    Some(parse_operation_fixture(
                        &fixture,
                        &["committed_meetings", "accepted_note"],
                    )),
                ),
            };
            let operations = OperationStore::open(&storage).unwrap();
            let operation_id = request.operation_id();
            operations.write_request(&request).unwrap();
            if let Some(result) = &result {
                operations.write_result(operation_id, result).unwrap();
            }
            if let Some(applied_meeting) = &applied_meeting {
                write_meeting(&directory, applied_meeting).unwrap();
            }
            let meeting_before = fs::read(directory.join("meeting.json")).unwrap();
            let microphone_before = fs::read(directory.join("capture/mic.wav")).unwrap();
            let system_before = fs::read(directory.join("capture/system.wav")).unwrap();

            assert!(
                matches!(
                    delete_meeting_audio_manually(
                        &storage,
                        &MeetingStorageCoordination::default(),
                        MEETING_ID,
                    ),
                    Err(ManualAudioDeletionError::NonterminalProductOperation)
                ),
                "case {case:?} must refuse before deletion"
            );
            assert_eq!(
                fs::read(directory.join("meeting.json")).unwrap(),
                meeting_before
            );
            assert_eq!(
                fs::read(directory.join("capture/mic.wav")).unwrap(),
                microphone_before
            );
            assert_eq!(
                fs::read(directory.join("capture/system.wav")).unwrap(),
                system_before
            );
            assert!(!directory.join("deletion/audio-deletion.json").exists());
        }
    }

    #[test]
    fn invalid_manual_deletion_ids_are_refused_before_lease_or_path_access() {
        let (_temp, storage) = storage();
        let nested_alias = storage.path().join("meetings/nested/meeting");
        create_private_dir(&nested_alias).unwrap();
        durable_create_new(&nested_alias.join("meeting.json"), b"alias-sentinel").unwrap();
        let alias_before = fs::read(nested_alias.join("meeting.json")).unwrap();
        let invalid_ids = [
            "nested/meeting".to_owned(),
            ".".to_owned(),
            "a".repeat(129),
            "malformed id".to_owned(),
        ];
        let coordination = MeetingStorageCoordination::default();
        let _existing_alias_leases: Vec<_> = invalid_ids
            .iter()
            .map(|meeting_id| coordination.acquire(meeting_id).unwrap())
            .collect();

        for meeting_id in &invalid_ids {
            assert!(matches!(
                delete_meeting_audio_manually(&storage, &coordination, meeting_id),
                Err(ManualAudioDeletionError::Meeting(MeetingError::Malformed(
                    "meeting identifier mismatch"
                )))
            ));
        }

        assert_eq!(
            fs::read(nested_alias.join("meeting.json")).unwrap(),
            alias_before
        );
        assert!(!nested_alias.join("deletion").exists());
        assert!(!storage.path().join("operations").exists());
    }

    /// Releasing audio removes the recordings it was told about and nothing
    /// else in the meeting directory.
    ///
    /// Added 2026-08-06 with § D, whose surface tells the operator in shipped
    /// copy that their own note is "kept with the transcript, not with the
    /// audio, so the retention period does not remove it." That promise is
    /// about someone's own words, so it is proved against the real release
    /// path rather than argued from how the path is written. This crate does
    /// not know what an operator note is, which is the point: the property is
    /// that release names its artifacts and never sweeps the directory.
    #[test]
    fn releasing_audio_leaves_entries_the_release_was_never_told_about() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "meeting-a", Some(10));
        durable_create_new(
            &directory.join("operator-note.json"),
            br#"{"schema":"operator-note/1","text":"what I actually thought"}"#,
        )
        .unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::AudioReleased("meeting-a".into())]
        );
        assert!(!directory.join("capture/mic.wav").exists());
        assert!(!directory.join("capture/system.wav").exists());
        assert_eq!(
            fs::read(directory.join("operator-note.json")).unwrap(),
            br#"{"schema":"operator-note/1","text":"what I actually thought"}"#,
        );
        // And the record still verifies, so an unrecognised entry beside the
        // named artifacts does not make the meeting fail its own checks.
        let released = load_meeting(&directory).unwrap();
        verify_record_artifacts(&directory, &released).unwrap();
    }

    #[test]
    fn due_retention_preserves_content_lifecycle() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "meeting-a", Some(10));
        create_private_dir(&directory.join("transcript")).unwrap();
        durable_create_new(&directory.join("transcript/retained.json"), b"{}\n").unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::AudioReleased("meeting-a".into())]
        );
        assert!(!directory.join("capture/mic.wav").exists());
        assert!(!directory.join("capture/system.wav").exists());
        assert!(directory.join("transcript/retained.json").exists());
        let final_meeting = load_meeting(&directory).unwrap();
        assert_eq!(final_meeting.lifecycle, MeetingLifecycle::Captured);
        assert_eq!(final_meeting.retention.state, AudioState::Released);
        verify_record_artifacts(&directory, &final_meeting).unwrap();
    }

    #[test]
    fn active_meeting_is_deferred_without_read_or_mutation() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "meeting-active", Some(10));
        durable_replace(&directory.join("meeting.json"), b"write-in-progress").unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let active = HashSet::from(["meeting-active".to_string()]);

        assert_eq!(
            execute_due_retention_excluding(&storage, 10, &active).unwrap(),
            vec![RetentionOutcome::DeferredActive("meeting-active".into())]
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
    }

    #[test]
    fn missing_audio_without_completed_receipt_is_quarantined_without_rewrite() {
        let (_temp, storage) = storage();
        let (directory, meeting) = fixture(&storage, "meeting-b", Some(10));
        fs::remove_file(directory.join("capture/mic.wav")).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("meeting-b".into())]
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert_eq!(meeting.retention.state, AudioState::Retained);
    }

    #[test]
    fn tampered_staged_audio_is_quarantined_without_further_deletion() {
        let (_temp, storage) = storage();
        let (directory, mut meeting) = fixture(&storage, "meeting-c", Some(10));
        begin_audio_deletion(&directory, &mut meeting);
        fs::rename(
            directory.join("capture/mic.wav"),
            directory.join("deletion/mic.wav.staged"),
        )
        .unwrap();
        durable_replace(&directory.join("deletion/mic.wav.staged"), b"tampered").unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("meeting-c".into())]
        );
        assert!(directory.join("deletion/mic.wav.staged").exists());
        assert!(directory.join("capture/system.wav").exists());
    }

    #[test]
    fn tampered_second_audio_is_detected_before_either_artifact_moves() {
        let (_temp, storage) = storage();
        let (directory, mut meeting) = fixture(&storage, "meeting-d", Some(10));
        begin_audio_deletion(&directory, &mut meeting);
        durable_replace(&directory.join("capture/system.wav"), b"tampered-system").unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("meeting-d".into())]
        );
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert!(!directory.join("deletion/mic.wav.staged").exists());
        assert!(!directory.join("deletion/system.wav.staged").exists());
    }

    #[test]
    fn missing_second_audio_is_detected_before_the_first_artifact_moves() {
        let (_temp, storage) = storage();
        let (directory, mut meeting) = fixture(&storage, "meeting-e", Some(10));
        begin_audio_deletion(&directory, &mut meeting);
        fs::remove_file(directory.join("capture/system.wav")).unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("meeting-e".into())]
        );
        assert!(directory.join("capture/mic.wav").exists());
        assert!(!directory.join("capture/system.wav").exists());
        assert!(!directory.join("deletion/mic.wav.staged").exists());
        assert!(!directory.join("deletion/system.wav.staged").exists());
    }

    #[test]
    fn malformed_meeting_does_not_abort_another_meeting() {
        let (_temp, storage) = storage();
        let (bad, _) = fixture(&storage, "bad", None);
        fixture(&storage, "good", None);
        durable_replace(&bad.join("meeting.json"), b"not-json").unwrap();

        let outcomes = execute_due_retention(&storage, 10).unwrap();
        assert!(outcomes.contains(&RetentionOutcome::Quarantined("bad".into())));
        assert!(outcomes.contains(&RetentionOutcome::NotDue("good".into())));
    }

    #[test]
    fn malformed_deletion_receipt_does_not_abort_another_meeting() {
        let (_temp, storage) = storage();
        let (bad, _) = fixture(&storage, "bad-receipt", None);
        fixture(&storage, "good-receipt", None);
        create_private_dir(&bad.join("deletion")).unwrap();
        durable_create_new(&bad.join("deletion/audio-deletion.json"), b"not-json").unwrap();

        let outcomes = execute_due_retention(&storage, 10).unwrap();
        assert!(outcomes.contains(&RetentionOutcome::Quarantined("bad-receipt".into())));
        assert!(outcomes.contains(&RetentionOutcome::NotDue("good-receipt".into())));
    }

    #[test]
    fn changed_completed_receipt_is_not_silently_rebound() {
        let (_temp, storage) = storage();
        let (directory, _) = fixture(&storage, "receipt-tamper", Some(10));
        execute_due_retention(&storage, 10).unwrap();
        let meeting_before = fs::read(directory.join("meeting.json")).unwrap();
        let receipt_path = directory.join("deletion/audio-deletion.json");
        let mut receipt: AudioDeletionReceipt =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        receipt.capture_session_sha256 = "0".repeat(64);
        durable_replace(&receipt_path, &serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("receipt-tamper".into())]
        );
        assert_eq!(
            fs::read(directory.join("meeting.json")).unwrap(),
            meeting_before
        );
    }

    #[test]
    fn due_retention_atomically_releases_bound_partial_names() {
        let (_temp, storage) = storage();
        let (directory, _) = partial_fixture(&storage, "partial-release");

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::AudioReleased("partial-release".into())]
        );
        assert!(!directory.join("capture/.mic.wav.partial").exists());
        assert!(!directory.join("capture/.system.wav.partial").exists());
        assert!(!directory.join("deletion/.mic.wav.partial.staged").exists());
        assert!(
            !directory
                .join("deletion/.system.wav.partial.staged")
                .exists()
        );
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::RecoveredInterrupted);
        assert_eq!(meeting.retention.state, AudioState::Released);
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join("deletion/audio-deletion.json")).unwrap(),
        )
        .unwrap();
        let names: Vec<_> = receipt["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|artifact| artifact["relative_name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["capture/.mic.wav.partial", "capture/.system.wav.partial"]
        );
    }

    #[test]
    fn tampered_partial_pair_is_rejected_before_either_leg_moves() {
        let (_temp, storage) = storage();
        let (directory, _) = partial_fixture(&storage, "partial-tamper");
        durable_replace(&directory.join("capture/.system.wav.partial"), &[9_u8; 44]).unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("partial-tamper".into())]
        );
        assert!(directory.join("capture/.mic.wav.partial").exists());
        assert!(directory.join("capture/.system.wav.partial").exists());
        assert!(!directory.join("deletion/.mic.wav.partial.staged").exists());
        assert!(
            !directory
                .join("deletion/.system.wav.partial.staged")
                .exists()
        );
        assert!(!directory.join("deletion/audio-deletion.json").exists());
    }

    #[test]
    fn unbound_duplicate_name_is_rejected_before_retention_mutates_state() {
        let (_temp, storage) = storage();
        let (directory, _) = partial_fixture(&storage, "partial-duplicate");
        durable_create_new(&directory.join("capture/mic.wav"), &[7_u8; 44]).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        assert_eq!(
            execute_due_retention(&storage, 10).unwrap(),
            vec![RetentionOutcome::Quarantined("partial-duplicate".into())]
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(!directory.join("deletion/audio-deletion.json").exists());
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/.mic.wav.partial").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn profile_lifecycle_requires_the_bound_process_writer_authority() {
        let (_temp, storage) = storage();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        let baseline = writer
            .profile_lifecycle_authority()
            .initialize_or_open()
            .unwrap();
        assert_eq!(baseline.receipt_sequence(), 0);
        assert!(!baseline.profile_present());
        assert!(matches!(
            AppDataWriterLock::acquire(&storage),
            Err(AppDataWriterLockError::Contended)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn active_meeting_blocks_profile_initialization_before_mutation() {
        let (_temp, storage) = storage();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        let coordination = writer.coordination();
        let _active = coordination.acquire("active-meeting").unwrap();
        assert!(matches!(
            writer.profile_lifecycle_authority().initialize_or_open(),
            Err(ProfileLifecycleAdmissionError::ActiveMeeting)
        ));
        assert!(!storage.path().join("profile/voiceprint.json").exists());
        assert!(
            !storage
                .path()
                .join("profile/.lifecycle.initializing")
                .exists()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn explicit_legacy_preservation_uses_the_same_writer_and_meeting_gate() {
        let (_temp, storage) = storage();
        let legacy = b"legacy-profile-sentinel";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        let coordination = writer.coordination();
        let active = coordination.acquire("active-meeting").unwrap();

        assert!(matches!(
            writer
                .profile_lifecycle_authority()
                .preserve_legacy_for_review(),
            Err(ProfileLifecycleAdmissionError::ActiveMeeting)
        ));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );
        assert!(!storage.path().join("profile/lifecycle").exists());

        drop(active);
        let baseline = writer
            .profile_lifecycle_authority()
            .preserve_legacy_for_review()
            .unwrap();
        assert!(baseline.profile_present());
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn profile_reset_uses_writer_authority_and_refuses_active_meetings() {
        let (_temp, storage) = storage();
        let legacy = b"safe-but-not-activated-legacy-profile";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        writer
            .profile_lifecycle_authority()
            .preserve_legacy_for_review()
            .unwrap();
        let coordination = writer.coordination();
        let active = coordination.acquire("active-meeting").unwrap();

        assert!(matches!(
            writer
                .profile_lifecycle_authority()
                .reset_profile("123e4567-e89b-12d3-a456-426614174000", 10,),
            Err(ProfileLifecycleAdmissionError::ActiveMeeting)
        ));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );

        drop(active);
        assert!(matches!(
            writer.profile_lifecycle_authority().reset_profile(
                "123e4567-e89b-12d3-a456-426614174000",
                10,
            ),
            Ok(ProfileResetOutcome::Removed {
                ref operation_id,
                completed_reset_count: 1,
            }) if operation_id == "123e4567-e89b-12d3-a456-426614174000"
        ));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            b""
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn profile_enrollment_uses_writer_authority_and_refuses_active_meetings() {
        let (_temp, storage) = storage();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        writer
            .profile_lifecycle_authority()
            .initialize_or_open()
            .unwrap();
        let candidate = b"synthetic-admitted-profile";
        let digest = format!("{:x}", Sha256::digest(candidate));
        let coordination = writer.coordination();
        let active = coordination.acquire("active-meeting").unwrap();

        assert!(matches!(
            writer
                .profile_lifecycle_authority()
                .enroll_admitted_profile(
                    "223e4567-e89b-12d3-a456-426614174000",
                    20,
                    candidate,
                    &digest,
                ),
            Err(ProfileLifecycleAdmissionError::ActiveMeeting)
        ));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            b""
        );

        drop(active);
        assert!(matches!(
            writer
                .profile_lifecycle_authority()
                .enroll_admitted_profile(
                    "223e4567-e89b-12d3-a456-426614174000",
                    20,
                    candidate,
                    &digest,
                ),
            Ok(ProfileEnrollmentOutcome::Activated { ref profile_sha256, .. })
                if profile_sha256 == &digest
        ));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            candidate
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn strict_profile_bridge_binds_worker_digest_and_retries_cleanup_separately() {
        let (_temp, storage) = storage();
        let writer = AppDataWriterLock::acquire(&storage).unwrap();
        writer
            .profile_lifecycle_authority()
            .initialize_or_open()
            .unwrap();
        let operation_id = "223e4567-e89b-12d3-a456-426614174000";
        let candidates = storage.path().join("profile-candidates");
        let candidate_dir = candidates.join(operation_id);
        create_private_dir(&candidates).unwrap();
        create_private_dir(&candidate_dir).unwrap();
        let candidate = b"synthetic-strict-loader-admitted-profile";
        durable_create_new(&candidate_dir.join("voiceprint.json"), candidate).unwrap();
        let expected_digest = format!("{:x}", Sha256::digest(candidate));
        let worker = EnrollmentWorkerFixture::new(storage.path().to_path_buf(), false);

        assert_eq!(
            writer
                .profile_lifecycle_authority()
                .enroll_profile_candidate(&worker, operation_id, 20)
                .unwrap(),
            ProfileEnrollmentCompletion::CleanupPending {
                operation_id: operation_id.into(),
                profile_sha256: expected_digest.clone(),
            }
        );
        assert!(candidate_dir.exists());
        let active = writer
            .profile_lifecycle_authority()
            .initialize_or_open()
            .unwrap();
        assert!(active.profile_active());
        assert_eq!(active.active_enrollment_id(), Some(operation_id));
        assert_eq!(active.profile_sha256(), expected_digest);
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            candidate
        );

        *worker.discard_available.lock().unwrap() = true;
        assert_eq!(
            writer
                .profile_lifecycle_authority()
                .cleanup_active_enrollment_candidate(&worker)
                .unwrap(),
            ProfileEnrollmentCompletion::Enrolled {
                operation_id: operation_id.into(),
                profile_sha256: expected_digest,
            }
        );
        assert!(!candidate_dir.exists());
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            candidate
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn writer_authority_never_redirects_to_a_replacement_app_data_root() {
        let (_temp, storage) = storage();
        let original_root = storage.path().to_path_buf();
        let displaced_root = original_root.with_file_name("app.displaced");
        let original_writer = AppDataWriterLock::acquire(&storage).unwrap();

        fs::rename(&original_root, &displaced_root).unwrap();
        create_private_dir(&original_root).unwrap();
        for child in ["diagnostics", "profile", "meetings"] {
            create_private_dir(&original_root.join(child)).unwrap();
        }
        let replacement_writer = AppDataWriterLock::acquire(&storage).unwrap();
        let replacement_coordination = replacement_writer.coordination();
        let _replacement_active = replacement_coordination
            .acquire("replacement-active")
            .unwrap();

        assert!(matches!(
            original_writer
                .profile_lifecycle_authority()
                .initialize_or_open(),
            Err(ProfileLifecycleAdmissionError::AuthorityLost)
        ));
        assert!(!original_root.join("profile/voiceprint.json").exists());
        assert!(!displaced_root.join("profile/voiceprint.json").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn replaced_writer_lock_never_creates_a_second_profile_authority() {
        let (_temp, storage) = storage();
        let original_writer = AppDataWriterLock::acquire(&storage).unwrap();
        let lock_path = storage.path().join(".writer.lock");
        fs::rename(&lock_path, storage.path().join(".writer.lock.displaced")).unwrap();
        durable_create_new(&lock_path, b"").unwrap();
        let replacement_writer = AppDataWriterLock::acquire(&storage).unwrap();
        let replacement_coordination = replacement_writer.coordination();
        let _replacement_active = replacement_coordination
            .acquire("replacement-active")
            .unwrap();

        assert!(matches!(
            original_writer
                .profile_lifecycle_authority()
                .initialize_or_open(),
            Err(ProfileLifecycleAdmissionError::AuthorityLost)
        ));
        assert!(!storage.path().join("profile/voiceprint.json").exists());
        assert!(
            !storage
                .path()
                .join("profile/.lifecycle.initializing")
                .exists()
        );
    }
}
