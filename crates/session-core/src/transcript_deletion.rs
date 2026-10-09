//! Transcript deletion — the `transcript-deletion/1` audited removal.
//!
//! This removes source transcript revisions and every generated note derived
//! from them, while preserving the capture, any retained audio, the meeting
//! record, its library name, and the operator's own note. It is deliberately a
//! different state machine from `audio-deletion/1` and `meeting-deletion/1`:
//! each names a different set of data and leaves a different meeting behind.
//!
//! The record is detached from derived artifacts before their directories move.
//! A crash therefore leaves either an intact readable transcript, or a captured
//! meeting whose pending receipt is completed at startup — never a detail view
//! that can mistake partially deleted text for retained evidence.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::meeting::{
    MAX_RECEIPT_BYTES, MeetingError, MeetingLifecycle, MeetingRecord, PendingStorageOperation,
    hash_private_file, load_meeting, open_private_file, require_private_directory, valid_opaque_id,
    verify_record_artifacts, write_meeting,
};
use crate::meeting_coordination::{MeetingCoordinationError, MeetingStorageCoordination};
use crate::operation_store::{OperationStore, OperationStoreError, StoredOperationRequest};
use crate::storage::{
    StorageRoot, create_private_dir, durable_create_new, durable_replace, sync_directory,
};

const RECEIPT_RELATIVE_PATH: &str = "deletion/transcript-deletion.json";
const STAGING_DIRECTORY: &str = "transcript-deletion-staged";
const MAX_INVENTORY_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptDeletionReceipt {
    schema: TranscriptDeletionSchema,
    meeting_id: String,
    state: TranscriptDeletionState,
    prior_lifecycle: MeetingLifecycle,
    source_transcript_sha256: String,
    notes_directory: bool,
    artifacts: Vec<DeletedTranscriptArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TranscriptDeletionSchema {
    #[serde(rename = "transcript-deletion/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TranscriptDeletionState {
    /// Receipt written and the record may still point to its source material.
    Deleting,
    /// The record is detached and all source directories are staged privately.
    Staged,
    /// Source directories are gone and the meeting remains captured-only.
    Removed,
}

/// A removed derivative file, recorded by digest rather than content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedTranscriptArtifact {
    relative_name: String,
    byte_size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptDeletionOutcome {
    /// The target meeting is active, so nothing was inspected or changed.
    DeferredActive,
    /// Transcript revisions and their generated notes were removed.
    TranscriptRemoved,
    /// A prior incomplete transcript removal was recovered and completed.
    RecoveredRemoval,
    /// The transcript deletion receipt already describes a completed removal.
    AlreadyRemoved,
}

#[derive(Debug, Error)]
pub enum TranscriptDeletionError {
    #[error("meeting storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    OperationStore(#[from] OperationStoreError),
    #[error("meeting has a nonterminal product operation")]
    NonterminalProductOperation,
    #[error("meeting has no admitted transcript")]
    NoTranscript,
    #[error("no such meeting")]
    NoSuchMeeting,
    #[error("transcript deletion receipt is malformed")]
    MalformedReceipt,
    #[error("meeting storage contains an entry that is not a regular private file")]
    UnsafeEntry,
    #[error("meeting storage holds more entries than a transcript inventory may describe")]
    InventoryTooLarge,
}

/// The sole capability that may remove an admitted transcript while keeping a
/// meeting. It is constructed only by borrowing the held app-data writer lock.
pub struct TranscriptDeletionAuthority<'a> {
    pub(crate) storage: &'a StorageRoot,
    pub(crate) coordination: &'a MeetingStorageCoordination,
}

impl TranscriptDeletionAuthority<'_> {
    /// Removes one exact, non-active meeting's transcript and generated notes.
    /// The caller must separately obtain the operator's reviewed confirmation.
    pub fn delete_transcript(
        &self,
        meeting_id: &str,
    ) -> Result<TranscriptDeletionOutcome, TranscriptDeletionError> {
        delete_meeting_transcript(self.storage, self.coordination, meeting_id)
    }
}

fn meeting_directory(
    storage: &StorageRoot,
    meeting_id: &str,
) -> Result<PathBuf, TranscriptDeletionError> {
    storage
        .resolve(&Path::new("meetings").join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()).into())
}

fn receipt_path(meeting_dir: &Path) -> PathBuf {
    meeting_dir.join(RECEIPT_RELATIVE_PATH)
}

fn deletion_dir(meeting_dir: &Path) -> PathBuf {
    meeting_dir.join("deletion")
}

fn staging_dir(meeting_dir: &Path) -> PathBuf {
    deletion_dir(meeting_dir).join(STAGING_DIRECTORY)
}

fn load_receipt(path: &Path) -> Result<TranscriptDeletionReceipt, TranscriptDeletionError> {
    let bytes = crate::meeting::read_private_bytes(path, MAX_RECEIPT_BYTES)?;
    let receipt: TranscriptDeletionReceipt =
        serde_json::from_slice(&bytes).map_err(|_| TranscriptDeletionError::MalformedReceipt)?;
    validate_receipt_shape(&receipt)?;
    Ok(receipt)
}

fn write_receipt(
    path: &Path,
    receipt: &TranscriptDeletionReceipt,
    create: bool,
) -> Result<(), TranscriptDeletionError> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    if create {
        durable_create_new(path, &bytes)?;
    } else {
        durable_replace(path, &bytes)?;
    }
    Ok(())
}

fn private_directory_exists(path: &Path) -> Result<bool, TranscriptDeletionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            require_private_directory(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn private_file_exists(path: &Path) -> Result<bool, TranscriptDeletionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(TranscriptDeletionError::UnsafeEntry);
            }
            open_private_file(path).map_err(|_| TranscriptDeletionError::UnsafeEntry)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_receipt_shape(
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if !valid_opaque_id(&receipt.meeting_id)
        || !valid_sha256(&receipt.source_transcript_sha256)
        || !matches!(
            receipt.prior_lifecycle,
            MeetingLifecycle::TranscriptReady
                | MeetingLifecycle::SummaryFailed
                | MeetingLifecycle::Ready
        )
        || receipt.artifacts.is_empty()
    {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    let mut previous: Option<&str> = None;
    let mut source_is_listed = false;
    for artifact in &receipt.artifacts {
        if !valid_sha256(&artifact.sha256) {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
        if previous.is_some_and(|previous| previous >= artifact.relative_name.as_str()) {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
        previous = Some(&artifact.relative_name);
        let components: Vec<_> = Path::new(&artifact.relative_name).components().collect();
        let Some(Component::Normal(directory)) = components.first().copied() else {
            return Err(TranscriptDeletionError::MalformedReceipt);
        };
        if components.len() < 2
            || components
                .iter()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
        match directory.to_str() {
            Some("transcript") => {
                source_is_listed |= artifact.sha256 == receipt.source_transcript_sha256;
            }
            Some("notes") if receipt.notes_directory => {}
            _ => return Err(TranscriptDeletionError::MalformedReceipt),
        }
    }
    if !source_is_listed {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    Ok(())
}

fn append_inventory(
    root: &Path,
    prefix: &str,
    artifacts: &mut Vec<DeletedTranscriptArtifact>,
) -> Result<(), TranscriptDeletionError> {
    require_private_directory(root)?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        require_private_directory(&directory)?;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(TranscriptDeletionError::UnsafeEntry);
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                return Err(TranscriptDeletionError::UnsafeEntry);
            }
            if artifacts.len() >= MAX_INVENTORY_ENTRIES {
                return Err(TranscriptDeletionError::InventoryTooLarge);
            }
            // `open_private_file` rechecks mode and refuses symlinks at open
            // time, so a path cannot change type between inventory and digest.
            open_private_file(&path).map_err(|_| TranscriptDeletionError::UnsafeEntry)?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| TranscriptDeletionError::UnsafeEntry)?
                .to_str()
                .ok_or(TranscriptDeletionError::UnsafeEntry)?;
            artifacts.push(DeletedTranscriptArtifact {
                relative_name: format!("{prefix}/{relative}"),
                byte_size: metadata.len(),
                sha256: hash_private_file(&path)?,
            });
        }
    }
    Ok(())
}

fn take_live_inventory(
    meeting_dir: &Path,
) -> Result<(bool, Vec<DeletedTranscriptArtifact>), TranscriptDeletionError> {
    let transcript = meeting_dir.join("transcript");
    if !private_directory_exists(&transcript)? {
        return Err(TranscriptDeletionError::NoTranscript);
    }
    let notes = meeting_dir.join("notes");
    let notes_directory = private_directory_exists(&notes)?;
    let mut artifacts = Vec::new();
    append_inventory(&transcript, "transcript", &mut artifacts)?;
    if notes_directory {
        append_inventory(&notes, "notes", &mut artifacts)?;
    }
    artifacts.sort_by(|left, right| left.relative_name.cmp(&right.relative_name));
    Ok((notes_directory, artifacts))
}

fn inventory_contains(
    artifacts: &[DeletedTranscriptArtifact],
    relative_name: &str,
    sha256: &str,
) -> bool {
    artifacts
        .iter()
        .any(|artifact| artifact.relative_name == relative_name && artifact.sha256 == sha256)
}

fn validate_initial_binding(
    meeting: &MeetingRecord,
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if !matches!(
        receipt.prior_lifecycle,
        MeetingLifecycle::TranscriptReady
            | MeetingLifecycle::SummaryFailed
            | MeetingLifecycle::Ready
    ) || meeting.lifecycle != receipt.prior_lifecycle
    {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    let Some(transcript) = meeting.artifacts.current_transcript.as_ref() else {
        return Err(TranscriptDeletionError::MalformedReceipt);
    };
    if transcript.sha256 != receipt.source_transcript_sha256
        || !inventory_contains(
            &receipt.artifacts,
            &transcript.relative_path,
            &transcript.sha256,
        )
    {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    if let Some(note) = meeting.artifacts.current_note.as_ref() {
        if !inventory_contains(
            &receipt.artifacts,
            &note.json.relative_path,
            &note.json.sha256,
        ) || !inventory_contains(
            &receipt.artifacts,
            &note.markdown.relative_path,
            &note.markdown.sha256,
        ) {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
    }
    Ok(())
}

fn detach_record(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if meeting.pending_storage_operation == Some(PendingStorageOperation::TranscriptDeletionV1) {
        if meeting.lifecycle != MeetingLifecycle::Captured
            || meeting.artifacts.current_transcript.is_some()
            || meeting.artifacts.current_note.is_some()
        {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
        return Ok(());
    }
    if meeting.pending_storage_operation.is_some() {
        return Err(TranscriptDeletionError::NonterminalProductOperation);
    }
    validate_initial_binding(meeting, receipt)?;
    meeting.lifecycle = MeetingLifecycle::Captured;
    meeting.artifacts.current_transcript = None;
    meeting.artifacts.current_note = None;
    meeting.pending_storage_operation = Some(PendingStorageOperation::TranscriptDeletionV1);
    write_meeting(meeting_dir, meeting)?;
    Ok(())
}

fn stage_target(
    meeting_dir: &Path,
    stage_dir: &Path,
    name: &str,
    expected: bool,
) -> Result<(), TranscriptDeletionError> {
    let live = meeting_dir.join(name);
    let staged = stage_dir.join(name);
    let live_exists = private_directory_exists(&live)?;
    let staged_exists = private_directory_exists(&staged)?;
    match (live_exists, staged_exists, expected) {
        (true, true, _) => Err(TranscriptDeletionError::MalformedReceipt),
        (true, false, true) => {
            fs::rename(&live, &staged)?;
            sync_directory(meeting_dir)?;
            sync_directory(stage_dir)?;
            Ok(())
        }
        (false, true, true) => Ok(()),
        (false, false, true) => Err(TranscriptDeletionError::MalformedReceipt),
        (false, false, false) => Ok(()),
        (true, false, false) | (false, true, false) => {
            Err(TranscriptDeletionError::MalformedReceipt)
        }
    }
}

fn validate_staged_layout(
    meeting_dir: &Path,
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if private_directory_exists(&meeting_dir.join("transcript"))?
        || private_directory_exists(&meeting_dir.join("notes"))?
    {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    let stage = staging_dir(meeting_dir);
    if !private_directory_exists(&stage)? {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    let mut expected = vec!["transcript"];
    if receipt.notes_directory {
        expected.push("notes");
    }
    let mut seen = Vec::new();
    for entry in fs::read_dir(&stage)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or(TranscriptDeletionError::UnsafeEntry)?;
        if !expected.iter().any(|expected_name| *expected_name == name) {
            return Err(TranscriptDeletionError::UnsafeEntry);
        }
        require_private_directory(&entry.path())?;
        seen.push(name);
    }
    seen.sort();
    expected.sort();
    if seen != expected {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    let mut actual = Vec::new();
    append_inventory(&stage.join("transcript"), "transcript", &mut actual)?;
    if receipt.notes_directory {
        append_inventory(&stage.join("notes"), "notes", &mut actual)?;
    }
    actual.sort_by(|left, right| left.relative_name.cmp(&right.relative_name));
    if actual != receipt.artifacts {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    Ok(())
}

fn staged_material_is_gone(meeting_dir: &Path) -> Result<bool, TranscriptDeletionError> {
    if private_directory_exists(&meeting_dir.join("transcript"))?
        || private_directory_exists(&meeting_dir.join("notes"))?
    {
        return Ok(false);
    }
    Ok(!private_directory_exists(&staging_dir(meeting_dir))?)
}

fn remove_staged_material(
    meeting_dir: &Path,
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if staged_material_is_gone(meeting_dir)? {
        return Ok(());
    }
    validate_staged_layout(meeting_dir, receipt)?;
    let stage = staging_dir(meeting_dir);
    fs::remove_dir_all(&stage)?;
    sync_directory(&deletion_dir(meeting_dir))?;
    Ok(())
}

fn terminal_record_is_consistent(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
    receipt: &TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if receipt.state != TranscriptDeletionState::Removed
        || meeting.lifecycle != MeetingLifecycle::Captured
        || meeting.artifacts.current_transcript.is_some()
        || meeting.artifacts.current_note.is_some()
        || meeting.pending_storage_operation.is_some()
        || !staged_material_is_gone(meeting_dir)?
    {
        return Err(TranscriptDeletionError::MalformedReceipt);
    }
    Ok(())
}

fn finish_removal(
    meeting_dir: &Path,
    receipt_path: &Path,
    meeting: &mut MeetingRecord,
    mut receipt: TranscriptDeletionReceipt,
) -> Result<(), TranscriptDeletionError> {
    if receipt.state == TranscriptDeletionState::Deleting {
        detach_record(meeting_dir, meeting, &receipt)?;
        let stage = staging_dir(meeting_dir);
        create_private_dir(&stage)?;
        stage_target(meeting_dir, &stage, "transcript", true)?;
        stage_target(meeting_dir, &stage, "notes", receipt.notes_directory)?;
        validate_staged_layout(meeting_dir, &receipt)?;
        receipt.state = TranscriptDeletionState::Staged;
        write_receipt(receipt_path, &receipt, false)?;
    }

    if receipt.state == TranscriptDeletionState::Staged {
        remove_staged_material(meeting_dir, &receipt)?;
        receipt.state = TranscriptDeletionState::Removed;
        write_receipt(receipt_path, &receipt, false)?;
    }

    if receipt.state == TranscriptDeletionState::Removed {
        if meeting.pending_storage_operation == Some(PendingStorageOperation::TranscriptDeletionV1)
        {
            meeting.pending_storage_operation = None;
            write_meeting(meeting_dir, meeting)?;
        }
        terminal_record_is_consistent(meeting_dir, meeting, &receipt)?;
    }
    Ok(())
}

fn has_nonterminal_product_operation(
    storage: &StorageRoot,
    meeting_id: &str,
) -> Result<bool, TranscriptDeletionError> {
    let operations = OperationStore::open(storage)?;
    Ok(operations.scan()?.values().any(|receipt| {
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
    }))
}

pub(crate) fn delete_meeting_transcript(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    meeting_id: &str,
) -> Result<TranscriptDeletionOutcome, TranscriptDeletionError> {
    if !valid_opaque_id(meeting_id) {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }
    let _lease = match coordination.acquire(meeting_id) {
        Ok(lease) => lease,
        Err(MeetingCoordinationError::AlreadyActive) => {
            return Ok(TranscriptDeletionOutcome::DeferredActive);
        }
        Err(error) => return Err(error.into()),
    };
    let _sequence = coordination.lock_sequence()?;
    let meeting_dir = meeting_directory(storage, meeting_id)?;
    if !meeting_dir.exists() {
        return Err(TranscriptDeletionError::NoSuchMeeting);
    }
    require_private_directory(&meeting_dir)?;
    let mut meeting = load_meeting(&meeting_dir)?;
    if meeting.meeting_id != meeting_id {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }
    let receipt_path = receipt_path(&meeting_dir);
    if private_file_exists(&receipt_path)? {
        let receipt = load_receipt(&receipt_path)?;
        if receipt.meeting_id != meeting_id {
            return Err(TranscriptDeletionError::MalformedReceipt);
        }
        let already_removed = receipt.state == TranscriptDeletionState::Removed
            && meeting.pending_storage_operation.is_none();
        finish_removal(&meeting_dir, &receipt_path, &mut meeting, receipt)?;
        return Ok(if already_removed {
            TranscriptDeletionOutcome::AlreadyRemoved
        } else {
            TranscriptDeletionOutcome::RecoveredRemoval
        });
    }

    if meeting.pending_storage_operation.is_some()
        || has_nonterminal_product_operation(storage, meeting_id)?
    {
        return Err(TranscriptDeletionError::NonterminalProductOperation);
    }
    if !matches!(
        meeting.lifecycle,
        MeetingLifecycle::TranscriptReady
            | MeetingLifecycle::SummaryFailed
            | MeetingLifecycle::Ready
    ) || meeting.artifacts.current_transcript.is_none()
    {
        return Err(TranscriptDeletionError::NoTranscript);
    }
    verify_record_artifacts(&meeting_dir, &meeting)?;
    let (notes_directory, artifacts) = take_live_inventory(&meeting_dir)?;
    let source_transcript_sha256 = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .expect("checked above")
        .sha256
        .clone();
    let receipt = TranscriptDeletionReceipt {
        schema: TranscriptDeletionSchema::V1,
        meeting_id: meeting_id.into(),
        state: TranscriptDeletionState::Deleting,
        prior_lifecycle: meeting.lifecycle,
        source_transcript_sha256,
        notes_directory,
        artifacts,
    };
    create_private_dir(&deletion_dir(&meeting_dir))?;
    write_receipt(&receipt_path, &receipt, true)?;
    finish_removal(&meeting_dir, &receipt_path, &mut meeting, receipt)?;
    Ok(TranscriptDeletionOutcome::TranscriptRemoved)
}

/// Whether a meeting has a completed, internally consistent transcript-removal
/// receipt. This is intentionally a read: it creates no directories or files.
pub fn transcript_deletion_completed(
    storage: &StorageRoot,
    meeting_id: &str,
) -> Result<bool, TranscriptDeletionError> {
    if !valid_opaque_id(meeting_id) {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }
    let meeting_dir = meeting_directory(storage, meeting_id)?;
    if !meeting_dir.exists() {
        return Ok(false);
    }
    require_private_directory(&meeting_dir)?;
    let receipt_path = receipt_path(&meeting_dir);
    if !private_file_exists(&receipt_path)? {
        return Ok(false);
    }
    let receipt = load_receipt(&receipt_path)?;
    if receipt.meeting_id != meeting_id || receipt.state != TranscriptDeletionState::Removed {
        return Ok(false);
    }
    let meeting = load_meeting(&meeting_dir)?;
    terminal_record_is_consistent(&meeting_dir, &meeting, &receipt)?;
    Ok(true)
}

/// Completes interrupted transcript deletion before readers reconstruct a
/// library. The receipt stays next to the retained meeting, so this scans only
/// readable meeting records rather than a broad untrusted filesystem walk.
pub fn reconcile_pending_transcript_deletions(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
) -> Result<Vec<String>, TranscriptDeletionError> {
    let meetings = storage
        .resolve(Path::new("meetings"))
        .map_err(|error| io::Error::other(error.to_string()))?;
    require_private_directory(&meetings)?;
    let mut ids = Vec::new();
    for entry in fs::read_dir(&meetings)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(meeting_id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_opaque_id(&meeting_id) {
            continue;
        }
        let receipt = receipt_path(&entry.path());
        if !private_file_exists(&receipt)? {
            continue;
        }
        let loaded = load_meeting(&entry.path())?;
        if loaded.pending_storage_operation != Some(PendingStorageOperation::TranscriptDeletionV1)
            && !matches!(
                load_receipt(&receipt)?.state,
                TranscriptDeletionState::Deleting | TranscriptDeletionState::Staged
            )
        {
            continue;
        }
        match delete_meeting_transcript(storage, coordination, &meeting_id)? {
            TranscriptDeletionOutcome::TranscriptRemoved
            | TranscriptDeletionOutcome::RecoveredRemoval => ids.push(meeting_id),
            TranscriptDeletionOutcome::DeferredActive
            | TranscriptDeletionOutcome::AlreadyRemoved => {}
        }
    }
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::{
        AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts, MeetingSchema,
        NoteRevisionRef, artifact_ref, retention_policy_sha256,
    };
    use crate::storage::durable_create_new;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    fn storage() -> (TempDir, StorageRoot) {
        let temporary = TempDir::new().unwrap();
        let repository = temporary.path().join("repo");
        create_private_dir(&repository).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app"), &repository).unwrap();
        (temporary, storage)
    }

    fn fixture(storage: &StorageRoot, id: &str) -> PathBuf {
        let directory = meeting_directory(storage, id).unwrap();
        create_private_dir(&directory).unwrap();
        let transcript_bytes = b"we agreed to ship on friday".as_slice();
        let note_json_bytes = b"{\"claim\":\"ship friday\"}".as_slice();
        let note_markdown_bytes = b"# Ship Friday".as_slice();
        let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
        let transcript_path = format!("transcript/{}.json", digest(transcript_bytes));
        let note_json_path = format!("notes/{}.json", digest(note_json_bytes));
        let note_markdown_path = format!("notes/{}.md", digest(note_markdown_bytes));
        for (relative, bytes) in [
            ("attempt.json", b"attempt".as_slice()),
            ("ownership.json", b"ownership".as_slice()),
            ("capture/session.json", b"session".as_slice()),
            ("capture/mic.wav", b"mic".as_slice()),
            ("capture/system.wav", b"system".as_slice()),
            ("operator-note.json", b"{\"text\":\"ask legal\"}".as_slice()),
        ] {
            durable_create_new(&directory.join(relative), bytes).unwrap();
        }
        durable_create_new(&directory.join(&transcript_path), transcript_bytes).unwrap();
        durable_create_new(&directory.join(&note_json_path), note_json_bytes).unwrap();
        durable_create_new(&directory.join(&note_markdown_path), note_markdown_bytes).unwrap();
        let rule = AudioRetentionRule::UntilManualDeletion;
        let transcript = artifact_ref(&directory, &transcript_path).unwrap();
        let note = NoteRevisionRef {
            json: artifact_ref(&directory, &note_json_path).unwrap(),
            markdown: artifact_ref(&directory, &note_markdown_path).unwrap(),
            source_transcript_sha256: transcript.sha256.clone(),
        };
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::Ready,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: None,
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: Some(artifact_ref(&directory, "capture/mic.wav").unwrap()),
                system_audio: Some(artifact_ref(&directory, "capture/system.wav").unwrap()),
                current_transcript: Some(transcript),
                current_note: Some(note),
            },
            pending_storage_operation: None,
        };
        write_meeting(&directory, &meeting).unwrap();
        directory
    }

    #[test]
    fn transcript_removal_preserves_audio_and_the_operators_note() {
        let (_temporary, storage) = storage();
        let directory = fixture(&storage, "transcript-gone");
        let coordination = MeetingStorageCoordination::default();

        assert_eq!(
            delete_meeting_transcript(&storage, &coordination, "transcript-gone").unwrap(),
            TranscriptDeletionOutcome::TranscriptRemoved
        );
        assert!(!directory.join("transcript").exists());
        assert!(!directory.join("notes").exists());
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert!(directory.join("operator-note.json").exists());
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::Captured);
        assert!(meeting.artifacts.current_transcript.is_none());
        assert!(meeting.artifacts.current_note.is_none());
        assert!(meeting.pending_storage_operation.is_none());
        assert!(transcript_deletion_completed(&storage, "transcript-gone").unwrap());
        assert_eq!(
            delete_meeting_transcript(&storage, &coordination, "transcript-gone").unwrap(),
            TranscriptDeletionOutcome::AlreadyRemoved
        );

        let receipt = fs::read_to_string(receipt_path(&directory)).unwrap();
        assert!(!receipt.contains("we agreed to ship"));
        assert!(!receipt.contains("ship friday"));
        assert!(receipt.contains("transcript/"));
    }

    #[test]
    fn active_meeting_is_refused_without_creating_a_receipt() {
        let (_temporary, storage) = storage();
        let directory = fixture(&storage, "transcript-active");
        let coordination = MeetingStorageCoordination::default();
        let _lease = coordination.acquire("transcript-active").unwrap();

        assert_eq!(
            delete_meeting_transcript(&storage, &coordination, "transcript-active").unwrap(),
            TranscriptDeletionOutcome::DeferredActive
        );
        assert!(directory.join("transcript").exists());
        assert!(!receipt_path(&directory).exists());
    }

    #[test]
    fn a_torn_deleting_receipt_is_completed_on_the_next_attempt() {
        let (_temporary, storage) = storage();
        let directory = fixture(&storage, "transcript-torn");
        let coordination = MeetingStorageCoordination::default();
        let mut meeting = load_meeting(&directory).unwrap();
        let (notes_directory, artifacts) = take_live_inventory(&directory).unwrap();
        let receipt = TranscriptDeletionReceipt {
            schema: TranscriptDeletionSchema::V1,
            meeting_id: "transcript-torn".into(),
            state: TranscriptDeletionState::Deleting,
            prior_lifecycle: meeting.lifecycle,
            source_transcript_sha256: meeting
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .sha256
                .clone(),
            notes_directory,
            artifacts,
        };
        create_private_dir(&deletion_dir(&directory)).unwrap();
        write_receipt(&receipt_path(&directory), &receipt, true).unwrap();
        // Simulate the durable record transition immediately before a crash.
        detach_record(&directory, &mut meeting, &receipt).unwrap();

        assert_eq!(
            delete_meeting_transcript(&storage, &coordination, "transcript-torn").unwrap(),
            TranscriptDeletionOutcome::RecoveredRemoval
        );
        assert!(transcript_deletion_completed(&storage, "transcript-torn").unwrap());
    }

    #[test]
    fn captured_meeting_without_a_transcript_is_not_deleted_as_a_guess() {
        let (_temporary, storage) = storage();
        let directory = fixture(&storage, "no-transcript");
        let mut meeting = load_meeting(&directory).unwrap();
        meeting.lifecycle = MeetingLifecycle::Captured;
        meeting.artifacts.current_transcript = None;
        meeting.artifacts.current_note = None;
        write_meeting(&directory, &meeting).unwrap();
        let coordination = MeetingStorageCoordination::default();

        assert!(matches!(
            delete_meeting_transcript(&storage, &coordination, "no-transcript"),
            Err(TranscriptDeletionError::NoTranscript)
        ));
        assert!(directory.join("transcript").exists());
    }
}
