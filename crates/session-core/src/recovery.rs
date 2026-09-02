use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use serde_json::from_slice;
use thiserror::Error;

use crate::meeting::{
    AudioState, MAX_RECEIPT_BYTES, MICROPHONE_AUDIO_PATH, MICROPHONE_PARTIAL_AUDIO_PATH,
    MeetingError, MeetingLifecycle, MeetingRecord, SYSTEM_AUDIO_PATH, SYSTEM_PARTIAL_AUDIO_PATH,
    artifact_ref, captured_capture_is_finalized, load_meeting, read_private_bytes,
    require_private_directory, resolve_artifact, verify_artifact_ref, verify_record_artifacts,
    verify_record_static_artifacts, write_capture_interruption_receipt, write_meeting,
};
use crate::retention::{
    MeetingRetentionResult, RetentionError, preflight_meeting_retention,
    reconcile_meeting_retention,
};
use crate::storage::StorageRoot;
use crate::supervision::{
    GroupSignaler, OwnershipReceipt, ProcessInspector, RecoveryCompletion,
    recover_owned_group_and_wait,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub meetings: Vec<RecoveredMeeting>,
    pub blocks_capture: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredMeeting {
    pub meeting_id: String,
    pub disposition: RecoveryDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDisposition {
    Valid,
    RecoveredInterrupted,
    RecoveredAudioDeletion,
    /// A `captured` meeting whose capture genuinely finished: a legitimate
    /// transcription source the background queue owns and retention defers to.
    /// Startup names it here so it is never left to be silently fought over by
    /// the queue and the retention pass; the record is not mutated.
    CapturedAwaitingTranscription,
    Quarantined(RecoveryCode),
    OwnershipAmbiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryCode {
    MalformedMeeting,
    ArtifactMismatch,
    OwnershipMalformed,
    RetentionMismatch,
}

impl RecoveryCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MalformedMeeting => "meeting_recovery_malformed",
            Self::ArtifactMismatch => "meeting_recovery_artifact_mismatch",
            Self::OwnershipMalformed => "meeting_recovery_ownership_malformed",
            Self::RetentionMismatch => "meeting_recovery_retention_mismatch",
        }
    }
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("meeting storage cannot be enumerated")]
    StorageUnavailable,
    #[error(transparent)]
    Io(#[from] io::Error),
}

enum OneMeeting {
    Disposition(RecoveryDisposition),
    OwnershipBlocked,
    OwnershipMalformed,
}

pub fn scan_and_recover(
    storage: &StorageRoot,
    now_epoch_seconds: u64,
    inspector: &dyn ProcessInspector,
    signaler: &dyn GroupSignaler,
    shutdown_grace: Duration,
) -> Result<RecoveryReport, RecoveryError> {
    let meetings_dir = storage
        .resolve(Path::new("meetings"))
        .map_err(|_| RecoveryError::StorageUnavailable)?;
    let entries = fs::read_dir(meetings_dir).map_err(|_| RecoveryError::StorageUnavailable)?;
    let mut report = RecoveryReport {
        meetings: Vec::new(),
        blocks_capture: false,
    };

    for entry in entries {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                report.blocks_capture = true;
                report.meetings.push(RecoveredMeeting {
                    meeting_id: id,
                    disposition: RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting),
                });
                continue;
            }
        };
        if !file_type.is_dir() {
            report.blocks_capture = true;
            report.meetings.push(RecoveredMeeting {
                meeting_id: id,
                disposition: RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting),
            });
            continue;
        }
        let meeting_dir = entry.path();
        let result = recover_one(
            &meeting_dir,
            now_epoch_seconds,
            inspector,
            signaler,
            shutdown_grace,
        );
        let disposition = match result {
            Ok(OneMeeting::Disposition(disposition)) => disposition,
            Ok(OneMeeting::OwnershipBlocked) => {
                report.blocks_capture = true;
                RecoveryDisposition::OwnershipAmbiguous
            }
            Ok(OneMeeting::OwnershipMalformed) => {
                report.blocks_capture = true;
                RecoveryDisposition::Quarantined(RecoveryCode::OwnershipMalformed)
            }
            Err(RecoveryOneError::Unclassifiable(MeetingError::ArtifactMismatch)) => {
                report.blocks_capture = true;
                RecoveryDisposition::Quarantined(RecoveryCode::ArtifactMismatch)
            }
            Err(RecoveryOneError::Unclassifiable(_)) => {
                report.blocks_capture = true;
                RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting)
            }
            Err(RecoveryOneError::Meeting(MeetingError::ArtifactMismatch)) => {
                RecoveryDisposition::Quarantined(RecoveryCode::ArtifactMismatch)
            }
            Err(RecoveryOneError::Meeting(_)) => {
                RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting)
            }
            Err(RecoveryOneError::Retention(_)) => {
                RecoveryDisposition::Quarantined(RecoveryCode::RetentionMismatch)
            }
        };
        report.meetings.push(RecoveredMeeting {
            meeting_id: id,
            disposition,
        });
    }
    Ok(report)
}

#[derive(Debug, Error)]
enum RecoveryOneError {
    #[error("meeting record could not be classified")]
    Unclassifiable(#[source] MeetingError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    Retention(#[from] RetentionError),
}

fn recover_one(
    meeting_dir: &Path,
    now_epoch_seconds: u64,
    inspector: &dyn ProcessInspector,
    signaler: &dyn GroupSignaler,
    shutdown_grace: Duration,
) -> Result<OneMeeting, RecoveryOneError> {
    let mut meeting = load_meeting(meeting_dir).map_err(RecoveryOneError::Unclassifiable)?;
    if meeting.lifecycle == MeetingLifecycle::Captured {
        return recover_captured(meeting_dir, &mut meeting, now_epoch_seconds);
    }
    if meeting.lifecycle != MeetingLifecycle::Incomplete {
        let result =
            reconcile_meeting_retention(meeting_dir, &mut meeting, now_epoch_seconds, true)?;
        return Ok(OneMeeting::Disposition(match result {
            MeetingRetentionResult::NotDue => RecoveryDisposition::Valid,
            MeetingRetentionResult::AudioReleased | MeetingRetentionResult::RecoveredRemoval => {
                RecoveryDisposition::RecoveredAudioDeletion
            }
        }));
    }

    let Some(ownership_ref) = &meeting.artifacts.ownership else {
        return Ok(OneMeeting::OwnershipMalformed);
    };
    if verify_artifact_ref(meeting_dir, ownership_ref).is_err() {
        return Ok(OneMeeting::OwnershipMalformed);
    }
    let path = match resolve_artifact(meeting_dir, &ownership_ref.relative_path) {
        Ok(path) => path,
        Err(_) => return Ok(OneMeeting::OwnershipMalformed),
    };
    let bytes = match read_private_bytes(&path, MAX_RECEIPT_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(OneMeeting::OwnershipMalformed),
    };
    let receipt: OwnershipReceipt = match from_slice(&bytes) {
        Ok(receipt) => receipt,
        Err(_) => return Ok(OneMeeting::OwnershipMalformed),
    };
    if !receipt.validate() {
        return Ok(OneMeeting::OwnershipMalformed);
    }
    let completion =
        match recover_owned_group_and_wait(&receipt, inspector, signaler, shutdown_grace) {
            Ok(completion) => completion,
            Err(_) => return Ok(OneMeeting::OwnershipBlocked),
        };
    match completion {
        RecoveryCompletion::NoChildrenLive | RecoveryCompletion::StoppedExactGroup => {}
        RecoveryCompletion::AmbiguousIdentity | RecoveryCompletion::StillRunning => {
            return Ok(OneMeeting::OwnershipBlocked);
        }
    }
    verify_record_static_artifacts(meeting_dir, &meeting)?;
    preflight_meeting_retention(meeting_dir, &meeting)?;

    bind_interrupted_artifacts(meeting_dir, &mut meeting)?;
    write_meeting(meeting_dir, &meeting)?;
    let _ = reconcile_meeting_retention(meeting_dir, &mut meeting, now_epoch_seconds, true)?;
    Ok(OneMeeting::Disposition(
        RecoveryDisposition::RecoveredInterrupted,
    ))
}

/// Classify a `captured` meeting rather than folding it into the generic
/// retention reconcile, which is what left a quit-mid-finalize meeting to be
/// silently fought over by the transcription queue and the retention pass.
///
/// Two honest outcomes, decided by the on-disk capture receipt:
///
///   * A genuinely finalized capture (`capture-session/2`, status complete,
///     audit blocks present) is a real transcription source. The background
///     queue owns it and retention defers to it, so recovery only names it
///     (`CapturedAwaitingTranscription`) and does not touch the record.
///   * A capture that never finished (missing / incomplete / unreadable
///     receipt) is salvaged into the same recovered-interrupted shape a
///     crash-during-recording produces: the on-disk audio is bound, a fresh
///     interruption receipt replaces the unfinished one, and the stale
///     transcription obligation is cleared so the meeting reads honestly and
///     carries its normal delete / trash actions instead of trapping the app.
fn recover_captured(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
    now_epoch_seconds: u64,
) -> Result<OneMeeting, RecoveryOneError> {
    let finalized =
        captured_capture_is_finalized(meeting_dir, meeting).map_err(RecoveryOneError::Meeting)?;
    if finalized {
        verify_record_artifacts(meeting_dir, meeting)?;
        return Ok(OneMeeting::Disposition(
            RecoveryDisposition::CapturedAwaitingTranscription,
        ));
    }
    salvage_unfinalized_capture(meeting_dir, meeting)?;
    let _ = reconcile_meeting_retention(meeting_dir, meeting, now_epoch_seconds, true)?;
    Ok(OneMeeting::Disposition(
        RecoveryDisposition::RecoveredInterrupted,
    ))
}

/// Turn a quit-mid-finalize `captured` meeting into the recovered-interrupted
/// shape. The capture is done (no live capture worker to signal, and startup
/// stops every worker before this scan), so this only reconciles storage:
/// discard the unfinished capture receipt, rebind whatever audio survived on
/// disk, and drop the stale transcription request that would otherwise keep
/// the audio pinned and refuse deletion.
fn salvage_unfinalized_capture(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
) -> Result<(), RecoveryOneError> {
    let session = meeting_dir.join("capture/session.json");
    if fs::symlink_metadata(&session).is_ok() {
        fs::remove_file(&session).map_err(|error| RecoveryOneError::Meeting(error.into()))?;
    }
    // The request references the capture-session digest we just discarded, so
    // it can never be honored. Removing the whole per-meeting queue directory
    // is the reclassification's own cleanup — it is not a mutation of a live,
    // in-flight receipt, and it is what frees delete / retention from the
    // "transcription still needs this audio" guard.
    let queue_dir = meeting_dir.join("transcription-queue");
    if fs::symlink_metadata(&queue_dir).is_ok() {
        fs::remove_dir_all(&queue_dir).map_err(|error| RecoveryOneError::Meeting(error.into()))?;
    }
    meeting.artifacts.capture_session = None;
    bind_interrupted_artifacts(meeting_dir, meeting)?;
    write_meeting(meeting_dir, meeting)?;
    Ok(())
}

fn bind_interrupted_artifacts(
    meeting_dir: &Path,
    meeting: &mut MeetingRecord,
) -> Result<(), MeetingError> {
    let inventory = discover_interrupted_capture(meeting_dir)?;
    meeting.artifacts.microphone_audio = inventory.microphone;
    meeting.artifacts.system_audio = inventory.system;
    meeting.lifecycle = MeetingLifecycle::RecoveredInterrupted;
    meeting.retention.state = if meeting.artifacts.microphone_audio.is_some()
        || meeting.artifacts.system_audio.is_some()
    {
        AudioState::Retained
    } else {
        AudioState::NeverCreated
    };
    meeting.retention.deletion_receipt = None;
    meeting.pending_storage_operation = None;
    meeting.artifacts.capture_session = if meeting.retention.state == AudioState::Retained {
        if inventory.session_present {
            Some(artifact_ref(meeting_dir, "capture/session.json")?)
        } else {
            Some(write_capture_interruption_receipt(meeting_dir, meeting)?)
        }
    } else {
        None
    };
    verify_record_static_artifacts(meeting_dir, meeting)?;
    Ok(())
}

struct InterruptedCaptureInventory {
    session_present: bool,
    microphone: Option<crate::meeting::ArtifactRef>,
    system: Option<crate::meeting::ArtifactRef>,
}

fn discover_interrupted_capture(
    meeting_dir: &Path,
) -> Result<InterruptedCaptureInventory, MeetingError> {
    let capture_dir = meeting_dir.join("capture");
    match fs::symlink_metadata(&capture_dir) {
        Ok(_) => require_private_directory(&capture_dir)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(InterruptedCaptureInventory {
                session_present: false,
                microphone: None,
                system: None,
            });
        }
        Err(error) => return Err(error.into()),
    }

    let mut names = std::collections::HashSet::new();
    for entry in fs::read_dir(&capture_dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| MeetingError::Malformed("capture artifact name is not UTF-8"))?;
        let recognized = matches!(
            name.as_str(),
            "session.json" | "mic.wav" | "system.wav" | ".mic.wav.partial" | ".system.wav.partial"
        );
        if !recognized || !names.insert(name) {
            return Err(MeetingError::Malformed(
                "capture directory has an unexpected artifact shape",
            ));
        }
    }

    let canonical_mic = names.contains("mic.wav");
    let canonical_system = names.contains("system.wav");
    let partial_mic = names.contains(".mic.wav.partial");
    let partial_system = names.contains(".system.wav.partial");
    let canonical = canonical_mic || canonical_system;
    let partial = partial_mic || partial_system;
    if canonical && partial {
        return Err(MeetingError::Malformed(
            "capture directory mixes final and partial WAV names",
        ));
    }
    if canonical && !(canonical_mic && canonical_system) {
        return Err(MeetingError::Malformed(
            "capture directory has an incomplete promoted WAV pair",
        ));
    }
    let session_present = names.contains("session.json");
    if !canonical && !partial && session_present {
        return Err(MeetingError::Malformed(
            "capture receipt exists without interrupted audio",
        ));
    }

    let microphone_path = if canonical_mic {
        Some(MICROPHONE_AUDIO_PATH)
    } else if partial_mic {
        Some(MICROPHONE_PARTIAL_AUDIO_PATH)
    } else {
        None
    };
    let system_path = if canonical_system {
        Some(SYSTEM_AUDIO_PATH)
    } else if partial_system {
        Some(SYSTEM_PARTIAL_AUDIO_PATH)
    } else {
        None
    };
    Ok(InterruptedCaptureInventory {
        session_present,
        microphone: microphone_path
            .map(|relative| artifact_ref(meeting_dir, relative))
            .transpose()?,
        system: system_path
            .map(|relative| artifact_ref(meeting_dir, relative))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;

    use super::*;
    use crate::meeting::{
        ArtifactRef, AudioRetention, AudioRetentionRule, MeetingArtifacts, MeetingSchema,
        retention_policy_sha256,
    };
    use crate::retention::meeting_dir;
    use crate::storage::{create_private_dir, durable_create_new, durable_replace};
    use crate::supervision::{OwnershipSchema, ProcessIdentity, ProcessInspection};
    use tempfile::TempDir;

    struct FakeInspector(HashMap<u32, ProcessInspection>);

    impl ProcessInspector for FakeInspector {
        fn inspect(&self, pid: u32) -> io::Result<ProcessInspection> {
            Ok(self
                .0
                .get(&pid)
                .cloned()
                .unwrap_or(ProcessInspection::Absent))
        }
    }

    struct FakeSignaler(Cell<u32>);

    impl GroupSignaler for FakeSignaler {
        fn terminate(&self, _process_group_id: i32) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }

    fn make_storage() -> (TempDir, StorageRoot) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        (temp, storage)
    }

    fn write_incomplete(
        storage: &StorageRoot,
        id: &str,
        ownership: Option<OwnershipReceipt>,
    ) -> PathBuf {
        let directory = meeting_dir(storage, id).unwrap();
        create_private_dir(&directory).unwrap();
        durable_create_new(&directory.join("attempt.json"), b"attempt").unwrap();
        let ownership_ref = ownership.map(|receipt| {
            durable_create_new(
                &directory.join("ownership.json"),
                &serde_json::to_vec_pretty(&receipt).unwrap(),
            )
            .unwrap();
            artifact_ref(&directory, "ownership.json").unwrap()
        });
        let rule = AudioRetentionRule::DeleteAfter { seconds: 30 };
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::Incomplete,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: Some(100),
                state: AudioState::NeverCreated,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: ownership_ref,
                capture_session: None,
                microphone_audio: None,
                system_audio: None,
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
        directory
    }

    fn identity(pid: u32, start: u64) -> ProcessIdentity {
        ProcessIdentity {
            pid,
            start_time_epoch_seconds: start,
            executable_path: PathBuf::from("/fixed/worker"),
            executable_sha256: "d".repeat(64),
        }
    }

    fn ownership(expected: ProcessIdentity) -> OwnershipReceipt {
        OwnershipReceipt {
            schema: OwnershipSchema::V1,
            process_group_id: expected.pid as i32,
            application_build_sha256: "a".repeat(64),
            worker_build_sha256: "b".repeat(64),
            tap_build_sha256: "c".repeat(64),
            children: vec![expected],
        }
    }

    fn private_wav(marker: u8) -> Vec<u8> {
        vec![marker; 44]
    }

    #[test]
    fn clean_promoted_pair_without_session_gets_non_product_interruption_receipt() {
        let (_temp, storage) = make_storage();
        let expected = identity(44, 10);
        let directory = write_incomplete(&storage, "promoted", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), &private_wav(1)).unwrap();
        durable_create_new(&directory.join("capture/system.wav"), &private_wav(2)).unwrap();
        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(!report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::RecoveredInterrupted
        );
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::RecoveredInterrupted);
        assert_eq!(meeting.retention.state, AudioState::Retained);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert_eq!(
            meeting.artifacts.microphone_audio.unwrap().relative_path,
            "capture/mic.wav"
        );
        assert_eq!(
            meeting.artifacts.system_audio.unwrap().relative_path,
            "capture/system.wav"
        );
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("capture/session.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["schema"], "capture-interruption/1");
        assert_ne!(receipt["schema"], "capture-session/2");
        assert_eq!(receipt["meeting_id"], "promoted");
        assert_eq!(receipt["artifacts"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn interrupted_partial_pair_is_bound_under_its_original_private_names() {
        let (_temp, storage) = make_storage();
        let expected = identity(45, 10);
        let directory = write_incomplete(&storage, "partials", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("capture/.mic.wav.partial"), &private_wav(3)).unwrap();
        durable_create_new(
            &directory.join("capture/.system.wav.partial"),
            &private_wav(4),
        )
        .unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::RecoveredInterrupted
        );
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.retention.state, AudioState::Retained);
        assert_eq!(
            meeting.artifacts.microphone_audio.unwrap().relative_path,
            "capture/.mic.wav.partial"
        );
        assert_eq!(
            meeting.artifacts.system_audio.unwrap().relative_path,
            "capture/.system.wav.partial"
        );
        assert!(directory.join("capture/.mic.wav.partial").exists());
        assert!(directory.join("capture/.system.wav.partial").exists());
    }

    #[test]
    fn one_leg_partial_is_still_bound_to_retention() {
        let (_temp, storage) = make_storage();
        let expected = identity(145, 10);
        let directory = write_incomplete(&storage, "one-leg", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("capture/.mic.wav.partial"), &private_wav(5)).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::RecoveredInterrupted
        );
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.retention.state, AudioState::Retained);
        assert_eq!(
            meeting.artifacts.microphone_audio.unwrap().relative_path,
            "capture/.mic.wav.partial"
        );
        assert!(meeting.artifacts.system_audio.is_none());
    }

    #[test]
    fn mismatched_interruption_receipt_is_quarantined_without_rebinding() {
        let (_temp, storage) = make_storage();
        let expected = identity(46, 10);
        let directory = write_incomplete(&storage, "receipt-mismatch", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        let mic = private_wav(5);
        durable_create_new(&directory.join("capture/.mic.wav.partial"), &mic).unwrap();
        let receipt = serde_json::json!({
            "schema": "capture-interruption/1",
            "status": "interrupted",
            "reason": "fresh-process-recovery",
            "meeting_id": "receipt-mismatch",
            "artifacts": [{
                "name": ".mic.wav.partial",
                "bytes": mic.len(),
                "sha256": "0".repeat(64),
                "mode": "0600"
            }]
        });
        durable_create_new(
            &directory.join("capture/session.json"),
            &serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::ArtifactMismatch)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(directory.join("capture/.mic.wav.partial").exists());
    }

    #[test]
    fn duplicate_final_and_partial_names_are_quarantined_untouched() {
        let (_temp, storage) = make_storage();
        let expected = identity(47, 10);
        let directory = write_incomplete(&storage, "duplicate", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), &private_wav(6)).unwrap();
        durable_create_new(&directory.join("capture/system.wav"), &private_wav(7)).unwrap();
        durable_create_new(&directory.join("capture/.mic.wav.partial"), &private_wav(8)).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(matches!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(_)
        ));
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(!directory.join("capture/session.json").exists());
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/.mic.wav.partial").exists());
    }

    #[test]
    fn symlinked_partial_audio_is_quarantined_without_creating_a_receipt() {
        let (_temp, storage) = make_storage();
        let expected = identity(48, 10);
        let directory = write_incomplete(&storage, "symlink", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        let target = storage.path().join("symlink-target.wav");
        durable_create_new(&target, &private_wav(9)).unwrap();
        symlink(&target, directory.join("capture/.mic.wav.partial")).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(matches!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(_)
        ));
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert!(!directory.join("capture/session.json").exists());
        assert_eq!(fs::read(target).unwrap(), private_wav(9));
    }

    #[test]
    fn reused_pid_is_not_signalled_and_blocks_capture() {
        let (_temp, storage) = make_storage();
        let expected = identity(44, 10);
        let directory = write_incomplete(&storage, "ambiguous", Some(ownership(expected)));
        let signaler = FakeSignaler(Cell::new(0));
        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::from([(
                44,
                ProcessInspection::Identity(identity(44, 11)),
            )])),
            &signaler,
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(signaler.0.get(), 0);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::OwnershipAmbiguous
        );
        assert_eq!(
            load_meeting(&directory).unwrap().lifecycle,
            MeetingLifecycle::Incomplete
        );
    }

    #[test]
    fn unavailable_process_identity_is_not_signalled_and_blocks_capture() {
        let (_temp, storage) = make_storage();
        let expected = identity(46, 10);
        let directory = write_incomplete(&storage, "unavailable", Some(ownership(expected)));
        let signaler = FakeSignaler(Cell::new(0));
        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::from([(46, ProcessInspection::Unavailable)])),
            &signaler,
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(signaler.0.get(), 0);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::OwnershipAmbiguous
        );
        assert_eq!(
            load_meeting(&directory).unwrap().lifecycle,
            MeetingLifecycle::Incomplete
        );
    }

    #[test]
    fn missing_ownership_blocks_capture_without_mutating_record() {
        let (_temp, storage) = make_storage();
        let expected = identity(47, 10);
        let directory = write_incomplete(&storage, "missing-owner", Some(ownership(expected)));
        fs::remove_file(directory.join("ownership.json")).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::OwnershipMalformed)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
    }

    #[test]
    fn absent_ownership_reference_blocks_capture_without_mutating_record() {
        let (_temp, storage) = make_storage();
        let directory = write_incomplete(&storage, "absent-owner-ref", None);
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::OwnershipMalformed)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
    }

    #[test]
    fn changed_ownership_digest_blocks_capture_without_mutating_record() {
        let (_temp, storage) = make_storage();
        let expected = identity(48, 10);
        let directory = write_incomplete(&storage, "changed-owner", Some(ownership(expected)));
        durable_replace(&directory.join("ownership.json"), b"changed").unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::OwnershipMalformed)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
    }

    #[test]
    fn malformed_meeting_isolated_from_valid_recovery() {
        let (_temp, storage) = make_storage();
        let bad = write_incomplete(&storage, "bad", None);
        let expected = identity(45, 10);
        let good = write_incomplete(&storage, "good", Some(ownership(expected)));
        durable_replace(&bad.join("meeting.json"), b"not-json").unwrap();
        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert!(report.meetings.iter().any(|meeting| {
            meeting.meeting_id == "bad"
                && matches!(
                    meeting.disposition,
                    RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting)
                )
        }));
        assert_eq!(
            load_meeting(&good).unwrap().lifecycle,
            MeetingLifecycle::RecoveredInterrupted
        );
    }

    #[test]
    fn unreadable_meeting_record_is_unclassifiable_and_blocks_capture() {
        let (_temp, storage) = make_storage();
        let directory = write_incomplete(&storage, "unreadable", None);
        fs::set_permissions(
            directory.join("meeting.json"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::MalformedMeeting)
        );
    }

    #[test]
    fn malformed_deletion_state_is_found_before_interrupted_record_rewrite() {
        let (_temp, storage) = make_storage();
        let expected = identity(49, 10);
        let directory = write_incomplete(&storage, "bad-deletion", Some(ownership(expected)));
        create_private_dir(&directory.join("capture")).unwrap();
        durable_create_new(&directory.join("capture/session.json"), b"incomplete").unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), b"partial-mic").unwrap();
        create_private_dir(&directory.join("deletion")).unwrap();
        durable_create_new(&directory.join("deletion/audio-deletion.json"), b"not-json").unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(!report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::Quarantined(RecoveryCode::RetentionMismatch)
        );
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::Incomplete);
        assert!(meeting.artifacts.capture_session.is_none());
        assert!(meeting.artifacts.microphone_audio.is_none());
        assert!(directory.join("capture/mic.wav").exists());
    }

    #[test]
    fn malformed_ownership_blocks_capture_without_mutating_record() {
        let (_temp, storage) = make_storage();
        let directory = write_incomplete(&storage, "ownership", None);
        durable_create_new(&directory.join("ownership.json"), b"not-json").unwrap();
        let mut meeting = load_meeting(&directory).unwrap();
        meeting.artifacts.ownership = Some(ArtifactRef {
            relative_path: "ownership.json".into(),
            sha256: crate::meeting::hash_private_file(&directory.join("ownership.json")).unwrap(),
        });
        write_meeting(&directory, &meeting).unwrap();
        let before = fs::read(directory.join("meeting.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            10,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(report.blocks_capture);
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
    }

    /// A `captured` meeting with real audio, an ownership receipt, a pending
    /// transcription request, and the given `capture/session.json` bytes.
    fn write_captured(storage: &StorageRoot, id: &str, session_json: &[u8]) -> PathBuf {
        let directory = meeting_dir(storage, id).unwrap();
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        create_private_dir(&directory.join("transcription-queue")).unwrap();
        durable_create_new(&directory.join("attempt.json"), b"attempt").unwrap();
        durable_create_new(
            &directory.join("ownership.json"),
            &serde_json::to_vec_pretty(&ownership(identity(90, 10))).unwrap(),
        )
        .unwrap();
        durable_create_new(&directory.join("capture/session.json"), session_json).unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), &private_wav(1)).unwrap();
        durable_create_new(&directory.join("capture/system.wav"), &private_wav(2)).unwrap();
        // A request receipt keeps the transcription obligation alive, exactly as
        // the live poisoned meeting carried one.
        durable_create_new(
            &directory.join("transcription-queue/request.json"),
            b"request",
        )
        .unwrap();
        let rule = AudioRetentionRule::DeleteAfter { seconds: 30 };
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: id.into(),
            lifecycle: MeetingLifecycle::Captured,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: Some(1_000_000),
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
        directory
    }

    fn complete_session_receipt() -> Vec<u8> {
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "capture-session/2",
            "status": "complete",
            "started_at": "2026-09-01T02:58:28+0000",
            "finalized_at": "2026-09-01T19:58:48-0700",
            "health": {"schema": "capture-health/1", "usable": true},
            "reconciliation": {"legs": {}},
            "artifacts": []
        }))
        .unwrap()
    }

    #[test]
    fn finalized_captured_meeting_is_classified_awaiting_transcription_without_mutation() {
        let (_temp, storage) = make_storage();
        let directory = write_captured(&storage, "finalized", &complete_session_receipt());
        let before = fs::read(directory.join("meeting.json")).unwrap();
        let before_session = fs::read(directory.join("capture/session.json")).unwrap();

        let report = scan_and_recover(
            &storage,
            1_000,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(!report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::CapturedAwaitingTranscription
        );
        // A legitimate transcription source is named, never rewritten: the
        // record, its finished receipt, and its pending request all survive.
        assert_eq!(fs::read(directory.join("meeting.json")).unwrap(), before);
        assert_eq!(
            fs::read(directory.join("capture/session.json")).unwrap(),
            before_session
        );
        assert!(directory.join("transcription-queue/request.json").exists());
        assert_eq!(
            load_meeting(&directory).unwrap().lifecycle,
            MeetingLifecycle::Captured
        );
    }

    #[test]
    fn unfinalized_captured_meeting_is_salvaged_to_recovered_interrupted() {
        let (_temp, storage) = make_storage();
        // Quit mid-finalize: the receipt on disk is not a finished
        // capture-session/2 (here, unparseable), while the audio survived.
        let directory = write_captured(&storage, "unfinalized", b"not-a-finished-session");

        let report = scan_and_recover(
            &storage,
            1_000,
            &FakeInspector(HashMap::new()),
            &FakeSignaler(Cell::new(0)),
            Duration::from_millis(1),
        )
        .unwrap();

        assert!(!report.blocks_capture);
        assert_eq!(
            report.meetings[0].disposition,
            RecoveryDisposition::RecoveredInterrupted
        );
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::RecoveredInterrupted);
        assert_eq!(meeting.retention.state, AudioState::Retained);
        // The audio is bound and re-described by a fresh interruption receipt,
        // and the stale transcription obligation is gone so delete / retention
        // no longer choke on it.
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert!(!directory.join("transcription-queue").exists());
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("capture/session.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["schema"], "capture-interruption/1");
        verify_record_artifacts(&directory, &meeting).unwrap();
    }
}
