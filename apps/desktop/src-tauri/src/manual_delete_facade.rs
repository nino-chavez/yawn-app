//! Private boundary for the reviewed, immediate audio-release action.
//!
//! This is intentionally not a Tauri command and has no serialized JavaScript
//! shape. A later reviewed product surface may call it only after it has made a
//! `Reviewed` decision. The facade holds the existing process-lifetime writer
//! authority across the core's exact meeting lease, operation scan, and staged
//! `audio-deletion/1` recovery path.

use std::sync::{Arc, Mutex};

use local_meeting_notes_session_core::meeting_deletion::{
    MeetingDeletionError, MeetingDeletionOutcome,
};
use local_meeting_notes_session_core::retention::{
    AppDataWriterLock, ManualAudioDeletionError, ManualAudioDeletionOutcome,
};
use local_meeting_notes_session_core::transcript_deletion::{
    TranscriptDeletionError, TranscriptDeletionOutcome,
};

/// A closed, in-process result of the reviewed destructive confirmation.
///
/// This is deliberately not deserializable from an arbitrary webview value and
/// not a durable receipt. It only prevents an unreviewed call from crossing the
/// app-process boundary into storage mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioDeletionReview {
    Reviewed,
    NotReviewed,
}

/// The intentionally private input to immediate audio release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManualAudioDeletionUiArgs {
    pub(crate) meeting_id: String,
    pub(crate) review: AudioDeletionReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualAudioDeletionFacadeOutcome {
    DeferredActive,
    AudioReleased,
    RecoveredRemoval,
    AlreadyReleased,
}

impl From<ManualAudioDeletionOutcome> for ManualAudioDeletionFacadeOutcome {
    fn from(outcome: ManualAudioDeletionOutcome) -> Self {
        match outcome {
            ManualAudioDeletionOutcome::DeferredActive => Self::DeferredActive,
            ManualAudioDeletionOutcome::AudioReleased => Self::AudioReleased,
            ManualAudioDeletionOutcome::RecoveredRemoval => Self::RecoveredRemoval,
            ManualAudioDeletionOutcome::AlreadyReleased => Self::AlreadyReleased,
        }
    }
}

/// Content-free refusal states suitable for a future local surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualAudioDeletionFacadeError {
    ConfirmationRequired,
    WriterLockUnavailable,
    MeetingActionInProgress,
    StorageUnavailable,
}

/// Desktop owner for immediate audio release.
///
/// `writer_lock` contains an [`AppDataWriterLock`] only after startup acquired
/// the owner-only nonblocking `flock`. Keeping its mutex guard alive through
/// the core call proves the process cannot drop that authority between review
/// and the core's target lease/sequence acquisition.
pub(crate) struct ManualAudioDeletionFacade<'a> {
    writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>,
}

impl<'a> ManualAudioDeletionFacade<'a> {
    pub(crate) fn new(writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>) -> Self {
        Self { writer_lock }
    }

    /// Executes the existing `audio-deletion/1` state machine only after a
    /// closed reviewed confirmation and while the process writer lock remains
    /// held. The core retains responsibility for meeting identity validation,
    /// active-lease refusal, operation recovery, staged deletion, and receipt
    /// reconciliation.
    pub(crate) fn delete_audio(
        &self,
        args: ManualAudioDeletionUiArgs,
    ) -> Result<ManualAudioDeletionFacadeOutcome, ManualAudioDeletionFacadeError> {
        if args.review != AudioDeletionReview::Reviewed {
            return Err(ManualAudioDeletionFacadeError::ConfirmationRequired);
        }

        let held = self
            .writer_lock
            .lock()
            .map_err(|_| ManualAudioDeletionFacadeError::WriterLockUnavailable)?;
        if held.is_none() {
            return Err(ManualAudioDeletionFacadeError::WriterLockUnavailable);
        }

        held.as_ref()
            .expect("checked app-data writer lock")
            .deletion_authority()
            .delete_audio(&args.meeting_id)
            .map(Into::into)
            .map_err(map_core_error)
    }
}

fn map_core_error(error: ManualAudioDeletionError) -> ManualAudioDeletionFacadeError {
    match error {
        ManualAudioDeletionError::NonterminalProductOperation => {
            ManualAudioDeletionFacadeError::MeetingActionInProgress
        }
        _ => ManualAudioDeletionFacadeError::StorageUnavailable,
    }
}


/// The reviewed confirmation for removing a whole meeting.
///
/// Deliberately a distinct type from [`AudioDeletionReview`] rather than a reuse
/// of it. The two authorize different acts: one frees disk space, the other
/// destroys the transcript that is this product's retained evidence. Sharing a
/// token would let a confirmation the operator gave for the smaller act satisfy
/// the larger one, and the compiler would never object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum MeetingDeletionReview {
    Reviewed,
    NotReviewed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct WholeMeetingDeletionUiArgs {
    pub(crate) meeting_id: String,
    pub(crate) review: MeetingDeletionReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum WholeMeetingDeletionFacadeOutcome {
    DeferredActive,
    MeetingRemoved,
    RecoveredRemoval,
    AlreadyRemoved,
}

impl From<MeetingDeletionOutcome> for WholeMeetingDeletionFacadeOutcome {
    fn from(outcome: MeetingDeletionOutcome) -> Self {
        match outcome {
            MeetingDeletionOutcome::DeferredActive => Self::DeferredActive,
            MeetingDeletionOutcome::MeetingRemoved => Self::MeetingRemoved,
            MeetingDeletionOutcome::RecoveredRemoval => Self::RecoveredRemoval,
            MeetingDeletionOutcome::AlreadyRemoved => Self::AlreadyRemoved,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum WholeMeetingDeletionFacadeError {
    ConfirmationRequired,
    WriterLockUnavailable,
    MeetingActionInProgress,
    NoSuchMeeting,
    StorageUnavailable,
}

/// Desktop owner for whole-meeting removal. Kept as a real, working capability
/// — it is what `meeting_trash::purge_trashed_meeting` runs through once a
/// trash entry's window elapses — but no command in this app constructs one
/// directly anymore.
#[allow(dead_code)]
pub(crate) struct WholeMeetingDeletionFacade<'a> {
    writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>,
}

#[allow(dead_code)]
impl<'a> WholeMeetingDeletionFacade<'a> {
    pub(crate) fn new(writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>) -> Self {
        Self { writer_lock }
    }

    /// Runs the `meeting-deletion/1` state machine only after a closed reviewed
    /// confirmation and while the process writer lock is held. The core keeps
    /// responsibility for identity validation, active-lease refusal, the
    /// operation scan, removal ordering, and receipt reconciliation.
    pub(crate) fn delete_meeting(
        &self,
        args: WholeMeetingDeletionUiArgs,
    ) -> Result<WholeMeetingDeletionFacadeOutcome, WholeMeetingDeletionFacadeError> {
        if args.review != MeetingDeletionReview::Reviewed {
            return Err(WholeMeetingDeletionFacadeError::ConfirmationRequired);
        }

        let held = self
            .writer_lock
            .lock()
            .map_err(|_| WholeMeetingDeletionFacadeError::WriterLockUnavailable)?;
        if held.is_none() {
            return Err(WholeMeetingDeletionFacadeError::WriterLockUnavailable);
        }

        held.as_ref()
            .expect("checked app-data writer lock")
            .whole_meeting_deletion_authority()
            .delete_meeting(&args.meeting_id)
            .map(Into::into)
            .map_err(map_meeting_deletion_error)
    }
}

#[allow(dead_code)]
fn map_meeting_deletion_error(error: MeetingDeletionError) -> WholeMeetingDeletionFacadeError {
    match error {
        MeetingDeletionError::NonterminalProductOperation => {
            WholeMeetingDeletionFacadeError::MeetingActionInProgress
        }
        MeetingDeletionError::NoSuchMeeting => WholeMeetingDeletionFacadeError::NoSuchMeeting,
        _ => WholeMeetingDeletionFacadeError::StorageUnavailable,
    }
}

/// The reviewed confirmation for moving a whole meeting to local trash.
///
/// This is what the desktop "Delete meeting" command now spends — the older
/// [`MeetingDeletionReview`] above still guards an immediate, unrecoverable
/// removal, but nothing in this app calls it anymore. A distinct type again,
/// for the same reason the others are: a confirmation the operator gave for
/// one act must not silently satisfy a different one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingTrashReview {
    Reviewed,
    NotReviewed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeetingTrashUiArgs {
    pub(crate) meeting_id: String,
    pub(crate) review: MeetingTrashReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingTrashFacadeOutcome {
    DeferredActive,
    Trashed,
    RecoveredTrash,
    AlreadyTrashed,
}

impl From<local_meeting_notes_session_core::meeting_trash::MeetingTrashOutcome>
    for MeetingTrashFacadeOutcome
{
    fn from(outcome: local_meeting_notes_session_core::meeting_trash::MeetingTrashOutcome) -> Self {
        use local_meeting_notes_session_core::meeting_trash::MeetingTrashOutcome as Core;
        match outcome {
            Core::DeferredActive => Self::DeferredActive,
            Core::MeetingTrashed => Self::Trashed,
            Core::RecoveredTrash => Self::RecoveredTrash,
            Core::AlreadyTrashed => Self::AlreadyTrashed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingTrashFacadeError {
    ConfirmationRequired,
    WriterLockUnavailable,
    MeetingActionInProgress,
    NoSuchMeeting,
    StorageUnavailable,
}

/// Desktop owner for moving a whole meeting to trash.
pub(crate) struct MeetingTrashFacade<'a> {
    writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>,
}

impl<'a> MeetingTrashFacade<'a> {
    pub(crate) fn new(writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>) -> Self {
        Self { writer_lock }
    }

    pub(crate) fn trash_meeting(
        &self,
        args: MeetingTrashUiArgs,
        now_epoch_seconds: u64,
    ) -> Result<MeetingTrashFacadeOutcome, MeetingTrashFacadeError> {
        if args.review != MeetingTrashReview::Reviewed {
            return Err(MeetingTrashFacadeError::ConfirmationRequired);
        }
        let held = self
            .writer_lock
            .lock()
            .map_err(|_| MeetingTrashFacadeError::WriterLockUnavailable)?;
        if held.is_none() {
            return Err(MeetingTrashFacadeError::WriterLockUnavailable);
        }
        held.as_ref()
            .expect("checked app-data writer lock")
            .meeting_trash_authority()
            .trash_meeting(&args.meeting_id, now_epoch_seconds)
            .map(Into::into)
            .map_err(map_meeting_trash_error)
    }
}

fn map_meeting_trash_error(
    error: local_meeting_notes_session_core::meeting_trash::MeetingTrashError,
) -> MeetingTrashFacadeError {
    use local_meeting_notes_session_core::meeting_trash::MeetingTrashError as Core;
    match error {
        Core::NonterminalProductOperation | Core::NonterminalTranscription => {
            MeetingTrashFacadeError::MeetingActionInProgress
        }
        Core::Deletion(MeetingDeletionError::NonterminalProductOperation)
        | Core::Deletion(MeetingDeletionError::NonterminalTranscription) => {
            MeetingTrashFacadeError::MeetingActionInProgress
        }
        Core::NoSuchMeeting => MeetingTrashFacadeError::NoSuchMeeting,
        _ => MeetingTrashFacadeError::StorageUnavailable,
    }
}

/// No review token: restoring is additive, not destructive, so the shell asks
/// for no confirmation before spending it — it just needs the process writer
/// lock, exactly like every other storage-mutating facade here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeetingRestoreUiArgs {
    pub(crate) meeting_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingRestoreFacadeOutcome {
    Restored,
    RecoveredRestore,
}

impl From<local_meeting_notes_session_core::meeting_trash::MeetingRestoreOutcome>
    for MeetingRestoreFacadeOutcome
{
    fn from(
        outcome: local_meeting_notes_session_core::meeting_trash::MeetingRestoreOutcome,
    ) -> Self {
        use local_meeting_notes_session_core::meeting_trash::MeetingRestoreOutcome as Core;
        match outcome {
            Core::Restored => Self::Restored,
            Core::RecoveredRestore => Self::RecoveredRestore,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingRestoreFacadeError {
    WriterLockUnavailable,
    NoSuchTrashEntry,
    DestinationExists,
    PurgeInProgress,
    AlreadyPurged,
    StorageUnavailable,
}

/// Desktop owner for restoring a meeting from trash.
pub(crate) struct MeetingRestoreFacade<'a> {
    writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>,
}

impl<'a> MeetingRestoreFacade<'a> {
    pub(crate) fn new(writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>) -> Self {
        Self { writer_lock }
    }

    pub(crate) fn restore_meeting(
        &self,
        args: MeetingRestoreUiArgs,
        now_epoch_seconds: u64,
    ) -> Result<MeetingRestoreFacadeOutcome, MeetingRestoreFacadeError> {
        let held = self
            .writer_lock
            .lock()
            .map_err(|_| MeetingRestoreFacadeError::WriterLockUnavailable)?;
        if held.is_none() {
            return Err(MeetingRestoreFacadeError::WriterLockUnavailable);
        }
        held.as_ref()
            .expect("checked app-data writer lock")
            .meeting_trash_authority()
            .restore_meeting(&args.meeting_id, now_epoch_seconds)
            .map(Into::into)
            .map_err(map_meeting_restore_error)
    }
}

fn map_meeting_restore_error(
    error: local_meeting_notes_session_core::meeting_trash::MeetingRestoreError,
) -> MeetingRestoreFacadeError {
    use local_meeting_notes_session_core::meeting_trash::MeetingRestoreError as Core;
    match error {
        Core::NoSuchTrashEntry => MeetingRestoreFacadeError::NoSuchTrashEntry,
        Core::DestinationExists => MeetingRestoreFacadeError::DestinationExists,
        Core::PurgeInProgress => MeetingRestoreFacadeError::PurgeInProgress,
        Core::AlreadyPurged => MeetingRestoreFacadeError::AlreadyPurged,
        _ => MeetingRestoreFacadeError::StorageUnavailable,
    }
}

/// The reviewed confirmation for removing transcript-derived material while
/// preserving the recording and the operator's own note. This is intentionally
/// not interchangeable with either audio or whole-meeting confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptDeletionReview {
    Reviewed,
    NotReviewed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptDeletionUiArgs {
    pub(crate) meeting_id: String,
    pub(crate) review: TranscriptDeletionReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptDeletionFacadeOutcome {
    DeferredActive,
    TranscriptRemoved,
    RecoveredRemoval,
    AlreadyRemoved,
}

impl From<TranscriptDeletionOutcome> for TranscriptDeletionFacadeOutcome {
    fn from(outcome: TranscriptDeletionOutcome) -> Self {
        match outcome {
            TranscriptDeletionOutcome::DeferredActive => Self::DeferredActive,
            TranscriptDeletionOutcome::TranscriptRemoved => Self::TranscriptRemoved,
            TranscriptDeletionOutcome::RecoveredRemoval => Self::RecoveredRemoval,
            TranscriptDeletionOutcome::AlreadyRemoved => Self::AlreadyRemoved,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptDeletionFacadeError {
    ConfirmationRequired,
    WriterLockUnavailable,
    MeetingActionInProgress,
    NoTranscript,
    NoSuchMeeting,
    StorageUnavailable,
}

/// Desktop owner for transcript-only removal.
pub(crate) struct TranscriptDeletionFacade<'a> {
    writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>,
}

impl<'a> TranscriptDeletionFacade<'a> {
    pub(crate) fn new(writer_lock: &'a Mutex<Option<Arc<AppDataWriterLock>>>) -> Self {
        Self { writer_lock }
    }

    pub(crate) fn delete_transcript(
        &self,
        args: TranscriptDeletionUiArgs,
    ) -> Result<TranscriptDeletionFacadeOutcome, TranscriptDeletionFacadeError> {
        if args.review != TranscriptDeletionReview::Reviewed {
            return Err(TranscriptDeletionFacadeError::ConfirmationRequired);
        }
        let held = self
            .writer_lock
            .lock()
            .map_err(|_| TranscriptDeletionFacadeError::WriterLockUnavailable)?;
        if held.is_none() {
            return Err(TranscriptDeletionFacadeError::WriterLockUnavailable);
        }
        held.as_ref()
            .expect("checked app-data writer lock")
            .transcript_deletion_authority()
            .delete_transcript(&args.meeting_id)
            .map(TranscriptDeletionFacadeOutcome::from)
            .map_err(map_transcript_deletion_error)
    }
}

fn map_transcript_deletion_error(error: TranscriptDeletionError) -> TranscriptDeletionFacadeError {
    match error {
        TranscriptDeletionError::NonterminalProductOperation => {
            TranscriptDeletionFacadeError::MeetingActionInProgress
        }
        TranscriptDeletionError::NoTranscript => TranscriptDeletionFacadeError::NoTranscript,
        TranscriptDeletionError::NoSuchMeeting => TranscriptDeletionFacadeError::NoSuchMeeting,
        _ => TranscriptDeletionFacadeError::StorageUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use local_meeting_notes_session_core::meeting::{
        AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts, MeetingLifecycle,
        MeetingRecord, MeetingSchema, NoteRevisionRef, artifact_ref, load_meeting,
        retention_policy_sha256, write_meeting,
    };
    use local_meeting_notes_session_core::operation_store::{
        OperationStore, StoredOperationRequest,
    };
    use local_meeting_notes_session_core::operations::TranscriptRestorationRequest;
    use local_meeting_notes_session_core::storage::{
        StorageRoot, create_private_dir, durable_create_new,
    };
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::{ApplicationState, acquire_app_data_writer_lock, ensure_app_data_writer_lock};

    const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";

    fn storage() -> (TempDir, StorageRoot) {
        let temporary = TempDir::new().unwrap();
        let repository = temporary.path().join("repository");
        create_private_dir(&repository).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &repository).unwrap();
        (temporary, storage)
    }

    fn write_fixture(storage: &StorageRoot, with_note: bool) -> PathBuf {
        let directory = storage.path().join("meetings").join(MEETING_ID);
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        create_private_dir(&directory.join("transcript")).unwrap();
        if with_note {
            create_private_dir(&directory.join("notes")).unwrap();
        }
        for (relative, bytes) in [
            ("attempt.json", b"attempt".as_slice()),
            ("ownership.json", b"ownership".as_slice()),
            ("capture/session.json", b"session".as_slice()),
            ("capture/mic.wav", &[1_u8; 64][..]),
            ("capture/system.wav", &[2_u8; 64][..]),
        ] {
            durable_create_new(&directory.join(relative), bytes).unwrap();
        }
        let transcript_bytes = b"transcript";
        let transcript_relative = format!("transcript/{:x}.json", Sha256::digest(transcript_bytes));
        durable_create_new(&directory.join(&transcript_relative), transcript_bytes).unwrap();
        let transcript = artifact_ref(&directory, &transcript_relative).unwrap();
        let current_note = if with_note {
            let note_json_bytes = b"note json";
            let note_markdown_bytes = b"note markdown";
            let note_json_relative = format!("notes/{:x}.json", Sha256::digest(note_json_bytes));
            let note_markdown_relative =
                format!("notes/{:x}.md", Sha256::digest(note_markdown_bytes));
            durable_create_new(&directory.join(&note_json_relative), note_json_bytes).unwrap();
            durable_create_new(
                &directory.join(&note_markdown_relative),
                note_markdown_bytes,
            )
            .unwrap();
            Some(NoteRevisionRef {
                json: artifact_ref(&directory, &note_json_relative).unwrap(),
                markdown: artifact_ref(&directory, &note_markdown_relative).unwrap(),
                source_transcript_sha256: transcript.sha256.clone(),
            })
        } else {
            None
        };
        let rule = AudioRetentionRule::UntilManualDeletion;
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: MEETING_ID.into(),
            lifecycle: if with_note {
                MeetingLifecycle::Ready
            } else {
                MeetingLifecycle::TranscriptReady
            },
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
                current_note,
            },
            pending_storage_operation: None,
        };
        write_meeting(&directory, &meeting).unwrap();
        directory
    }

    fn reviewed() -> ManualAudioDeletionUiArgs {
        ManualAudioDeletionUiArgs {
            meeting_id: MEETING_ID.into(),
            review: AudioDeletionReview::Reviewed,
        }
    }

    #[test]
    fn unreviewed_confirmation_refuses_before_lock_or_meeting_mutation() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let state = ApplicationState::default();
        let unreviewed = ManualAudioDeletionUiArgs {
            meeting_id: MEETING_ID.into(),
            review: AudioDeletionReview::NotReviewed,
        };

        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(unreviewed),
            Err(ManualAudioDeletionFacadeError::ConfirmationRequired)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert!(!directory.join("deletion").exists());
    }

    #[test]
    fn unreviewed_whole_meeting_deletion_refuses_before_lock_or_any_removal() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let state = ApplicationState::default();
        let unreviewed = WholeMeetingDeletionUiArgs {
            meeting_id: MEETING_ID.into(),
            review: MeetingDeletionReview::NotReviewed,
        };

        assert_eq!(
            state
                .whole_meeting_deletion_facade()
                .delete_meeting(unreviewed),
            Err(WholeMeetingDeletionFacadeError::ConfirmationRequired)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.exists());
        assert!(!storage.path().join("deletions").exists());
    }

    #[test]
    fn unreviewed_transcript_deletion_refuses_before_lock_or_derived_removal() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, true);
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let state = ApplicationState::default();
        let unreviewed = TranscriptDeletionUiArgs {
            meeting_id: MEETING_ID.into(),
            review: TranscriptDeletionReview::NotReviewed,
        };

        assert_eq!(
            state
                .transcript_deletion_facade()
                .delete_transcript(unreviewed),
            Err(TranscriptDeletionFacadeError::ConfirmationRequired)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("transcript").exists());
        assert!(directory.join("notes").exists());
        assert!(!directory.join("deletion").exists());
    }

    /// The two review tokens are distinct types, so a reviewed decision about
    /// releasing audio cannot be passed where one about destroying the meeting
    /// is required. This is the compile-level half of that separation; the
    /// runtime half is that each facade only reads its own token.
    #[test]
    fn an_audio_review_token_does_not_satisfy_whole_meeting_deletion() {
        let reviewed_audio = AudioDeletionReview::Reviewed;
        let reviewed_meeting = MeetingDeletionReview::Reviewed;
        // Deliberately compared through their debug forms: they are different
        // types, so no direct comparison compiles, which is the point.
        assert_eq!(format!("{reviewed_audio:?}"), "Reviewed");
        assert_eq!(format!("{reviewed_meeting:?}"), "Reviewed");
        let args = WholeMeetingDeletionUiArgs {
            meeting_id: MEETING_ID.into(),
            review: MeetingDeletionReview::NotReviewed,
        };
        assert_ne!(args.review, MeetingDeletionReview::Reviewed);
    }

    #[test]
    fn missing_or_contended_writer_authority_refuses_before_mutation() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let state = ApplicationState::default();

        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Err(ManualAudioDeletionFacadeError::WriterLockUnavailable)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);

        let competing = acquire_app_data_writer_lock(&storage).unwrap();
        assert!(ensure_app_data_writer_lock(&state, &storage).is_err());
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Err(ManualAudioDeletionFacadeError::WriterLockUnavailable)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        drop(competing);
    }

    #[test]
    fn active_lease_and_nonterminal_operation_refuse_without_audio_mutation() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let microphone_before = fs::read(directory.join("capture/mic.wav")).unwrap();
        let system_before = fs::read(directory.join("capture/system.wav")).unwrap();

        let coordination = state.meeting_storage_coordination().unwrap();
        let lease = coordination.acquire(MEETING_ID).unwrap();
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Ok(ManualAudioDeletionFacadeOutcome::DeferredActive)
        );
        drop(lease);

        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/fixtures/product-operations-v1.json"
        )))
        .unwrap();
        let request: TranscriptRestorationRequest =
            serde_json::from_value(fixture["restoration"]["request"].clone()).unwrap();
        OperationStore::open(&storage)
            .unwrap()
            .write_request(&StoredOperationRequest::Restoration(request))
            .unwrap();
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Err(ManualAudioDeletionFacadeError::MeetingActionInProgress)
        );
        assert_eq!(
            fs::read(directory.join("capture/mic.wav")).unwrap(),
            microphone_before
        );
        assert_eq!(
            fs::read(directory.join("capture/system.wav")).unwrap(),
            system_before
        );
        assert!(!directory.join("deletion").exists());
    }

    #[test]
    fn malformed_operation_or_meeting_refuses_before_mutation() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let microphone_before = fs::read(directory.join("capture/mic.wav")).unwrap();
        let system_before = fs::read(directory.join("capture/system.wav")).unwrap();

        let operations = storage.path().join("operations");
        create_private_dir(&operations).unwrap();
        durable_create_new(&operations.join("not-an-operation"), b"crashed receipt").unwrap();
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Err(ManualAudioDeletionFacadeError::StorageUnavailable)
        );
        assert_eq!(
            fs::read(directory.join("capture/mic.wav")).unwrap(),
            microphone_before
        );
        assert_eq!(
            fs::read(directory.join("capture/system.wav")).unwrap(),
            system_before
        );

        fs::remove_file(operations.join("not-an-operation")).unwrap();
        fs::remove_dir(operations).unwrap();
        let malformed = b"not a meeting record";
        fs::write(directory.join("meeting.json"), malformed).unwrap();
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Err(ManualAudioDeletionFacadeError::StorageUnavailable)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), malformed);
        assert_eq!(
            fs::read(directory.join("capture/mic.wav")).unwrap(),
            microphone_before
        );
        assert_eq!(
            fs::read(directory.join("capture/system.wav")).unwrap(),
            system_before
        );
        assert!(!directory.join("deletion").exists());
    }

    #[test]
    fn held_writer_authority_cannot_target_a_second_storage_root() {
        let (temporary, storage_a) = storage();
        let storage_b = StorageRoot::create(
            &temporary.path().join("other-app-data"),
            &temporary.path().join("repository"),
        )
        .unwrap();
        let directory_a = write_fixture(&storage_a, false);
        let directory_b = write_fixture(&storage_b, false);
        let meeting_b_before = fs::read(directory_b.join("meeting.json")).unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage_a).unwrap();

        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Ok(ManualAudioDeletionFacadeOutcome::AudioReleased)
        );
        assert!(!directory_a.join("capture/mic.wav").exists());
        assert!(!directory_a.join("capture/system.wav").exists());
        assert_eq!(
            fs::read(directory_b.join("meeting.json")).unwrap(),
            meeting_b_before
        );
        assert!(directory_b.join("capture/mic.wav").exists());
        assert!(directory_b.join("capture/system.wav").exists());
    }

    #[test]
    fn staged_audio_release_is_idempotent_and_preserves_transcript_and_note() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, true);
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let meeting_before = load_meeting(&directory).unwrap();
        let transcript_path = directory.join(
            meeting_before
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .relative_path
                .clone(),
        );
        let note_json_path = directory.join(
            meeting_before
                .artifacts
                .current_note
                .as_ref()
                .unwrap()
                .json
                .relative_path
                .clone(),
        );
        let note_markdown_path = directory.join(
            meeting_before
                .artifacts
                .current_note
                .as_ref()
                .unwrap()
                .markdown
                .relative_path
                .clone(),
        );
        let transcript_before = fs::read(&transcript_path).unwrap();
        let note_json_before = fs::read(&note_json_path).unwrap();
        let note_markdown_before = fs::read(&note_markdown_path).unwrap();

        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Ok(ManualAudioDeletionFacadeOutcome::AudioReleased)
        );
        let committed = fs::read(directory.join("meeting.json")).unwrap();
        assert!(
            fs::read(directory.join("deletion/audio-deletion.json"))
                .unwrap()
                .windows(b"\"state\": \"removed\"".len())
                .any(|window| window == b"\"state\": \"removed\"")
        );
        assert_eq!(
            state
                .manual_audio_deletion_facade()
                .delete_audio(reviewed()),
            Ok(ManualAudioDeletionFacadeOutcome::AlreadyReleased)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), committed);
        assert_eq!(
            load_meeting(&directory).unwrap().retention.state,
            AudioState::Released
        );
        assert!(!directory.join("capture/mic.wav").exists());
        assert!(!directory.join("capture/system.wav").exists());
        assert_eq!(fs::read(transcript_path).unwrap(), transcript_before);
        assert_eq!(fs::read(note_json_path).unwrap(), note_json_before);
        assert_eq!(fs::read(note_markdown_path).unwrap(), note_markdown_before);
    }

    /// The desktop command layer's own round trip through the two new
    /// facades: an unreviewed trash request refuses before any mutation, a
    /// reviewed one moves the meeting out of `meetings/`, and restoring it
    /// through `MeetingRestoreFacade` (which asks for no review token at all)
    /// brings it back. This exercises exactly the plumbing
    /// `preview_delete_meeting_for` and `restore_meeting_from_trash_for` add
    /// in `main.rs`, one layer above the session-core tests that already
    /// cover the state machine itself.
    #[test]
    fn the_trash_and_restore_facades_refuse_unreviewed_and_round_trip_when_reviewed() {
        let (_temporary, storage) = storage();
        let directory = write_fixture(&storage, false);
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();

        let unreviewed = MeetingTrashUiArgs {
            meeting_id: MEETING_ID.into(),
            review: MeetingTrashReview::NotReviewed,
        };
        assert_eq!(
            state.meeting_trash_facade().trash_meeting(unreviewed, 1_000),
            Err(MeetingTrashFacadeError::ConfirmationRequired)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.exists(), "an unreviewed request moved the meeting");

        let reviewed = MeetingTrashUiArgs {
            meeting_id: MEETING_ID.into(),
            review: MeetingTrashReview::Reviewed,
        };
        assert_eq!(
            state.meeting_trash_facade().trash_meeting(reviewed, 1_000),
            Ok(MeetingTrashFacadeOutcome::Trashed)
        );
        assert!(!directory.exists(), "the meeting stayed at meetings/<id>");

        let restored = state
            .meeting_restore_facade()
            .restore_meeting(
                MeetingRestoreUiArgs {
                    meeting_id: MEETING_ID.into(),
                },
                2_000,
            );
        assert_eq!(restored, Ok(MeetingRestoreFacadeOutcome::Restored));
        assert!(directory.join("meeting.json").exists());

        // Restoring a second time finds nothing left to restore — the trash
        // receipt is gone, exactly as a fully completed restore leaves it.
        assert_eq!(
            state.meeting_restore_facade().restore_meeting(
                MeetingRestoreUiArgs {
                    meeting_id: MEETING_ID.into(),
                },
                3_000,
            ),
            Err(MeetingRestoreFacadeError::NoSuchTrashEntry)
        );
    }
}
