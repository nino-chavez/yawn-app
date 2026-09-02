use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::storage::{durable_create_new, durable_replace};

pub const MAX_MEETING_RECORD_BYTES: u64 = 256 * 1024;
pub const MAX_RECEIPT_BYTES: u64 = 256 * 1024;
pub(crate) const MICROPHONE_AUDIO_PATH: &str = "capture/mic.wav";
pub(crate) const SYSTEM_AUDIO_PATH: &str = "capture/system.wav";
pub(crate) const MICROPHONE_PARTIAL_AUDIO_PATH: &str = "capture/.mic.wav.partial";
pub(crate) const SYSTEM_PARTIAL_AUDIO_PATH: &str = "capture/.system.wav.partial";
pub(crate) const AUDIO_ARTIFACT_PATHS: [&str; 4] = [
    MICROPHONE_AUDIO_PATH,
    SYSTEM_AUDIO_PATH,
    MICROPHONE_PARTIAL_AUDIO_PATH,
    SYSTEM_PARTIAL_AUDIO_PATH,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetingRecord {
    pub schema: MeetingSchema,
    pub meeting_id: String,
    pub lifecycle: MeetingLifecycle,
    pub retention: AudioRetention,
    pub artifacts: MeetingArtifacts,
    pub pending_storage_operation: Option<PendingStorageOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeetingSchema {
    #[serde(rename = "meeting/2")]
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingLifecycle {
    Incomplete,
    Captured,
    TranscriptReady,
    TranscriptionFailed,
    SummaryFailed,
    Ready,
    RecoveredInterrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioRetention {
    pub rule: AudioRetentionRule,
    pub policy_sha256: String,
    pub next_deletion_at_epoch_seconds: Option<u64>,
    pub state: AudioState,
    pub deletion_receipt: Option<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AudioRetentionRule {
    DeleteAfter { seconds: u64 },
    UntilManualDeletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioState {
    NeverCreated,
    Retained,
    Deleting,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetingArtifacts {
    pub attempt: ArtifactRef,
    pub ownership: Option<ArtifactRef>,
    pub capture_session: Option<ArtifactRef>,
    pub microphone_audio: Option<ArtifactRef>,
    pub system_audio: Option<ArtifactRef>,
    pub current_transcript: Option<ArtifactRef>,
    pub current_note: Option<NoteRevisionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteRevisionRef {
    pub json: ArtifactRef,
    pub markdown: ArtifactRef,
    pub source_transcript_sha256: String,
}

impl NoteRevisionRef {
    pub fn validate(&self) -> Result<(), MeetingError> {
        validate_digest_named_ref(&self.json, "notes", "json")?;
        validate_digest_named_ref(&self.markdown, "notes", "md")?;
        validate_sha256(&self.source_transcript_sha256)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub relative_path: String,
    pub sha256: String,
}

impl ArtifactRef {
    pub fn validate_digest_named(
        &self,
        directory: &str,
        extension: &str,
    ) -> Result<(), MeetingError> {
        validate_digest_named_ref(self, directory, extension)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingStorageOperation {
    #[serde(rename = "audio-deletion/1")]
    AudioDeletionV1,
    /// The transcript and every generated note are being removed, while the
    /// capture evidence and the operator's own note remain with the meeting.
    #[serde(rename = "transcript-deletion/1")]
    TranscriptDeletionV1,
}

#[derive(Debug, Error)]
pub enum MeetingError {
    #[error("meeting record is malformed: {0}")]
    Malformed(&'static str),
    #[error("meeting artifact is missing or changed")]
    ArtifactMismatch,
    #[error("private meeting storage is invalid")]
    InvalidPrivateStorage,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl MeetingRecord {
    pub fn validate(&self, directory_id: &str) -> Result<(), MeetingError> {
        if !valid_opaque_id(directory_id) || self.meeting_id != directory_id {
            return Err(MeetingError::Malformed("meeting identifier mismatch"));
        }
        if self.retention.policy_sha256 != retention_policy_sha256(&self.retention.rule) {
            return Err(MeetingError::Malformed("retention policy digest mismatch"));
        }
        match self.retention.rule {
            AudioRetentionRule::DeleteAfter { seconds } => {
                if seconds == 0 || self.retention.next_deletion_at_epoch_seconds.is_none() {
                    return Err(MeetingError::Malformed("invalid automatic retention rule"));
                }
            }
            AudioRetentionRule::UntilManualDeletion => {
                if self.retention.next_deletion_at_epoch_seconds.is_some() {
                    return Err(MeetingError::Malformed("manual retention has a deadline"));
                }
            }
        }

        validate_ref(&self.artifacts.attempt, "attempt.json")?;
        if let Some(ownership) = &self.artifacts.ownership {
            validate_ref(ownership, "ownership.json")?;
        }
        if let Some(session) = &self.artifacts.capture_session {
            validate_ref(session, "capture/session.json")?;
        }
        let recovered_interruption = self.lifecycle == MeetingLifecycle::RecoveredInterrupted;
        if let Some(mic) = &self.artifacts.microphone_audio {
            validate_audio_ref(
                mic,
                MICROPHONE_AUDIO_PATH,
                MICROPHONE_PARTIAL_AUDIO_PATH,
                recovered_interruption,
            )?;
        }
        if let Some(system) = &self.artifacts.system_audio {
            validate_audio_ref(
                system,
                SYSTEM_AUDIO_PATH,
                SYSTEM_PARTIAL_AUDIO_PATH,
                recovered_interruption,
            )?;
        }
        if let Some(transcript) = &self.artifacts.current_transcript {
            validate_digest_named_ref(transcript, "transcript", "json")?;
        }
        if let Some(note) = &self.artifacts.current_note {
            note.validate()?;
        }

        let complete_capture = self.artifacts.capture_session.is_some()
            && self.artifacts.microphone_audio.is_some()
            && self.artifacts.system_audio.is_some();
        let any_capture = self.artifacts.capture_session.is_some()
            || self.artifacts.microphone_audio.is_some()
            || self.artifacts.system_audio.is_some();
        let any_audio =
            self.artifacts.microphone_audio.is_some() || self.artifacts.system_audio.is_some();
        let transcript = self.artifacts.current_transcript.is_some();
        let note = self.artifacts.current_note.is_some();
        let audio_deletion_pending = self.pending_storage_operation
            == Some(PendingStorageOperation::AudioDeletionV1);
        let transcript_deletion_pending = self.pending_storage_operation
            == Some(PendingStorageOperation::TranscriptDeletionV1);

        match self.lifecycle {
            MeetingLifecycle::Incomplete => {
                if any_capture || transcript || note {
                    return Err(MeetingError::Malformed(
                        "incomplete meeting points to committed artifacts",
                    ));
                }
            }
            MeetingLifecycle::Captured | MeetingLifecycle::TranscriptionFailed => {
                if !complete_capture || transcript || note || self.artifacts.ownership.is_none() {
                    return Err(MeetingError::Malformed(
                        "invalid captured meeting artifacts",
                    ));
                }
            }
            MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed => {
                if !complete_capture || !transcript || note || self.artifacts.ownership.is_none() {
                    return Err(MeetingError::Malformed(
                        "invalid transcript-ready artifacts",
                    ));
                }
            }
            MeetingLifecycle::Ready => {
                if !complete_capture || !transcript || !note || self.artifacts.ownership.is_none() {
                    return Err(MeetingError::Malformed("invalid ready meeting artifacts"));
                }
                let current = self
                    .artifacts
                    .current_transcript
                    .as_ref()
                    .expect("checked above");
                let source = &self
                    .artifacts
                    .current_note
                    .as_ref()
                    .expect("checked above")
                    .source_transcript_sha256;
                if source != &current.sha256 {
                    return Err(MeetingError::Malformed("note binds another transcript"));
                }
            }
            MeetingLifecycle::RecoveredInterrupted => {
                if transcript || note {
                    return Err(MeetingError::Malformed(
                        "interrupted meeting points to derived artifacts",
                    ));
                }
                if any_audio != self.artifacts.capture_session.is_some() {
                    return Err(MeetingError::Malformed(
                        "interrupted audio and its capture receipt disagree",
                    ));
                }
                validate_interrupted_audio_family(&self.artifacts)?;
            }
        }

        match self.retention.state {
            AudioState::NeverCreated => {
                if any_audio
                    || self.retention.deletion_receipt.is_some()
                    || self.pending_storage_operation.is_some()
                {
                    return Err(MeetingError::Malformed("invalid never-created audio state"));
                }
            }
            AudioState::Retained => {
                if !any_audio
                    || self.retention.deletion_receipt.is_some()
                    || audio_deletion_pending
                {
                    return Err(MeetingError::Malformed("invalid retained audio state"));
                }
            }
            AudioState::Deleting => {
                if !any_audio
                    || self.retention.deletion_receipt.is_some()
                    || !audio_deletion_pending
                {
                    return Err(MeetingError::Malformed("invalid deleting audio state"));
                }
            }
            AudioState::Released => {
                let Some(receipt) = &self.retention.deletion_receipt else {
                    return Err(MeetingError::Malformed("released audio lacks a receipt"));
                };
                validate_ref(receipt, "deletion/audio-deletion.json")?;
                if !any_audio || audio_deletion_pending {
                    return Err(MeetingError::Malformed("invalid released audio state"));
                }
            }
        }

        // `transcript-deletion/1` first detaches all derived evidence from the
        // record, then stages and removes the transcript and notes directories.
        // The only durable intermediate that may be presented to a reader is
        // therefore a captured meeting with no source transcript or generated
        // note. Keeping this pairing exact prevents an audio-deletion receipt
        // from being mistaken for authority to remove text, or a dangling
        // transcript pointer from surviving an accepted deletion.
        if transcript_deletion_pending
            && (self.lifecycle != MeetingLifecycle::Captured || transcript || note)
        {
            return Err(MeetingError::Malformed(
                "invalid pending transcript deletion state",
            ));
        }
        if self.pending_storage_operation.is_some()
            && !audio_deletion_pending
            && !transcript_deletion_pending
        {
            return Err(MeetingError::Malformed("unknown pending storage operation"));
        }

        if self.lifecycle == MeetingLifecycle::Incomplete
            && self.retention.state != AudioState::NeverCreated
        {
            return Err(MeetingError::Malformed("incomplete meeting owns audio"));
        }
        if self.lifecycle == MeetingLifecycle::RecoveredInterrupted {
            if any_audio && self.retention.state == AudioState::NeverCreated {
                return Err(MeetingError::Malformed("partial audio is not retained"));
            }
            if !any_audio && self.retention.state != AudioState::NeverCreated {
                return Err(MeetingError::Malformed("interrupted meeting has no audio"));
            }
        }
        Ok(())
    }
}

pub fn retention_policy_sha256(rule: &AudioRetentionRule) -> String {
    #[derive(Serialize)]
    struct CanonicalPolicy<'a> {
        schema: &'static str,
        rule: &'a AudioRetentionRule,
    }
    let bytes = serde_json::to_vec(&CanonicalPolicy {
        schema: "audio-retention-policy/1",
        rule,
    })
    .expect("retention policy is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

pub fn load_meeting(meeting_dir: &Path) -> Result<MeetingRecord, MeetingError> {
    require_private_directory(meeting_dir)?;
    let directory_id = meeting_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(MeetingError::Malformed("meeting directory name is invalid"))?;
    let bytes = read_private_bytes(&meeting_dir.join("meeting.json"), MAX_MEETING_RECORD_BYTES)?;
    let meeting: MeetingRecord = serde_json::from_slice(&bytes)?;
    meeting.validate(directory_id)?;
    Ok(meeting)
}

pub fn write_meeting(meeting_dir: &Path, meeting: &MeetingRecord) -> Result<(), MeetingError> {
    let directory_id = meeting_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(MeetingError::Malformed("meeting directory name is invalid"))?;
    meeting.validate(directory_id)?;
    durable_replace(
        &meeting_dir.join("meeting.json"),
        &serde_json::to_vec_pretty(meeting)?,
    )?;
    Ok(())
}

pub fn verify_record_artifacts(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<(), MeetingError> {
    verify_record_static_artifacts(meeting_dir, meeting)?;
    if meeting.retention.state == AudioState::Retained {
        for reference in [
            meeting.artifacts.microphone_audio.as_ref(),
            meeting.artifacts.system_audio.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            verify_artifact_ref(meeting_dir, reference)?;
        }
    }
    if let Some(reference) = &meeting.retention.deletion_receipt {
        verify_artifact_ref(meeting_dir, reference)?;
    }
    Ok(())
}

pub fn verify_record_static_artifacts(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<(), MeetingError> {
    verify_artifact_ref(meeting_dir, &meeting.artifacts.attempt)?;
    if let Some(reference) = &meeting.artifacts.ownership {
        verify_artifact_ref(meeting_dir, reference)?;
    }
    if let Some(reference) = &meeting.artifacts.capture_session {
        verify_artifact_ref(meeting_dir, reference)?;
    }
    if meeting.lifecycle == MeetingLifecycle::RecoveredInterrupted {
        verify_recovered_capture_receipt(meeting_dir, meeting)?;
    }
    if let Some(reference) = &meeting.artifacts.current_transcript {
        verify_artifact_ref(meeting_dir, reference)?;
    }
    if let Some(note) = &meeting.artifacts.current_note {
        verify_artifact_ref(meeting_dir, &note.json)?;
        verify_artifact_ref(meeting_dir, &note.markdown)?;
    }
    Ok(())
}

pub(crate) fn write_capture_interruption_receipt(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<ArtifactRef, MeetingError> {
    let artifacts = receipt_artifacts(meeting_dir, meeting, true)?;
    if artifacts.is_empty() || artifacts.iter().any(|artifact| artifact.bytes < 44) {
        return Err(MeetingError::Malformed(
            "an interruption receipt requires readable WAV storage",
        ));
    }
    let receipt = CaptureInterruptionReceipt {
        schema: CaptureInterruptionSchema::V1,
        status: CaptureInterruptionStatus::Interrupted,
        reason: CaptureInterruptionReason::FreshProcessRecovery,
        meeting_id: meeting.meeting_id.clone(),
        artifacts,
    };
    durable_create_new(
        &meeting_dir.join("capture/session.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    artifact_ref(meeting_dir, "capture/session.json")
}

/// Whether a captured meeting's on-disk capture receipt is a complete,
/// finalized `capture-session/2` — the shape a real finish writes.
///
/// A quit a moment into worker-finalize can leave a meeting at `captured`
/// lifecycle whose `capture/session.json` was never written as a finished
/// session (missing entirely, still `incomplete`, or lacking its `health` /
/// `reconciliation` audit blocks). Recovery uses this to tell a legitimate
/// transcription source apart from a quit-mid-finalize meeting: the first is
/// left for the background queue, the second is salvaged to the interrupted
/// shape. A receipt that cannot be read at all reads as not finalized rather
/// than raising — an unreadable receipt is exactly the interrupted case.
pub(crate) fn captured_capture_is_finalized(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<bool, MeetingError> {
    let Some(session) = &meeting.artifacts.capture_session else {
        return Ok(false);
    };
    if verify_artifact_ref(meeting_dir, session).is_err() {
        return Ok(false);
    }
    let path = match resolve_artifact(meeting_dir, &session.relative_path) {
        Ok(path) => path,
        Err(_) => return Ok(false),
    };
    let bytes = match read_private_bytes(&path, MAX_RECEIPT_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    if value.get("schema").and_then(Value::as_str) != Some("capture-session/2") {
        return Ok(false);
    }
    Ok(value.get("status").and_then(Value::as_str) == Some("complete")
        && value.get("started_at").is_some_and(Value::is_string)
        && value.get("finalized_at").is_some_and(Value::is_string)
        && value.get("health").is_some_and(Value::is_object)
        && value.get("reconciliation").is_some_and(Value::is_object))
}

fn verify_recovered_capture_receipt(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<(), MeetingError> {
    let references = audio_references(meeting);
    let Some(session) = &meeting.artifacts.capture_session else {
        return if references.is_empty() {
            Ok(())
        } else {
            Err(MeetingError::Malformed(
                "interrupted audio lacks its capture receipt",
            ))
        };
    };
    if references.is_empty() {
        return Err(MeetingError::Malformed(
            "interruption receipt has no retained audio",
        ));
    }

    verify_artifact_ref(meeting_dir, session)?;
    let bytes = read_private_bytes(
        &resolve_artifact(meeting_dir, &session.relative_path)?,
        MAX_RECEIPT_BYTES,
    )?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .ok_or(MeetingError::Malformed("capture receipt lacks a schema"))?;
    let inspect_live_files = meeting.retention.state == AudioState::Retained;
    let expected = receipt_artifacts(meeting_dir, meeting, inspect_live_files)?;

    let actual = match schema {
        "capture-interruption/1" => {
            let receipt: CaptureInterruptionReceipt = serde_json::from_value(value)?;
            if receipt.meeting_id != meeting.meeting_id {
                return Err(MeetingError::ArtifactMismatch);
            }
            receipt.artifacts
        }
        "capture-session/2" => {
            let receipt: CaptureSessionReceipt = serde_json::from_value(value)?;
            if receipt.status != CaptureSessionStatus::Complete
                || !receipt.started_at.is_string()
                || !receipt.finalized_at.is_string()
                || !receipt.health.is_object()
                || !receipt.reconciliation.is_object()
                || references
                    .iter()
                    .any(|reference| reference.relative_path.starts_with("capture/."))
            {
                return Err(MeetingError::Malformed(
                    "capture session is not a complete canonical acquisition",
                ));
            }
            receipt.artifacts
        }
        _ => {
            return Err(MeetingError::Malformed(
                "capture receipt schema is not recognized",
            ));
        }
    };

    if !receipt_artifacts_match(&actual, &expected, inspect_live_files) {
        return Err(MeetingError::ArtifactMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureReceiptArtifact {
    name: String,
    bytes: u64,
    sha256: String,
    mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureInterruptionReceipt {
    schema: CaptureInterruptionSchema,
    status: CaptureInterruptionStatus,
    reason: CaptureInterruptionReason,
    meeting_id: String,
    artifacts: Vec<CaptureReceiptArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum CaptureInterruptionSchema {
    #[serde(rename = "capture-interruption/1")]
    V1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum CaptureInterruptionStatus {
    #[serde(rename = "interrupted")]
    Interrupted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum CaptureInterruptionReason {
    #[serde(rename = "fresh-process-recovery")]
    FreshProcessRecovery,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureSessionReceipt {
    #[serde(rename = "schema")]
    _schema: CaptureSessionSchema,
    status: CaptureSessionStatus,
    started_at: Value,
    finalized_at: Value,
    health: Value,
    #[serde(default, rename = "quality")]
    _quality: Option<Value>,
    #[serde(default, rename = "microphone")]
    _microphone: Option<Value>,
    /// Spans where the operator held the recording open with no audio being
    /// acquired. Absent on every receipt written before pause existed, which
    /// reads as "no pauses" rather than as an unreadable receipt.
    #[serde(default, rename = "pauses")]
    _pauses: Option<Value>,
    reconciliation: Value,
    artifacts: Vec<CaptureReceiptArtifact>,
}

#[derive(Debug, Deserialize)]
enum CaptureSessionSchema {
    #[serde(rename = "capture-session/2")]
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CaptureSessionStatus {
    Incomplete,
    Complete,
    Failed,
    Abandoned,
}

fn receipt_artifacts(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
    inspect_live_files: bool,
) -> Result<Vec<CaptureReceiptArtifact>, MeetingError> {
    let mut artifacts = Vec::new();
    for reference in audio_references(meeting) {
        let name = Path::new(&reference.relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(MeetingError::Malformed("audio artifact name is invalid"))?
            .to_owned();
        let bytes = if inspect_live_files {
            verify_artifact_ref(meeting_dir, reference)?;
            open_private_file(&resolve_artifact(meeting_dir, &reference.relative_path)?)?
                .metadata()?
                .len()
        } else {
            0
        };
        artifacts.push(CaptureReceiptArtifact {
            name,
            bytes,
            sha256: reference.sha256.clone(),
            mode: "0600".into(),
        });
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(artifacts)
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

fn receipt_artifacts_match(
    actual: &[CaptureReceiptArtifact],
    expected: &[CaptureReceiptArtifact],
    compare_bytes: bool,
) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.name == expected.name
                && actual.sha256 == expected.sha256
                && actual.mode == expected.mode
                && actual.bytes >= 44
                && (!compare_bytes || actual.bytes == expected.bytes)
        })
}

pub fn artifact_ref(meeting_dir: &Path, relative_path: &str) -> Result<ArtifactRef, MeetingError> {
    let path = resolve_artifact(meeting_dir, relative_path)?;
    Ok(ArtifactRef {
        relative_path: relative_path.to_owned(),
        sha256: hash_private_file(&path)?,
    })
}

pub fn verify_artifact_ref(
    meeting_dir: &Path,
    reference: &ArtifactRef,
) -> Result<(), MeetingError> {
    let path = resolve_artifact(meeting_dir, &reference.relative_path)?;
    if hash_private_file(&path)? != reference.sha256 {
        return Err(MeetingError::ArtifactMismatch);
    }
    Ok(())
}

pub fn resolve_artifact(meeting_dir: &Path, relative_path: &str) -> Result<PathBuf, MeetingError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MeetingError::Malformed("artifact path is unsafe"));
    }
    let mut parent = meeting_dir.to_path_buf();
    let component_count = relative.components().count();
    for component in relative
        .components()
        .take(component_count.saturating_sub(1))
    {
        parent.push(component.as_os_str());
        require_private_directory(&parent)?;
    }
    Ok(meeting_dir.join(relative))
}

pub fn read_private_bytes(path: &Path, maximum: u64) -> Result<Vec<u8>, MeetingError> {
    let mut file = open_private_file(path)?;
    let length = file.metadata()?.len();
    if length > maximum {
        return Err(MeetingError::InvalidPrivateStorage);
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn hash_private_file(path: &Path) -> Result<String, MeetingError> {
    let mut file = open_private_file(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn open_private_file(path: &Path) -> Result<File, MeetingError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o777 != 0o600 {
        return Err(MeetingError::InvalidPrivateStorage);
    }
    Ok(file)
}

pub fn require_private_directory(path: &Path) -> Result<(), MeetingError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_dir()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(MeetingError::InvalidPrivateStorage);
    }
    Ok(())
}

fn validate_ref(reference: &ArtifactRef, expected_path: &str) -> Result<(), MeetingError> {
    if reference.relative_path != expected_path {
        return Err(MeetingError::Malformed(
            "artifact path does not match its role",
        ));
    }
    validate_sha256(&reference.sha256)
}

fn validate_audio_ref(
    reference: &ArtifactRef,
    canonical_path: &str,
    partial_path: &str,
    allow_partial: bool,
) -> Result<(), MeetingError> {
    let accepted = reference.relative_path == canonical_path
        || (allow_partial && reference.relative_path == partial_path);
    if !accepted {
        return Err(MeetingError::Malformed(
            "audio artifact path does not match its role",
        ));
    }
    validate_sha256(&reference.sha256)
}

fn validate_interrupted_audio_family(artifacts: &MeetingArtifacts) -> Result<(), MeetingError> {
    let microphone_path = artifacts
        .microphone_audio
        .as_ref()
        .map(|reference| reference.relative_path.as_str());
    let system_path = artifacts
        .system_audio
        .as_ref()
        .map(|reference| reference.relative_path.as_str());
    let canonical_count = [microphone_path, system_path]
        .into_iter()
        .flatten()
        .filter(|path| matches!(*path, MICROPHONE_AUDIO_PATH | SYSTEM_AUDIO_PATH))
        .count();
    let partial_count = [microphone_path, system_path]
        .into_iter()
        .flatten()
        .filter(|path| {
            matches!(
                *path,
                MICROPHONE_PARTIAL_AUDIO_PATH | SYSTEM_PARTIAL_AUDIO_PATH
            )
        })
        .count();
    if canonical_count > 0 && partial_count > 0 {
        return Err(MeetingError::Malformed(
            "interrupted audio mixes final and partial names",
        ));
    }
    if canonical_count == 1 {
        return Err(MeetingError::Malformed(
            "final audio is not a complete promoted pair",
        ));
    }
    Ok(())
}

fn validate_digest_named_ref(
    reference: &ArtifactRef,
    directory: &str,
    extension: &str,
) -> Result<(), MeetingError> {
    validate_sha256(&reference.sha256)?;
    let expected = format!("{directory}/{}.{}", reference.sha256, extension);
    validate_ref(reference, &expected)
}

fn validate_sha256(value: &str) -> Result<(), MeetingError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MeetingError::Malformed("invalid sha256"));
    }
    Ok(())
}

pub(crate) fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn reference(path: impl Into<String>, character: char) -> ArtifactRef {
        ArtifactRef {
            relative_path: path.into(),
            sha256: digest(character),
        }
    }

    fn captured() -> MeetingRecord {
        let rule = AudioRetentionRule::DeleteAfter { seconds: 30 };
        MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: "meeting-a".into(),
            lifecycle: MeetingLifecycle::Captured,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: Some(40),
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: reference("attempt.json", 'a'),
                ownership: Some(reference("ownership.json", 'b')),
                capture_session: Some(reference("capture/session.json", 'c')),
                microphone_audio: Some(reference("capture/mic.wav", 'd')),
                system_audio: Some(reference("capture/system.wav", 'e')),
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        }
    }

    #[test]
    fn captured_shape_is_closed_and_valid() {
        let meeting = captured();
        meeting.validate("meeting-a").unwrap();
        let mut value = serde_json::to_value(meeting).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MeetingRecord>(value).is_err());

        let mut value = serde_json::to_value(captured()).unwrap();
        value["retention"]["rule"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MeetingRecord>(value).is_err());
    }

    #[test]
    fn ready_requires_a_note_bound_to_the_current_transcript() {
        let mut meeting = captured();
        meeting.lifecycle = MeetingLifecycle::Ready;
        let transcript_digest = digest('f');
        meeting.artifacts.current_transcript = Some(reference(
            format!("transcript/{transcript_digest}.json"),
            'f',
        ));
        assert!(meeting.validate("meeting-a").is_err());

        let note_digest = digest('1');
        let markdown_digest = digest('2');
        meeting.artifacts.current_note = Some(NoteRevisionRef {
            json: reference(format!("notes/{note_digest}.json"), '1'),
            markdown: reference(format!("notes/{markdown_digest}.md"), '2'),
            source_transcript_sha256: transcript_digest,
        });
        meeting.validate("meeting-a").unwrap();

        meeting
            .artifacts
            .current_note
            .as_mut()
            .unwrap()
            .markdown
            .relative_path = format!("notes/{note_digest}.md");
        assert!(meeting.validate("meeting-a").is_err());
    }

    #[test]
    fn audio_release_does_not_erase_ready_content_state() {
        let mut meeting = captured();
        meeting.retention.state = AudioState::Released;
        meeting.retention.deletion_receipt = Some(reference("deletion/audio-deletion.json", '9'));
        meeting.validate("meeting-a").unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::Captured);
    }

    #[test]
    fn unsafe_or_non_digest_named_artifacts_are_refused() {
        let mut meeting = captured();
        meeting.artifacts.capture_session = Some(reference("../session.json", 'c'));
        assert!(meeting.validate("meeting-a").is_err());

        let mut meeting = captured();
        meeting.artifacts.current_transcript = Some(reference("transcript/current.json", 'f'));
        meeting.lifecycle = MeetingLifecycle::TranscriptReady;
        assert!(meeting.validate("meeting-a").is_err());
    }

    #[test]
    fn capture_session_v2_accepts_quality_and_microphone_without_relaxing_receipt_checks() {
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        fn private_file(path: &Path, bytes: &[u8]) {
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let temporary = TempDir::new().unwrap();
        let meeting_dir = temporary.path().join("meeting-a");
        fs::create_dir(&meeting_dir).unwrap();
        fs::set_permissions(&meeting_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let capture_dir = meeting_dir.join("capture");
        fs::create_dir(&capture_dir).unwrap();
        fs::set_permissions(&capture_dir, fs::Permissions::from_mode(0o700)).unwrap();

        private_file(&meeting_dir.join("attempt.json"), b"attempt");
        private_file(&meeting_dir.join("ownership.json"), b"ownership");
        private_file(&capture_dir.join("mic.wav"), &[0_u8; 64]);
        private_file(&capture_dir.join("system.wav"), &[1_u8; 64]);

        let mut meeting = captured();
        meeting.artifacts.attempt = artifact_ref(&meeting_dir, "attempt.json").unwrap();
        meeting.artifacts.ownership = Some(artifact_ref(&meeting_dir, "ownership.json").unwrap());
        meeting.artifacts.microphone_audio =
            Some(artifact_ref(&meeting_dir, MICROPHONE_AUDIO_PATH).unwrap());
        meeting.artifacts.system_audio =
            Some(artifact_ref(&meeting_dir, SYSTEM_AUDIO_PATH).unwrap());

        let receipt = serde_json::json!({
            "schema": "capture-session/2",
            "status": "complete",
            "started_at": "2000-01-01T00:00:00+0000",
            "finalized_at": "2000-01-01T00:00:01+0000",
            "health": {"schema": "capture-health/1", "usable": true},
            "quality": {
                "schema": "capture-quality/1",
                "observations": {"silence": {"status": "unknown"}}
            },
            "microphone": {
                "schema": "capture-microphone/1",
                "index": 4,
                "name": "Fixture microphone"
            },
            "pauses": {
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 16_000, "resumed_at_samples": 48_000}]
            },
            "reconciliation": {"legs": {}},
            "artifacts": [
                {
                    "name": "mic.wav",
                    "bytes": 64,
                    "sha256": meeting.artifacts.microphone_audio.as_ref().unwrap().sha256,
                    "mode": "0600"
                },
                {
                    "name": "system.wav",
                    "bytes": 64,
                    "sha256": meeting.artifacts.system_audio.as_ref().unwrap().sha256,
                    "mode": "0600"
                }
            ]
        });
        private_file(
            &capture_dir.join("session.json"),
            serde_json::to_string_pretty(&receipt).unwrap().as_bytes(),
        );
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());

        verify_recovered_capture_receipt(&meeting_dir, &meeting).unwrap();

        // Omitting the new siblings remains valid for legacy capture-session/2.
        let mut legacy = receipt.clone();
        legacy.as_object_mut().unwrap().remove("quality");
        legacy.as_object_mut().unwrap().remove("microphone");
        legacy.as_object_mut().unwrap().remove("pauses");
        private_file(
            &capture_dir.join("session.json"),
            serde_json::to_string_pretty(&legacy).unwrap().as_bytes(),
        );
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        verify_recovered_capture_receipt(&meeting_dir, &meeting).unwrap();

        // The closed receipt still refuses an unrelated sibling.
        let mut unknown = legacy;
        unknown["unexpected"] = serde_json::json!(true);
        private_file(
            &capture_dir.join("session.json"),
            serde_json::to_string_pretty(&unknown).unwrap().as_bytes(),
        );
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        assert!(verify_recovered_capture_receipt(&meeting_dir, &meeting).is_err());

        // Receipt parsing never substitutes for the existing byte/hash checks.
        private_file(
            &capture_dir.join("session.json"),
            serde_json::to_string_pretty(&receipt).unwrap().as_bytes(),
        );
        private_file(&capture_dir.join("mic.wav"), &[9_u8; 64]);
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        assert!(verify_recovered_capture_receipt(&meeting_dir, &meeting).is_err());
    }

    #[test]
    fn partial_audio_names_are_only_valid_for_one_interrupted_family() {
        let mut meeting = captured();
        meeting.lifecycle = MeetingLifecycle::RecoveredInterrupted;
        meeting.artifacts.microphone_audio = Some(reference("capture/.mic.wav.partial", 'd'));
        meeting.artifacts.system_audio = Some(reference("capture/.system.wav.partial", 'e'));
        meeting.validate("meeting-a").unwrap();

        meeting.artifacts.system_audio = Some(reference("capture/system.wav", 'e'));
        assert!(meeting.validate("meeting-a").is_err());

        meeting.artifacts.microphone_audio = Some(reference("capture/mic.wav", 'd'));
        meeting.artifacts.system_audio = None;
        assert!(meeting.validate("meeting-a").is_err());

        meeting.lifecycle = MeetingLifecycle::Captured;
        meeting.artifacts.microphone_audio = Some(reference("capture/.mic.wav.partial", 'd'));
        meeting.artifacts.system_audio = Some(reference("capture/.system.wav.partial", 'e'));
        assert!(meeting.validate("meeting-a").is_err());
    }
}
