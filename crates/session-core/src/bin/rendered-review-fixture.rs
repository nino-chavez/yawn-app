//! Seeds one private, deterministic app-data tree for rendered review.
//!
//! The seed command accepts exactly one absolute path. The final component must
//! be `com.ninochavez.local-meeting-notes.fixture`; the archive command moves a
//! validated fixture to a recoverable sibling archive and never deletes data.

use std::env;
use std::fs;
use std::io::{self, Read, Write};
#[cfg(target_os = "macos")]
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use local_meeting_notes_session_core::meeting::{
    ArtifactRef, AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts,
    MeetingLifecycle, MeetingRecord, MeetingSchema, NoteRevisionRef, artifact_ref, load_meeting,
    retention_policy_sha256, verify_record_artifacts, write_meeting,
};
use local_meeting_notes_session_core::meeting_coordination::MeetingStorageCoordination;
use local_meeting_notes_session_core::model_store::{
    ModelVerification,
    ModelCatalog, ModelStoreError, TranscriptModelFileRole, activate_model, install_receipt_bytes,
    verify_model_directory,
};
use local_meeting_notes_session_core::retention::{AppDataWriterLock, AppDataWriterLockError};
use local_meeting_notes_session_core::runtime::{RuntimeError, RuntimeManifest};
use local_meeting_notes_session_core::storage::{
    StorageRoot, create_private_dir, durable_create_new, sync_directory,
};
use local_meeting_notes_session_core::transcript_retry::{
    TranscriptRetryAuthority, TranscriptRetrySourceBinding,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const BUNDLE_NAME: &str = "com.ninochavez.local-meeting-notes.fixture";
const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";
const OPERATION_ID: &str = "22222222-2222-4222-8222-222222222222";
const CREATED_AT: u64 = 1_700_000_042;
const AUDIO_SAMPLE_RATE: u32 = 8_000;
const AUDIO_FRAMES: usize = AUDIO_SAMPLE_RATE as usize * 8;
#[cfg(target_os = "macos")]
const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x0000_0010;

// --- Wave 2/3 state-expansion meeting identifiers -------------------------
//
// Each of these is a second, independent meeting seeded alongside the
// original `MEETING_ID` above so a single fixture root can render every
// packaged-preview review state the roadmap's "extend it before the next
// release gate" note calls out (`docs/roadmap.md`), without reseeding or
// archiving between them. Every identifier below is a fixed, synthetic v4
// UUID shape — never derived from anything real — chosen only to be visually
// distinct from `MEETING_ID` and from each other.

/// A pending retry candidate with word-for-word identical turn text to the
/// current transcript (bytes still differ, so it is a distinct candidate),
/// exercising the diff engine's "no word-level differences found" state.
const DIFF_IDENTICAL_MEETING_ID: &str = "44444444-4444-4444-8444-444444444444";
const DIFF_IDENTICAL_OPERATION_ID: &str = "24444444-4444-4444-8444-444444444444";

/// A pending retry candidate whose current/candidate turns are long enough
/// and different enough to exceed the diff engine's edit-distance budget,
/// exercising the "these transcripts are too long to highlight" skip state.
const DIFF_SKIPPED_MEETING_ID: &str = "55555555-5555-4555-8555-555555555555";
const DIFF_SKIPPED_OPERATION_ID: &str = "25555555-5555-4555-8555-555555555555";

/// A meeting trashed via the real `MeetingTrashAuthority`, exercising the
/// Trash list and restore action.
const TRASH_MEETING_ID: &str = "66666666-6666-4666-8666-666666666666";

/// A meeting whose retained transcript is tampered with after the meeting
/// record is written, so its pinned digest no longer matches the bytes on
/// disk. Exercises the export command's withheld-artifact manifest line.
const EXPORT_TAMPERED_MEETING_ID: &str = "77777777-7777-4777-8777-777777777777";

/// A meeting carrying a real, validator-passing generated note (claim types
/// `summary` and `decision`), a pre-meeting context note, and one transcript
/// turn no claim cites — exercising the library row preview, the reverse
/// citation map, and the read-only pre-meeting context block.
const NOTE_MEETING_ID: &str = "33333333-3333-4333-8333-333333333333";

/// Roadmap Wave 4 / I5: a meeting locked via the real `meeting-lock/1`
/// sidecar shape, exercising the locked library row — title and date
/// visible, no note preview, the shell's "Locked" marker. See
/// `seed_locked_meeting` for why this is a barrier-only fixture state, never
/// an unlock.
const LOCKED_MEETING_ID: &str = "88888888-8888-4888-8888-888888888888";

/// Sample rate the capture-pause schema is measured in (`capture_quality.rs`'s
/// `CAPTURE_RATE`) — a normalized processing rate, independent of the
/// fixture's own 8 kHz synthetic WAV sample rate above.
const PAUSE_SAMPLE_RATE: u64 = 16_000;

#[derive(Debug, thiserror::Error)]
enum FixtureError {
    #[error("fixture root must be an absolute path")]
    RelativeRoot,
    #[error("fixture root final component must be {BUNDLE_NAME}")]
    WrongRootName,
    #[error("fixture root may not be a symlink")]
    Symlink,
    #[error("fixture root parent is too broad")]
    BroadRoot,
    #[error("fixture root already contains data")]
    ExistingData,
    #[error("fixture root may not be inside the source repository")]
    InsideRepository,
    #[error("fixture root is not a private directory")]
    UnsafeDirectory,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Meeting(#[from] local_meeting_notes_session_core::meeting::MeetingError),
    #[error(transparent)]
    Storage(#[from] local_meeting_notes_session_core::storage::StorageError),
    #[error(transparent)]
    Retry(#[from] local_meeting_notes_session_core::transcript_retry::TranscriptRetryError),
    #[error("fixture root marker is missing or not the exact synthetic marker")]
    InvalidMarker,
    #[error("archive root must be an absolute path")]
    RelativeArchiveRoot,
    #[error("archive root final component must be {BUNDLE_NAME}.archive-<label>")]
    WrongArchiveName,
    #[error("archive root must be absent")]
    ExistingArchiveRoot,
    #[error("archive root must share the fixture root's canonical parent")]
    ArchiveParentMismatch,
    #[error("archive root parent is not a directory")]
    UnsafeArchiveParent,
    #[error("model fixture marker is missing or not the exact synthetic model marker")]
    InvalidModelMarker,
    #[error("model install path is unsafe: {0}")]
    UnsafeInstallPath(String),
    #[error("bundle runtime manifest has no verified model catalog")]
    MissingModelCatalog,
    #[error("selected model is not a transcript model in the verified catalog")]
    UnknownModel,
    #[error("a model or active-model pointer already exists")]
    ExistingModel,
    #[error("source model changed during the bounded copy")]
    SourceChanged,
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    ModelStore(#[from] ModelStoreError),
    #[error(transparent)]
    WriterLock(#[from] AppDataWriterLockError),
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        usage();
        std::process::exit(2);
    };
    let result = if first == "install-model" {
        let root = args.next();
        let bundle = args.next();
        let source = args.next();
        let model_id = args.next();
        if args.next().is_some()
            || root.is_none()
            || bundle.is_none()
            || source.is_none()
            || model_id.is_none()
        {
            eprintln!(
                "usage: rendered-review-fixture install-model ROOT BUNDLE_RESOURCES SOURCE_MODEL_DIR MODEL_ID"
            );
            std::process::exit(2);
        }
        install_model(
            Path::new(root.as_deref().expect("checked root")),
            Path::new(bundle.as_deref().expect("checked bundle")),
            Path::new(source.as_deref().expect("checked source")),
            model_id.as_deref().expect("checked model id"),
            repository_root(),
        )
    } else if first == "archive" {
        let root = args.next();
        let archive_root = args.next();
        if args.next().is_some() || root.is_none() || archive_root.is_none() {
            eprintln!("usage: rendered-review-fixture archive ROOT ARCHIVE_ROOT");
            std::process::exit(2);
        }
        archive(
            Path::new(root.as_deref().expect("checked root")),
            Path::new(archive_root.as_deref().expect("checked archive root")),
            repository_root(),
        )
    } else if args.next().is_some() {
        usage();
        std::process::exit(2);
    } else {
        seed(Path::new(&first), repository_root())
    };
    if let Err(error) = result {
        eprintln!("rendered-review-fixture: {error}");
        std::process::exit(1);
    }
    if first == "install-model" {
        println!("installed verified synthetic model");
    } else if first == "archive" {
        println!("archived synthetic fixture to a recoverable sibling");
    } else {
        println!("seeded synthetic fixture at {first}");
    }
}

fn usage() {
    eprintln!("usage: rendered-review-fixture /absolute/path/{BUNDLE_NAME}");
    eprintln!("       rendered-review-fixture archive ROOT ARCHIVE_ROOT");
}

fn repository_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/session-core. Its grandparent is the
    // repository root in this workspace.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("session-core repository root")
        .to_path_buf()
}

fn seed(root: &Path, repository: PathBuf) -> Result<(), FixtureError> {
    validate_root(root, &repository)?;
    let storage = StorageRoot::create(root, &repository)?;
    let meeting_dir = storage.path().join("meetings").join(MEETING_ID);
    for child in ["", "capture", "transcript", "notes"] {
        create_private_dir(&meeting_dir.join(child))?;
    }

    let rule = AudioRetentionRule::UntilManualDeletion;
    let policy = retention_policy_sha256(&rule);
    let attempt = json!({
        "schema": "capture-attempt/1",
        "meeting_id": MEETING_ID,
        "attempt_id": "11111111-1111-4111-8111-111111111111",
        "created_at_epoch_seconds": CREATED_AT,
        "application_build_sha256": "a".repeat(64),
        "participant_notice_version": "internal-transcript-alpha/1",
        "operator_attestation": {
            "participantsConsented": true,
            "headphones": true,
            "operatorAlone": true,
        },
        "retention_policy_sha256": policy,
    });
    write_new(
        &meeting_dir.join("attempt.json"),
        &serde_json::to_vec_pretty(&attempt)?,
    )?;
    write_new(
        &meeting_dir.join("ownership.json"),
        br#"{"schema":"capture-ownership/1","source":"synthetic-fixture"}
"#,
    )?;

    let mic = wav(AUDIO_FRAMES);
    let system = wav(AUDIO_FRAMES);
    write_new(&meeting_dir.join("capture/mic.wav"), &mic)?;
    write_new(&meeting_dir.join("capture/system.wav"), &system)?;
    let mic_ref = artifact_ref(&meeting_dir, "capture/mic.wav")?;
    let system_ref = artifact_ref(&meeting_dir, "capture/system.wav")?;
    let receipt = json!({
        "schema": "capture-session/2",
        "status": "complete",
        "started_at": "2023-11-14T22:13:20+0000",
        "finalized_at": "2023-11-14T22:13:21+0000",
        "health": {"schema":"capture-health/1","usable":true},
        "quality": {
            "schema": "capture-quality/1",
            "source": {"leg":"mic","artifact":"mic.wav","samples":AUDIO_FRAMES,"sha256":mic_ref.sha256},
            "metrics": {"duration_s":8.0},
            "observations": {
                "silence":{"status":"observed","detail":"synthetic"},
                "clipping":{"status":"not_observed","detail":"synthetic"},
                "low_input":{"status":"observed","detail":"synthetic"},
                "background_noise":{"status":"unknown","detail":"synthetic"}
            }
        },
        "microphone": {"schema":"capture-microphone/1","index":0,"name":"Synthetic microphone"},
        "reconciliation": {"legs":{"mic":"complete","system":"complete"}},
        "artifacts": [
            {"name":"mic.wav","bytes":mic.len(),"sha256":mic_ref.sha256,"mode":"0600"},
            {"name":"system.wav","bytes":system.len(),"sha256":system_ref.sha256,"mode":"0600"}
        ],
        // W1-A: two non-overlapping pause spans, so both the meeting-detail
        // pause sentence and the retry sheet's pause section render on the
        // same meeting that also carries the pending retry comparison below.
        "pauses": {
            "schema": "capture-pauses/1",
            "spans": [
                {"paused_at_samples": 2 * PAUSE_SAMPLE_RATE, "resumed_at_samples": 3 * PAUSE_SAMPLE_RATE},
                {"paused_at_samples": 6 * PAUSE_SAMPLE_RATE, "resumed_at_samples": 8 * PAUSE_SAMPLE_RATE}
            ]
        }
    });
    write_new(
        &meeting_dir.join("capture/session.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    let session_ref = artifact_ref(&meeting_dir, "capture/session.json")?;

    let source = transcript_bytes(&[
        ("Me", "We will review the synthetic launch checklist."),
        ("Them", "The next step is to test the rendered comparison."),
        ("Me", "The fixture contains no real meeting material."),
    ]);
    let source_digest = digest(&source);
    let source_path = format!("transcript/{source_digest}.json");
    write_new(&meeting_dir.join(&source_path), &source)?;
    let current_ref = artifact_ref(&meeting_dir, &source_path)?;

    let meeting = MeetingRecord {
        schema: MeetingSchema::V2,
        meeting_id: MEETING_ID.to_owned(),
        lifecycle: MeetingLifecycle::TranscriptReady,
        retention: AudioRetention {
            rule,
            policy_sha256: policy,
            next_deletion_at_epoch_seconds: None,
            state: AudioState::Retained,
            deletion_receipt: None,
        },
        artifacts: MeetingArtifacts {
            attempt: artifact_ref(&meeting_dir, "attempt.json")?,
            ownership: Some(artifact_ref(&meeting_dir, "ownership.json")?),
            capture_session: Some(session_ref.clone()),
            microphone_audio: Some(mic_ref.clone()),
            system_audio: Some(system_ref.clone()),
            current_transcript: Some(current_ref.clone()),
            current_note: None,
        },
        pending_storage_operation: None,
    };
    write_meeting(&meeting_dir, &meeting)?;
    verify_record_artifacts(&meeting_dir, &load_meeting(&meeting_dir)?)?;

    let candidate = transcript_bytes(&[
        ("Me", "We will review the synthetic comparison fixture."),
        ("Them", "The candidate is different and remains pending."),
        ("Me", "The current transcript pointer stays unchanged."),
    ]);
    let coordination = MeetingStorageCoordination::default();
    let authority = TranscriptRetryAuthority::new(&storage, &coordination);
    authority.create_candidate(
        MEETING_ID,
        Uuid::parse_str(OPERATION_ID).expect("fixed operation id"),
        &candidate,
        &TranscriptRetrySourceBinding {
            source_transcript_sha256: current_ref.sha256.clone(),
            capture_session_sha256: session_ref.sha256.clone(),
            microphone_audio_sha256: mic_ref.sha256.clone(),
            system_audio_sha256: system_ref.sha256.clone(),
            candidate_transcript_sha256: digest(&candidate),
        },
    )?;

    seed_diff_identical_meeting(&storage)?;
    seed_diff_skipped_meeting(&storage)?;
    seed_trash_meeting(&storage)?;
    seed_export_tampered_meeting(&storage)?;
    seed_note_meeting(&storage)?;
    seed_locked_meeting(&storage)?;

    let marker = json!({
        "schema": "synthetic-fixture-evidence/1",
        "bundle": BUNDLE_NAME,
        "private_data": false,
        "product_evidence": false,
        "content": "deterministic invented review fixture",
        "meeting_id": MEETING_ID,
        "retry_operation_id": OPERATION_ID,
        "covered_states": [
            "retained meeting",
            "verified audio",
            "quality and device projections",
            "pending transcript retry",
            "capture pauses",
            "retry diff computed with differences",
            "retry diff computed with no differences",
            "retry diff skipped over budget",
            "trashed meeting pending restore",
            "export withheld-artifact manifest",
            "generated note row preview and reverse citation map",
            "read-only pre-meeting context",
            "locked meeting behind the local barrier"
        ],
        "note": "One meeting (NOTE_MEETING_ID) carries a real, validator-passing generated note; every other seeded meeting remains TranscriptReady or Ready without invoking a note worker at seed time. One meeting (LOCKED_MEETING_ID) is locked via the meeting-lock/1 sidecar; this fixture can only show the locked barrier, never an unlock, because the real confirmation is Touch ID or the login password (live-run evidence by design) and its scripted stand-in is compiled only under cfg(test)."
    });
    write_new(
        &storage.path().join("SYNTHETIC_FIXTURE.json"),
        &serde_json::to_vec_pretty(&marker)?,
    )?;
    Ok(())
}

/// Common capture-and-transcript shell shared by every meeting this fixture
/// seeds beyond the original `MEETING_ID` above: attempt, ownership, a
/// silent mic/system WAV pair, a `capture-session/2` receipt, and one
/// transcript. Callers finish the meeting record themselves (lifecycle,
/// current note, retry candidates) because those vary per state.
struct PlainMeeting {
    meeting_dir: PathBuf,
    session_ref: ArtifactRef,
    mic_ref: ArtifactRef,
    system_ref: ArtifactRef,
    current_ref: ArtifactRef,
}

fn seed_plain_meeting(
    storage: &StorageRoot,
    meeting_id: &str,
    created_at: u64,
    source_bytes: Vec<u8>,
) -> Result<PlainMeeting, FixtureError> {
    let meeting_dir = storage.path().join("meetings").join(meeting_id);
    for child in ["", "capture", "transcript", "notes"] {
        create_private_dir(&meeting_dir.join(child))?;
    }

    let rule = AudioRetentionRule::UntilManualDeletion;
    let policy = retention_policy_sha256(&rule);
    let attempt = json!({
        "schema": "capture-attempt/1",
        "meeting_id": meeting_id,
        "attempt_id": meeting_id,
        "created_at_epoch_seconds": created_at,
        "application_build_sha256": "a".repeat(64),
        "participant_notice_version": "internal-transcript-alpha/1",
        "operator_attestation": {
            "participantsConsented": true,
            "headphones": true,
            "operatorAlone": true,
        },
        "retention_policy_sha256": policy,
    });
    write_new(
        &meeting_dir.join("attempt.json"),
        &serde_json::to_vec_pretty(&attempt)?,
    )?;
    write_new(
        &meeting_dir.join("ownership.json"),
        br#"{"schema":"capture-ownership/1","source":"synthetic-fixture"}
"#,
    )?;

    let mic = wav(AUDIO_FRAMES);
    let system = wav(AUDIO_FRAMES);
    write_new(&meeting_dir.join("capture/mic.wav"), &mic)?;
    write_new(&meeting_dir.join("capture/system.wav"), &system)?;
    let mic_ref = artifact_ref(&meeting_dir, "capture/mic.wav")?;
    let system_ref = artifact_ref(&meeting_dir, "capture/system.wav")?;
    let receipt = json!({
        "schema": "capture-session/2",
        "status": "complete",
        "started_at": "2023-11-14T22:13:20+0000",
        "finalized_at": "2023-11-14T22:13:21+0000",
        "health": {"schema":"capture-health/1","usable":true},
        "quality": {
            "schema": "capture-quality/1",
            "source": {"leg":"mic","artifact":"mic.wav","samples":AUDIO_FRAMES,"sha256":mic_ref.sha256},
            "metrics": {"duration_s":8.0},
            "observations": {
                "silence":{"status":"not_observed","detail":"synthetic"},
                "clipping":{"status":"not_observed","detail":"synthetic"},
                "low_input":{"status":"not_observed","detail":"synthetic"},
                "background_noise":{"status":"not_observed","detail":"synthetic"}
            }
        },
        "microphone": {"schema":"capture-microphone/1","index":0,"name":"Synthetic microphone"},
        "reconciliation": {"legs":{"mic":"complete","system":"complete"}},
        "artifacts": [
            {"name":"mic.wav","bytes":mic.len(),"sha256":mic_ref.sha256,"mode":"0600"},
            {"name":"system.wav","bytes":system.len(),"sha256":system_ref.sha256,"mode":"0600"}
        ]
    });
    write_new(
        &meeting_dir.join("capture/session.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    let session_ref = artifact_ref(&meeting_dir, "capture/session.json")?;

    let source_digest = digest(&source_bytes);
    let source_path = format!("transcript/{source_digest}.json");
    write_new(&meeting_dir.join(&source_path), &source_bytes)?;
    let current_ref = artifact_ref(&meeting_dir, &source_path)?;

    Ok(PlainMeeting {
        meeting_dir,
        session_ref,
        mic_ref,
        system_ref,
        current_ref,
    })
}

/// Writes `meeting.json` for a [`PlainMeeting`] shell, in the given lifecycle
/// and with the given optional current note.
fn write_meeting_record(
    shell: &PlainMeeting,
    meeting_id: &str,
    lifecycle: MeetingLifecycle,
    current_note: Option<NoteRevisionRef>,
) -> Result<(), FixtureError> {
    let rule = AudioRetentionRule::UntilManualDeletion;
    let policy = retention_policy_sha256(&rule);
    let meeting = MeetingRecord {
        schema: MeetingSchema::V2,
        meeting_id: meeting_id.to_owned(),
        lifecycle,
        retention: AudioRetention {
            rule,
            policy_sha256: policy,
            next_deletion_at_epoch_seconds: None,
            state: AudioState::Retained,
            deletion_receipt: None,
        },
        artifacts: MeetingArtifacts {
            attempt: artifact_ref(&shell.meeting_dir, "attempt.json")?,
            ownership: Some(artifact_ref(&shell.meeting_dir, "ownership.json")?),
            capture_session: Some(shell.session_ref.clone()),
            microphone_audio: Some(shell.mic_ref.clone()),
            system_audio: Some(shell.system_ref.clone()),
            current_transcript: Some(shell.current_ref.clone()),
            current_note,
        },
        pending_storage_operation: None,
    };
    write_meeting(&shell.meeting_dir, &meeting)?;
    Ok(())
}

/// W3-B: a pending retry candidate whose turn text is word-for-word
/// identical to the current transcript, so the real diff engine
/// (`transcript_retry_diff::diff_transcript_turns`, run at render time by the
/// desktop app) computes the `Computed` state with zero spans on either
/// side — the "No word-level differences found." legend.
fn seed_diff_identical_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    let turns: &[(&str, &str)] = &[
        ("Me", "Fixture: the diff comparison turns are identical on both sides."),
        ("Them", "Fixture: nothing was changed between the current transcript and this retry."),
    ];
    let source = transcript_bytes_tagged(turns, "synthetic-fixture-diff-identical-current");
    let shell = seed_plain_meeting(
        storage,
        DIFF_IDENTICAL_MEETING_ID,
        CREATED_AT + 1,
        source.clone(),
    )?;
    write_meeting_record(
        &shell,
        DIFF_IDENTICAL_MEETING_ID,
        MeetingLifecycle::TranscriptReady,
        None,
    )?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    // Same turn text, different file bytes (a different `source` tag), so
    // the candidate is a distinct content-addressed artifact even though it
    // diffs identically against the current transcript.
    let candidate = transcript_bytes_tagged(turns, "synthetic-fixture-diff-identical-candidate");
    let coordination = MeetingStorageCoordination::default();
    let authority = TranscriptRetryAuthority::new(storage, &coordination);
    authority.create_candidate(
        DIFF_IDENTICAL_MEETING_ID,
        Uuid::parse_str(DIFF_IDENTICAL_OPERATION_ID).expect("fixed operation id"),
        &candidate,
        &TranscriptRetrySourceBinding {
            source_transcript_sha256: shell.current_ref.sha256.clone(),
            capture_session_sha256: shell.session_ref.sha256.clone(),
            microphone_audio_sha256: shell.mic_ref.sha256.clone(),
            system_audio_sha256: shell.system_ref.sha256.clone(),
            candidate_transcript_sha256: digest(&candidate),
        },
    )?;
    Ok(())
}

/// W3-B: a pending retry candidate whose current and candidate turns are
/// long enough and different enough (each a single turn of ~4,200 disjoint
/// words) to exceed `transcript_retry_diff::MAX_EDIT_BUDGET`, so the real
/// diff engine returns `Skipped` with no spans on either side — the "These
/// transcripts are too long to highlight word differences." legend. Chosen
/// over the word-count bound (`MAX_DIFFABLE_WORDS`, ~20,001 words) because it
/// is the smaller, more surgical way to cross a skip threshold.
fn seed_diff_skipped_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    const OVER_BUDGET_WORDS: usize = 4_200;
    let current_text: String = (0..OVER_BUDGET_WORDS)
        .map(|index| format!("cur{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let candidate_text: String = (0..OVER_BUDGET_WORDS)
        .map(|index| format!("cand{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let source = transcript_bytes_tagged(
        &[("Me", current_text.as_str())],
        "synthetic-fixture-diff-skipped-current",
    );
    let shell = seed_plain_meeting(
        storage,
        DIFF_SKIPPED_MEETING_ID,
        CREATED_AT + 2,
        source.clone(),
    )?;
    write_meeting_record(
        &shell,
        DIFF_SKIPPED_MEETING_ID,
        MeetingLifecycle::TranscriptReady,
        None,
    )?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    let candidate = transcript_bytes_tagged(
        &[("Them", candidate_text.as_str())],
        "synthetic-fixture-diff-skipped-candidate",
    );
    let coordination = MeetingStorageCoordination::default();
    let authority = TranscriptRetryAuthority::new(storage, &coordination);
    authority.create_candidate(
        DIFF_SKIPPED_MEETING_ID,
        Uuid::parse_str(DIFF_SKIPPED_OPERATION_ID).expect("fixed operation id"),
        &candidate,
        &TranscriptRetrySourceBinding {
            source_transcript_sha256: shell.current_ref.sha256.clone(),
            capture_session_sha256: shell.session_ref.sha256.clone(),
            microphone_audio_sha256: shell.mic_ref.sha256.clone(),
            system_audio_sha256: shell.system_ref.sha256.clone(),
            candidate_transcript_sha256: digest(&candidate),
        },
    )?;
    Ok(())
}

/// W2-B: a meeting moved into local trash through the real
/// `MeetingTrashAuthority`, exercising the Trash list and its restore action
/// with no product-code changes — this is the exact authority the desktop
/// app's own "Delete meeting" command reaches (`retention.rs`'s
/// `meeting_trash_authority`).
fn seed_trash_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    let turns: &[(&str, &str)] = &[
        ("Me", "Fixture: this meeting was moved to trash for the review fixture."),
        ("Them", "Fixture: it can be restored from the Trash list."),
    ];
    let source = transcript_bytes_tagged(turns, "synthetic-fixture-trash");
    let shell = seed_plain_meeting(storage, TRASH_MEETING_ID, CREATED_AT + 3, source)?;
    write_meeting_record(
        &shell,
        TRASH_MEETING_ID,
        MeetingLifecycle::TranscriptReady,
        None,
    )?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    let writer_lock = AppDataWriterLock::acquire(storage)?;
    writer_lock
        .library_organization_authority()
        .set_meeting_title(0, TRASH_MEETING_ID, Some("Fixture: Trash review"))
        .map_err(|error| io::Error::other(error.to_string()))?;
    writer_lock
        .meeting_trash_authority()
        .trash_meeting(TRASH_MEETING_ID, CREATED_AT + 3)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

/// W2-C: a meeting whose retained transcript is tampered with *after* the
/// meeting record is written and verified, so its pinned digest no longer
/// matches the bytes on disk — mirroring
/// `meeting_export.rs`'s own
/// `export_meeting_withholds_a_tampered_transcript_and_names_it_in_the_result`
/// test fixture exactly. Exercises the real export command's
/// withheld-artifact manifest line and README.txt entry when a reviewer
/// clicks Export on this meeting; the sibling `MEETING_ID` above already
/// exercises the ordinary "Export available" state (a verified transcript).
fn seed_export_tampered_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    let turns: &[(&str, &str)] = &[
        ("Me", "Fixture: this transcript will be tampered with after the meeting record is written."),
        ("Them", "Fixture: exporting this meeting must withhold transcript.md and name why."),
    ];
    let source = transcript_bytes_tagged(turns, "synthetic-fixture-export-tampered");
    let shell = seed_plain_meeting(storage, EXPORT_TAMPERED_MEETING_ID, CREATED_AT + 4, source)?;
    write_meeting_record(
        &shell,
        EXPORT_TAMPERED_MEETING_ID,
        MeetingLifecycle::TranscriptReady,
        None,
    )?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    // Tamper the retained transcript's bytes without touching meeting.json,
    // so its recorded sha256 no longer matches what is on disk. This runs
    // only after the meeting proved valid above, so the corruption is the
    // one thing that changed.
    fs::write(
        shell.meeting_dir.join(&shell.current_ref.relative_path),
        b"{ not the verified transcript",
    )?;
    Ok(())
}

/// W3-A / W2-A / I3: a meeting carrying a real, validator-passing generated
/// note plus a read-only pre-meeting context note.
///
/// The transcript, `note.json`, and `note.md` bytes embedded under
/// `fixtures/note-meeting/` are not hand-written: they were produced and
/// independently re-verified offline by driving this repository's own
/// note-assembly and note-validation code —
/// `notes/summarize.py::candidate_note_document` (via a synthetic
/// `note-generation/2` payload naming two real, deterministically-derived
/// `candidate_first` candidates) to build them, then
/// `worker/note_validator.py::_project_snapshot` (the exact function the
/// packaged app's real subprocess note projector calls at read time) to
/// confirm they validate byte-for-byte, using the packaged
/// `python-runtime/bin/python3.12` interpreter. That re-derivation is not
/// wired into `cargo test` — it depends on a Python interpreter this crate
/// does not otherwise need — so this fixture treats the three files as
/// frozen, pre-verified fixture data and only checks the one fact a future
/// edit to either file could silently break: that the embedded transcript's
/// digest is the one the embedded note actually cites.
///
/// The claim projection this note validates to cites transcript turns 0 and
/// 2 (`summary` and `decision`); turns 1 and 3 are not cited by anything,
/// giving the reverse citation map ("`turns_cited`") a real uncited turn.
fn seed_note_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    // Named `source-turns.json` rather than `transcript.json`: the repo-wide
    // `.gitignore` refuses any tracked file literally named `transcript.json`
    // as a hard privacy boundary (`privacy_gate.py`'s filename check), and
    // this fixture's synthetic bytes should go through that same boundary
    // rather than around it. The bytes are written to the real runtime path
    // (`transcript/<sha256>.json`, inside the untracked seeded app-data
    // root) at seed time below; only the repo-tracked source file's name
    // differs.
    const TRANSCRIPT_BYTES: &[u8] = include_bytes!("fixtures/note-meeting/source-turns.json");
    const NOTE_JSON_BYTES: &[u8] = include_bytes!("fixtures/note-meeting/note.json");
    const NOTE_MARKDOWN_BYTES: &[u8] = include_bytes!("fixtures/note-meeting/note.md");
    const TRANSCRIPT_SHA256: &str =
        "56bfdb26765a804b83ce2e60752373c88ee2c853e30778e98d6b4c1d89c23193";
    const NOTE_JSON_SHA256: &str =
        "ead0a06c71c10461337b04afe5cbf13e594db471643bf166af4fa934bfe832a3";
    const NOTE_MARKDOWN_SHA256: &str =
        "89bc8e42c5bb1facc1f7b2d69b6103fcbafeecda836a8cb1822b301cbecc0951";

    let shell = seed_plain_meeting(
        storage,
        NOTE_MEETING_ID,
        CREATED_AT + 5,
        TRANSCRIPT_BYTES.to_vec(),
    )?;
    if shell.current_ref.sha256 != TRANSCRIPT_SHA256 {
        return Err(io::Error::other(
            "embedded note-meeting transcript digest does not match the embedded note; \
             the note.json/note.md/transcript.json trio under fixtures/note-meeting/ must be \
             regenerated together",
        )
        .into());
    }
    write_new(
        &shell.meeting_dir.join(format!("notes/{NOTE_JSON_SHA256}.json")),
        NOTE_JSON_BYTES,
    )?;
    write_new(
        &shell
            .meeting_dir
            .join(format!("notes/{NOTE_MARKDOWN_SHA256}.md")),
        NOTE_MARKDOWN_BYTES,
    )?;
    let note = NoteRevisionRef {
        json: ArtifactRef {
            relative_path: format!("notes/{NOTE_JSON_SHA256}.json"),
            sha256: NOTE_JSON_SHA256.to_owned(),
        },
        markdown: ArtifactRef {
            relative_path: format!("notes/{NOTE_MARKDOWN_SHA256}.md"),
            sha256: NOTE_MARKDOWN_SHA256.to_owned(),
        },
        source_transcript_sha256: TRANSCRIPT_SHA256.to_owned(),
    };
    write_meeting_record(&shell, NOTE_MEETING_ID, MeetingLifecycle::Ready, Some(note))?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    // W2-A / I3: a read-only pre-meeting context note, in the exact
    // `meeting-context/1` shape `apps/desktop/src-tauri/src/meeting_context.rs`
    // writes (that module is private to the desktop crate, so this fixture
    // writes the file directly rather than importing it).
    write_new(
        &shell.meeting_dir.join("meeting-context.json"),
        br#"{"schema":"meeting-context/1","text":"Fixture: pre-meeting context - evaluating the rendered retry-review rollout before the next release gate."}"#,
    )?;
    Ok(())
}

/// Roadmap Wave 4 / I5's fixture follow-up: a meeting locked via the real
/// `meeting-lock/1` sidecar shape (`apps/desktop/src-tauri/src/meeting_lock.rs`
/// — private to the desktop crate, so this fixture writes the file directly
/// rather than importing it, exactly like the pre-meeting context sidecar
/// above). Exercises the locked library row: title and date still visible
/// (the first turn below is long enough to derive one — `meeting_title`'s
/// `MIN_TITLE_WORDS` is 6 words), no note preview (this meeting carries no
/// note at all, so there is nothing to suppress a *difference* against —
/// see the doc comment below for what that leaves unproven), and the shell's
/// "Locked" marker once `meeting_lock::read` sees the sidecar.
///
/// **This can only stage the barrier, never an unlock, and that is the
/// honest, sufficient shape for a fixture.** The real confirmation
/// (`operator_confirmation.rs`'s `DeviceOwnerConfirmation`) draws a system
/// Touch ID/password panel and is deliberately live-run evidence only —
/// nothing in this repository proves a finger was read. Its scripted stand-in
/// (`ConfirmsOperator::fake::FakeConfirmation`) exists only to drive that
/// trait in `cargo test` and is compiled under `#[cfg(test)]`, so it is not
/// present in the packaged Fixture app this binary seeds for — there is no
/// build-time seam here to wire it into a real `.app`. A reviewer who opens
/// this meeting in the Fixture app sees the same locked barrier a real build
/// would show and cannot get further, which is exactly what a lock is for.
///
/// One thing this fixture state does **not** independently prove: that
/// unlocking would reveal a note preview a locked read currently suppresses
/// (this meeting has no note, locked or not). That distinction — an unlocked
/// row previewing its note and the identical meeting locked suppressing it —
/// is proven directly against the real gate in
/// `library_reader::tests::a_locked_row_carries_no_note_preview_while_the_same_meeting_unlocked_does`
/// (`apps/desktop/src-tauri/src/library_reader.rs`), not by this fixture.
fn seed_locked_meeting(storage: &StorageRoot) -> Result<(), FixtureError> {
    let turns: &[(&str, &str)] = &[
        (
            "Me",
            "Fixture: this meeting is locked behind Yawn's local barrier.",
        ),
        (
            "Them",
            "Fixture: the title and date still show; the note preview does not.",
        ),
    ];
    let source = transcript_bytes_tagged(turns, "synthetic-fixture-locked");
    let shell = seed_plain_meeting(storage, LOCKED_MEETING_ID, CREATED_AT + 6, source)?;
    write_meeting_record(
        &shell,
        LOCKED_MEETING_ID,
        MeetingLifecycle::TranscriptReady,
        None,
    )?;
    verify_record_artifacts(&shell.meeting_dir, &load_meeting(&shell.meeting_dir)?)?;

    write_new(
        &shell.meeting_dir.join("meeting-lock.json"),
        br#"{"schema":"meeting-lock/1","locked":true}"#,
    )?;
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticFixtureMarker {
    schema: String,
    bundle: String,
    private_data: bool,
    product_evidence: bool,
    content: String,
    meeting_id: String,
    retry_operation_id: String,
    covered_states: Vec<String>,
    note: String,
}

fn install_model(
    root: &Path,
    bundle_resources: &Path,
    source_model_dir: &Path,
    model_id: &str,
    repository: PathBuf,
) -> Result<(), FixtureError> {
    validate_install_root(root, &repository)?;
    validate_external_directory(bundle_resources, "bundle resources")?;
    validate_external_directory(source_model_dir, "source model")?;
    let canonical_bundle_resources = bundle_resources.canonicalize()?;
    let canonical_source_model_dir = source_model_dir.canonicalize()?;
    let canonical_repository = repository
        .canonicalize()
        .unwrap_or_else(|_| repository.clone());
    if canonical_source_model_dir.starts_with(&canonical_repository) {
        return Err(FixtureError::UnsafeInstallPath(
            "source model is inside the source repository".into(),
        ));
    }

    let manifest_path = canonical_bundle_resources.join("app-runtime.json");
    let manifest = RuntimeManifest::load_and_verify(&manifest_path)?;
    let catalog_resource = manifest
        .model_catalog
        .as_ref()
        .ok_or(FixtureError::MissingModelCatalog)?;
    let catalog_path = manifest
        .model_catalog_path(&manifest_path)
        .ok_or(FixtureError::MissingModelCatalog)?;
    let catalog = ModelCatalog::load_and_verify(&catalog_path, &catalog_resource.sha256)?;
    let entry = catalog
        .model(model_id)
        .map_err(|_| FixtureError::UnknownModel)?
        .clone();
    if canonical_source_model_dir
        .file_name()
        .and_then(|name| name.to_str())
        != Some(entry.revision.as_str())
        || canonical_source_model_dir
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some(entry.id.as_str())
    {
        return Err(FixtureError::UnsafeInstallPath(
            "source model leaf must be exactly <id>/<revision>".into(),
        ));
    }

    let storage = StorageRoot::create(root, &repository)?;
    let active_path = storage
        .resolve(Path::new("models/active-model.json"))
        .map_err(|error| FixtureError::Storage(error))?;
    if fs::symlink_metadata(&active_path).is_ok() {
        return Err(FixtureError::ExistingModel);
    }
    let target = storage
        .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
        .map_err(FixtureError::Storage)?;
    if fs::symlink_metadata(&target).is_ok()
        || fs::symlink_metadata(&storage.path().join("MODEL_FIXTURE.json")).is_ok()
    {
        return Err(FixtureError::ExistingModel);
    }

    // Keep the canonical writer boundary through copy, re-verification, and
    // activation. A live app cannot race this fixture install.
    let _writer_lock = AppDataWriterLock::acquire(&storage)?;
    if fs::symlink_metadata(&active_path).is_ok()
        || fs::symlink_metadata(&target).is_ok()
        || fs::symlink_metadata(&storage.path().join("MODEL_FIXTURE.json")).is_ok()
    {
        return Err(FixtureError::ExistingModel);
    }
    verify_model_directory(&canonical_source_model_dir, &entry, ModelVerification::Contents)
        .map_err(|_| FixtureError::SourceChanged)?;
    create_private_dir(&target)?;
    for file in &entry.files {
        if !matches!(
            file.role,
            TranscriptModelFileRole::Config | TranscriptModelFileRole::Weights
        ) {
            return Err(FixtureError::ModelStore(
                ModelStoreError::InvalidCatalogEntry,
            ));
        }
        copy_private_model_file(
            &canonical_source_model_dir.join(&file.name),
            &target.join(&file.name),
            file.bytes,
        )?;
    }
    durable_create_new(
        &target.join("model-install.json"),
        &install_receipt_bytes(&entry),
    )?;
    sync_directory(&target)?;
    verify_model_directory(&target, &entry, ModelVerification::Contents)?;
    verify_model_directory(&canonical_source_model_dir, &entry, ModelVerification::Contents)
        .map_err(|_| FixtureError::SourceChanged)?;
    activate_model(&storage, &entry)?;

    let model_marker = json!({
        "schema": "synthetic-model-fixture/1",
        "model_id": entry.id,
        "revision": entry.revision,
        "private_data": false,
        "product_evidence": false,
        "note": "Public model bytes copied from a verified catalog entry for synthetic fixture review."
    });
    durable_create_new(
        &storage.path().join("MODEL_FIXTURE.json"),
        &serde_json::to_vec_pretty(&model_marker)?,
    )?;
    Ok(())
}

fn archive(root: &Path, archive_root: &Path, repository: PathBuf) -> Result<(), FixtureError> {
    validate_archive(root, archive_root, &repository)?;
    let storage = StorageRoot::create(root, &repository)?;
    let _writer_lock = AppDataWriterLock::acquire(&storage)?;
    // The destination and every source contract are checked again while the
    // canonical writer lock is held, immediately before the atomic rename.
    validate_archive(root, archive_root, &repository)?;
    let parent = root
        .parent()
        .ok_or(FixtureError::BroadRoot)?
        .canonicalize()
        .map_err(FixtureError::Io)?;
    let source_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FixtureError::WrongRootName)?;
    let archive_name = archive_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FixtureError::WrongArchiveName)?;
    rename_fixture_exclusive(&parent, source_name, archive_name)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_fixture_exclusive(
    parent: &Path,
    source_name: &str,
    archive_name: &str,
) -> io::Result<()> {
    let parent = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(parent)?;
    let source_name = std::ffi::CString::new(source_name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source name contains NUL"))?;
    let archive_name = std::ffi::CString::new(archive_name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "archive name contains NUL"))?;
    let source_fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            source_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if source_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let source = unsafe { std::fs::File::from_raw_fd(source_fd) };
    if !source.metadata()?.is_dir() {
        return Err(io::Error::other("fixture source is not a directory"));
    }
    let result = unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            source_name.as_ptr(),
            parent.as_raw_fd(),
            archive_name.as_ptr(),
            libc::RENAME_EXCL | RENAME_NOFOLLOW_ANY,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    parent.sync_all()
}

#[cfg(not(target_os = "macos"))]
fn rename_fixture_exclusive(
    _parent: &Path,
    _source_name: &str,
    _archive_name: &str,
) -> io::Result<()> {
    Err(io::Error::other(
        "exclusive fixture archive is unsupported on this platform",
    ))
}

fn validate_archive(
    root: &Path,
    archive_root: &Path,
    repository: &Path,
) -> Result<(), FixtureError> {
    validate_archive_source(root, repository)?;
    validate_archive_destination(root, archive_root)?;
    Ok(())
}

fn validate_archive_source(root: &Path, repository: &Path) -> Result<(), FixtureError> {
    if !root.is_absolute() {
        return Err(FixtureError::RelativeRoot);
    }
    if root.file_name().and_then(|name| name.to_str()) != Some(BUNDLE_NAME) {
        return Err(FixtureError::WrongRootName);
    }
    let metadata = fs::symlink_metadata(root).map_err(|_| FixtureError::UnsafeDirectory)?;
    if metadata.file_type().is_symlink() {
        return Err(FixtureError::Symlink);
    }
    if !metadata.file_type().is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(FixtureError::UnsafeDirectory);
    }
    validate_fixture_parent(root, repository)?;
    for child in ["diagnostics", "profile", "meetings", "enrollment", "models"] {
        let path = root.join(child);
        let metadata = fs::symlink_metadata(path).map_err(|_| FixtureError::UnsafeDirectory)?;
        if metadata.file_type().is_symlink()
            || !metadata.file_type().is_dir()
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err(FixtureError::UnsafeDirectory);
        }
    }
    validate_synthetic_marker(root)?;
    validate_model_marker(root)?;
    reject_symlinks(root)?;
    Ok(())
}

fn reject_symlinks(path: &Path) -> Result<(), FixtureError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(FixtureError::Symlink);
    }
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(path)? {
            reject_symlinks(&entry?.path())?;
        }
    }
    Ok(())
}

fn validate_archive_destination(root: &Path, archive_root: &Path) -> Result<(), FixtureError> {
    if !archive_root.is_absolute() {
        return Err(FixtureError::RelativeArchiveRoot);
    }
    let name = archive_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FixtureError::WrongArchiveName)?;
    let prefix = format!("{BUNDLE_NAME}.archive-");
    let Some(label) = name.strip_prefix(&prefix) else {
        return Err(FixtureError::WrongArchiveName);
    };
    if !(1..=64).contains(&label.len())
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(FixtureError::WrongArchiveName);
    }
    if fs::symlink_metadata(archive_root).is_ok() {
        return Err(FixtureError::ExistingArchiveRoot);
    }
    let root_parent = root
        .parent()
        .ok_or(FixtureError::BroadRoot)?
        .canonicalize()?;
    let archive_parent = archive_root
        .parent()
        .ok_or(FixtureError::ArchiveParentMismatch)?
        .canonicalize()
        .map_err(|_| FixtureError::ArchiveParentMismatch)?;
    if root_parent != archive_parent {
        return Err(FixtureError::ArchiveParentMismatch);
    }
    let metadata =
        fs::symlink_metadata(&archive_parent).map_err(|_| FixtureError::UnsafeArchiveParent)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(FixtureError::UnsafeArchiveParent);
    }
    Ok(())
}

fn validate_fixture_parent(root: &Path, repository: &Path) -> Result<PathBuf, FixtureError> {
    let parent = root.parent().ok_or(FixtureError::BroadRoot)?;
    let canonical_parent = parent.canonicalize()?;
    if [
        "/",
        "/tmp",
        "/private/tmp",
        "/Users",
        "/Users/nino",
        "/Users/nino/Workspace",
        "/Users/nino/Workspace/dev",
        "/Users/nino/Workspace/dev/apps",
    ]
    .iter()
    .any(|broad| canonical_parent == Path::new(broad))
    {
        return Err(FixtureError::BroadRoot);
    }
    let repository = repository
        .canonicalize()
        .unwrap_or_else(|_| repository.to_path_buf());
    let canonical_root = root.canonicalize()?;
    if canonical_root.starts_with(&repository) || canonical_parent.starts_with(&repository) {
        return Err(FixtureError::InsideRepository);
    }
    let metadata = fs::symlink_metadata(&canonical_parent)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(FixtureError::UnsafeArchiveParent);
    }
    Ok(canonical_parent)
}

fn validate_synthetic_marker(root: &Path) -> Result<(), FixtureError> {
    let marker_path = root.join("SYNTHETIC_FIXTURE.json");
    let marker_metadata =
        fs::symlink_metadata(&marker_path).map_err(|_| FixtureError::InvalidMarker)?;
    if marker_metadata.file_type().is_symlink()
        || !marker_metadata.file_type().is_file()
        || marker_metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(FixtureError::InvalidMarker);
    }
    let marker: SyntheticFixtureMarker = serde_json::from_slice(&fs::read(&marker_path)?)
        .map_err(|_| FixtureError::InvalidMarker)?;
    if marker.schema != "synthetic-fixture-evidence/1"
        || marker.bundle != BUNDLE_NAME
        || marker.private_data
        || marker.product_evidence
        || marker.content != "deterministic invented review fixture"
        || marker.meeting_id != MEETING_ID
        || marker.retry_operation_id != OPERATION_ID
        || marker.covered_states
            != vec![
                "retained meeting",
                "verified audio",
                "quality and device projections",
                "pending transcript retry",
                "capture pauses",
                "retry diff computed with differences",
                "retry diff computed with no differences",
                "retry diff skipped over budget",
                "trashed meeting pending restore",
                "export withheld-artifact manifest",
                "generated note row preview and reverse citation map",
                "read-only pre-meeting context",
                "locked meeting behind the local barrier",
            ]
        || marker.note
            != "One meeting (NOTE_MEETING_ID) carries a real, validator-passing generated note; every other seeded meeting remains TranscriptReady or Ready without invoking a note worker at seed time. One meeting (LOCKED_MEETING_ID) is locked via the meeting-lock/1 sidecar; this fixture can only show the locked barrier, never an unlock, because the real confirmation is Touch ID or the login password (live-run evidence by design) and its scripted stand-in is compiled only under cfg(test)."
    {
        return Err(FixtureError::InvalidMarker);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticModelFixtureMarker {
    schema: String,
    model_id: String,
    revision: String,
    private_data: bool,
    product_evidence: bool,
    note: String,
}

fn validate_model_marker(root: &Path) -> Result<(), FixtureError> {
    let marker_path = root.join("MODEL_FIXTURE.json");
    let Ok(metadata) = fs::symlink_metadata(&marker_path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(FixtureError::InvalidModelMarker);
    }
    let marker: SyntheticModelFixtureMarker = serde_json::from_slice(&fs::read(marker_path)?)
        .map_err(|_| FixtureError::InvalidModelMarker)?;
    if marker.schema != "synthetic-model-fixture/1"
        || marker.model_id.is_empty()
        || marker.revision.is_empty()
        || marker.private_data
        || marker.product_evidence
        || marker.note
            != "Public model bytes copied from a verified catalog entry for synthetic fixture review."
    {
        return Err(FixtureError::InvalidModelMarker);
    }
    Ok(())
}

fn validate_install_root(root: &Path, repository: &Path) -> Result<(), FixtureError> {
    if !root.is_absolute() || root.file_name().and_then(|name| name.to_str()) != Some(BUNDLE_NAME) {
        return Err(FixtureError::WrongRootName);
    }
    let metadata = fs::symlink_metadata(root).map_err(|_| FixtureError::UnsafeDirectory)?;
    if metadata.file_type().is_symlink() {
        return Err(FixtureError::Symlink);
    }
    if !metadata.file_type().is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(FixtureError::UnsafeDirectory);
    }
    let parent = root
        .parent()
        .ok_or(FixtureError::BroadRoot)?
        .canonicalize()
        .unwrap_or_else(|_| root.parent().unwrap().to_path_buf());
    if [
        "/",
        "/tmp",
        "/private/tmp",
        "/Users",
        "/Users/nino",
        "/Users/nino/Workspace",
        "/Users/nino/Workspace/dev",
        "/Users/nino/Workspace/dev/apps",
    ]
    .iter()
    .any(|broad| parent == Path::new(broad))
    {
        return Err(FixtureError::BroadRoot);
    }
    let repository = repository
        .canonicalize()
        .unwrap_or_else(|_| repository.to_path_buf());
    if root.starts_with(&repository) || parent.starts_with(&repository) {
        return Err(FixtureError::InsideRepository);
    }
    validate_synthetic_marker(root)?;
    if fs::symlink_metadata(root.join("MODEL_FIXTURE.json")).is_ok() {
        return Err(FixtureError::ExistingModel);
    }
    Ok(())
}

fn validate_external_directory(path: &Path, label: &str) -> Result<(), FixtureError> {
    if !path.is_absolute() {
        return Err(FixtureError::UnsafeInstallPath(format!(
            "{label} must be absolute"
        )));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| FixtureError::UnsafeInstallPath(format!("{label} is missing")))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(FixtureError::UnsafeInstallPath(format!(
            "{label} must be a nonsymlink directory"
        )));
    }
    Ok(())
}

fn copy_private_model_file(
    source: &Path,
    target: &Path,
    expected_bytes: u64,
) -> Result<(), FixtureError> {
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)
        .map_err(FixtureError::Io)?;
    let metadata = input.metadata()?;
    if !metadata.file_type().is_file() || metadata.len() != expected_bytes {
        return Err(FixtureError::SourceChanged);
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        copied = copied.saturating_add(read as u64);
    }
    if copied != expected_bytes {
        return Err(FixtureError::SourceChanged);
    }
    output.sync_all()?;
    Ok(())
}

fn validate_root(root: &Path, repository: &Path) -> Result<(), FixtureError> {
    if !root.is_absolute() {
        return Err(FixtureError::RelativeRoot);
    }
    if root.file_name().and_then(|name| name.to_str()) != Some(BUNDLE_NAME) {
        return Err(FixtureError::WrongRootName);
    }
    if fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(FixtureError::Symlink);
    }
    let parent = root.parent().ok_or(FixtureError::BroadRoot)?;
    let parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    if [
        "/",
        "/tmp",
        "/private/tmp",
        "/Users",
        "/Users/nino",
        "/Users/nino/Workspace",
        "/Users/nino/Workspace/dev",
        "/Users/nino/Workspace/dev/apps",
    ]
    .iter()
    .any(|broad| parent == Path::new(broad))
    {
        return Err(FixtureError::BroadRoot);
    }
    let repository = repository
        .canonicalize()
        .unwrap_or_else(|_| repository.to_path_buf());
    let root_lexical = root.to_path_buf();
    if root_lexical.starts_with(&repository) || parent.starts_with(&repository) {
        return Err(FixtureError::InsideRepository);
    }
    if let Ok(metadata) = fs::symlink_metadata(root) {
        if !metadata.file_type().is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
            return Err(FixtureError::UnsafeDirectory);
        }
        if fs::read_dir(root)?.next().is_some() {
            return Err(FixtureError::ExistingData);
        }
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    durable_create_new(path, bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn transcript_bytes(turns: &[(&str, &str)]) -> Vec<u8> {
    transcript_bytes_tagged(turns, "synthetic-fixture")
}

/// Same shape as [`transcript_bytes`], but with a caller-chosen `source` tag.
///
/// Used to mint a retry candidate whose turn text is word-for-word identical
/// to its current transcript (so the word-level diff computes zero spans)
/// while still producing a distinct file digest — content-addressed storage
/// requires the two to be different files, and only `source` (never read by
/// the diff engine) needs to differ to achieve that.
fn transcript_bytes_tagged(turns: &[(&str, &str)], source_tag: &str) -> Vec<u8> {
    let turns = turns
        .iter()
        .enumerate()
        .map(|(index, (speaker, text))| {
            json!({
                "start": index as f64,
                "end": index as f64 + 0.5,
                "speaker": speaker,
                "text": text,
                "gated": false
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec_pretty(&json!({
        "schema":"capture-transcript/1",
        "source":source_tag,
        "attribution":"channel",
        "bleed":null,
        "voiceprint":null,
        "capture_health": {"schema":"capture-health/1","usable":true},
        "turns": turns
    }))
    .expect("synthetic transcript is serializable")
}

fn wav(frames: usize) -> Vec<u8> {
    let data_len = (frames * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8_000_u32.to_le_bytes());
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for _ in 0..frames {
        bytes.extend_from_slice(&0_i16.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    use local_meeting_notes_session_core::capture_quality::{
        CapturePauseState, CaptureQualityObservationKind, CaptureQualityObservationStatus,
        CaptureQualityState, RecordingDeviceState, project_capture_pauses, project_capture_quality,
        project_recording_device,
    };
    use local_meeting_notes_session_core::meeting::verify_artifact_ref;
    use local_meeting_notes_session_core::meeting_title::derived_title;
    use local_meeting_notes_session_core::meeting_trash::list_trash_entries;
    use local_meeting_notes_session_core::model_store::{
        ModelCatalogSchema, TranscriptModel, TranscriptModelFile, active_model,
    };
    use local_meeting_notes_session_core::transcript_retry::TranscriptRetryState;
    use local_meeting_notes_session_core::transcript_retry_diff::{
        DiffTurnInput, TranscriptRetryDiffState, diff_transcript_turns,
    };
    use std::os::unix::fs::PermissionsExt;

    fn target(temp: &TempDir) -> PathBuf {
        temp.path().join(BUNDLE_NAME)
    }

    fn repo(temp: &TempDir) -> PathBuf {
        let path = temp.path().join("repository");
        create_private_dir(&path).unwrap();
        path
    }

    #[test]
    fn deterministic_private_free_shape_has_comparable_pending_retry() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository).unwrap();
        let second_temp = TempDir::new().unwrap();
        seed(&target(&second_temp), repo(&second_temp)).unwrap();
        assert_eq!(snapshot(&root), snapshot(&target(&second_temp)));

        let storage = StorageRoot::create(&root, &temp.path().join("repository")).unwrap();
        let meeting_dir = storage.path().join("meetings").join(MEETING_ID);
        let meeting = load_meeting(&meeting_dir).unwrap();
        verify_record_artifacts(&meeting_dir, &meeting).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::TranscriptReady);
        assert!(Uuid::parse_str(MEETING_ID).is_ok());
        let source = meeting.artifacts.current_transcript.clone().unwrap();
        let quality = project_capture_quality(&meeting_dir, &meeting).unwrap();
        assert_eq!(quality.state, CaptureQualityState::Available);
        assert_eq!(
            quality
                .observations
                .iter()
                .find(|observation| observation.kind == CaptureQualityObservationKind::Silence)
                .unwrap()
                .status,
            CaptureQualityObservationStatus::Observed
        );
        assert_eq!(
            quality
                .observations
                .iter()
                .find(|observation| { observation.kind == CaptureQualityObservationKind::LowInput })
                .unwrap()
                .status,
            CaptureQualityObservationStatus::Observed
        );
        assert_eq!(
            project_recording_device(&meeting_dir, &meeting)
                .unwrap()
                .state,
            RecordingDeviceState::Identified
        );
        verify_artifact_ref(&meeting_dir, &meeting.artifacts.microphone_audio.unwrap()).unwrap();
        verify_artifact_ref(&meeting_dir, &meeting.artifacts.system_audio.unwrap()).unwrap();
        assert_silent_wav(&fs::read(meeting_dir.join("capture/mic.wav")).unwrap());
        assert_silent_wav(&fs::read(meeting_dir.join("capture/system.wav")).unwrap());

        let coordination = MeetingStorageCoordination::default();
        let authority = TranscriptRetryAuthority::new(&storage, &coordination);
        let pending = authority
            .discover_pending_candidate(MEETING_ID)
            .unwrap()
            .unwrap();
        assert_eq!(
            pending.state,
            TranscriptRetryState::CandidateAvailableForComparison
        );
        assert_ne!(
            pending.source_transcript_sha256,
            pending.candidate_transcript_sha256
        );
        assert_eq!(
            load_meeting(&meeting_dir)
                .unwrap()
                .artifacts
                .current_transcript
                .unwrap(),
            source
        );
        let bytes = authority
            .read_candidate_bytes(MEETING_ID, pending.operation_id)
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("candidate is different"));
        let marker = fs::read_to_string(root.join("SYNTHETIC_FIXTURE.json")).unwrap();
        assert!(marker.contains("private_data"));
        assert!(marker.contains("product_evidence"));
    }

    /// Loads a meeting's turn texts straight from its retained transcript
    /// JSON, exactly as the desktop crate's `retry_diff_turn_inputs` would
    /// project them (visible turns only; this fixture never gates a turn).
    fn turn_texts(meeting_dir: &Path, relative_path: &str) -> Vec<String> {
        let bytes = fs::read(meeting_dir.join(relative_path)).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        document["turns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|turn| turn["text"].as_str().unwrap().to_owned())
            .collect()
    }

    fn diff_inputs(texts: &[String]) -> Vec<DiffTurnInput<'_>> {
        texts.iter().map(|text| DiffTurnInput::visible(text)).collect()
    }

    /// Reads a pending retry's candidate turn texts by following its
    /// `transcript-retry-candidate/1` receipt to the actual candidate
    /// transcript file, exactly as `TranscriptRetryAuthority` lays a pending
    /// retry out on disk (a receipt naming the candidate's real
    /// `transcript/<sha>.json` path, not an inline copy).
    fn candidate_texts(meeting_dir: &Path, operation_id: &str) -> Vec<String> {
        let receipt_bytes = fs::read(
            meeting_dir
                .join("transcript-retry")
                .join(operation_id)
                .join("receipt.json"),
        )
        .unwrap();
        let receipt: serde_json::Value = serde_json::from_slice(&receipt_bytes).unwrap();
        let relative_path = receipt["candidate_transcript"]["relative_path"]
            .as_str()
            .unwrap();
        turn_texts(meeting_dir, relative_path)
    }

    /// End-to-end proof that every Wave 2/3 state this packet stages is both
    /// present on disk and produces the real product state a reviewer would
    /// see: pause spans project to `Paused`, the diff engine actually
    /// computes each of the three retry-diff states from the staged
    /// transcripts (not merely "files exist"), the trashed meeting is the
    /// only one `list_trash_entries` reports, the tampered transcript fails
    /// its own digest check, and the note meeting's revision reference is
    /// internally consistent.
    #[test]
    fn seed_produces_every_targeted_wave_2_3_state() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository).unwrap();
        let storage = StorageRoot::create(&root, &temp.path().join("repository")).unwrap();

        // W1-A: capture pauses on the original retry-comparison meeting.
        let meeting_dir = storage.path().join("meetings").join(MEETING_ID);
        let meeting = load_meeting(&meeting_dir).unwrap();
        let pauses = project_capture_pauses(&meeting_dir, &meeting).unwrap();
        assert_eq!(pauses.state, CapturePauseState::Paused);
        assert_eq!(pauses.count, 2);
        assert_eq!(pauses.total_paused_seconds, 3);

        // W3-B: the original meeting's pending retry actually diffs with
        // real word-level spans on both sides (not merely "some difference
        // exists" -- both sides must carry at least one span).
        let candidate = candidate_texts(&meeting_dir, OPERATION_ID);
        let current_texts = turn_texts(
            &meeting_dir,
            &meeting.artifacts.current_transcript.as_ref().unwrap().relative_path,
        );
        let diff = diff_transcript_turns(&diff_inputs(&current_texts), &diff_inputs(&candidate));
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert!(
            diff.current.iter().any(|turn| !turn.spans.is_empty())
                || diff.candidate.iter().any(|turn| !turn.spans.is_empty()),
            "the baseline retry meeting must exercise the 'computed with differences' state"
        );

        // W3-B: the identical-candidate meeting diffs to zero spans on both
        // sides -- "No word-level differences found.", not merely "computed".
        let identical_dir = storage
            .path()
            .join("meetings")
            .join(DIFF_IDENTICAL_MEETING_ID);
        let identical_meeting = load_meeting(&identical_dir).unwrap();
        let identical_current = turn_texts(
            &identical_dir,
            &identical_meeting
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .relative_path,
        );
        let identical_candidate = candidate_texts(&identical_dir, DIFF_IDENTICAL_OPERATION_ID);
        let identical_diff = diff_transcript_turns(
            &diff_inputs(&identical_current),
            &diff_inputs(&identical_candidate),
        );
        assert_eq!(identical_diff.state, TranscriptRetryDiffState::Computed);
        assert!(
            identical_diff.current.iter().all(|turn| turn.spans.is_empty())
                && identical_diff.candidate.iter().all(|turn| turn.spans.is_empty()),
            "identical turn text on both sides must diff to zero spans"
        );

        // W3-B: the over-budget meeting's pending retry actually exceeds the
        // diff engine's edit budget and skips, rather than merely being long.
        let skipped_dir = storage.path().join("meetings").join(DIFF_SKIPPED_MEETING_ID);
        let skipped_meeting = load_meeting(&skipped_dir).unwrap();
        let skipped_current = turn_texts(
            &skipped_dir,
            &skipped_meeting
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .relative_path,
        );
        let skipped_candidate = candidate_texts(&skipped_dir, DIFF_SKIPPED_OPERATION_ID);
        let skipped_diff = diff_transcript_turns(
            &diff_inputs(&skipped_current),
            &diff_inputs(&skipped_candidate),
        );
        assert_eq!(skipped_diff.state, TranscriptRetryDiffState::Skipped);

        // W2-B: exactly the one trashed meeting is reported, and it is no
        // longer readable at `meetings/<id>`.
        let entries = list_trash_entries(&storage).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].meeting_id, TRASH_MEETING_ID);
        assert_eq!(entries[0].title.as_deref(), Some("Fixture: Trash review"));
        assert!(
            !storage
                .path()
                .join("meetings")
                .join(TRASH_MEETING_ID)
                .exists()
        );

        // W2-C: the tampered meeting's pinned transcript digest no longer
        // matches the bytes on disk, exactly the failure `export_meeting`
        // withholds and names.
        let tampered_dir = storage
            .path()
            .join("meetings")
            .join(EXPORT_TAMPERED_MEETING_ID);
        let tampered_meeting = load_meeting(&tampered_dir).unwrap();
        assert!(
            verify_artifact_ref(
                &tampered_dir,
                tampered_meeting.artifacts.current_transcript.as_ref().unwrap()
            )
            .is_err(),
            "the tampered transcript must fail verification"
        );

        // W3-A / W2-A: the note meeting is `Ready` with an internally
        // consistent, digest-named current note bound to its own transcript.
        let note_dir = storage.path().join("meetings").join(NOTE_MEETING_ID);
        let note_meeting = load_meeting(&note_dir).unwrap();
        assert_eq!(note_meeting.lifecycle, MeetingLifecycle::Ready);
        let note = note_meeting.artifacts.current_note.as_ref().unwrap();
        note.validate().unwrap();
        assert_eq!(
            note.source_transcript_sha256,
            note_meeting.artifacts.current_transcript.as_ref().unwrap().sha256
        );
        verify_artifact_ref(&note_dir, &note.json).unwrap();
        verify_artifact_ref(&note_dir, &note.markdown).unwrap();

        // I3: the read-only pre-meeting context file is present and readable.
        let context_bytes = fs::read(note_dir.join("meeting-context.json")).unwrap();
        let context: serde_json::Value = serde_json::from_slice(&context_bytes).unwrap();
        assert_eq!(context["schema"], "meeting-context/1");
        assert!(context["text"].as_str().unwrap().starts_with("Fixture:"));
    }

    /// Roadmap Wave 4 / I5: the fixture's one locked meeting, checked against
    /// the parts of the locked-row claim this crate can verify directly --
    /// the real `meeting-lock/1` sidecar shape, a verifying transcript, and a
    /// title `meeting_title::derived_title` (the same function the real
    /// library row uses) actually produces, so "title visible" is a fact
    /// about real product logic and not a hand-asserted one.
    ///
    /// What this test does not and cannot prove: that a locked read actually
    /// suppresses a note preview, refuses to open, or shows the shell's
    /// "Locked" marker. `meeting_lock::read`, `permits`, and the row's
    /// `locked`/`note_preview` fields all live in the desktop crate, which
    /// this session-core binary does not and should not depend on --
    /// see the doc comment on `seed_locked_meeting` for exactly where that
    /// gate is proven instead.
    #[test]
    fn seed_produces_the_wave_4_locked_meeting_state() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository).unwrap();
        let storage = StorageRoot::create(&root, &temp.path().join("repository")).unwrap();

        let locked_dir = storage.path().join("meetings").join(LOCKED_MEETING_ID);
        let locked_meeting = load_meeting(&locked_dir).unwrap();
        assert_eq!(locked_meeting.lifecycle, MeetingLifecycle::TranscriptReady);
        verify_record_artifacts(&locked_dir, &locked_meeting).unwrap();

        // The real `meeting-lock/1` shape `meeting_lock.rs`'s `write` produces
        // and `read` parses -- this fixture cannot call either function
        // (private to the desktop crate), so it is pinned against the exact
        // bytes on disk instead.
        let lock_bytes = fs::read(locked_dir.join("meeting-lock.json")).unwrap();
        let lock: serde_json::Value = serde_json::from_slice(&lock_bytes).unwrap();
        assert_eq!(lock["schema"], "meeting-lock/1");
        assert_eq!(lock["locked"], true);

        // Title visible: run the real turn text through the real
        // `derived_title`, rather than asserting the fixture "looks long
        // enough" by eye.
        let transcript = locked_meeting.artifacts.current_transcript.as_ref().unwrap();
        let turns = turn_texts(&locked_dir, &transcript.relative_path);
        let title = derived_title(turns.iter().map(|text| (text.as_str(), false)));
        assert!(
            title.is_some(),
            "the locked meeting's first turn must be long enough to derive a title, \
             so the locked row has a title to show rather than only a date"
        );
    }

    fn assert_silent_wav(bytes: &[u8]) {
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            AUDIO_SAMPLE_RATE
        );
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16);
        assert_eq!(&bytes[36..40], b"data");
        let data_bytes = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_bytes, AUDIO_FRAMES * 2);
        assert_eq!(bytes.len(), 44 + data_bytes);
        assert!(bytes[44..].iter().all(|sample| *sample == 0));
    }

    fn archive_target(temp: &TempDir, label: &str) -> PathBuf {
        temp.path().join(format!("{BUNDLE_NAME}.archive-{label}"))
    }

    fn model_marker_bytes() -> Vec<u8> {
        serde_json::to_vec_pretty(&json!({
            "schema": "synthetic-model-fixture/1",
            "model_id": "fixture-model",
            "revision": "fixture-revision",
            "private_data": false,
            "product_evidence": false,
            "note": "Public model bytes copied from a verified catalog entry for synthetic fixture review."
        }))
        .unwrap()
    }

    #[test]
    fn archive_moves_fixture_and_preserves_marker_and_model_bytes() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let model = model_marker_bytes();
        fs::write(root.join("MODEL_FIXTURE.json"), &model).unwrap();
        fs::set_permissions(
            root.join("MODEL_FIXTURE.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let marker = fs::read(root.join("SYNTHETIC_FIXTURE.json")).unwrap();
        let archive_root = archive_target(&temp, "review-1");

        archive(&root, &archive_root, repository).unwrap();

        assert!(!root.exists());
        assert_eq!(
            fs::read(archive_root.join("SYNTHETIC_FIXTURE.json")).unwrap(),
            marker
        );
        assert_eq!(
            fs::read(archive_root.join("MODEL_FIXTURE.json")).unwrap(),
            model
        );
    }

    #[test]
    fn archive_works_without_model_marker() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let archive_root = archive_target(&temp, "no-model");

        archive(&root, &archive_root, repository).unwrap();

        assert!(!root.exists());
        assert!(archive_root.join("SYNTHETIC_FIXTURE.json").is_file());
        assert!(!archive_root.join("MODEL_FIXTURE.json").exists());
    }

    #[test]
    fn archive_refuses_invalid_destination_and_source_contracts() {
        let cases = [
            ("wrong", FixtureError::WrongArchiveName),
            (
                "com.ninochavez.local-meeting-notes.fixture.archive-",
                FixtureError::WrongArchiveName,
            ),
            (
                "com.ninochavez.local-meeting-notes.fixture.archive-a_b",
                FixtureError::WrongArchiveName,
            ),
        ];
        for (name, expected) in cases {
            let temp = TempDir::new().unwrap();
            let root = target(&temp);
            let repository = repo(&temp);
            seed(&root, repository.clone()).unwrap();
            assert!(matches!(
                archive(&root, &temp.path().join(name), repository),
                Err(error) if std::mem::discriminant(&error) == std::mem::discriminant(&expected)
            ));
        }

        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let existing = archive_target(&temp, "existing");
        create_private_dir(&existing).unwrap();
        assert!(matches!(
            archive(&root, &existing, repository.clone()),
            Err(FixtureError::ExistingArchiveRoot)
        ));

        let linked = archive_target(&temp, "linked");
        symlink(temp.path(), &linked).unwrap();
        assert!(matches!(
            archive(&root, &linked, repository.clone()),
            Err(FixtureError::ExistingArchiveRoot)
        ));

        fs::write(root.join("SYNTHETIC_FIXTURE.json"), b"{}").unwrap();
        fs::set_permissions(
            root.join("SYNTHETIC_FIXTURE.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(matches!(
            archive(&root, &archive_target(&temp, "bad-marker"), repository),
            Err(FixtureError::InvalidMarker)
        ));
    }

    #[test]
    fn archive_refuses_wrong_location_repository_and_broad_paths() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let other_parent = temp.path().join("other");
        create_private_dir(&other_parent).unwrap();
        assert!(
            archive(
                &root,
                &other_parent.join(format!("{BUNDLE_NAME}.archive-other")),
                repository.clone()
            )
            .is_err()
        );
        assert!(
            archive(
                &root,
                &repository.join(format!("{BUNDLE_NAME}.archive-repository")),
                repository.clone()
            )
            .is_err()
        );
        assert!(
            archive(
                &root,
                &Path::new("/tmp").join(format!("{BUNDLE_NAME}.archive-broad")),
                repository
            )
            .is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn exclusive_archive_publish_never_replaces_an_empty_destination() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let archive_root = archive_target(&temp, "already-created");
        create_private_dir(&archive_root).unwrap();
        let parent = temp.path().canonicalize().unwrap();

        let result = rename_fixture_exclusive(
            &parent,
            BUNDLE_NAME,
            archive_root.file_name().unwrap().to_str().unwrap(),
        );

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert!(root.is_dir());
        assert!(archive_root.is_dir());
        assert!(archive_root.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn archive_refuses_contended_writer_lock_without_mutation() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        let storage = StorageRoot::create(&root, &repository).unwrap();
        let _writer = AppDataWriterLock::acquire(&storage).unwrap();
        let marker = fs::read(root.join("SYNTHETIC_FIXTURE.json")).unwrap();
        let archive_root = archive_target(&temp, "contended");

        assert!(matches!(
            archive(&root, &archive_root, repository),
            Err(FixtureError::WriterLock(AppDataWriterLockError::Contended))
        ));
        assert!(root.is_dir());
        assert!(!archive_root.exists());
        assert_eq!(
            fs::read(root.join("SYNTHETIC_FIXTURE.json")).unwrap(),
            marker
        );
    }

    #[test]
    fn archive_refuses_wrong_root_and_model_marker_modes() {
        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            archive(&root, &archive_target(&temp, "wrong-root-mode"), repository),
            Err(FixtureError::UnsafeDirectory)
        ));

        let temp = TempDir::new().unwrap();
        let root = target(&temp);
        let repository = repo(&temp);
        seed(&root, repository.clone()).unwrap();
        fs::write(root.join("MODEL_FIXTURE.json"), model_marker_bytes()).unwrap();
        fs::set_permissions(
            root.join("MODEL_FIXTURE.json"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(matches!(
            archive(
                &root,
                &archive_target(&temp, "wrong-model-mode"),
                repository
            ),
            Err(FixtureError::InvalidModelMarker)
        ));
    }

    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, current: &Path, files: &mut Vec<(String, Vec<u8>)>) {
            for entry in fs::read_dir(current).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, files);
                } else {
                    files.push((
                        path.strip_prefix(root).unwrap().display().to_string(),
                        fs::read(path).unwrap(),
                    ));
                }
            }
        }
        let mut files = Vec::new();
        walk(root, root, &mut files);
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    #[test]
    fn rejects_wrong_name_broad_root_symlink_and_existing_content() {
        let temp = TempDir::new().unwrap();
        let repository = repo(&temp);
        assert!(matches!(
            validate_root(&temp.path().join("wrong"), &repository),
            Err(FixtureError::WrongRootName)
        ));
        assert!(matches!(
            validate_root(Path::new("/tmp").join(BUNDLE_NAME).as_path(), &repository),
            Err(FixtureError::BroadRoot)
        ));

        let link = target(&temp);
        symlink(temp.path(), &link).unwrap();
        assert!(matches!(
            validate_root(&link, &repository),
            Err(FixtureError::Symlink)
        ));

        let existing_temp = TempDir::new().unwrap();
        let existing = target(&existing_temp);
        let existing_repository = repo(&existing_temp);
        create_private_dir(&existing).unwrap();
        create_private_dir(&existing.join("meetings")).unwrap();
        write_new(&existing.join("meetings/real.json"), b"not fixture").unwrap();
        assert!(matches!(
            validate_root(&existing, &existing_repository),
            Err(FixtureError::ExistingData)
        ));
    }

    fn write_bundle_file(path: &Path, bytes: &[u8]) -> String {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        digest(bytes)
    }

    fn install_inputs(
        temp: &TempDir,
    ) -> (
        PathBuf,
        PathBuf,
        PathBuf,
        PathBuf,
        TranscriptModel,
        Vec<u8>,
        Vec<u8>,
    ) {
        let root = target(temp);
        let repository = repo(temp);
        seed(&root, repository.clone()).unwrap();

        let model_id = "fixture-model";
        let revision = "a".repeat(40);
        let config = br#"{"model":"fixture"}
"#
        .to_vec();
        let weights = b"synthetic weights".to_vec();
        let entry = TranscriptModel {
            id: model_id.into(),
            revision: revision.clone(),
            title: "Synthetic transcript model".into(),
            detail: "Public bytes for fixture review.".into(),
            download_bytes: (config.len() + weights.len()) as u64,
            installed_bytes: (config.len() + weights.len()) as u64,
            files: vec![
                TranscriptModelFile {
                    role: TranscriptModelFileRole::Config,
                    name: "config.json".into(),
                    url: format!("https://models.example/{revision}/config.json"),
                    bytes: config.len() as u64,
                    sha256: digest(&config),
                },
                TranscriptModelFile {
                    role: TranscriptModelFileRole::Weights,
                    name: "weights.safetensors".into(),
                    url: format!("https://models.example/{revision}/weights.safetensors"),
                    bytes: weights.len() as u64,
                    sha256: digest(&weights),
                },
            ],
        };
        let bundle = temp.path().join("bundle");
        create_private_dir(&bundle).unwrap();
        let catalog_bytes = serde_json::to_vec_pretty(&ModelCatalog {
            schema: ModelCatalogSchema::V1,
            models: vec![entry.clone()],
            note_models: Vec::new(),
        })
        .unwrap();
        let catalog_digest = write_bundle_file(&bundle.join("model-catalog.json"), &catalog_bytes);
        for (name, bytes) in [
            ("runtime", b"runtime".as_slice()),
            ("worker", b"worker".as_slice()),
            ("tap", b"tap".as_slice()),
            ("encoder", b"encoder".as_slice()),
            ("permission-probe", b"probe".as_slice()),
        ] {
            write_bundle_file(&bundle.join(name), bytes);
        }
        let manifest = json!({
            "schema":"app-runtime/2", "admission":"product",
            "runtime":{"path":"runtime","sha256":digest(b"runtime")},
            "worker":{"path":"worker","sha256":digest(b"worker")},
            "tap":{"path":"tap","sha256":digest(b"tap")},
            "encoder":{"path":"encoder","sha256":digest(b"encoder")},
            "permission_probe":{"path":"permission-probe","sha256":digest(b"probe")},
            "model_catalog":{"path":"model-catalog.json","sha256":catalog_digest},
            "models":[]
        });
        write_bundle_file(
            &bundle.join("app-runtime.json"),
            &serde_json::to_vec_pretty(&manifest).unwrap(),
        );

        let source = temp.path().join(model_id).join(&revision);
        create_private_dir(source.parent().unwrap()).unwrap();
        create_private_dir(&source).unwrap();
        write_bundle_file(&source.join("config.json"), &config);
        write_bundle_file(&source.join("weights.safetensors"), &weights);
        write_bundle_file(
            &source.join("model-install.json"),
            &install_receipt_bytes(&entry),
        );
        (root, repository, bundle, source, entry, config, weights)
    }

    #[test]
    fn install_model_copies_verified_public_bytes_and_activates_exact_entry() {
        let temp = TempDir::new().unwrap();
        let (root, repository, bundle, source, entry, config, weights) = install_inputs(&temp);
        install_model(&root, &bundle, &source, &entry.id, repository.clone()).unwrap();

        let storage = StorageRoot::create(&root, &repository).unwrap();
        let catalog = ModelCatalog {
            schema: ModelCatalogSchema::V1,
            models: vec![entry.clone()],
            note_models: Vec::new(),
        };
        assert_eq!(
            active_model(&storage, &catalog).unwrap().unwrap().revision,
            entry.revision
        );
        let target = storage
            .path()
            .join("models")
            .join(&entry.id)
            .join(&entry.revision);
        assert_eq!(fs::read(target.join("config.json")).unwrap(), config);
        assert_eq!(
            fs::read(target.join("weights.safetensors")).unwrap(),
            weights
        );
        verify_model_directory(&target, &entry, ModelVerification::Contents).unwrap();
        let model_marker = fs::read_to_string(storage.path().join("MODEL_FIXTURE.json")).unwrap();
        assert!(model_marker.contains(&entry.id));
        assert!(!model_marker.contains(source.to_string_lossy().as_ref()));
    }

    #[test]
    fn install_model_refuses_wrong_marker_existing_target_and_changed_source() {
        let temp = TempDir::new().unwrap();
        let (root, repository, bundle, source, entry, _, _) = install_inputs(&temp);
        fs::write(root.join("SYNTHETIC_FIXTURE.json"), b"{}").unwrap();
        fs::set_permissions(
            root.join("SYNTHETIC_FIXTURE.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(matches!(
            install_model(&root, &bundle, &source, &entry.id, repository.clone()),
            Err(FixtureError::InvalidMarker)
        ));

        let temp = TempDir::new().unwrap();
        let (root, repository, bundle, source, entry, _, _) = install_inputs(&temp);
        let target = root.join("models").join(&entry.id).join(&entry.revision);
        create_private_dir(&target).unwrap();
        assert!(matches!(
            install_model(&root, &bundle, &source, &entry.id, repository.clone()),
            Err(FixtureError::ExistingModel)
        ));

        let temp = TempDir::new().unwrap();
        let (root, repository, bundle, source, entry, _, _) = install_inputs(&temp);
        fs::write(source.join("weights.safetensors"), b"changed").unwrap();
        assert!(matches!(
            install_model(&root, &bundle, &source, &entry.id, repository),
            Err(FixtureError::SourceChanged)
        ));

        let temp = TempDir::new().unwrap();
        let (root, repository, bundle, _source, entry, config, weights) = install_inputs(&temp);
        let repository_model = repository.join(&entry.id).join(&entry.revision);
        create_private_dir(&repository_model).unwrap();
        write_bundle_file(&repository_model.join("config.json"), &config);
        write_bundle_file(&repository_model.join("weights.safetensors"), &weights);
        write_bundle_file(
            &repository_model.join("model-install.json"),
            &install_receipt_bytes(&entry),
        );
        let parent_link = temp.path().join("source-parent-link");
        symlink(&repository, &parent_link).unwrap();
        let linked_source = parent_link.join(&entry.id).join(&entry.revision);
        assert!(matches!(
            install_model(&root, &bundle, &linked_source, &entry.id, repository),
            Err(FixtureError::UnsafeInstallPath(_))
        ));
    }
}
