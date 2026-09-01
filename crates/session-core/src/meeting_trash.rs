//! Local trash for whole-meeting deletion — the `trash-entry/1` staged
//! machine (roadmap intake I9, with I6's "restore never overwrites" rule
//! applied).
//!
//! Before this file, `meeting_deletion.rs`'s `delete_meeting_wholly` was the
//! only route to a meeting's disappearance, and it was permanent the instant
//! it ran. That machine's shape — a receipt on disk before the first byte
//! moves, an ordering that is itself the safety property, and idempotent
//! resumption after a crash — is exactly right for what trash needs; it does
//! not need replacing, only extending in front of and behind it:
//!
//! - **In front:** deleting a meeting now moves its directory to
//!   `<root>/trash/<id>/` instead of destroying it, and writes this file's own
//!   receipt (`trash-entry/1`) recording when it was trashed, when its 30-day
//!   window ends, and — so a restore can bring back more than bytes — the
//!   title and folder it carried in the library at that moment.
//! - **Behind:** once that window elapses, purging a trash entry runs
//!   `meeting_deletion::purge_trashed_meeting`, which is the exact same
//!   Deleting→Staged→Removed state machine as before, pointed at
//!   `trash/<id>` instead of `meetings/<id>`. The permanent-removal receipt it
//!   produces (`meeting-deletion/1` at `deletions/<id>.json`) is what carries
//!   the audit trail of that meeting's end; this file's own receipt is
//!   deleted once that finishes, because a meeting either has a trash entry
//!   or it is gone — never a stale pointer to a directory that no longer
//!   exists.
//!
//! Restore is the mirror image of deletion, not a reuse of its ordering:
//! deletion removes the library row *before* the meeting record disappears,
//! so no reader ever sees a row naming a meeting that is not there. Restore
//! must therefore reinstate that row *last*, after the directory is fully
//! back on disk — `library_metadata::reinstate_meeting_organization` enforces
//! this directly, since it requires the meeting to already be readable at
//! `meetings/<id>`.
//!
//! **Audio retention is a privacy promise, not a convenience, and trash does
//! not pause it.** A trashed meeting's `next_deletion_at_epoch_seconds` still
//! fires on schedule — see `retention::execute_due_retention_in_trash` — so a
//! meeting can come back from trash with its audio already honestly released,
//! exactly as it could have while still live.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::library_metadata;
use crate::meeting::{load_meeting, read_private_bytes, require_private_directory, valid_opaque_id, MeetingError, MAX_RECEIPT_BYTES};
use crate::meeting_coordination::{MeetingCoordinationError, MeetingStorageCoordination};
use crate::meeting_deletion::{self, DeletedArtifact, MeetingDeletionError, MeetingDeletionOutcome};
use crate::operation_store::{OperationStore, OperationStoreError, StoredOperationRequest};
use crate::storage::{create_private_dir, durable_create_new, durable_replace, sync_directory, StorageRoot};
use crate::transcription_queue::TranscriptionQueueError;

/// Directory holding both trash receipts (`<id>.json`) and the trashed
/// meeting directories themselves (`<id>/`), as a child of the storage root.
pub(crate) const TRASH_DIR: &str = "trash";

/// Directory holding restore audit receipts, as a child of the storage root.
const RESTORES_DIR: &str = "restores";

/// One constant, no setting — settings stay small, and 30 days is the
/// governing constraint's own number.
///
/// Every test exercising this constant supplies its own `now_epoch_seconds`;
/// none observes real elapsed time. What is proven is the arithmetic
/// (`purge_after = trashed_at + TRASH_WINDOW_SECONDS`) and the `<=` comparison
/// `execute_due_trash_purge` makes against a caller-supplied `now` — not that
/// the real 30-second scheduled tick actually reaches this constant's value
/// in wall-clock time after 30 real days.
pub const TRASH_WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrashEntryReceipt {
    schema: TrashEntrySchema,
    meeting_id: String,
    state: TrashEntryState,
    deleted_at_epoch_seconds: u64,
    purge_after_epoch_seconds: u64,
    /// What the library remembered about this meeting the moment it was
    /// trashed, so restore can bring it back rather than leaving it unfiled.
    organization: Option<TrashedOrganization>,
    /// A snapshot taken once, at the moment of trashing — not revalidated
    /// afterward. Audio retention keeps running on a trashed meeting (see
    /// `retention::execute_due_retention_in_trash`), so this list can stop
    /// matching what is actually on disk within the same trash window; the
    /// purge that eventually removes the directory re-inventories it fresh
    /// rather than trusting this field. Treat it as "what was trashed," not
    /// "what remains."
    artifacts: Vec<DeletedArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TrashEntrySchema {
    #[serde(rename = "trash-entry/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TrashEntryState {
    /// Receipt written and inventoried; the organization row may still exist
    /// and the directory may still be at `meetings/<id>`.
    Trashing,
    /// The directory is at `trash/<id>`. This is the steady, recoverable
    /// state — the only one `list_trash_entries` reports.
    Trashed,
    /// A restore is in progress: the row is forgotten again is not needed
    /// (restore never touched it), but the directory may be at either
    /// location and the library row has not yet been reinstated.
    Restoring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrashedOrganization {
    title: Option<String>,
    folder_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingTrashOutcome {
    /// The meeting is active. Nothing was inspected and nothing was mutated.
    DeferredActive,
    /// The meeting now lives at `trash/<id>`, recoverable until its window.
    MeetingTrashed,
    /// An interrupted move to trash was found and completed.
    RecoveredTrash,
    /// A completed trash entry already existed for this identifier.
    AlreadyTrashed,
}

#[derive(Debug, Error)]
pub enum MeetingTrashError {
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
    #[error(transparent)]
    TranscriptionQueue(#[from] TranscriptionQueueError),
    #[error(transparent)]
    Deletion(#[from] MeetingDeletionError),
    #[error("meeting has nonterminal transcription work")]
    NonterminalTranscription,
    #[error("meeting has a nonterminal product operation")]
    NonterminalProductOperation,
    #[error("trash receipt is malformed")]
    MalformedReceipt,
    #[error("no such meeting")]
    NoSuchMeeting,
    #[error("this meeting is being restored from trash")]
    RestoreInProgress,
    #[error("meeting organization row could not be removed")]
    OrganizationRowRetained,
    #[error("a meeting already occupies the trash location")]
    DestinationExists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingRestoreOutcome {
    /// The meeting is back at `meetings/<id>` and its remembered name, if
    /// any, was reinstated.
    Restored,
    /// An interrupted restore was found and completed.
    RecoveredRestore,
}

#[derive(Debug, Error)]
pub enum MeetingRestoreError {
    #[error("meeting storage coordination is unavailable")]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("trash receipt is malformed")]
    MalformedReceipt,
    #[error("no such trash entry")]
    NoSuchTrashEntry,
    #[error("this meeting is still being moved to trash")]
    TrashInProgress,
    /// Refused loudly rather than merged. A collision is not expected —
    /// meeting identifiers are v4 UUIDs — but restore never overwrites, so
    /// this is checked before any mutation rather than assumed impossible.
    #[error("a meeting already exists at the restore destination")]
    DestinationExists,
    /// A permanent-removal receipt already exists for this identifier and has
    /// not reached `removed`. Restoring while a purge is actively unwinding
    /// the same directory would race it, so this is refused outright rather
    /// than guessed at.
    #[error("this meeting is being purged from trash")]
    PurgeInProgress,
    /// The trash receipt read `Trashed`, but nothing was actually found at
    /// either `trash/<id>` or `meetings/<id>` once this ran — almost always a
    /// purge that finished (its own audit trail is `deletions/<id>.json`)
    /// while this receipt still read `Trashed`, most likely because the
    /// process crashed between the purge completing and this receipt being
    /// cleaned up. There is nothing left to restore. The stale receipt is
    /// removed as part of returning this error, so this cannot recur for the
    /// same identifier.
    #[error("this meeting was already purged and cannot be restored")]
    AlreadyPurged,
}

/// One row for the quiet secondary "Trash" list. Only entries in the stable
/// `Trashed` state are ever reported — a mid-move or mid-restore entry is
/// never something a person clicks on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntrySummary {
    pub meeting_id: String,
    pub title: Option<String>,
    pub deleted_at_epoch_seconds: u64,
    pub purge_after_epoch_seconds: u64,
}

/// One outcome of a scheduled purge pass, for diagnostic logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashPurgeOutcome {
    Purged(String),
    DeferredActive(String),
    Quarantined(String),
}

/// The sole capability that may move a whole meeting to trash or restore one
/// from it.
///
/// Obtained only by borrowing an [`crate::retention::AppDataWriterLock`],
/// exactly like [`crate::meeting_deletion::WholeMeetingDeletionAuthority`].
///
/// ```compile_fail
/// use local_meeting_notes_session_core::meeting_trash::trash_meeting_wholly;
/// # let _ = trash_meeting_wholly;
/// ```
pub struct MeetingTrashAuthority<'a> {
    pub(crate) storage: &'a StorageRoot,
    pub(crate) coordination: &'a MeetingStorageCoordination,
}

impl MeetingTrashAuthority<'_> {
    pub fn trash_meeting(
        &self,
        meeting_id: &str,
        now_epoch_seconds: u64,
    ) -> Result<MeetingTrashOutcome, MeetingTrashError> {
        trash_meeting_wholly(self.storage, self.coordination, meeting_id, now_epoch_seconds)
    }

    pub fn restore_meeting(
        &self,
        meeting_id: &str,
        now_epoch_seconds: u64,
    ) -> Result<MeetingRestoreOutcome, MeetingRestoreError> {
        restore_meeting_from_trash(self.storage, self.coordination, meeting_id, now_epoch_seconds)
    }
}

fn trash_dir_lazy(storage: &StorageRoot) -> Result<PathBuf, MeetingTrashError> {
    let path = storage
        .resolve(Path::new(TRASH_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !path.exists() {
        create_private_dir(&path)?;
    }
    Ok(path)
}

fn receipt_path(storage: &StorageRoot, meeting_id: &str) -> Result<PathBuf, io::Error> {
    let directory = storage
        .resolve(Path::new(TRASH_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(directory.join(format!("{meeting_id}.json")))
}

fn load_receipt(path: &Path) -> Result<TrashEntryReceipt, MeetingTrashError> {
    let bytes = read_private_bytes(path, MAX_RECEIPT_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| MeetingTrashError::MalformedReceipt)
}

fn write_receipt(path: &Path, receipt: &TrashEntryReceipt, create: bool) -> Result<(), io::Error> {
    let bytes = serde_json::to_vec_pretty(receipt).map_err(io::Error::other)?;
    if create {
        durable_create_new(path, &bytes)
    } else {
        durable_replace(path, &bytes)
    }
}

/// A meeting's whole-meeting deletion must still preserve source audio until
/// transcription is either committed or terminal, exactly as
/// `meeting_deletion::delete_meeting_wholly` requires — trashing is still a
/// directory move that a live transcription worker could be mid-write into.
fn ensure_safe_to_move(storage: &StorageRoot, meeting_id: &str) -> Result<(), MeetingTrashError> {
    meeting_deletion::ensure_transcription_safe_for_destructive_work(storage, meeting_id).map_err(
        |error| match error {
            MeetingDeletionError::NonterminalTranscription => {
                MeetingTrashError::NonterminalTranscription
            }
            other => MeetingTrashError::Deletion(other),
        },
    )
}

fn snapshot_organization(storage: &StorageRoot, meeting_id: &str) -> Option<TrashedOrganization> {
    let library_metadata::MetadataState::Valid(document) =
        library_metadata::read_library_metadata(storage)
    else {
        return None;
    };
    let row = document
        .meetings
        .iter()
        .find(|row| row.meeting_id == meeting_id)?;
    if row.title.is_none() && row.folder_id.is_none() {
        return None;
    }
    Some(TrashedOrganization {
        title: row.title.clone(),
        folder_id: row.folder_id.clone(),
    })
}

pub(crate) fn trash_meeting_wholly(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    meeting_id: &str,
    now_epoch_seconds: u64,
) -> Result<MeetingTrashOutcome, MeetingTrashError> {
    if !valid_opaque_id(meeting_id) {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }

    let _lease = match coordination.acquire(meeting_id) {
        Ok(lease) => lease,
        Err(MeetingCoordinationError::AlreadyActive) => {
            return Ok(MeetingTrashOutcome::DeferredActive);
        }
        Err(error) => return Err(error.into()),
    };
    let _sequence = coordination.lock_sequence()?;

    let receipt_path = receipt_path(storage, meeting_id)?;
    let live_dir = storage
        .resolve(&Path::new("meetings").join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()))?;
    let trashed_dir = storage
        .resolve(&Path::new(TRASH_DIR).join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()))?;

    if receipt_path.exists() {
        let receipt = load_receipt(&receipt_path)?;
        if receipt.meeting_id != meeting_id {
            return Err(MeetingTrashError::MalformedReceipt);
        }
        return match receipt.state {
            TrashEntryState::Trashing => {
                finish_trashing(storage, &live_dir, &trashed_dir, &receipt_path, receipt)?;
                Ok(MeetingTrashOutcome::RecoveredTrash)
            }
            TrashEntryState::Trashed => Ok(MeetingTrashOutcome::AlreadyTrashed),
            // A restore already in flight owns this meeting's location; a
            // second trash call racing it would not know which directory is
            // authoritative, so it is refused rather than guessed.
            TrashEntryState::Restoring => Err(MeetingTrashError::RestoreInProgress),
        };
    }

    if !live_dir.exists() {
        return Err(MeetingTrashError::NoSuchMeeting);
    }
    require_private_directory(&live_dir)?;

    let meeting = load_meeting(&live_dir)?;
    if meeting.meeting_id != meeting_id {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }

    ensure_safe_to_move(storage, meeting_id)?;

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
        return Err(MeetingTrashError::NonterminalProductOperation);
    }

    let artifacts = meeting_deletion::take_inventory(&live_dir)?;
    let organization = snapshot_organization(storage, meeting_id);
    let receipt = TrashEntryReceipt {
        schema: TrashEntrySchema::V1,
        meeting_id: meeting_id.to_string(),
        state: TrashEntryState::Trashing,
        deleted_at_epoch_seconds: now_epoch_seconds,
        purge_after_epoch_seconds: now_epoch_seconds.saturating_add(TRASH_WINDOW_SECONDS),
        organization,
        artifacts,
    };
    // The receipt — including the inventory and the remembered name — exists
    // on disk before the organization row is touched or the directory moves.
    // That ordering is what makes an interrupted trash move recoverable.
    trash_dir_lazy(storage)?;
    write_receipt(&receipt_path, &receipt, true)?;
    finish_trashing(storage, &live_dir, &trashed_dir, &receipt_path, receipt)?;
    Ok(MeetingTrashOutcome::MeetingTrashed)
}

/// Drives the receipt from `Trashing` to `Trashed`.
///
/// Mirrors `meeting_deletion::finish_removal`'s ordering exactly: the
/// organization row goes first, for the same reason it does there — a row
/// outliving its meeting quarantines every other title and folder, not just
/// this one's — and every transition is written before the mutation it
/// authorizes.
fn finish_trashing(
    storage: &StorageRoot,
    live_dir: &Path,
    trashed_dir: &Path,
    receipt_path: &Path,
    mut receipt: TrashEntryReceipt,
) -> Result<(), MeetingTrashError> {
    if receipt.state != TrashEntryState::Trashing {
        return Ok(());
    }
    library_metadata::forget_meeting(storage, &receipt.meeting_id)
        .map_err(|()| MeetingTrashError::OrganizationRowRetained)?;

    if live_dir.exists() {
        if trashed_dir.exists() {
            return Err(MeetingTrashError::DestinationExists);
        }
        fs::rename(live_dir, trashed_dir)?;
        if let Some(parent) = trashed_dir.parent() {
            sync_directory(parent)?;
        }
        if let Some(parent) = live_dir.parent() {
            sync_directory(parent)?;
        }
    }
    receipt.state = TrashEntryState::Trashed;
    write_receipt(receipt_path, &receipt, false)?;
    Ok(())
}

pub(crate) fn restore_meeting_from_trash(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    meeting_id: &str,
    restored_at_epoch_seconds: u64,
) -> Result<MeetingRestoreOutcome, MeetingRestoreError> {
    if !valid_opaque_id(meeting_id) {
        return Err(MeetingError::Malformed("meeting identifier mismatch").into());
    }

    let _lease = coordination.acquire(meeting_id).map_err(|error| match error {
        // A meeting that is somehow both active and sitting in trash cannot
        // happen through this crate's own entry points, but refusing rather
        // than assuming keeps this consistent with every other
        // storage-mutating path here.
        MeetingCoordinationError::AlreadyActive => MeetingRestoreError::TrashInProgress,
        other => other.into(),
    })?;
    let _sequence = coordination.lock_sequence()?;

    let receipt_path = receipt_path(storage, meeting_id).map_err(MeetingRestoreError::Io)?;
    if !receipt_path.exists() {
        return Err(MeetingRestoreError::NoSuchTrashEntry);
    }
    let mut receipt = load_restore_receipt(&receipt_path)?;
    if receipt.meeting_id != meeting_id {
        return Err(MeetingRestoreError::MalformedReceipt);
    }
    if receipt.state == TrashEntryState::Trashing {
        return Err(MeetingRestoreError::TrashInProgress);
    }
    // A permanent-removal receipt for this identifier means a purge is
    // either actively unwinding `trash/<id>` right now or crashed partway
    // through doing so. Racing that directory is refused outright; the
    // `AlreadyPurged` path below is what catches a purge that finished
    // cleanly while this receipt still read `Trashed`.
    if meeting_deletion::pending_deletion_ids(storage)
        .unwrap_or_default()
        .iter()
        .any(|id| id == meeting_id)
    {
        return Err(MeetingRestoreError::PurgeInProgress);
    }
    let resumed = receipt.state == TrashEntryState::Restoring;

    let live_dir = storage
        .resolve(&Path::new("meetings").join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()))?;
    let trashed_dir = storage
        .resolve(&Path::new(TRASH_DIR).join(meeting_id))
        .map_err(|error| io::Error::other(error.to_string()))?;

    if receipt.state == TrashEntryState::Trashed {
        if live_dir.exists() {
            return Err(MeetingRestoreError::DestinationExists);
        }
        receipt.state = TrashEntryState::Restoring;
        write_receipt(&receipt_path, &receipt, false).map_err(MeetingRestoreError::Io)?;
    }

    if trashed_dir.exists() {
        if live_dir.exists() {
            return Err(MeetingRestoreError::DestinationExists);
        }
        fs::rename(&trashed_dir, &live_dir)?;
        if let Some(parent) = live_dir.parent() {
            sync_directory(parent)?;
        }
        if let Some(parent) = trashed_dir.parent() {
            sync_directory(parent)?;
        }
    }

    // Nothing may proceed past this point on faith. If the meeting is not
    // actually loadable here — the reachable case is a purge that finished
    // (its audit trail is `deletions/<id>.json`, not this receipt) while this
    // receipt still read `Trashed`, most likely because the process crashed
    // between that purge completing and this receipt being cleaned up —
    // there is nothing to restore. Reinstating a library row or reporting
    // success here would be exactly the dangling-row failure whole-meeting
    // deletion's row-first ordering exists to prevent, arriving through the
    // inverse path.
    match load_meeting(&live_dir) {
        Ok(meeting) if meeting.meeting_id == meeting_id => {}
        _ => {
            let _ = fs::remove_file(&receipt_path);
            return Err(MeetingRestoreError::AlreadyPurged);
        }
    }

    // The meeting is fully back on disk and confirmed loadable. Only now does
    // its remembered name reach the library — reinstating it earlier would
    // let a reader see a row naming a meeting that is not there yet.
    if let Some(organization) = &receipt.organization {
        let _ = library_metadata::reinstate_meeting_organization(
            storage,
            meeting_id,
            organization.title.as_deref(),
            organization.folder_id.as_deref(),
        );
    }

    write_restore_audit_receipt(storage, &receipt, restored_at_epoch_seconds)
        .map_err(MeetingRestoreError::Io)?;
    let _ = fs::remove_file(&receipt_path);

    Ok(if resumed {
        MeetingRestoreOutcome::RecoveredRestore
    } else {
        MeetingRestoreOutcome::Restored
    })
}

fn load_restore_receipt(path: &Path) -> Result<TrashEntryReceipt, MeetingRestoreError> {
    let bytes = read_private_bytes(path, MAX_RECEIPT_BYTES).map_err(MeetingRestoreError::Meeting)?;
    serde_json::from_slice(&bytes).map_err(|_| MeetingRestoreError::MalformedReceipt)
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RestoreAuditReceipt<'a> {
    schema: &'static str,
    meeting_id: &'a str,
    trashed_at_epoch_seconds: u64,
    purge_after_epoch_seconds: u64,
    restored_at_epoch_seconds: u64,
}

/// A separate, purely informational record of the restore — not a state
/// machine of its own. The trash receipt above already carries the
/// crash-resumable steps; this exists only so a restore leaves an audit trail
/// behind once its trash receipt is gone.
fn write_restore_audit_receipt(
    storage: &StorageRoot,
    receipt: &TrashEntryReceipt,
    restored_at_epoch_seconds: u64,
) -> Result<(), io::Error> {
    let directory = storage
        .resolve(Path::new(RESTORES_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !directory.exists() {
        create_private_dir(&directory)?;
    }
    let path = directory.join(format!("{}.json", receipt.meeting_id));
    let record = RestoreAuditReceipt {
        schema: "meeting-restore/1",
        meeting_id: &receipt.meeting_id,
        trashed_at_epoch_seconds: receipt.deleted_at_epoch_seconds,
        purge_after_epoch_seconds: receipt.purge_after_epoch_seconds,
        restored_at_epoch_seconds,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(io::Error::other)?;
    durable_replace(&path, &bytes)
}

/// Identity-only rows for the quiet "Trash" list. A read, and behaves like
/// one: an absent `trash/` directory means nothing has ever been trashed, so
/// it reports an empty list rather than creating the directory.
pub fn list_trash_entries(storage: &StorageRoot) -> Result<Vec<TrashEntrySummary>, MeetingTrashError> {
    let directory = storage
        .resolve(Path::new(TRASH_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Ok(receipt) = load_receipt(&entry.path()) else {
            continue;
        };
        if receipt.state != TrashEntryState::Trashed {
            continue;
        }
        entries.push(TrashEntrySummary {
            meeting_id: receipt.meeting_id,
            title: receipt.organization.and_then(|organization| organization.title),
            deleted_at_epoch_seconds: receipt.deleted_at_epoch_seconds,
            purge_after_epoch_seconds: receipt.purge_after_epoch_seconds,
        });
    }
    entries.sort_by(|left, right| right.deleted_at_epoch_seconds.cmp(&left.deleted_at_epoch_seconds));
    Ok(entries)
}

#[derive(Debug, Error)]
pub enum TrashReconcileError {
    #[error(transparent)]
    Trash(#[from] MeetingTrashError),
    #[error(transparent)]
    Restore(#[from] MeetingRestoreError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Completes every interrupted trash move or restore.
///
/// Runs at startup, before retention and before any library read — the same
/// position `meeting_deletion::reconcile_pending_meeting_deletions` holds —
/// so a meeting mid-move or mid-restore is never read as anything but what it
/// will finish as.
pub fn reconcile_pending_trash(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    now_epoch_seconds: u64,
) -> Result<Vec<String>, TrashReconcileError> {
    let directory = storage
        .resolve(Path::new(TRASH_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut completed = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Ok(receipt) = load_receipt(&entry.path()) else {
            continue;
        };
        match receipt.state {
            TrashEntryState::Trashing => {
                match trash_meeting_wholly(storage, coordination, &receipt.meeting_id, now_epoch_seconds)
                {
                    Ok(MeetingTrashOutcome::RecoveredTrash | MeetingTrashOutcome::MeetingTrashed) => {
                        completed.push(receipt.meeting_id);
                    }
                    Ok(_) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            TrashEntryState::Restoring => {
                match restore_meeting_from_trash(
                    storage,
                    coordination,
                    &receipt.meeting_id,
                    now_epoch_seconds,
                ) {
                    Ok(_) => completed.push(receipt.meeting_id),
                    // Not a reconciliation failure: `restore_meeting_from_trash`
                    // already removed the stale receipt itself. Treating this
                    // as fatal here would fail startup over a ghost this same
                    // call just cleaned up.
                    Err(MeetingRestoreError::AlreadyPurged) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            TrashEntryState::Trashed => {
                // A `Trashed` receipt whose directory is gone from
                // `trash/<id>` is a ghost: the only way to reach this state is
                // a purge that finished — its own audit trail lives in
                // `deletions/<id>.json` — while this receipt still read
                // `Trashed`, almost always because the process crashed
                // between the purge completing and this receipt being
                // cleaned up. Left alone it would keep offering a Restore
                // button for a meeting that no longer exists anywhere, which
                // is exactly what `restore_meeting_from_trash`'s own
                // `AlreadyPurged` check refuses — this removes it proactively
                // instead of waiting for that refusal to be the first sign of
                // trouble.
                let trashed_dir = storage
                    .resolve(&Path::new(TRASH_DIR).join(&receipt.meeting_id))
                    .map_err(|error| io::Error::other(error.to_string()))?;
                if !trashed_dir.exists() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
    Ok(completed)
}

/// Purges every trash entry whose 30-day window has elapsed.
///
/// Runs from the same scheduled tick that already runs audio retention (see
/// `retention::execute_scheduled_retention` in the desktop crate), reusing
/// its scheduling idiom rather than a second timer. Each due entry is handed
/// to `meeting_deletion::purge_trashed_meeting` — the existing
/// Deleting→Staged→Removed permanent path, pointed at `trash/<id>` — and this
/// entry's own receipt is removed once that finishes, successfully or as an
/// already-completed no-op, so the quiet list never carries a pointer to a
/// directory that is no longer there.
pub fn execute_due_trash_purge(
    storage: &StorageRoot,
    coordination: &MeetingStorageCoordination,
    now_epoch_seconds: u64,
) -> Result<Vec<TrashPurgeOutcome>, MeetingTrashError> {
    let directory = storage
        .resolve(Path::new(TRASH_DIR))
        .map_err(|error| io::Error::other(error.to_string()))?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut outcomes = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Ok(receipt) = load_receipt(&entry.path()) else {
            continue;
        };
        if receipt.state != TrashEntryState::Trashed {
            continue;
        }
        if receipt.purge_after_epoch_seconds > now_epoch_seconds {
            continue;
        }
        let meeting_id = receipt.meeting_id.clone();
        match meeting_deletion::purge_trashed_meeting(storage, coordination, &meeting_id) {
            Ok(
                MeetingDeletionOutcome::MeetingRemoved
                | MeetingDeletionOutcome::RecoveredRemoval
                | MeetingDeletionOutcome::AlreadyRemoved,
            ) => {
                let _ = fs::remove_file(entry.path());
                outcomes.push(TrashPurgeOutcome::Purged(meeting_id));
            }
            Ok(MeetingDeletionOutcome::DeferredActive) => {
                outcomes.push(TrashPurgeOutcome::DeferredActive(meeting_id));
            }
            Err(_) => outcomes.push(TrashPurgeOutcome::Quarantined(meeting_id)),
        }
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::{
        artifact_ref, retention_policy_sha256, AudioRetention, AudioRetentionRule, AudioState,
        MeetingArtifacts, MeetingLifecycle, MeetingRecord, MeetingSchema,
    };
    use crate::storage::durable_create_new as durable_create_new_fixture;
    use tempfile::TempDir;

    fn storage() -> (TempDir, StorageRoot) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        (temp, storage)
    }

    fn fixture(storage: &StorageRoot, id: &str) -> PathBuf {
        let directory = storage.resolve(&Path::new("meetings").join(id)).unwrap();
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        create_private_dir(&directory.join("transcription-queue")).unwrap();
        for (relative, bytes) in [
            ("attempt.json", b"attempt".as_slice()),
            ("ownership.json", b"ownership".as_slice()),
            ("capture/session.json", b"session".as_slice()),
            ("capture/mic.wav", b"mic".as_slice()),
            ("capture/system.wav", b"system".as_slice()),
            ("transcript.json", b"we agreed to ship on friday".as_slice()),
        ] {
            durable_create_new_fixture(&directory.join(relative), bytes).unwrap();
        }
        let rule = AudioRetentionRule::UntilManualDeletion;
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::Captured,
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
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        };
        durable_create_new_fixture(
            &directory.join("meeting.json"),
            &serde_json::to_vec_pretty(&meeting).unwrap(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn trashing_a_meeting_moves_it_and_writes_a_receipt_with_inventory_and_purge_window() {
        let (_temp, storage) = storage();
        let live = fixture(&storage, "gone-but-recoverable");
        let coordination = MeetingStorageCoordination::default();

        assert_eq!(
            trash_meeting_wholly(&storage, &coordination, "gone-but-recoverable", 1_000).unwrap(),
            MeetingTrashOutcome::MeetingTrashed
        );
        assert!(!live.exists(), "the meeting stayed at meetings/<id>");
        let trashed = storage
            .resolve(&Path::new(TRASH_DIR).join("gone-but-recoverable"))
            .unwrap();
        assert!(trashed.join("meeting.json").exists());
        assert!(trashed.join("capture/mic.wav").exists());
        assert!(
            trashed.join("transcript.json").exists(),
            "moving to trash must keep the transcript, unlike a permanent delete"
        );

        let entries = list_trash_entries(&storage).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].meeting_id, "gone-but-recoverable");
        assert_eq!(entries[0].deleted_at_epoch_seconds, 1_000);
        assert_eq!(
            entries[0].purge_after_epoch_seconds,
            1_000 + TRASH_WINDOW_SECONDS
        );
    }

    #[test]
    fn trashing_removes_the_organization_row_and_restoring_brings_it_back_last() {
        let (_temp, storage) = storage();
        fixture(&storage, "titled");
        library_metadata::set_meeting_title(&storage, 0, "titled", Some("Kickoff")).unwrap();
        let coordination = MeetingStorageCoordination::default();

        trash_meeting_wholly(&storage, &coordination, "titled", 1_000).unwrap();
        match library_metadata::read_library_metadata(&storage) {
            library_metadata::MetadataState::Valid(document) => {
                assert!(document.meetings.is_empty(), "the row survived trashing");
            }
            _ => panic!("expected a valid, empty record after the row was forgotten"),
        }

        assert_eq!(
            restore_meeting_from_trash(&storage, &coordination, "titled", 2_000).unwrap(),
            MeetingRestoreOutcome::Restored
        );
        let live = storage.resolve(&Path::new("meetings").join("titled")).unwrap();
        assert!(live.join("meeting.json").exists());
        match library_metadata::read_library_metadata(&storage) {
            library_metadata::MetadataState::Valid(document) => {
                assert_eq!(document.meetings.len(), 1);
                assert_eq!(document.meetings[0].title.as_deref(), Some("Kickoff"));
            }
            _ => panic!("expected the reinstated title"),
        }
        assert!(
            list_trash_entries(&storage).unwrap().is_empty(),
            "a restored meeting must not still appear in Trash"
        );
    }

    #[test]
    fn restore_refuses_loudly_when_the_destination_already_exists() {
        let (_temp, storage) = storage();
        fixture(&storage, "collides");
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "collides", 1_000).unwrap();

        // Simulate the impossible-in-practice case: something occupies the
        // live slot again before restore runs.
        let live = storage.resolve(&Path::new("meetings").join("collides")).unwrap();
        create_private_dir(&live).unwrap();
        durable_create_new_fixture(&live.join("marker.json"), b"not the trashed meeting").unwrap();

        assert!(matches!(
            restore_meeting_from_trash(&storage, &coordination, "collides", 2_000),
            Err(MeetingRestoreError::DestinationExists)
        ));
        let trashed = storage.resolve(&Path::new(TRASH_DIR).join("collides")).unwrap();
        assert!(
            trashed.exists(),
            "a refused restore must not have moved or merged the trashed copy"
        );
        assert!(
            durable_create_new_fixture(&live.join("marker2.json"), b"untouched").is_ok(),
            "the colliding directory must be untouched"
        );
    }

    #[test]
    fn purging_after_the_window_removes_the_trashed_copy_and_the_trash_entry() {
        let (_temp, storage) = storage();
        fixture(&storage, "aged-out");
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "aged-out", 1_000).unwrap();

        // Not yet due.
        let outcomes =
            execute_due_trash_purge(&storage, &coordination, 1_000 + TRASH_WINDOW_SECONDS - 1)
                .unwrap();
        assert!(outcomes.is_empty());
        assert_eq!(list_trash_entries(&storage).unwrap().len(), 1);

        let outcomes =
            execute_due_trash_purge(&storage, &coordination, 1_000 + TRASH_WINDOW_SECONDS).unwrap();
        assert_eq!(outcomes, vec![TrashPurgeOutcome::Purged("aged-out".into())]);
        let trashed = storage.resolve(&Path::new(TRASH_DIR).join("aged-out")).unwrap();
        assert!(!trashed.exists(), "the trashed directory survived its purge");
        assert!(
            list_trash_entries(&storage).unwrap().is_empty(),
            "a purged entry must disappear from the quiet list, not linger"
        );
        // The permanent-removal machinery's own audit trail is the survivor.
        assert!(storage.path().join("deletions/aged-out.json").exists());
    }

    #[test]
    fn a_trash_move_interrupted_after_the_row_is_removed_resumes_on_reconcile() {
        let (_temp, storage) = storage();
        let live = fixture(&storage, "interrupted-trash");
        library_metadata::set_meeting_title(&storage, 0, "interrupted-trash", Some("Standup"))
            .unwrap();
        let coordination = MeetingStorageCoordination::default();

        // A crash between the row removal and the directory rename leaves
        // exactly this: receipt in `Trashing`, row already gone, directory
        // still at `meetings/<id>`.
        library_metadata::forget_meeting(&storage, "interrupted-trash").unwrap();
        let receipt = TrashEntryReceipt {
            schema: TrashEntrySchema::V1,
            meeting_id: "interrupted-trash".into(),
            state: TrashEntryState::Trashing,
            deleted_at_epoch_seconds: 500,
            purge_after_epoch_seconds: 500 + TRASH_WINDOW_SECONDS,
            organization: Some(TrashedOrganization {
                title: Some("Standup".into()),
                folder_id: None,
            }),
            artifacts: meeting_deletion::take_inventory(&live).unwrap(),
        };
        trash_dir_lazy(&storage).unwrap();
        write_receipt(&receipt_path(&storage, "interrupted-trash").unwrap(), &receipt, true).unwrap();

        let completed = reconcile_pending_trash(&storage, &coordination, 600).unwrap();
        assert_eq!(completed, vec!["interrupted-trash".to_string()]);
        assert!(!live.exists());
        let trashed = storage
            .resolve(&Path::new(TRASH_DIR).join("interrupted-trash"))
            .unwrap();
        assert!(trashed.join("meeting.json").exists());
        assert_eq!(list_trash_entries(&storage).unwrap().len(), 1);
    }

    #[test]
    fn a_restore_interrupted_after_the_move_resumes_and_still_reinstates_the_title() {
        let (_temp, storage) = storage();
        fixture(&storage, "interrupted-restore");
        library_metadata::set_meeting_title(&storage, 0, "interrupted-restore", Some("Retro"))
            .unwrap();
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "interrupted-restore", 1_000).unwrap();

        // Simulate a crash after the directory moved back but before the
        // organization row was reinstated and the receipt removed.
        let path = receipt_path(&storage, "interrupted-restore").unwrap();
        let mut receipt = load_receipt(&path).unwrap();
        receipt.state = TrashEntryState::Restoring;
        write_receipt(&path, &receipt, false).unwrap();
        let trashed = storage
            .resolve(&Path::new(TRASH_DIR).join("interrupted-restore"))
            .unwrap();
        let live = storage
            .resolve(&Path::new("meetings").join("interrupted-restore"))
            .unwrap();
        fs::rename(&trashed, &live).unwrap();

        let completed = reconcile_pending_trash(&storage, &coordination, 2_000).unwrap();
        assert_eq!(completed, vec!["interrupted-restore".to_string()]);
        assert!(live.join("meeting.json").exists());
        match library_metadata::read_library_metadata(&storage) {
            library_metadata::MetadataState::Valid(document) => {
                assert_eq!(document.meetings[0].title.as_deref(), Some("Retro"));
            }
            _ => panic!("expected the reinstated title after resumed restore"),
        }
        assert!(!path.exists(), "the trash receipt must be gone after restore");
    }

    #[test]
    fn an_active_meeting_is_not_trashed() {
        let (_temp, storage) = storage();
        let live = fixture(&storage, "busy");
        let coordination = MeetingStorageCoordination::default();
        let _held = coordination.acquire("busy").unwrap();

        assert_eq!(
            trash_meeting_wholly(&storage, &coordination, "busy", 1_000).unwrap(),
            MeetingTrashOutcome::DeferredActive
        );
        assert!(live.join("meeting.json").exists());
        assert!(!receipt_path(&storage, "busy").unwrap().exists());
    }

    #[test]
    fn retention_still_runs_on_a_trashed_meeting_and_a_restore_shows_the_honest_state() {
        let (_temp, storage) = storage();
        let live = fixture(&storage, "retained-then-trashed");
        // Swap the fixture's default `UntilManualDeletion` rule for one with a
        // due deadline, so retention has real work to do while the meeting
        // sits in trash.
        let mut meeting = load_meeting(&live).unwrap();
        let rule = AudioRetentionRule::DeleteAfter { seconds: 86_400 };
        meeting.retention.policy_sha256 = retention_policy_sha256(&rule);
        meeting.retention.rule = rule;
        meeting.retention.next_deletion_at_epoch_seconds = Some(1_500);
        crate::meeting::write_meeting(&live, &meeting).unwrap();

        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "retained-then-trashed", 1_000).unwrap();

        // Retention's own schedule fires while the meeting sits in trash — the
        // trash window has not elapsed, but the audio deadline has.
        let outcomes = crate::retention::execute_due_retention(&storage, 2_000).unwrap();
        assert!(
            outcomes
                .iter()
                .any(|outcome| matches!(outcome, crate::retention::RetentionOutcome::AudioReleased(id) if id == "retained-then-trashed")),
            "audio retention did not run on a trashed meeting: {outcomes:?}"
        );
        let trashed = storage
            .resolve(&Path::new(TRASH_DIR).join("retained-then-trashed"))
            .unwrap();
        assert!(!trashed.join("capture/mic.wav").exists());

        assert_eq!(
            restore_meeting_from_trash(&storage, &coordination, "retained-then-trashed", 3_000)
                .unwrap(),
            MeetingRestoreOutcome::Restored
        );
        let live = storage
            .resolve(&Path::new("meetings").join("retained-then-trashed"))
            .unwrap();
        let restored = load_meeting(&live).unwrap();
        assert_eq!(
            restored.retention.state,
            AudioState::Released,
            "a restored meeting must show the same honest audio-deleted state"
        );
    }

    /// The bug an earlier version of this file had: a `Trashed` receipt can
    /// outlive the directory it names — a purge that finished while the
    /// receipt still read `Trashed`, most likely because the process crashed
    /// between the purge completing and the receipt being cleaned up.
    /// Restoring that ghost must refuse, not reinstate a library row for a
    /// meeting that is not on disk anywhere — which `library_read` would
    /// treat as reason to quarantine every title and folder in the record,
    /// not just this one's.
    #[test]
    fn restore_refuses_and_cleans_up_a_ghost_receipt_left_by_a_completed_purge() {
        let (_temp, storage) = storage();
        fixture(&storage, "ghost");
        library_metadata::set_meeting_title(&storage, 0, "ghost", Some("Standup")).unwrap();
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "ghost", 1_000).unwrap();

        // Simulate a purge that fully removed the trashed directory (and
        // wrote its own `deletions/ghost.json` audit receipt) but crashed
        // before this file's own trash receipt could be cleaned up.
        assert_eq!(
            meeting_deletion::purge_trashed_meeting(&storage, &coordination, "ghost").unwrap(),
            MeetingDeletionOutcome::MeetingRemoved
        );
        let trash_receipt = receipt_path(&storage, "ghost").unwrap();
        assert!(
            trash_receipt.exists(),
            "precondition: the trash receipt outlives the purge in this scenario"
        );

        assert!(matches!(
            restore_meeting_from_trash(&storage, &coordination, "ghost", 2_000),
            Err(MeetingRestoreError::AlreadyPurged)
        ));
        assert!(
            !trash_receipt.exists(),
            "the ghost receipt must not survive a refused restore"
        );
        assert!(
            !storage.resolve(&Path::new("meetings").join("ghost")).unwrap().exists(),
            "a refused restore must not have created anything at meetings/<id>"
        );
        match library_metadata::read_library_metadata(&storage) {
            library_metadata::MetadataState::Valid(document) => {
                assert!(
                    document.meetings.is_empty(),
                    "a refused restore must never write a library row for a meeting that is not there"
                );
            }
            other => panic!("expected a valid, empty record, got {other:?}", other = "unavailable"),
        }
    }

    /// A permanent-removal receipt in flight — `deletions/<id>.json` present
    /// and not yet `removed` — must block restore outright rather than race
    /// the same directory the purge is unwinding.
    #[test]
    fn restore_refuses_while_a_permanent_purge_is_actively_in_progress() {
        let (_temp, storage) = storage();
        fixture(&storage, "mid-purge");
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "mid-purge", 1_000).unwrap();
        let trashed = storage.resolve(&Path::new(TRASH_DIR).join("mid-purge")).unwrap();
        assert!(trashed.join("meeting.json").exists());

        // Hand-write the exact `meeting-deletion/1` receipt shape
        // `meeting_deletion.rs` produces mid-flight — its types are private to
        // that module, so this pins the wire shape rather than importing it.
        let deletions_dir = storage.resolve(Path::new("deletions")).unwrap();
        create_private_dir(&deletions_dir).unwrap();
        durable_create_new_fixture(
            &deletions_dir.join("mid-purge.json"),
            br#"{"schema":"meeting-deletion/1","meeting_id":"mid-purge","state":"deleting","artifacts":[]}"#,
        )
        .unwrap();

        assert!(matches!(
            restore_meeting_from_trash(&storage, &coordination, "mid-purge", 2_000),
            Err(MeetingRestoreError::PurgeInProgress)
        ));
        assert!(
            trashed.join("meeting.json").exists(),
            "a refused restore must not have touched the trashed directory"
        );
        assert!(
            receipt_path(&storage, "mid-purge").unwrap().exists(),
            "a refused restore must leave the trash receipt as it found it"
        );
    }

    /// The startup path: `reconcile_pending_meeting_deletions` runs first and
    /// can finish an interrupted purge; `reconcile_pending_trash` must then
    /// drop the now-stale `Trashed` receipt itself rather than leaving a
    /// Restore button pointed at nothing for the Trash list to keep offering.
    #[test]
    fn reconcile_drops_a_trashed_receipt_whose_directory_a_completed_purge_already_removed() {
        let (_temp, storage) = storage();
        fixture(&storage, "swept");
        let coordination = MeetingStorageCoordination::default();
        trash_meeting_wholly(&storage, &coordination, "swept", 1_000).unwrap();

        // The purge itself completes (mirroring what
        // `reconcile_pending_meeting_deletions` would do at startup for an
        // interrupted one) without this file's own receipt being told.
        meeting_deletion::purge_trashed_meeting(&storage, &coordination, "swept").unwrap();
        assert_eq!(list_trash_entries(&storage).unwrap().len(), 1, "precondition: the ghost still lists");

        reconcile_pending_trash(&storage, &coordination, 2_000).unwrap();

        assert!(
            list_trash_entries(&storage).unwrap().is_empty(),
            "a ghost trash entry survived reconciliation"
        );
        assert!(!receipt_path(&storage, "swept").unwrap().exists());
    }
}
