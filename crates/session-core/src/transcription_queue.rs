//! Durable, source-bound transcription work.
//!
//! A queue item is a directory of immutable receipts.  There is no mutable
//! state file: request, claim, result, terminal, and commit receipts are
//! created once and recovery derives the state from their presence.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::meeting::{
    load_meeting, read_private_bytes, require_private_directory, resolve_artifact, valid_opaque_id,
    verify_artifact_ref, verify_record_artifacts, write_meeting, ArtifactRef, AudioState,
    MeetingError, MeetingLifecycle,
};
use crate::storage::{create_private_dir, durable_create_new, StorageError, StorageRoot};

const QUEUE_DIRECTORY: &str = "transcription-queue";
const REQUEST_FILE: &str = "request.json";
const CLAIMS_DIRECTORY: &str = "claims";
const CLAIM_FILE: &str = "claim.json";
const CLAIM_RELEASE_FILE: &str = "claim-release.json";
const RESULT_FILE: &str = "result.json";
const TERMINAL_FILE: &str = "terminal.json";
const COMMIT_FILE: &str = "commit.json";
const MAX_QUEUE_RECEIPT_BYTES: u64 = 256 * 1024;
const MAX_TRANSCRIPT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionRequest {
    pub schema: TranscriptionRequestSchema,
    pub request_id: Uuid,
    pub meeting_id: String,
    pub capture_session_sha256: String,
    pub microphone_audio_sha256: String,
    pub system_audio_sha256: String,
    pub model_identity: String,
    /// New requests bind the engine inputs here.  `None` is reserved for
    /// legacy receipts, whose `model_identity` remains their frozen Whisper
    /// producer identity.
    #[serde(default)]
    pub producer: Option<TranscriptionProducer>,
    pub worker_runtime_identity: String,
    pub enqueued_at_epoch_seconds: u64,
}

/// The closed producer identity recorded before a capture enters the queue.
/// It prevents a later Settings choice from reinterpreting queued audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "engine", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TranscriptionProducer {
    Whisper { model_identity: String },
    AppleNative {
        locale: String,
        helper_sha256: String,
        asset_identity: String,
        os_version: String,
    },
}

impl TranscriptionProducer {
    pub fn whisper(model_identity: String) -> Self {
        Self::Whisper { model_identity }
    }

    pub fn validates(&self) -> bool {
        match self {
            Self::Whisper { model_identity } => valid_identity(model_identity, 256),
            Self::AppleNative { locale, helper_sha256, asset_identity, os_version } => {
                valid_identity(locale, 64)
                    && helper_sha256.len() == 64
                    && helper_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && asset_identity == "os-managed"
                    && !os_version.is_empty() && os_version.len() <= 1024
            }
        }
    }
}

fn valid_identity(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.@".contains(&byte))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionRequestSchema {
    #[serde(rename = "transcription-request/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionClaim {
    pub schema: TranscriptionClaimSchema,
    pub request_id: Uuid,
    pub claim_id: Uuid,
    pub worker_runtime_identity: String,
    pub claimed_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionClaimRelease {
    pub schema: TranscriptionClaimReleaseSchema,
    pub request_id: Uuid,
    pub claim_id: Uuid,
    pub released_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionClaimReleaseSchema {
    #[serde(rename = "transcription-claim-release/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionClaimSchema {
    #[serde(rename = "transcription-claim/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionResult {
    pub schema: TranscriptionResultSchema,
    pub request_id: Uuid,
    pub meeting_id: String,
    pub transcript: ArtifactRef,
    pub result_sha256: String,
    pub produced_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionResultSchema {
    #[serde(rename = "transcription-result/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionTerminal {
    pub schema: TranscriptionTerminalSchema,
    pub request_id: Uuid,
    pub kind: TranscriptionTerminalKind,
    pub at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionTerminalSchema {
    #[serde(rename = "transcription-terminal/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptionTerminalKind {
    Failed,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionCommit {
    pub schema: TranscriptionCommitSchema,
    pub request_id: Uuid,
    pub meeting_id: String,
    pub result_sha256: String,
    pub committed_meeting_sha256: String,
    pub committed_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionCommitSchema {
    #[serde(rename = "transcription-commit/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionQueueItem {
    pub request: TranscriptionRequest,
    pub claim: Option<TranscriptionClaim>,
    pub result: Option<TranscriptionResult>,
    pub terminal: Option<TranscriptionTerminal>,
    pub commit: Option<TranscriptionCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDiscovery {
    pub items: Vec<TranscriptionQueueItem>,
    pub orphan_captured_meetings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum TranscriptionQueueError {
    #[error("transcription queue storage is invalid")]
    InvalidPrivateStorage,
    #[error("transcription queue receipt is malformed: {0}")]
    Malformed(&'static str),
    #[error("transcription queue request already exists")]
    AlreadyExists,
    #[error("transcription queue request conflicts with existing bytes")]
    ConflictingRequest,
    #[error("transcription queue item is missing or changed")]
    SourceChanged,
    #[error("transcription queue has another active claim")]
    AlreadyClaimed,
    #[error("transcription queue result is missing")]
    MissingResult,
    #[error("transcription queue item is terminal")]
    Terminal,
    #[error("transcription queue item is already committed")]
    AlreadyCommitted,
    #[error("transcription queue meeting is not eligible")]
    IneligibleMeeting,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub struct TranscriptionQueue<'a> {
    storage: &'a StorageRoot,
}

impl<'a> TranscriptionQueue<'a> {
    pub fn open(storage: &'a StorageRoot) -> Result<Self, TranscriptionQueueError> {
        Ok(Self { storage })
    }

    /// Enqueue only after the captured meeting and all three source artifacts
    /// have been reverified. Repeating the exact request is idempotent.
    pub fn enqueue(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        validate_request(&request)?;
        let meeting_dir = self.meeting_dir(&request.meeting_id)?;
        let meeting = self.load_source(&meeting_dir, &request)?;
        let dir = self.item_dir(&meeting_dir, request.request_id)?;
        if path_exists(&dir)? {
            let existing = self.load_item(&dir, request.request_id)?;
            if existing.request == request {
                return Ok(existing);
            }
            return Err(TranscriptionQueueError::ConflictingRequest);
        }
        create_private_dir(&dir)?;
        if let Err(error) = durable_create_new(&dir.join(REQUEST_FILE), &canonical(&request)?) {
            let _ = fs::remove_dir(&dir);
            return Err(error.into());
        }
        verify_source(&meeting_dir, &meeting, &request)?;
        self.load_item(&dir, request.request_id)
    }

    /// Discover all valid queue receipts and captures which were committed
    /// before the request receipt. Orphans are surfaced, never auto-claimed.
    pub fn discover(&self) -> Result<QueueDiscovery, TranscriptionQueueError> {
        let meetings = self.storage.resolve(Path::new("meetings"))?;
        require_private_directory(&meetings)?;
        let mut items = Vec::new();
        let mut orphans = Vec::new();
        let mut meeting_entries = fs::read_dir(meetings)?.collect::<Result<Vec<_>, _>>()?;
        meeting_entries.sort_by_key(|entry| entry.file_name());
        for entry in meeting_entries {
            if !entry.file_type()?.is_dir() {
                return Err(TranscriptionQueueError::InvalidPrivateStorage);
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let meeting_dir = entry.path();
            let meeting = match load_meeting(&meeting_dir) {
                Ok(meeting) => meeting,
                Err(_) => continue,
            };
            let queue_dir = meeting_dir.join(QUEUE_DIRECTORY);
            if !path_exists(&queue_dir)? {
                if meeting.lifecycle == MeetingLifecycle::Captured {
                    orphans.push(id);
                }
                continue;
            }
            require_private_directory(&queue_dir)?;
            let mut queue_entries = fs::read_dir(queue_dir)?.collect::<Result<Vec<_>, _>>()?;
            queue_entries.sort_by_key(|entry| entry.file_name());
            for item in queue_entries {
                if !item.file_type()?.is_dir() {
                    return Err(TranscriptionQueueError::InvalidPrivateStorage);
                }
                let request_id = Uuid::parse_str(&item.file_name().to_string_lossy())
                    .map_err(|_| TranscriptionQueueError::Malformed("request directory id"))?;
                let queue_item = self.load_item(&item.path(), request_id)?;
                verify_source(&meeting_dir, &meeting, &queue_item.request)?;
                if let Some(result) = &queue_item.result {
                    verify_artifact_ref(&meeting_dir, &result.transcript)?;
                }
                items.push(queue_item);
            }
        }
        Ok(QueueDiscovery {
            items,
            orphan_captured_meetings: orphans,
        })
    }

    /// Claim one request. Claims are immutable and are valid only while the
    /// source still matches. A fresh process should first inspect `discover`.
    pub fn claim(
        &self,
        request_id: Uuid,
        worker_runtime_identity: String,
        claimed_at_epoch_seconds: u64,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        if worker_runtime_identity.is_empty() || worker_runtime_identity.len() > 256 {
            return Err(TranscriptionQueueError::Malformed("worker identity"));
        }
        if self.discover()?.items.iter().any(|item| {
            item.request.request_id != request_id
                && item.claim.is_some()
                && item.commit.is_none()
                && item.terminal.is_none()
        }) {
            return Err(TranscriptionQueueError::AlreadyClaimed);
        }
        let (dir, meeting_dir) = self.locate(request_id)?;
        let item = self.load_item(&dir, request_id)?;
        let meeting = self.load_source(&meeting_dir, &item.request)?;
        verify_source(&meeting_dir, &meeting, &item.request)?;
        if item.terminal.is_some() || item.commit.is_some() {
            return Err(TranscriptionQueueError::Terminal);
        }
        if item.claim.is_some() {
            return Err(TranscriptionQueueError::AlreadyClaimed);
        }
        let claim = TranscriptionClaim {
            schema: TranscriptionClaimSchema::V1,
            request_id,
            claim_id: Uuid::new_v4(),
            worker_runtime_identity,
            claimed_at_epoch_seconds,
        };
        let claims_dir = dir.join(CLAIMS_DIRECTORY);
        create_private_dir(&claims_dir)?;
        let claim_dir = claims_dir.join(claim.claim_id.to_string());
        create_private_dir(&claim_dir)?;
        durable_create_new(&claim_dir.join(CLAIM_FILE), &canonical(&claim)?)?;
        self.load_item(&dir, request_id)
    }

    /// Recover a worker crash by appending an immutable release receipt.
    ///
    /// For an item still in flight, the source is checked again so stale or
    /// changed audio cannot be reclaimed. For an item whose transcription has
    /// already settled — a committed result or a terminal — the claim is stale
    /// by definition: the worker that held it is gone and the audio it once
    /// pointed at may legitimately no longer be a `Captured` source (a commit
    /// advances the meeting to `TranscriptReady`). Releasing such a claim must
    /// not depend on that source gate, or recovery loops forever refusing to
    /// clear a leftover claim on a finished item — the exact wedge that starved
    /// the whole queue and left a quit-mid-finalize meeting stuck at `captured`.
    pub fn release_claim(
        &self,
        request_id: Uuid,
        claim_id: Uuid,
        released_at_epoch_seconds: u64,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        let (dir, meeting_dir) = self.locate(request_id)?;
        let item = self.load_item(&dir, request_id)?;
        if item.commit.is_none() && item.terminal.is_none() {
            let meeting = self.load_source(&meeting_dir, &item.request)?;
            verify_source(&meeting_dir, &meeting, &item.request)?;
        }
        let claim = item
            .claim
            .as_ref()
            .ok_or(TranscriptionQueueError::Malformed("claim missing"))?;
        if claim.claim_id != claim_id {
            return Err(TranscriptionQueueError::Malformed("claim identifier"));
        }
        let release = TranscriptionClaimRelease {
            schema: TranscriptionClaimReleaseSchema::V1,
            request_id,
            claim_id,
            released_at_epoch_seconds,
        };
        let claim_dir = dir.join(CLAIMS_DIRECTORY).join(claim.claim_id.to_string());
        if !path_exists(&claim_dir.join(CLAIM_RELEASE_FILE))? {
            durable_create_new(&claim_dir.join(CLAIM_RELEASE_FILE), &canonical(&release)?)?;
        }
        self.load_item(&dir, request_id)
    }

    pub fn admit_result(
        &self,
        request_id: Uuid,
        transcript_bytes: &[u8],
        transcript: ArtifactRef,
        produced_at_epoch_seconds: u64,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        if transcript_bytes.is_empty() || transcript_bytes.len() as u64 > MAX_TRANSCRIPT_BYTES {
            return Err(TranscriptionQueueError::Malformed("transcript size"));
        }
        let (dir, meeting_dir) = self.locate(request_id)?;
        let item = self.load_item(&dir, request_id)?;
        let meeting = self.load_source(&meeting_dir, &item.request)?;
        verify_source(&meeting_dir, &meeting, &item.request)?;
        if item.terminal.is_some() || item.commit.is_some() {
            return Err(TranscriptionQueueError::Terminal);
        }
        let digest = digest_bytes(transcript_bytes);
        if transcript.sha256 != digest {
            return Err(TranscriptionQueueError::SourceChanged);
        }
        verify_transcript_ref(&transcript)?;
        create_private_dir(&meeting_dir.join("transcript"))?;
        let path = resolve_artifact(&meeting_dir, &transcript.relative_path)?;
        if path_exists(&path)? {
            if read_private_bytes(&path, MAX_TRANSCRIPT_BYTES)? != transcript_bytes {
                return Err(TranscriptionQueueError::SourceChanged);
            }
        } else {
            let parent = path
                .parent()
                .ok_or(TranscriptionQueueError::InvalidPrivateStorage)?;
            create_private_dir(parent)?;
            durable_create_new(&path, transcript_bytes)?;
        }
        verify_artifact_ref(&meeting_dir, &transcript)?;
        let result = TranscriptionResult {
            schema: TranscriptionResultSchema::V1,
            request_id,
            meeting_id: item.request.meeting_id.clone(),
            transcript,
            result_sha256: digest_bytes(transcript_bytes),
            produced_at_epoch_seconds,
        };
        durable_create_new(&dir.join(RESULT_FILE), &canonical(&result)?)?;
        self.load_item(&dir, request_id)
    }

    /// Atomically advances the meeting pointer after re-verifying source and
    /// transcript. Repeating the exact commit is harmless.
    pub fn commit(
        &self,
        request_id: Uuid,
        committed_at_epoch_seconds: u64,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        let (dir, meeting_dir) = self.locate(request_id)?;
        let item = self.load_item(&dir, request_id)?;
        let result = item
            .result
            .as_ref()
            .ok_or(TranscriptionQueueError::MissingResult)?;
        let meeting = load_meeting(&meeting_dir)?;
        verify_source(&meeting_dir, &meeting, &item.request)?;
        verify_artifact_ref(&meeting_dir, &result.transcript)?;
        if let Some(commit) = &item.commit {
            if commit.result_sha256 == result.result_sha256 {
                return Ok(item);
            }
            return Err(TranscriptionQueueError::ConflictingRequest);
        }
        if item.terminal.is_some() {
            return Err(TranscriptionQueueError::Terminal);
        }
        if meeting.lifecycle == MeetingLifecycle::TranscriptReady
            && meeting.artifacts.current_transcript.as_ref() == Some(&result.transcript)
        {
            // The meeting pointer was durably written before a process crash;
            // only the queue commit receipt remains to be repaired.
        } else {
            if meeting.lifecycle != MeetingLifecycle::Captured {
                return Err(TranscriptionQueueError::IneligibleMeeting);
            }
            let mut updated = meeting.clone();
            updated.lifecycle = MeetingLifecycle::TranscriptReady;
            updated.artifacts.current_transcript = Some(result.transcript.clone());
            updated.retention.state = AudioState::Retained;
            write_meeting(&meeting_dir, &updated)?;
        }
        let meeting_bytes = fs::read(meeting_dir.join("meeting.json"))?;
        let commit = TranscriptionCommit {
            schema: TranscriptionCommitSchema::V1,
            request_id,
            meeting_id: item.request.meeting_id.clone(),
            result_sha256: result.result_sha256.clone(),
            committed_meeting_sha256: digest_bytes(&meeting_bytes),
            committed_at_epoch_seconds,
        };
        durable_create_new(&dir.join(COMMIT_FILE), &canonical(&commit)?)?;
        self.load_item(&dir, request_id)
    }

    pub fn fail(
        &self,
        request_id: Uuid,
        kind: TranscriptionTerminalKind,
        at_epoch_seconds: u64,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        let (dir, meeting_dir) = self.locate(request_id)?;
        let item = self.load_item(&dir, request_id)?;
        let meeting = self.load_source(&meeting_dir, &item.request)?;
        verify_source(&meeting_dir, &meeting, &item.request)?;
        if item.commit.is_some() {
            return Err(TranscriptionQueueError::MissingResult);
        }
        if item.result.is_some() && kind != TranscriptionTerminalKind::Quarantined {
            return Err(TranscriptionQueueError::Malformed(
                "ordinary failure cannot follow an admitted result",
            ));
        }
        if let Some(existing) = &item.terminal {
            if existing.kind == kind {
                return Ok(item);
            }
            return Err(TranscriptionQueueError::ConflictingRequest);
        }
        let terminal = TranscriptionTerminal {
            schema: TranscriptionTerminalSchema::V1,
            request_id,
            kind,
            at_epoch_seconds,
        };
        durable_create_new(&dir.join(TERMINAL_FILE), &canonical(&terminal)?)?;
        self.load_item(&dir, request_id)
    }

    fn meeting_dir(&self, id: &str) -> Result<PathBuf, TranscriptionQueueError> {
        if !valid_opaque_id(id) {
            return Err(TranscriptionQueueError::Malformed("meeting identifier"));
        }
        let path = self
            .storage
            .resolve(Path::new("meetings").join(id).as_path())?;
        require_private_directory(&path)?;
        Ok(path)
    }

    fn item_dir(&self, meeting_dir: &Path, id: Uuid) -> Result<PathBuf, TranscriptionQueueError> {
        let queue = meeting_dir.join(QUEUE_DIRECTORY);
        create_private_dir(&queue)?;
        Ok(queue.join(id.to_string()))
    }

    fn locate(&self, id: Uuid) -> Result<(PathBuf, PathBuf), TranscriptionQueueError> {
        let meetings = self.storage.resolve(Path::new("meetings"))?;
        for entry in fs::read_dir(meetings)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path().join(QUEUE_DIRECTORY).join(id.to_string());
            if path_exists(&dir)? {
                return Ok((dir, entry.path()));
            }
        }
        Err(TranscriptionQueueError::Malformed("request not found"))
    }

    fn load_source(
        &self,
        meeting_dir: &Path,
        request: &TranscriptionRequest,
    ) -> Result<crate::meeting::MeetingRecord, TranscriptionQueueError> {
        let meeting = load_meeting(meeting_dir)?;
        if meeting.lifecycle != MeetingLifecycle::Captured
            || meeting.retention.state != AudioState::Retained
        {
            return Err(TranscriptionQueueError::IneligibleMeeting);
        }
        verify_source(meeting_dir, &meeting, request)?;
        Ok(meeting)
    }

    fn load_item(
        &self,
        dir: &Path,
        request_id: Uuid,
    ) -> Result<TranscriptionQueueItem, TranscriptionQueueError> {
        require_private_directory(dir)?;
        let request: TranscriptionRequest = read_json(&dir.join(REQUEST_FILE))?;
        if request.request_id != request_id {
            return Err(TranscriptionQueueError::Malformed("request identifier"));
        }
        let claim = load_live_claim(dir, request_id)?;
        let result = read_optional_json(&dir.join(RESULT_FILE))?;
        let terminal: Option<TranscriptionTerminal> = read_optional_json(&dir.join(TERMINAL_FILE))?;
        let commit = read_optional_json(&dir.join(COMMIT_FILE))?;
        if result.is_some()
            && terminal.as_ref().map(|value| value.kind)
                != Some(TranscriptionTerminalKind::Quarantined)
            && terminal.is_some()
        {
            return Err(TranscriptionQueueError::Malformed(
                "result and ordinary terminal both present",
            ));
        }
        if commit.is_some() && result.is_none() {
            return Err(TranscriptionQueueError::Malformed("commit without result"));
        }
        if commit.is_some() && terminal.is_some() {
            return Err(TranscriptionQueueError::Malformed(
                "commit and terminal both present",
            ));
        }
        Ok(TranscriptionQueueItem {
            request,
            claim,
            result,
            terminal,
            commit,
        })
    }
}

fn verify_source(
    meeting_dir: &Path,
    meeting: &crate::meeting::MeetingRecord,
    request: &TranscriptionRequest,
) -> Result<(), TranscriptionQueueError> {
    verify_record_artifacts(meeting_dir, meeting)?;
    let source = [
        meeting
            .artifacts
            .capture_session
            .as_ref()
            .map(|r| (&r.sha256, &request.capture_session_sha256)),
        meeting
            .artifacts
            .microphone_audio
            .as_ref()
            .map(|r| (&r.sha256, &request.microphone_audio_sha256)),
        meeting
            .artifacts
            .system_audio
            .as_ref()
            .map(|r| (&r.sha256, &request.system_audio_sha256)),
    ];
    if source
        .into_iter()
        .any(|pair| pair.is_none_or(|(actual, expected)| actual != expected))
    {
        return Err(TranscriptionQueueError::SourceChanged);
    }
    Ok(())
}

fn load_live_claim(
    dir: &Path,
    request_id: Uuid,
) -> Result<Option<TranscriptionClaim>, TranscriptionQueueError> {
    let claims_dir = dir.join(CLAIMS_DIRECTORY);
    if !path_exists(&claims_dir)? {
        return Ok(None);
    }
    require_private_directory(&claims_dir)?;
    let mut live = None;
    for entry in fs::read_dir(claims_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            return Err(TranscriptionQueueError::InvalidPrivateStorage);
        }
        let claim_dir = entry.path();
        let claim: TranscriptionClaim = read_json(&claim_dir.join(CLAIM_FILE))?;
        if claim.request_id != request_id {
            return Err(TranscriptionQueueError::Malformed(
                "claim request identifier",
            ));
        }
        let release: Option<TranscriptionClaimRelease> =
            read_optional_json(&claim_dir.join(CLAIM_RELEASE_FILE))?;
        if let Some(release) = release {
            if release.claim_id != claim.claim_id || release.request_id != request_id {
                return Err(TranscriptionQueueError::Malformed(
                    "claim release identifier",
                ));
            }
        } else if live.replace(claim).is_some() {
            return Err(TranscriptionQueueError::Malformed("multiple live claims"));
        }
    }
    Ok(live)
}

fn validate_request(request: &TranscriptionRequest) -> Result<(), TranscriptionQueueError> {
    if !valid_opaque_id(&request.meeting_id)
        || request.model_identity.is_empty()
        || request.worker_runtime_identity.is_empty()
    {
        return Err(TranscriptionQueueError::Malformed("request identity"));
    }
    if request.producer.as_ref().is_some_and(|producer| !producer.validates()) {
        return Err(TranscriptionQueueError::Malformed("transcription producer"));
    }
    for digest in [
        &request.capture_session_sha256,
        &request.microphone_audio_sha256,
        &request.system_audio_sha256,
    ] {
        if !is_sha256(digest) {
            return Err(TranscriptionQueueError::Malformed("source digest"));
        }
    }
    Ok(())
}

fn verify_transcript_ref(reference: &ArtifactRef) -> Result<(), TranscriptionQueueError> {
    if !reference.relative_path.starts_with("transcript/")
        || !reference.relative_path.ends_with(".json")
        || !is_sha256(&reference.sha256)
    {
        return Err(TranscriptionQueueError::Malformed(
            "transcript artifact reference",
        ));
    }
    if reference.relative_path.contains("..") {
        return Err(TranscriptionQueueError::InvalidPrivateStorage);
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(value)
}
fn path_exists(path: &Path) -> Result<bool, io::Error> {
    Ok(fs::symlink_metadata(path)
        .map(|m| !m.file_type().is_symlink())
        .unwrap_or(false))
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, TranscriptionQueueError> {
    Ok(serde_json::from_slice(&read_private_bytes(
        path,
        MAX_QUEUE_RECEIPT_BYTES,
    )?)?)
}
fn read_optional_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, TranscriptionQueueError> {
    match fs::symlink_metadata(path) {
        Ok(_) => read_json(path).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    use crate::meeting::{
        artifact_ref, retention_policy_sha256, AudioRetention, AudioRetentionRule,
        MeetingArtifacts, MeetingRecord, MeetingSchema,
    };
    use crate::storage::{create_private_dir, durable_replace};

    fn digest() -> String {
        "a".repeat(64)
    }

    fn request() -> TranscriptionRequest {
        TranscriptionRequest {
            schema: TranscriptionRequestSchema::V1,
            request_id: Uuid::new_v4(),
            meeting_id: "meeting-a".into(),
            capture_session_sha256: digest(),
            microphone_audio_sha256: digest(),
            system_audio_sha256: digest(),
            model_identity: "model/v1".into(),
            producer: None,
            worker_runtime_identity: "worker/v1".into(),
            enqueued_at_epoch_seconds: 1,
        }
    }

    fn fixture() -> (TempDir, StorageRoot, TranscriptionRequest) {
        let temp = TempDir::new().unwrap();
        let repository = temp.path().join("repo");
        fs::create_dir(&repository).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repository).unwrap();
        let meeting_dir = storage.path().join("meetings/meeting-a");
        create_private_dir(&meeting_dir).unwrap();
        create_private_dir(&meeting_dir.join("capture")).unwrap();
        for (path, bytes) in [
            ("attempt.json", b"attempt".as_slice()),
            ("ownership.json", b"ownership".as_slice()),
            ("capture/session.json", b"session".as_slice()),
            ("capture/mic.wav", b"mic".as_slice()),
            ("capture/system.wav", b"system".as_slice()),
        ] {
            durable_create_new(&meeting_dir.join(path), bytes).unwrap();
        }
        let rule = AudioRetentionRule::UntilManualDeletion;
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: "meeting-a".into(),
            lifecycle: MeetingLifecycle::Captured,
            retention: AudioRetention {
                rule: rule.clone(),
                policy_sha256: retention_policy_sha256(&rule),
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
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        };
        durable_create_new(
            &meeting_dir.join("meeting.json"),
            &serde_json::to_vec_pretty(&meeting).unwrap(),
        )
        .unwrap();
        let request = TranscriptionRequest {
            request_id: Uuid::new_v4(),
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
            ..request()
        };
        (temp, storage, request)
    }

    fn queue(storage: &StorageRoot) -> TranscriptionQueue<'_> {
        TranscriptionQueue::open(storage).unwrap()
    }

    #[test]
    fn request_requires_exact_source_digests() {
        assert!(validate_request(&request()).is_ok());
        let mut invalid = request();
        invalid.microphone_audio_sha256.pop();
        assert_eq!(
            validate_request(&invalid).unwrap_err().to_string(),
            "transcription queue receipt is malformed: source digest"
        );
    }

    #[test]
    fn transcript_reference_is_closed_to_json_transcript_directory() {
        let valid = ArtifactRef {
            relative_path: "transcript/abc.json".into(),
            sha256: digest(),
        };
        assert!(verify_transcript_ref(&valid).is_ok());
        for path in [
            "capture/abc.json",
            "transcript/../abc.json",
            "transcript/abc.txt",
        ] {
            let invalid = ArtifactRef {
                relative_path: path.into(),
                sha256: digest(),
            };
            assert!(verify_transcript_ref(&invalid).is_err());
        }
    }

    #[test]
    fn enqueue_is_source_bound_and_duplicate_is_idempotent() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        let first = queue.enqueue(request.clone()).unwrap();
        let second = queue.enqueue(request).unwrap();
        assert_eq!(first.request, second.request);
    }

    #[test]
    fn one_live_claim_can_be_released_and_reclaimed() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        let other = TranscriptionRequest {
            request_id: Uuid::new_v4(),
            ..request.clone()
        };
        queue.enqueue(other.clone()).unwrap();
        let claim = queue
            .claim(request.request_id, "worker/a".into(), 2)
            .unwrap();
        assert_eq!(
            queue
                .claim(other.request_id, "worker/b".into(), 2)
                .unwrap_err()
                .to_string(),
            "transcription queue has another active claim"
        );
        queue
            .release_claim(
                request.request_id,
                claim.claim.as_ref().unwrap().claim_id,
                3,
            )
            .unwrap();
        let reclaimed = queue.claim(other.request_id, "worker/b".into(), 4).unwrap();
        assert!(reclaimed.claim.is_some());
    }

    #[test]
    fn a_committed_item_with_a_lingering_claim_can_be_released() {
        // Reproduces the D-LOCK wedge: a worker committed a transcript but the
        // process died before its claim-release receipt was written, and the
        // commit advanced the meeting from Captured to TranscriptReady. Startup
        // recovery must be able to clear that stale claim; the old release path
        // demanded a still-Captured source and refused it with IneligibleMeeting,
        // which the recovery loop retried forever at 4 Hz and never got past to
        // transcribe any other meeting.
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        let claim = queue
            .claim(request.request_id, "worker/a".into(), 2)
            .unwrap();
        let bytes = br#"{"schema":"transcript/1","turns":[]}"#;
        let reference = ArtifactRef {
            relative_path: format!("transcript/{}.json", digest_bytes(bytes)),
            sha256: digest_bytes(bytes),
        };
        queue
            .admit_result(request.request_id, bytes, reference, 4)
            .unwrap();
        queue.commit(request.request_id, 5).unwrap();
        assert_eq!(
            load_meeting(&storage.path().join("meetings/meeting-a"))
                .unwrap()
                .lifecycle,
            MeetingLifecycle::TranscriptReady
        );

        let released = queue
            .release_claim(request.request_id, claim.claim.as_ref().unwrap().claim_id, 6)
            .unwrap();
        assert!(
            released.claim.is_none(),
            "the stale claim on a committed item is cleared, not left to loop"
        );
        assert!(released.commit.is_some());
    }

    #[test]
    fn changed_source_and_orphan_are_refused_or_discovered() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        durable_replace(
            &storage.path().join("meetings/meeting-a/capture/mic.wav"),
            b"changed",
        )
        .unwrap();
        assert!(matches!(
            queue.enqueue(request.clone()),
            Err(TranscriptionQueueError::SourceChanged)
                | Err(TranscriptionQueueError::Meeting(
                    MeetingError::ArtifactMismatch
                ))
        ));
        let discovery = queue.discover().unwrap();
        assert_eq!(discovery.orphan_captured_meetings, vec!["meeting-a"]);
    }

    #[test]
    fn result_admission_commit_and_pointer_before_commit_recovery_are_idempotent() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        let bytes = br#"{"schema":"transcript/1","turns":[]}"#;
        let reference = ArtifactRef {
            relative_path: format!("transcript/{}.json", digest_bytes(bytes)),
            sha256: digest_bytes(bytes),
        };
        queue
            .admit_result(request.request_id, bytes, reference, 4)
            .unwrap();
        queue.commit(request.request_id, 5).unwrap();
        let item_dir = storage.path().join(format!(
            "meetings/meeting-a/{QUEUE_DIRECTORY}/{}",
            request.request_id
        ));
        fs::remove_file(item_dir.join(COMMIT_FILE)).unwrap();
        queue.commit(request.request_id, 6).unwrap();
        assert!(item_dir.join(COMMIT_FILE).exists());
    }

    #[test]
    fn terminal_failure_rejects_later_result() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        queue
            .fail(
                request.request_id,
                TranscriptionTerminalKind::Quarantined,
                5,
            )
            .unwrap();
        let bytes = b"transcript";
        let digest = digest_bytes(bytes);
        let reference = ArtifactRef {
            relative_path: format!("transcript/{digest}.json"),
            sha256: digest,
        };
        assert!(matches!(
            queue.admit_result(request.request_id, bytes, reference, 6),
            Err(TranscriptionQueueError::Terminal)
        ));
    }

    #[test]
    fn admitted_result_can_be_quarantined_but_never_claimed_or_published() {
        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        let bytes = b"transcript";
        let digest = digest_bytes(bytes);
        let reference = ArtifactRef {
            relative_path: format!("transcript/{digest}.json"),
            sha256: digest,
        };
        queue
            .admit_result(request.request_id, bytes, reference, 6)
            .unwrap();
        let item = queue
            .fail(
                request.request_id,
                TranscriptionTerminalKind::Quarantined,
                7,
            )
            .unwrap();
        assert!(item.result.is_some());
        assert_eq!(
            item.terminal.unwrap().kind,
            TranscriptionTerminalKind::Quarantined
        );
        assert!(matches!(
            queue.claim(request.request_id, "worker".into(), 8),
            Err(TranscriptionQueueError::Terminal)
        ));
        assert!(matches!(
            queue.commit(request.request_id, 8),
            Err(TranscriptionQueueError::Terminal)
        ));
    }

    #[test]
    fn ordinary_failure_and_commit_plus_terminal_receipts_are_malformed() {
        {
            let (_temp, storage, request) = fixture();
            let queue = queue(&storage);
            queue.enqueue(request.clone()).unwrap();
            let bytes = b"transcript";
            let digest = digest_bytes(bytes);
            let reference = ArtifactRef {
                relative_path: format!("transcript/{digest}.json"),
                sha256: digest,
            };
            queue
                .admit_result(request.request_id, bytes, reference, 6)
                .unwrap();
            assert!(matches!(
                queue.fail(request.request_id, TranscriptionTerminalKind::Failed, 7),
                Err(TranscriptionQueueError::Malformed(_))
            ));
            let item_dir = storage.path().join(format!(
                "meetings/meeting-a/{QUEUE_DIRECTORY}/{}",
                request.request_id
            ));
            let terminal = TranscriptionTerminal {
                schema: TranscriptionTerminalSchema::V1,
                request_id: request.request_id,
                kind: TranscriptionTerminalKind::Failed,
                at_epoch_seconds: 7,
            };
            durable_create_new(
                &item_dir.join(TERMINAL_FILE),
                &canonical(&terminal).unwrap(),
            )
            .unwrap();
            assert!(matches!(
                queue.discover(),
                Err(TranscriptionQueueError::Malformed(_))
            ));
        }

        let (_temp, storage, request) = fixture();
        let queue = queue(&storage);
        queue.enqueue(request.clone()).unwrap();
        let bytes = b"transcript";
        let digest = digest_bytes(bytes);
        queue
            .admit_result(
                request.request_id,
                bytes,
                ArtifactRef {
                    relative_path: format!("transcript/{digest}.json"),
                    sha256: digest,
                },
                6,
            )
            .unwrap();
        queue.commit(request.request_id, 8).unwrap();
        let item_dir = storage.path().join(format!(
            "meetings/meeting-a/{QUEUE_DIRECTORY}/{}",
            request.request_id
        ));
        let terminal = TranscriptionTerminal {
            schema: TranscriptionTerminalSchema::V1,
            request_id: request.request_id,
            kind: TranscriptionTerminalKind::Quarantined,
            at_epoch_seconds: 9,
        };
        durable_create_new(
            &item_dir.join(TERMINAL_FILE),
            &canonical(&terminal).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            queue.discover(),
            Err(TranscriptionQueueError::Malformed(_))
        ));
    }
}
