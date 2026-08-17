//! Durable, source-bound transcript retry candidates.
//!
//! A retry is an immutable transcript revision next to the current revision.
//! Creating one never changes `meeting.json`.  An operator can keep the
//! current transcript or promote the candidate later; both decisions are
//! recorded in the candidate receipt and can be resumed after a crash.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::meeting::{
    ArtifactRef, AudioState, MAX_RECEIPT_BYTES, MeetingError, MeetingLifecycle, MeetingRecord,
    load_meeting, read_private_bytes, require_private_directory, valid_opaque_id,
    verify_artifact_ref, verify_record_artifacts, write_meeting,
};
use crate::meeting_coordination::{MeetingCoordinationError, MeetingStorageCoordination};
use crate::storage::{StorageRoot, create_private_dir, durable_create_new, durable_replace};

const RETRY_DIRECTORY: &str = "transcript-retry";
const RECEIPT_FILE: &str = "receipt.json";
const MAX_TRANSCRIPT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RETRY_DISCOVERY_ENTRIES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptRetryState {
    CandidateAvailableForComparison,
    CurrentKept,
    CandidatePromoted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRetryOutcome {
    CandidateAvailableForComparison,
    CurrentKept,
    CandidatePromoted,
}

impl From<TranscriptRetryState> for TranscriptRetryOutcome {
    fn from(state: TranscriptRetryState) -> Self {
        match state {
            TranscriptRetryState::CandidateAvailableForComparison => {
                Self::CandidateAvailableForComparison
            }
            TranscriptRetryState::CurrentKept => Self::CurrentKept,
            TranscriptRetryState::CandidatePromoted => Self::CandidatePromoted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptRetryCandidateSchema {
    #[serde(rename = "transcript-retry-candidate/1")]
    V1,
}

/// Closed source identity returned alongside a worker candidate. Every field
/// is checked against the current meeting and private artifacts before the
/// candidate is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRetrySourceBinding {
    pub source_transcript_sha256: String,
    pub capture_session_sha256: String,
    pub microphone_audio_sha256: String,
    pub system_audio_sha256: String,
    pub candidate_transcript_sha256: String,
}

/// The durable receipt for one immutable candidate and its operator decision.
/// The four source digests are captured before the worker result is admitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRetryCandidate {
    pub schema: TranscriptRetryCandidateSchema,
    pub operation_id: Uuid,
    pub meeting_id: String,
    pub state: TranscriptRetryState,
    pub source_transcript_sha256: String,
    pub capture_session_sha256: String,
    pub microphone_audio_sha256: String,
    pub system_audio_sha256: String,
    pub candidate_transcript: ArtifactRef,
}

/// Restart-discovery metadata for the one candidate awaiting comparison.
/// Deliberately omits filesystem paths and timestamps; native projection uses
/// `operation_id` with [`TranscriptRetryAuthority::read_candidate_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRetryPendingCandidate {
    pub operation_id: Uuid,
    pub meeting_id: String,
    pub state: TranscriptRetryState,
    pub source_transcript_sha256: String,
    pub capture_session_sha256: String,
    pub microphone_audio_sha256: String,
    pub system_audio_sha256: String,
    pub candidate_transcript_sha256: String,
}

impl From<&TranscriptRetryCandidate> for TranscriptRetryPendingCandidate {
    fn from(candidate: &TranscriptRetryCandidate) -> Self {
        Self {
            operation_id: candidate.operation_id,
            meeting_id: candidate.meeting_id.clone(),
            state: candidate.state,
            source_transcript_sha256: candidate.source_transcript_sha256.clone(),
            capture_session_sha256: candidate.capture_session_sha256.clone(),
            microphone_audio_sha256: candidate.microphone_audio_sha256.clone(),
            system_audio_sha256: candidate.system_audio_sha256.clone(),
            candidate_transcript_sha256: candidate.candidate_transcript.sha256.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum TranscriptRetryError {
    #[error("meeting storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("no such meeting")]
    NoSuchMeeting,
    #[error("meeting has no current transcript")]
    NoCurrentTranscript,
    #[error("meeting audio is released and cannot authorize a retry")]
    AudioReleased,
    #[error("retry candidate is missing or changed")]
    CandidateChanged,
    #[error("retry candidate belongs to another meeting")]
    CrossMeetingCandidate,
    #[error("retry operation receipt is malformed or incomplete")]
    MalformedOperation,
    #[error("retry operation already exists with different bytes")]
    ConflictingOperation,
    #[error("retry source capture has changed or is stale")]
    StaleSource,
    #[error("retry candidate binding does not match the reverified source")]
    BindingMismatch,
    #[error("retry candidate digest does not match its bytes")]
    CandidateDigestMismatch,
    #[error("meeting lifecycle does not admit transcript retry")]
    IneligibleLifecycle,
    #[error("meeting has a pending storage operation")]
    PendingStorageOperation,
    #[error("retry candidate is not in an available state")]
    InvalidState,
    #[error("another transcript retry candidate is awaiting a decision")]
    PendingCandidateExists,
    #[error("too many transcript retry operations to inspect safely")]
    DiscoveryBoundExceeded,
}

/// The only capability that can admit or decide a transcript retry.  The
/// caller holds the same process-local meeting lease used by capture/storage.
pub struct TranscriptRetryAuthority<'a> {
    pub(crate) storage: &'a StorageRoot,
    pub(crate) coordination: &'a MeetingStorageCoordination,
}

impl<'a> TranscriptRetryAuthority<'a> {
    pub fn new(storage: &'a StorageRoot, coordination: &'a MeetingStorageCoordination) -> Self {
        Self {
            storage,
            coordination,
        }
    }

    /// Stores an immutable candidate and source binding. This does not write
    /// or replace the meeting record's current transcript pointer.
    pub fn create_candidate(
        &self,
        meeting_id: &str,
        operation_id: Uuid,
        candidate_bytes: &[u8],
        binding: &TranscriptRetrySourceBinding,
    ) -> Result<TranscriptRetryOutcome, TranscriptRetryError> {
        let _lease = self.coordination.acquire(meeting_id)?;
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let meeting = self.load_source(&meeting_dir, meeting_id)?;
        if candidate_bytes.is_empty() || candidate_bytes.len() as u64 > MAX_TRANSCRIPT_BYTES {
            return Err(TranscriptRetryError::CandidateChanged);
        }
        validate_binding_shape(binding)?;
        if binding.candidate_transcript_sha256 != digest_bytes(candidate_bytes) {
            return Err(TranscriptRetryError::CandidateDigestMismatch);
        }

        let operation_dir = self.operation_dir(&meeting_dir, operation_id)?;
        if path_exists(&operation_dir)? {
            let candidate = load_receipt(&operation_dir, operation_id)?;
            self.validate_receipt(&meeting_dir, &meeting, &candidate, meeting_id)?;
            if candidate.candidate_transcript.sha256 != binding.candidate_transcript_sha256
                || !binding_matches_candidate(binding, &candidate)
            {
                return Err(TranscriptRetryError::ConflictingOperation);
            }
            verify_artifact_ref(&meeting_dir, &candidate.candidate_transcript)?;
            return Ok(candidate.state.into());
        }

        if meeting.artifacts.current_transcript.is_none() {
            return Err(TranscriptRetryError::NoCurrentTranscript);
        }
        if meeting.artifacts.capture_session.is_none()
            || meeting.artifacts.microphone_audio.is_none()
            || meeting.artifacts.system_audio.is_none()
        {
            return Err(TranscriptRetryError::StaleSource);
        }
        if !binding_matches_meeting(binding, &meeting) {
            return Err(TranscriptRetryError::BindingMismatch);
        }
        if self
            .discover_pending_candidate_locked(&meeting_dir, &meeting, meeting_id)?
            .is_some_and(|candidate| candidate.operation_id != operation_id)
        {
            return Err(TranscriptRetryError::PendingCandidateExists);
        }
        let candidate_sha = binding.candidate_transcript_sha256.clone();
        let candidate = TranscriptRetryCandidate {
            schema: TranscriptRetryCandidateSchema::V1,
            operation_id,
            meeting_id: meeting_id.to_owned(),
            state: TranscriptRetryState::CandidateAvailableForComparison,
            source_transcript_sha256: binding.source_transcript_sha256.clone(),
            capture_session_sha256: binding.capture_session_sha256.clone(),
            microphone_audio_sha256: binding.microphone_audio_sha256.clone(),
            system_audio_sha256: binding.system_audio_sha256.clone(),
            candidate_transcript: ArtifactRef {
                relative_path: format!("transcript/{candidate_sha}.json"),
                sha256: candidate_sha,
            },
        };
        validate_candidate_shape(&candidate)?;

        let transcript_dir = meeting_dir.join("transcript");
        create_private_dir(&transcript_dir)?;
        let candidate_path = meeting_dir.join(&candidate.candidate_transcript.relative_path);
        match fs::symlink_metadata(&candidate_path) {
            Ok(_) => {
                verify_artifact_ref(&meeting_dir, &candidate.candidate_transcript)
                    .map_err(|_| TranscriptRetryError::CandidateChanged)?;
                if read_private_bytes(&candidate_path, MAX_TRANSCRIPT_BYTES)? != candidate_bytes {
                    return Err(TranscriptRetryError::CandidateChanged);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                durable_create_new(&candidate_path, candidate_bytes)?;
            }
            Err(error) => return Err(error.into()),
        }

        create_private_dir(&meeting_dir.join(RETRY_DIRECTORY))?;
        create_private_dir(&operation_dir)?;
        let receipt_path = operation_dir.join(RECEIPT_FILE);
        durable_create_new(&receipt_path, &serde_json::to_vec_pretty(&candidate)?)?;
        Ok(TranscriptRetryOutcome::CandidateAvailableForComparison)
    }

    pub fn keep_current(
        &self,
        meeting_id: &str,
        operation_id: Uuid,
    ) -> Result<TranscriptRetryOutcome, TranscriptRetryError> {
        let _lease = self.coordination.acquire(meeting_id)?;
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let meeting = self.load_source(&meeting_dir, meeting_id)?;
        let operation_dir = self.operation_dir(&meeting_dir, operation_id)?;
        let mut candidate = load_receipt(&operation_dir, operation_id)?;
        self.validate_receipt(&meeting_dir, &meeting, &candidate, meeting_id)?;
        if candidate.state == TranscriptRetryState::CandidateAvailableForComparison
            && meeting.artifacts.current_transcript.as_ref()
                == Some(&candidate.candidate_transcript)
        {
            candidate.state = TranscriptRetryState::CandidatePromoted;
            write_receipt(&operation_dir, &candidate)?;
            return Ok(TranscriptRetryOutcome::CandidatePromoted);
        }
        if candidate.state != TranscriptRetryState::CandidateAvailableForComparison {
            return Ok(candidate.state.into());
        }
        candidate.state = TranscriptRetryState::CurrentKept;
        write_receipt(&operation_dir, &candidate)?;
        Ok(TranscriptRetryOutcome::CurrentKept)
    }

    /// Promotes only the candidate pointer and the lifecycle needed to make
    /// the new transcript readable. Existing note files remain on disk; their
    /// pointer is cleared because they bind the prior transcript.
    pub fn promote_candidate(
        &self,
        meeting_id: &str,
        operation_id: Uuid,
    ) -> Result<TranscriptRetryOutcome, TranscriptRetryError> {
        let _lease = self.coordination.acquire(meeting_id)?;
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let mut meeting = self.load_source(&meeting_dir, meeting_id)?;
        let operation_dir = self.operation_dir(&meeting_dir, operation_id)?;
        let mut candidate = load_receipt(&operation_dir, operation_id)?;
        self.validate_receipt(&meeting_dir, &meeting, &candidate, meeting_id)?;
        if candidate.state == TranscriptRetryState::CurrentKept {
            return Ok(TranscriptRetryOutcome::CurrentKept);
        }
        if candidate.state == TranscriptRetryState::CandidatePromoted {
            if meeting.artifacts.current_transcript.as_ref()
                != Some(&candidate.candidate_transcript)
            {
                return Err(TranscriptRetryError::MalformedOperation);
            }
            return Ok(TranscriptRetryOutcome::CandidatePromoted);
        }

        // A crash after meeting.json was published but before the terminal
        // receipt is durable is completed here, never rolled back.
        if meeting.artifacts.current_transcript.as_ref() == Some(&candidate.candidate_transcript) {
            candidate.state = TranscriptRetryState::CandidatePromoted;
            write_receipt(&operation_dir, &candidate)?;
            return Ok(TranscriptRetryOutcome::CandidatePromoted);
        }

        let current = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(TranscriptRetryError::NoCurrentTranscript)?;
        if current.sha256 != candidate.source_transcript_sha256 {
            return Err(TranscriptRetryError::StaleSource);
        }
        meeting.artifacts.current_transcript = Some(candidate.candidate_transcript.clone());
        meeting.artifacts.current_note = None;
        meeting.lifecycle = MeetingLifecycle::TranscriptReady;
        write_meeting(&meeting_dir, &meeting)?;
        candidate.state = TranscriptRetryState::CandidatePromoted;
        write_receipt(&operation_dir, &candidate)?;
        Ok(TranscriptRetryOutcome::CandidatePromoted)
    }

    /// Reads a candidate for comparison without changing any durable state.
    pub fn inspect_candidate(
        &self,
        meeting_id: &str,
        operation_id: Uuid,
    ) -> Result<TranscriptRetryCandidate, TranscriptRetryError> {
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let meeting = self.load_source(&meeting_dir, meeting_id)?;
        let candidate = load_receipt(
            &self.operation_dir(&meeting_dir, operation_id)?,
            operation_id,
        )?;
        self.validate_receipt(&meeting_dir, &meeting, &candidate, meeting_id)?;
        Ok(candidate)
    }

    /// Finds the single valid candidate still awaiting an operator decision.
    ///
    /// The scan is intentionally bounded and runs under the meeting lease so a
    /// restart cannot admit a second pending candidate from a stale directory
    /// view. Terminal receipts remain durable but are not returned as pending.
    pub fn discover_pending_candidate(
        &self,
        meeting_id: &str,
    ) -> Result<Option<TranscriptRetryPendingCandidate>, TranscriptRetryError> {
        let _lease = self.coordination.acquire(meeting_id)?;
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let retry_root = meeting_dir.join(RETRY_DIRECTORY);
        match fs::symlink_metadata(&retry_root) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let meeting = self.load_source(&meeting_dir, meeting_id)?;
        Ok(self
            .discover_pending_candidate_locked(&meeting_dir, &meeting, meeting_id)?
            .as_ref()
            .map(TranscriptRetryPendingCandidate::from))
    }

    /// Reads and verifies candidate bytes for native comparison projection.
    /// The returned bytes are bounded by the same transcript limit used at
    /// admission and are checked against the receipt digest while leased.
    pub fn read_candidate_bytes(
        &self,
        meeting_id: &str,
        operation_id: Uuid,
    ) -> Result<Vec<u8>, TranscriptRetryError> {
        let _lease = self.coordination.acquire(meeting_id)?;
        let meeting_dir = self.meeting_dir(meeting_id)?;
        let meeting = self.load_source(&meeting_dir, meeting_id)?;
        let candidate = load_receipt(
            &self.operation_dir(&meeting_dir, operation_id)?,
            operation_id,
        )?;
        self.validate_receipt(&meeting_dir, &meeting, &candidate, meeting_id)?;
        let path = meeting_dir.join(&candidate.candidate_transcript.relative_path);
        let bytes = read_private_bytes(&path, MAX_TRANSCRIPT_BYTES)
            .map_err(|_| TranscriptRetryError::CandidateChanged)?;
        if digest_bytes(&bytes) != candidate.candidate_transcript.sha256 {
            return Err(TranscriptRetryError::CandidateDigestMismatch);
        }
        Ok(bytes)
    }

    fn meeting_dir(&self, meeting_id: &str) -> Result<PathBuf, TranscriptRetryError> {
        let path = self
            .storage
            .resolve(&Path::new("meetings").join(meeting_id))
            .map_err(|_| TranscriptRetryError::NoSuchMeeting)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Err(TranscriptRetryError::NoSuchMeeting)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn operation_dir(
        &self,
        meeting_dir: &Path,
        operation_id: Uuid,
    ) -> Result<PathBuf, TranscriptRetryError> {
        let root = meeting_dir.join(RETRY_DIRECTORY);
        match fs::symlink_metadata(&root) {
            Ok(_) => require_private_directory(&root)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let path = root.join(operation_id.to_string());
        if let Ok(_) = fs::symlink_metadata(&path) {
            require_private_directory(&path)?;
        }
        Ok(path)
    }

    fn discover_pending_candidate_locked(
        &self,
        meeting_dir: &Path,
        meeting: &MeetingRecord,
        meeting_id: &str,
    ) -> Result<Option<TranscriptRetryCandidate>, TranscriptRetryError> {
        let retry_root = meeting_dir.join(RETRY_DIRECTORY);
        match fs::symlink_metadata(&retry_root) {
            Ok(_) => require_private_directory(&retry_root)
                .map_err(|_| TranscriptRetryError::MalformedOperation)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(TranscriptRetryError::MalformedOperation),
        }
        let mut entries = fs::read_dir(&retry_root)?;
        let mut pending = None;
        for index in 0..=MAX_RETRY_DISCOVERY_ENTRIES {
            let Some(entry) = entries.next() else {
                break;
            };
            let entry = entry.map_err(|_| TranscriptRetryError::MalformedOperation)?;
            if index == MAX_RETRY_DISCOVERY_ENTRIES {
                return Err(TranscriptRetryError::DiscoveryBoundExceeded);
            }
            let operation_id = entry
                .file_name()
                .to_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or(TranscriptRetryError::MalformedOperation)?;
            let operation_dir = entry.path();
            require_private_directory(&operation_dir)
                .map_err(|_| TranscriptRetryError::MalformedOperation)?;
            let candidate = load_receipt(&operation_dir, operation_id)?;
            if candidate.meeting_id != meeting_id || candidate.meeting_id != meeting.meeting_id {
                return Err(TranscriptRetryError::CrossMeetingCandidate);
            }
            match candidate.state {
                TranscriptRetryState::CandidateAvailableForComparison => {
                    // A pending candidate must still bind to today's source set.
                    self.validate_receipt(meeting_dir, meeting, &candidate, meeting_id)?;
                    if pending.is_some() {
                        return Err(TranscriptRetryError::PendingCandidateExists);
                    }
                    pending = Some(candidate);
                }
                TranscriptRetryState::CurrentKept | TranscriptRetryState::CandidatePromoted => {
                    // Terminal receipts are no longer admission blockers. Keep
                    // their candidate bytes integrity-checked without requiring
                    // the old source audio to remain current or retained.
                    verify_artifact_ref(meeting_dir, &candidate.candidate_transcript)
                        .map_err(|_| TranscriptRetryError::CandidateChanged)?;
                }
            }
        }
        Ok(pending)
    }

    fn load_source(
        &self,
        meeting_dir: &Path,
        meeting_id: &str,
    ) -> Result<MeetingRecord, TranscriptRetryError> {
        let meeting = load_meeting(meeting_dir)?;
        if meeting.meeting_id != meeting_id {
            return Err(TranscriptRetryError::CrossMeetingCandidate);
        }
        if meeting.retention.state != AudioState::Retained {
            return Err(TranscriptRetryError::AudioReleased);
        }
        if !matches!(
            meeting.lifecycle,
            MeetingLifecycle::TranscriptReady
                | MeetingLifecycle::SummaryFailed
                | MeetingLifecycle::Ready
        ) {
            return Err(TranscriptRetryError::IneligibleLifecycle);
        }
        if meeting.pending_storage_operation.is_some() {
            return Err(TranscriptRetryError::PendingStorageOperation);
        }
        verify_record_artifacts(meeting_dir, &meeting)?;
        Ok(meeting)
    }

    fn validate_receipt(
        &self,
        meeting_dir: &Path,
        meeting: &MeetingRecord,
        candidate: &TranscriptRetryCandidate,
        meeting_id: &str,
    ) -> Result<(), TranscriptRetryError> {
        validate_candidate_shape(candidate)?;
        if candidate.meeting_id != meeting_id || candidate.meeting_id != meeting.meeting_id {
            return Err(TranscriptRetryError::CrossMeetingCandidate);
        }
        let current = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(TranscriptRetryError::NoCurrentTranscript)?;
        let capture = meeting
            .artifacts
            .capture_session
            .as_ref()
            .ok_or(TranscriptRetryError::StaleSource)?;
        let microphone = meeting
            .artifacts
            .microphone_audio
            .as_ref()
            .ok_or(TranscriptRetryError::StaleSource)?;
        let system = meeting
            .artifacts
            .system_audio
            .as_ref()
            .ok_or(TranscriptRetryError::StaleSource)?;
        if candidate.state == TranscriptRetryState::CandidatePromoted {
            if current != &candidate.candidate_transcript {
                return Err(TranscriptRetryError::StaleSource);
            }
        } else if current == &candidate.candidate_transcript {
            // Meeting publication may have won a crash race with the receipt.
        } else if current.sha256 != candidate.source_transcript_sha256
            || capture.sha256 != candidate.capture_session_sha256
            || microphone.sha256 != candidate.microphone_audio_sha256
            || system.sha256 != candidate.system_audio_sha256
        {
            return Err(TranscriptRetryError::StaleSource);
        }
        verify_artifact_ref(meeting_dir, &candidate.candidate_transcript)
            .map_err(|_| TranscriptRetryError::CandidateChanged)
    }
}

fn path_exists(path: &Path) -> Result<bool, TranscriptRetryError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            require_private_directory(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn load_receipt(
    operation_dir: &Path,
    operation_id: Uuid,
) -> Result<TranscriptRetryCandidate, TranscriptRetryError> {
    require_private_directory(operation_dir)
        .map_err(|_| TranscriptRetryError::MalformedOperation)?;
    let path = operation_dir.join(RECEIPT_FILE);
    let bytes = read_private_bytes(&path, MAX_RECEIPT_BYTES)
        .map_err(|_| TranscriptRetryError::MalformedOperation)?;
    let candidate: TranscriptRetryCandidate =
        serde_json::from_slice(&bytes).map_err(|_| TranscriptRetryError::MalformedOperation)?;
    validate_candidate_shape(&candidate)?;
    if candidate.operation_id != operation_id {
        return Err(TranscriptRetryError::MalformedOperation);
    }
    Ok(candidate)
}

fn write_receipt(
    operation_dir: &Path,
    candidate: &TranscriptRetryCandidate,
) -> Result<(), TranscriptRetryError> {
    durable_replace(
        operation_dir.join(RECEIPT_FILE).as_path(),
        &serde_json::to_vec_pretty(candidate)?,
    )?;
    Ok(())
}

fn validate_candidate_shape(
    candidate: &TranscriptRetryCandidate,
) -> Result<(), TranscriptRetryError> {
    if candidate.schema != TranscriptRetryCandidateSchema::V1
        || !valid_opaque_id(&candidate.meeting_id)
        || !valid_sha256(&candidate.source_transcript_sha256)
        || !valid_sha256(&candidate.capture_session_sha256)
        || !valid_sha256(&candidate.microphone_audio_sha256)
        || !valid_sha256(&candidate.system_audio_sha256)
        || candidate.candidate_transcript.relative_path
            != format!("transcript/{}.json", candidate.candidate_transcript.sha256)
        || !valid_sha256(&candidate.candidate_transcript.sha256)
    {
        return Err(TranscriptRetryError::MalformedOperation);
    }
    Ok(())
}

fn validate_binding_shape(
    binding: &TranscriptRetrySourceBinding,
) -> Result<(), TranscriptRetryError> {
    if !valid_sha256(&binding.source_transcript_sha256)
        || !valid_sha256(&binding.capture_session_sha256)
        || !valid_sha256(&binding.microphone_audio_sha256)
        || !valid_sha256(&binding.system_audio_sha256)
        || !valid_sha256(&binding.candidate_transcript_sha256)
    {
        return Err(TranscriptRetryError::BindingMismatch);
    }
    Ok(())
}

fn binding_matches_meeting(
    binding: &TranscriptRetrySourceBinding,
    meeting: &MeetingRecord,
) -> bool {
    meeting
        .artifacts
        .current_transcript
        .as_ref()
        .is_some_and(|reference| reference.sha256 == binding.source_transcript_sha256)
        && meeting
            .artifacts
            .capture_session
            .as_ref()
            .is_some_and(|reference| reference.sha256 == binding.capture_session_sha256)
        && meeting
            .artifacts
            .microphone_audio
            .as_ref()
            .is_some_and(|reference| reference.sha256 == binding.microphone_audio_sha256)
        && meeting
            .artifacts
            .system_audio
            .as_ref()
            .is_some_and(|reference| reference.sha256 == binding.system_audio_sha256)
}

fn binding_matches_candidate(
    binding: &TranscriptRetrySourceBinding,
    candidate: &TranscriptRetryCandidate,
) -> bool {
    candidate.source_transcript_sha256 == binding.source_transcript_sha256
        && candidate.capture_session_sha256 == binding.capture_session_sha256
        && candidate.microphone_audio_sha256 == binding.microphone_audio_sha256
        && candidate.system_audio_sha256 == binding.system_audio_sha256
        && candidate.candidate_transcript.sha256 == binding.candidate_transcript_sha256
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::{
        AudioRetention, AudioRetentionRule, MAX_MEETING_RECORD_BYTES, MeetingArtifacts,
        MeetingSchema, artifact_ref,
    };
    use crate::storage::StorageRoot;
    use std::os::unix::fs::OpenOptionsExt;
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        storage: StorageRoot,
        coordination: MeetingStorageCoordination,
        meeting_dir: PathBuf,
        source: Vec<u8>,
    }

    fn private_file(path: &Path, bytes: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut file, bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app-data"), &repo).unwrap();
        let meeting_dir = storage.path().join("meetings/meeting-a");
        create_private_dir(&meeting_dir).unwrap();
        create_private_dir(&meeting_dir.join("capture")).unwrap();
        create_private_dir(&meeting_dir.join("transcript")).unwrap();
        let source = br#"{"schema":"capture-transcript/1"}"#.to_vec();
        private_file(&meeting_dir.join("attempt.json"), b"attempt");
        private_file(&meeting_dir.join("ownership.json"), b"owner");
        private_file(&meeting_dir.join("capture/session.json"), b"capture");
        private_file(&meeting_dir.join("capture/mic.wav"), &[1; 64]);
        private_file(&meeting_dir.join("capture/system.wav"), &[2; 64]);
        let src_path = meeting_dir
            .join("transcript")
            .join(format!("{}.json", digest_bytes(&source)));
        private_file(&src_path, &source);
        let rule = AudioRetentionRule::UntilManualDeletion;
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: "meeting-a".into(),
            lifecycle: MeetingLifecycle::TranscriptReady,
            retention: AudioRetention {
                policy_sha256: crate::meeting::retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: None,
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&meeting_dir, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&meeting_dir, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap()),
                microphone_audio: Some(artifact_ref(&meeting_dir, "capture/mic.wav").unwrap()),
                system_audio: Some(artifact_ref(&meeting_dir, "capture/system.wav").unwrap()),
                current_transcript: Some(
                    artifact_ref(
                        &meeting_dir,
                        &format!("transcript/{}.json", digest_bytes(&source)),
                    )
                    .unwrap(),
                ),
                current_note: None,
            },
            pending_storage_operation: None,
        };
        write_meeting(&meeting_dir, &meeting).unwrap();
        Fixture {
            _temp: temp,
            storage,
            coordination: MeetingStorageCoordination::default(),
            meeting_dir,
            source,
        }
    }

    fn binding(fixture: &Fixture, candidate: &[u8]) -> TranscriptRetrySourceBinding {
        let meeting = load_meeting(&fixture.meeting_dir).unwrap();
        TranscriptRetrySourceBinding {
            source_transcript_sha256: meeting
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            capture_session_sha256: meeting
                .artifacts
                .capture_session
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            microphone_audio_sha256: meeting
                .artifacts
                .microphone_audio
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            system_audio_sha256: meeting
                .artifacts
                .system_audio
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            candidate_transcript_sha256: digest_bytes(candidate),
        }
    }

    #[test]
    fn candidate_is_available_without_pointer_mutation_and_can_be_kept() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let before = read_private_bytes(
            &fixture.meeting_dir.join("meeting.json"),
            MAX_MEETING_RECORD_BYTES,
        )
        .unwrap();
        let id = Uuid::new_v4();
        assert_eq!(
            authority
                .create_candidate(
                    "meeting-a",
                    id,
                    b"candidate",
                    &binding(&fixture, b"candidate")
                )
                .unwrap(),
            TranscriptRetryOutcome::CandidateAvailableForComparison
        );
        assert_eq!(
            read_private_bytes(
                &fixture.meeting_dir.join("meeting.json"),
                MAX_MEETING_RECORD_BYTES
            )
            .unwrap(),
            before
        );
        assert_eq!(
            authority.keep_current("meeting-a", id).unwrap(),
            TranscriptRetryOutcome::CurrentKept
        );
        assert_eq!(
            authority.keep_current("meeting-a", id).unwrap(),
            TranscriptRetryOutcome::CurrentKept
        );
    }

    #[test]
    fn promotion_changes_only_current_pointer_and_preserves_bytes() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        let candidate = b"candidate transcript";
        let source_path = fixture
            .meeting_dir
            .join("transcript")
            .join(format!("{}.json", digest_bytes(&fixture.source)));
        assert_eq!(
            authority
                .create_candidate("meeting-a", id, candidate, &binding(&fixture, candidate))
                .unwrap(),
            TranscriptRetryOutcome::CandidateAvailableForComparison
        );
        assert_eq!(
            authority.promote_candidate("meeting-a", id).unwrap(),
            TranscriptRetryOutcome::CandidatePromoted
        );
        assert_eq!(
            read_private_bytes(&source_path, MAX_TRANSCRIPT_BYTES).unwrap(),
            fixture.source
        );
        let meeting = load_meeting(&fixture.meeting_dir).unwrap();
        assert_eq!(
            meeting.artifacts.current_transcript.unwrap().sha256,
            digest_bytes(candidate)
        );
        assert_eq!(
            authority.promote_candidate("meeting-a", id).unwrap(),
            TranscriptRetryOutcome::CandidatePromoted
        );
    }

    #[test]
    fn promotion_receipt_recovers_after_meeting_publish() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();
        let receipt = authority.inspect_candidate("meeting-a", id).unwrap();
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        meeting.artifacts.current_transcript = Some(receipt.candidate_transcript);
        meeting.lifecycle = MeetingLifecycle::TranscriptReady;
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();
        assert_eq!(
            authority.promote_candidate("meeting-a", id).unwrap(),
            TranscriptRetryOutcome::CandidatePromoted
        );
    }

    #[test]
    fn stale_binding_and_candidate_digest_are_rejected_before_any_write() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let mut stale = binding(&fixture, b"candidate");
        stale.source_transcript_sha256 = "a".repeat(64);
        assert!(matches!(
            authority.create_candidate("meeting-a", Uuid::new_v4(), b"candidate", &stale),
            Err(TranscriptRetryError::BindingMismatch)
        ));
        assert!(!fixture.meeting_dir.join(RETRY_DIRECTORY).exists());

        let mut mismatch = binding(&fixture, b"candidate");
        mismatch.candidate_transcript_sha256 = "b".repeat(64);
        assert!(matches!(
            authority.create_candidate("meeting-a", Uuid::new_v4(), b"candidate", &mismatch),
            Err(TranscriptRetryError::CandidateDigestMismatch)
        ));
        assert!(!fixture.meeting_dir.join(RETRY_DIRECTORY).exists());
    }

    #[test]
    fn ineligible_lifecycle_is_rejected_before_candidate_storage() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let candidate_binding = binding(&fixture, b"candidate");
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        meeting.lifecycle = MeetingLifecycle::Captured;
        meeting.artifacts.current_transcript = None;
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();
        assert!(matches!(
            authority.create_candidate(
                "meeting-a",
                Uuid::new_v4(),
                b"candidate",
                &candidate_binding
            ),
            Err(TranscriptRetryError::IneligibleLifecycle)
        ));
        assert!(!fixture.meeting_dir.join(RETRY_DIRECTORY).exists());
    }

    #[test]
    fn released_audio_and_stale_source_fail_closed() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        create_private_dir(&fixture.meeting_dir.join("deletion")).unwrap();
        private_file(
            &fixture.meeting_dir.join("deletion/audio-deletion.json"),
            b"released",
        );
        meeting.retention.state = AudioState::Released;
        meeting.retention.deletion_receipt =
            Some(artifact_ref(&fixture.meeting_dir, "deletion/audio-deletion.json").unwrap());
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();
        assert!(matches!(
            authority.create_candidate(
                "meeting-a",
                Uuid::new_v4(),
                b"other",
                &binding(&fixture, b"other"),
            ),
            Err(TranscriptRetryError::AudioReleased)
        ));
    }

    #[test]
    fn pending_candidate_is_discoverable_after_authority_restart() {
        let fixture = fixture();
        let id = Uuid::new_v4();
        TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination)
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();

        let restarted = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let discovered = restarted
            .discover_pending_candidate("meeting-a")
            .unwrap()
            .unwrap();
        assert_eq!(discovered.operation_id, id);
        assert_eq!(
            discovered.candidate_transcript_sha256,
            digest_bytes(b"candidate")
        );
        assert_eq!(
            discovered.state,
            TranscriptRetryState::CandidateAvailableForComparison
        );
    }

    #[test]
    fn a_second_pending_candidate_is_refused_but_same_operation_is_idempotent() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let first = Uuid::new_v4();
        authority
            .create_candidate("meeting-a", first, b"first", &binding(&fixture, b"first"))
            .unwrap();

        let second = Uuid::new_v4();
        assert_eq!(
            authority
                .create_candidate(
                    "meeting-a",
                    second,
                    b"second",
                    &binding(&fixture, b"second")
                )
                .unwrap_err()
                .to_string(),
            "another transcript retry candidate is awaiting a decision"
        );
        assert!(
            !fixture
                .meeting_dir
                .join(RETRY_DIRECTORY)
                .join(second.to_string())
                .exists()
        );
        assert_eq!(
            authority
                .create_candidate("meeting-a", first, b"first", &binding(&fixture, b"first"))
                .unwrap(),
            TranscriptRetryOutcome::CandidateAvailableForComparison
        );
    }

    #[test]
    fn stale_source_is_refused_during_restart_discovery() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();

        let newer = b"newer source";
        let newer_path = fixture
            .meeting_dir
            .join("transcript")
            .join(format!("{}.json", digest_bytes(newer)));
        private_file(&newer_path, newer);
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        meeting.artifacts.current_transcript = Some(
            artifact_ref(
                &fixture.meeting_dir,
                &format!("transcript/{}.json", digest_bytes(newer)),
            )
            .unwrap(),
        );
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();

        assert!(matches!(
            authority.discover_pending_candidate("meeting-a"),
            Err(TranscriptRetryError::StaleSource)
        ));
    }

    #[test]
    fn released_audio_is_refused_during_restart_discovery() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        create_private_dir(&fixture.meeting_dir.join("deletion")).unwrap();
        private_file(
            &fixture.meeting_dir.join("deletion/audio-deletion.json"),
            b"released",
        );
        meeting.retention.state = AudioState::Released;
        meeting.retention.deletion_receipt =
            Some(artifact_ref(&fixture.meeting_dir, "deletion/audio-deletion.json").unwrap());
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();

        assert!(matches!(
            authority.discover_pending_candidate("meeting-a"),
            Err(TranscriptRetryError::AudioReleased)
        ));
    }

    #[test]
    fn corrupt_or_cross_meeting_receipts_are_refused() {
        {
            let fixture = fixture();
            let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
            let corrupt = Uuid::new_v4();
            authority
                .create_candidate(
                    "meeting-a",
                    corrupt,
                    b"candidate",
                    &binding(&fixture, b"candidate"),
                )
                .unwrap();
            std::fs::write(
                fixture
                    .meeting_dir
                    .join(RETRY_DIRECTORY)
                    .join(corrupt.to_string())
                    .join(RECEIPT_FILE),
                b"not json",
            )
            .unwrap();
            assert!(matches!(
                authority.discover_pending_candidate("meeting-a"),
                Err(TranscriptRetryError::MalformedOperation)
            ));
        }

        let cross_fixture = fixture();
        let authority =
            TranscriptRetryAuthority::new(&cross_fixture.storage, &cross_fixture.coordination);
        let cross = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                cross,
                b"cross candidate",
                &binding(&cross_fixture, b"cross candidate"),
            )
            .unwrap();
        let receipt_path = cross_fixture
            .meeting_dir
            .join(RETRY_DIRECTORY)
            .join(cross.to_string())
            .join(RECEIPT_FILE);
        let mut receipt: TranscriptRetryCandidate =
            serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
        receipt.meeting_id = "meeting-b".into();
        std::fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(matches!(
            authority.discover_pending_candidate("meeting-a"),
            Err(TranscriptRetryError::CrossMeetingCandidate)
        ));
    }

    #[test]
    fn candidate_bytes_are_bounded_and_verified() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let id = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                id,
                b"candidate",
                &binding(&fixture, b"candidate"),
            )
            .unwrap();
        assert_eq!(
            authority.read_candidate_bytes("meeting-a", id).unwrap(),
            b"candidate"
        );

        std::fs::write(
            fixture
                .meeting_dir
                .join("transcript")
                .join(format!("{}.json", digest_bytes(b"candidate"))),
            b"tampered",
        )
        .unwrap();
        assert!(matches!(
            authority.read_candidate_bytes("meeting-a", id),
            Err(TranscriptRetryError::CandidateChanged)
                | Err(TranscriptRetryError::CandidateDigestMismatch)
        ));
    }

    #[test]
    fn terminal_candidates_are_not_reported_as_pending() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        let kept = Uuid::new_v4();
        authority
            .create_candidate("meeting-a", kept, b"kept", &binding(&fixture, b"kept"))
            .unwrap();
        authority.keep_current("meeting-a", kept).unwrap();
        assert_eq!(
            authority.discover_pending_candidate("meeting-a").unwrap(),
            None
        );

        let promoted = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                promoted,
                b"promoted",
                &binding(&fixture, b"promoted"),
            )
            .unwrap();
        authority.promote_candidate("meeting-a", promoted).unwrap();
        assert_eq!(
            authority.discover_pending_candidate("meeting-a").unwrap(),
            None
        );
    }

    #[test]
    fn discovery_allows_256_terminal_receipts_but_bounds_the_next_entry() {
        let fixture = fixture();
        let authority = TranscriptRetryAuthority::new(&fixture.storage, &fixture.coordination);
        for _ in 0..MAX_RETRY_DISCOVERY_ENTRIES {
            let operation_id = Uuid::new_v4();
            authority
                .create_candidate(
                    "meeting-a",
                    operation_id,
                    b"terminal",
                    &binding(&fixture, b"terminal"),
                )
                .unwrap();
            assert_eq!(
                authority.keep_current("meeting-a", operation_id).unwrap(),
                TranscriptRetryOutcome::CurrentKept
            );
        }
        assert_eq!(
            authority.discover_pending_candidate("meeting-a").unwrap(),
            None
        );

        let overflow = Uuid::new_v4();
        authority
            .create_candidate(
                "meeting-a",
                overflow,
                b"terminal",
                &binding(&fixture, b"terminal"),
            )
            .unwrap();
        authority.keep_current("meeting-a", overflow).unwrap();
        assert!(matches!(
            authority.discover_pending_candidate("meeting-a"),
            Err(TranscriptRetryError::DiscoveryBoundExceeded)
        ));
    }
}
