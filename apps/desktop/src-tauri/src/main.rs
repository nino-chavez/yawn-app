#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The library reader maps an already-built projection into closed DTOs. Most
// of its surface remains private and unregistered. Two of its methods --
// `search`/`search_filtered` and `open_search_result` -- do gain a Tauri
// command each as of roadmap intake W8-B, but only behind the disposable
// local flag in `search_probe.rs`: see that module and
// `preview_library_search`/`preview_library_open_search_result` below.
#[allow(dead_code)]
mod library_reader;
// Roadmap intake W8-B: the one-week local usage probe for the two
// cross-meeting exact-search commands. Owns the disposable on/off marker file
// and the append-only, content-free event log. See the module doc for why
// the flag is read fresh per call rather than cached.
mod search_probe;
// Roadmap packet W10: the once-only first-run sheet's dismissal marker. Owns
// only "has the operator dismissed it" and "mark it dismissed" -- the
// frontend combines that with the already-computed library total to decide
// whether the sheet renders. See the module doc for why the flag is read
// fresh per call rather than cached.
mod onboarding;

// The correction/regeneration facade is intentionally compiled but not wired
// into the current internal-alpha command set.
mod manual_delete_facade;
#[allow(dead_code)]
mod product_facade;
mod speaker_correction;
// First run's permission surface (§ H). Runs the manifest-verified permission
// probe and parses its output as untrusted input; holds no storage authority and
// reads no operator content.
mod corpus_embedder;
mod first_run;
// § D. The operator's own note, typed during the meeting. Interpretation, not
// evidence: a fixed path in the meeting directory, replaced atomically, with the
// frozen `meeting/2` contract untouched. See the module for why.
mod operator_note;
// Roadmap intake I3. Pre-meeting context, typed before or during capture.
// Mirrors `operator_note` decision for decision -- see the module for why --
// except that this one is allowed to reach a note-generation prompt.
mod meeting_context;
// § E1. Folders and the operator's own meeting titles — the five named
// commands and their closed response shape. Registered, unlike the facades
// above: `library/metadata.json` gained a writer on 2026-08-08 and the surface
// that reaches it is the point of having one.
mod library_organization;
mod model_download;
// The storage-backed coordinator and worker bridge behind product_facade.
// Managed as state so the facade commands can be registered in one move once
// the operator widens the packaged admission; the commands stay unregistered.
mod product_coordinator;
// Roadmap intake I2: a global hotkey that summons operator-note capture
// while a meeting is recording. Owns the shortcut binding, its
// register/unregister lifecycle, and the frontend focus event — nothing
// else. See the module docs for the governing constraint.
mod capture_shortcut;
// Roadmap intake I7+I8: assembles a completed meeting into plain per-item
// files (a Markdown note with source references, a readable transcript,
// operator content, and copied receipts) plus one compact archive of the same
// content, written only inside that meeting's own directory. Pure assembly
// over data `library_reader` has already digest-verified; see the module docs
// for what is copied verbatim versus freshly derived.
mod meeting_export;
// Roadmap intake I5: a per-meeting local-access barrier. Owns the
// `meeting-lock/1` sidecar, the closed set of actions a locked meeting
// refuses, the one gate decision every refusal is made by, and the single-use
// confirmation tokens that lift it. It is a barrier, not encryption — see the
// module docs for the honest ceiling of that claim.
mod meeting_lock;
// Roadmap intake I5's other half: the macOS device-owner check (Touch ID with
// the system's password fallback) behind a trait, so every gate path above is
// testable with a fake and the real prompt stays live-run evidence.
mod operator_confirmation;
// Packet W7-C: local start-latency evidence for design intake D2's stated-
// but-never-measured seconds budget. Owns the `capture-timing/1` sidecar,
// its clock model, and the one call site (past the `CaptureState::Recording`
// transition) that may write it. See the module docs for why.
mod capture_timing;

// W7-B (2026-09-01 desktop audit): stable machine codes for the small set of
// backend errors the frontend attaches a recovery action to, so that
// coupling no longer relies on exact string equality with a hand-maintained
// JS array. See the module docs for the two transports this covers.
mod error_codes;

use manual_delete_facade::{
    AudioDeletionReview, ManualAudioDeletionFacadeError, ManualAudioDeletionFacadeOutcome,
    ManualAudioDeletionUiArgs,
};

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use local_meeting_notes_session_core::diagnostic::write_private_diagnostic;
use local_meeting_notes_session_core::enrollment_guidance::{
    EnrollmentEvidence, GuidedEnrollmentStatus, evaluate_enrollment_evidence,
};
use local_meeting_notes_session_core::meeting::resolve_artifact;
use local_meeting_notes_session_core::meeting::{
    ArtifactRef, AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts,
    MeetingLifecycle, MeetingRecord, MeetingSchema, artifact_ref, load_meeting, read_private_bytes,
    retention_policy_sha256, verify_record_artifacts, write_meeting,
};
use local_meeting_notes_session_core::meeting_coordination::MeetingStorageCoordination;
use local_meeting_notes_session_core::meeting_deletion::reconcile_pending_meeting_deletions;
use local_meeting_notes_session_core::meeting_trash::{
    execute_due_trash_purge, list_trash_entries, reconcile_pending_trash, TrashPurgeOutcome,
};
use local_meeting_notes_session_core::model_store::{
    InstalledTranscriptModel, ModelCatalog, TranscriptModel, activate_model, active_model,
    active_note_model, deactivate_note_model, installed_model, model_is_stored,
    note_model_is_stored, remove_inactive_model, remove_inactive_note_model,
};
use local_meeting_notes_session_core::note_projection::{NoteProjector, UnavailableProjector};
use local_meeting_notes_session_core::note_projector_process::{
    GENERATE_MANIFEST_FILE, PROJECT_MANIFEST_FILE, admit_note_projector,
};
use local_meeting_notes_session_core::operations::TranscriptRetryUiArgs;
#[cfg(target_os = "macos")]
use local_meeting_notes_session_core::profile_lifecycle::ProfileLifecycleError;
use local_meeting_notes_session_core::protocol::{
    CaptureProgressState, Operation, ProgressEvent, ProtocolError, WorkerProgress, WorkerResult,
};
use local_meeting_notes_session_core::recovery::{
    RecoveryCode, RecoveryDisposition, scan_and_recover,
};
use local_meeting_notes_session_core::reducer::{
    CaptureState, ExclusiveOperation, Reducer, StartupState,
};
use local_meeting_notes_session_core::retention::{
    AppDataWriterLock, RetentionOutcome, execute_due_retention_excluding, meeting_dir,
};
#[cfg(target_os = "macos")]
use local_meeting_notes_session_core::retention::{
    ProfileEnrollmentAdmissionError, ProfileEnrollmentCompletion, ProfileEnrollmentWorker,
    ProfileEnrollmentWorkerError, ProfileLifecycleAdmissionError, SittingEvidenceAdmissionError,
    SittingEvidenceAuthority,
};
use local_meeting_notes_session_core::runtime::RuntimeManifest;
use local_meeting_notes_session_core::storage::{
    StorageRoot, create_private_dir, durable_create_new, sync_directory,
};
use local_meeting_notes_session_core::supervision::{
    OwnedChild, OwnershipReceipt, OwnershipSchema, ProcessIdentity, ProcessInspection,
    ProcessInspector, SupervisionError, SystemGroupSignaler, SystemProcessInspector,
    internal_alpha_operations,
};
use local_meeting_notes_session_core::transcript_deletion::reconcile_pending_transcript_deletions;
use local_meeting_notes_session_core::transcript_restoration::resolve_stored_transcript_primed;
use local_meeting_notes_session_core::transcript_retry::TranscriptRetryOutcome;
use local_meeting_notes_session_core::transcript_retry_diff::{
    DiffTurnInput, TranscriptRetryDiffState, diff_transcript_turns,
};
use local_meeting_notes_session_core::transcription_queue::{
    TranscriptionQueue, TranscriptionRequest,
};
use error_codes::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow};
use uuid::Uuid;

const CAPTURE_EVENT_MAX_BYTES: usize = 64 * 1024;
const ATTEMPT_MAX_BYTES: u64 = 256 * 1024;
const TRANSCRIPT_MAX_BYTES: u64 = 16 * 1024 * 1024;
const CAPTURE_ARM_TIMEOUT: Duration = Duration::from_secs(120);
const CAPTURE_STOP_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a pause or resume may stay unconfirmed by the capture helper.
/// Shorter than arming, which waits on a permission prompt a person may answer:
/// pausing releases hardware the helper already holds, and resuming reacquires
/// it with permission already granted.
const CAPTURE_PAUSE_TIMEOUT: Duration = Duration::from_secs(20);
/// One hour of the helper's 16 kHz mono s16le sitting stream. A dedicated
/// enrolment sitting runs minutes, not hours; this stops a runaway stream
/// long before the store's own 1 GiB raw bound would refuse the finalize
/// digest.
#[cfg(target_os = "macos")]
const SITTING_STREAM_MAX_BYTES: u64 = 16_000 * 2 * 60 * 60;
const WORKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TRANSCRIPT_REQUEST_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const PARTICIPANT_NOTICE_VERSION: &str = "internal-transcript-alpha/1";
const SETTINGS_WINDOW_LABEL: &str = "settings";
const RETAINED_AUDIO_PLAYER: &str = "/usr/bin/afplay";
/// `afplay` opens this fixed standard-input alias. The player never receives a
/// meeting path, and Rust owns the standard-descriptor setup without a custom
/// low-numbered descriptor that could conflict with its exec-error plumbing.
const RETAINED_AUDIO_STDIN: &str = "/dev/stdin";
#[cfg(feature = "preview-surface")]
const ACTIVE_WINDOW_LABEL: &str = "preview";
#[cfg(not(feature = "preview-surface"))]
const ACTIVE_WINDOW_LABEL: &str = "main";

fn show_settings_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        return Ok(window);
    }

    // Preferences-style window (docs/experience-brief-2026-09-02.md platform
    // strategy, adopt table: "closable, not modal" was already true — this
    // build is never given a parent, so it was never modal — but the fixed
    // 720x720 size with resizing, minimizing, and maximizing all disabled
    // read as a defect on the cold review. Resizable + minimizable now;
    // maximizable stays off because a settings surface has no full-window
    // use for the extra space, which the brief does not ask for either.
    tauri::WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("settings.html".into()),
    )
    .title("Yawn Settings")
    .inner_size(720.0, 720.0)
    .min_inner_size(560.0, 480.0)
    .resizable(true)
    .minimizable(true)
    .maximizable(false)
    .closable(true)
    .center()
    .focused(true)
    .build()
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    show_settings_window(&app)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Shows and focuses the active (`ACTIVE_WINDOW_LABEL`) window — never the
/// Settings window. The one place this logic lives; the tray's "Open Yawn"
/// item, the single-instance relaunch callback, and `RunEvent::Reopen` all
/// call it instead of repeating the show+focus pair a third or fourth time.
fn show_and_focus_active_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ACTIVE_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Whether a macOS Reopen (Dock click / re-activation) should show the active
/// window. Split out from the `RunEvent::Reopen` arm so the decision is
/// unit-testable without a live event loop: `has_visible_windows` mirrors
/// `NSApplication`'s own signal of whether *any* window (including Settings)
/// is currently visible, so this only says yes when the app is otherwise
/// invisible — it never steals focus from a window already on screen.
fn should_show_on_reopen(has_visible_windows: bool) -> bool {
    !has_visible_windows
}

struct ApplicationState {
    model: Mutex<AppModel>,
    // The storage, worker, and writer-lock slots are shared (Arc) so the
    // product coordinator can hold live handles to the same runtime state the
    // commands mutate, instead of a startup snapshot.
    storage: Arc<Mutex<Option<StorageContext>>>,
    runtime: Mutex<Option<RuntimeIdentity>>,
    worker: Arc<Mutex<Option<OwnedChild>>>,
    transcription_worker: Arc<Mutex<Option<OwnedChild>>>,
    transcription_executor: Mutex<Option<TranscriptionExecutorControl>>,
    capture_task: Mutex<Option<CaptureTaskControl>>,
    sitting_task: Mutex<Option<SittingTaskControl>>,
    command_lock: Mutex<()>,
    // The slot holds an Arc so a long-running owner (the sitting take)
    // can hold the writer without holding this mutex guard: readers that
    // only need the coordination handle stay unblocked for the take's
    // whole duration. Single-writer authority is the AppDataWriterLock
    // itself (the owner-only flock), not this mutex.
    app_data_writer_lock: Arc<Mutex<Option<Arc<AppDataWriterLock>>>>,
    retention_started: AtomicBool,
    permission_observation: Mutex<Option<first_run::FirstRunPermissions>>,
    model_install_active: AtomicBool,
    note_model_install_active: AtomicBool,
    note_model_setup: Mutex<NoteModelSetup>,
    preview_library: Mutex<Option<library_reader::LibraryReader>>,
    /// The one audio child the shell owns. This is deliberately a `Child`,
    /// never a PID: every stop and reap action is constrained to this exact
    /// process object rather than a reused system process identifier.
    audio_playback: Mutex<Option<RetainedAudioPlayback>>,
    preview_profile: Mutex<PreviewProfileSnapshot>,
    preview_enrollment: Mutex<PreviewEnrollmentSurface>,
    // Successful note-projector admission, cached off the hot library-rebuild
    // path. Only success is cached: a failed admission (no generate manifest,
    // no catalog entry, weights not yet downloaded) is cheap to re-derive and
    // must retry so a model installed mid-session activates without restart.
    note_projector: Mutex<Option<Arc<dyn NoteProjector>>>,
    /// Roadmap intake I5. At most one outstanding single-use confirmation for
    /// one action on one locked meeting. Held beside the library rather than
    /// inside it because minting one runs a human-scale prompt, which must
    /// happen with no library or storage lock held.
    locked_actions: Mutex<meeting_lock::LockedActionAuthority>,
    /// The device-owner check the lock commands run. A trait object so the
    /// gate paths are written against a seam rather than against
    /// LocalAuthentication directly; the shipped app always installs the real
    /// one.
    confirmation: Arc<dyn operator_confirmation::ConfirmsOperator>,
    /// Cache for `cached_verified_manifest` — see that function for why this
    /// exists. Keyed by manifest path rather than a single slot so a
    /// `retry_startup` that resolves a different storage root is still
    /// verified fresh rather than reusing a stale entry from a different
    /// path.
    verified_manifest_cache: Mutex<HashMap<PathBuf, Arc<RuntimeManifest>>>,
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self {
            model: Mutex::new(AppModel::default()),
            storage: Arc::new(Mutex::new(None)),
            runtime: Mutex::new(None),
            worker: Arc::new(Mutex::new(None)),
            transcription_worker: Arc::new(Mutex::new(None)),
            transcription_executor: Mutex::new(None),
            capture_task: Mutex::new(None),
            sitting_task: Mutex::new(None),
            command_lock: Mutex::new(()),
            app_data_writer_lock: Arc::new(Mutex::new(None)),
            retention_started: AtomicBool::new(false),
            permission_observation: Mutex::new(None),
            model_install_active: AtomicBool::new(false),
            note_model_install_active: AtomicBool::new(false),
            note_model_setup: Mutex::new(NoteModelSetup {
                state: "idle".into(),
                ..NoteModelSetup::default()
            }),
            preview_library: Mutex::new(None),
            audio_playback: Mutex::new(None),
            preview_profile: Mutex::new(PreviewProfileSnapshot::unavailable()),
            preview_enrollment: Mutex::new(PreviewEnrollmentSurface::unavailable()),
            note_projector: Mutex::new(None),
            locked_actions: Mutex::new(meeting_lock::LockedActionAuthority::default()),
            confirmation: Arc::new(operator_confirmation::DeviceOwnerConfirmation),
            verified_manifest_cache: Mutex::new(HashMap::new()),
        }
    }
}

impl Drop for ApplicationState {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.transcription_worker.lock() {
            if let Some(mut worker) = slot.take() {
                let _ = worker.stop_and_wait(Duration::from_millis(750));
            }
        }
        if let Ok(mut slot) = self.worker.lock() {
            if let Some(mut worker) = slot.take() {
                let _ = worker.stop_and_wait(Duration::from_millis(750));
            }
        }
        if let Ok(slot) = self.transcription_executor.get_mut() {
            if let Some(control) = slot.take() {
                control.stop.store(true, Ordering::SeqCst);
                let _ = control.handle.join();
            }
        }
        // App teardown owns the same child slot as explicit Stop. No PID is
        // retained or signalled after this state goes away.
        if let Ok(slot) = self.audio_playback.get_mut() {
            if let Some(mut playback) = slot.take() {
                playback.stop_and_reap();
            }
        }
    }
}

/// A fixed native player attached to one already-open, verified retained-audio
/// file. The child receives that file as standard input and opens only the
/// fixed `/dev/stdin` alias, which keeps the file authority out of the webview
/// and prevents `afplay` from reopening storage.
struct RetainedAudioPlayback {
    child: Child,
    source: library_reader::RetainedAudioSource,
}

impl RetainedAudioPlayback {
    fn spawn(grant: library_reader::LibraryAudioPlaybackGrant) -> io::Result<Self> {
        // `Stdio` performs the canonical child setup at descriptor zero. It
        // avoids reserving a low descriptor ourselves, which could collide
        // with Rust's private exec-error pipe on macOS.
        let input = grant.file().try_clone()?;
        let mut command = Command::new(RETAINED_AUDIO_PLAYER);
        command
            .arg(RETAINED_AUDIO_STDIN)
            .stdin(Stdio::from(input))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn()?;
        Ok(Self {
            child,
            source: grant.source(),
        })
    }

    fn source_name(&self) -> &'static str {
        match self.source {
            library_reader::RetainedAudioSource::Microphone => "microphone",
            library_reader::RetainedAudioSource::System => "system",
        }
    }

    fn poll(&mut self) -> io::Result<bool> {
        self.child.try_wait().map(|status| status.is_none())
    }

    fn stop_and_reap(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetainedAudioPlaybackResponse {
    state: &'static str,
    source: Option<&'static str>,
    message: &'static str,
    /// W7-B (2026-09-01 desktop audit): present only when `message` is one of
    /// the handful the frontend attaches a recovery action to. The frontend
    /// re-throws this response's `message` as an `Error` to reach that same
    /// recovery path (see `main.js`'s retained-audio handlers) -- `code`
    /// travels with it so that path no longer keys on exact string equality.
    code: Option<&'static str>,
}

fn audio_playback_response(
    state: &'static str,
    source: Option<&'static str>,
    message: &'static str,
) -> RetainedAudioPlaybackResponse {
    audio_playback_response_coded(state, source, message, None)
}

fn audio_playback_response_coded(
    state: &'static str,
    source: Option<&'static str>,
    message: &'static str,
    code: Option<&'static str>,
) -> RetainedAudioPlaybackResponse {
    RetainedAudioPlaybackResponse {
        state,
        source,
        message,
        code,
    }
}

/// Stops and reaps only the process object held by Yawn. A failed kill can
/// mean the child has just exited; `wait` is still attempted to collect it.
fn stop_owned_audio_playback(state: &ApplicationState) {
    let Ok(mut slot) = state.audio_playback.lock() else {
        return;
    };
    if let Some(mut playback) = slot.take() {
        playback.stop_and_reap();
    }
}

fn owned_audio_playback_status(state: &ApplicationState) -> RetainedAudioPlaybackResponse {
    let Ok(mut slot) = state.audio_playback.lock() else {
        return audio_playback_response_coded(
            "unavailable",
            None,
            "Retained audio is unavailable. Reopen Library and try again.",
            Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
        );
    };
    let Some(playback) = slot.as_mut() else {
        return audio_playback_response("idle", None, "No recording is playing.");
    };
    match playback.poll() {
        Ok(true) => {
            return audio_playback_response(
                "playing",
                Some(playback.source_name()),
                "Playing retained audio.",
            );
        }
        Err(_) => {
            if let Some(mut playback) = slot.take() {
                playback.stop_and_reap();
            }
            return audio_playback_response_coded(
                "unavailable",
                None,
                "Retained audio is unavailable. Reopen Library and try again.",
                Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
            );
        }
        Ok(false) => {}
    }
    slot.take();
    audio_playback_response("completed", None, "The recording finished.")
}

struct AppModel {
    reducer: Reducer,
    admission: String,
    retention_operational: bool,
    startup_message: String,
    model_setup: ModelSetupSnapshot,
    meeting_id: Option<String>,
    started_at_epoch_seconds: Option<u64>,
    capture_state_started_at_epoch_seconds: Option<u64>,
    transcription_last_worker_heartbeat_at_epoch_seconds: Option<u64>,
    background_transcription_active: bool,
    background_transcription_queued_count: usize,
    degraded: bool,
    /// A pause or resume the operator asked for, which the capture helper has
    /// not confirmed yet. The reducer stays on the state that is still true
    /// until the helper reports the change, so this is what the recorder
    /// surface reads to say the request is in flight rather than claiming a
    /// recording is paused before the audio has actually stopped.
    capture_pause_change_pending: bool,
    mic_state: Option<String>,
    system_state: Option<String>,
    turns: Vec<TranscriptTurn>,
    /// The digest the projection in `turns` was verified against.
    ///
    /// Held because the frozen restore shape requires it back, so a restore
    /// request can only ever name the transcript the operator was actually
    /// reading. It is a hash of an artifact, not content: the same value the
    /// Library path already hands the shell.
    current_transcript_sha256: Option<String>,
    warnings: Vec<String>,
    error: Option<String>,
}

impl Default for AppModel {
    fn default() -> Self {
        Self {
            reducer: Reducer::default(),
            admission: "internal-alpha".into(),
            retention_operational: false,
            startup_message: "Preparing your private workspace.".into(),
            model_setup: ModelSetupSnapshot::default(),
            meeting_id: None,
            started_at_epoch_seconds: None,
            capture_state_started_at_epoch_seconds: None,
            transcription_last_worker_heartbeat_at_epoch_seconds: None,
            background_transcription_active: false,
            background_transcription_queued_count: 0,
            degraded: false,
            capture_pause_change_pending: false,
            mic_state: None,
            system_state: None,
            turns: Vec::new(),
            current_transcript_sha256: None,
            warnings: Vec::new(),
            error: None,
        }
    }
}

impl AppModel {
    fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            startup: self.reducer.startup(),
            admission: self.admission.clone(),
            retention_operational: self.retention_operational,
            startup_message: self.startup_message.clone(),
            model_setup: self.model_setup.clone(),
            capture: self.reducer.capture(),
            meeting_id: self.meeting_id.clone(),
            started_at_epoch_seconds: self.started_at_epoch_seconds,
            capture_state_started_at_epoch_seconds: self.capture_state_started_at_epoch_seconds,
            transcription_last_worker_heartbeat_at_epoch_seconds: self
                .transcription_last_worker_heartbeat_at_epoch_seconds,
            background_transcription_active: self.background_transcription_active,
            background_transcription_queued_count: self.background_transcription_queued_count,
            degraded: self.degraded,
            capture_pause_change_pending: self.capture_pause_change_pending,
            mic_state: self.mic_state.clone(),
            system_state: self.system_state.clone(),
            turns: self.turns.clone(),
            current_transcript_sha256: self.current_transcript_sha256.clone(),
            warnings: self.warnings.clone(),
            error: self.error.clone(),
        }
    }

    fn clear_meeting_projection(&mut self) {
        self.meeting_id = None;
        self.started_at_epoch_seconds = None;
        self.capture_state_started_at_epoch_seconds = None;
        self.transcription_last_worker_heartbeat_at_epoch_seconds = None;
        self.degraded = false;
        self.capture_pause_change_pending = false;
        self.mic_state = None;
        self.system_state = None;
        self.turns.clear();
        self.current_transcript_sha256 = None;
        self.warnings.clear();
        self.error = None;
    }
}

#[derive(Clone, Serialize)]
struct AppSnapshot {
    startup: StartupState,
    admission: String,
    retention_operational: bool,
    startup_message: String,
    model_setup: ModelSetupSnapshot,
    capture: CaptureState,
    meeting_id: Option<String>,
    started_at_epoch_seconds: Option<u64>,
    capture_state_started_at_epoch_seconds: Option<u64>,
    transcription_last_worker_heartbeat_at_epoch_seconds: Option<u64>,
    background_transcription_active: bool,
    background_transcription_queued_count: usize,
    degraded: bool,
    capture_pause_change_pending: bool,
    mic_state: Option<String>,
    system_state: Option<String>,
    turns: Vec<TranscriptTurn>,
    current_transcript_sha256: Option<String>,
    warnings: Vec<String>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSetupSnapshot {
    state: String,
    options: Vec<ModelSetupOption>,
    selected_model_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
}

impl Default for ModelSetupSnapshot {
    fn default() -> Self {
        Self {
            state: "checking".into(),
            options: Vec::new(),
            selected_model_id: None,
            downloaded_bytes: 0,
            total_bytes: 0,
            error: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSetupOption {
    id: String,
    title: String,
    detail: String,
    download_bytes: u64,
    installed_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptModelSettingsSnapshot {
    state: String,
    options: Vec<TranscriptModelSettingsOption>,
    active_model_id: Option<String>,
    selected_model_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
    change_active: bool,
    can_change: bool,
    unavailable_reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptModelSettingsOption {
    id: String,
    title: String,
    detail: String,
    download_bytes: u64,
    installed_bytes: u64,
    stored: bool,
    active: bool,
}

/// Download-progress state for the optional note-generation model. Its own
/// struct rather than a reuse of `ModelSetupSnapshot` because that one is a
/// startup-flow state ("choose"/"downloading" gating the whole app); a note
/// model downloads beside a fully started app and gates nothing but itself.
#[derive(Clone, Default)]
struct NoteModelSetup {
    state: String,
    selected_model_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteModelSettingsSnapshot {
    state: String,
    options: Vec<NoteModelSettingsOption>,
    active_model_id: Option<String>,
    selected_model_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
    change_active: bool,
    can_change: bool,
    unavailable_reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteModelSettingsOption {
    id: String,
    title: String,
    detail: String,
    download_bytes: u64,
    installed_bytes: u64,
    stored: bool,
    active: bool,
}

impl From<&TranscriptModel> for ModelSetupOption {
    fn from(model: &TranscriptModel) -> Self {
        Self {
            id: model.id.clone(),
            title: model.title.clone(),
            detail: model.detail.clone(),
            download_bytes: model.download_bytes,
            installed_bytes: model.installed_bytes,
        }
    }
}

#[derive(Clone, Serialize)]
struct TranscriptTurn {
    #[serde(rename = "sourceTurnIndex")]
    source_turn_index: u32,
    #[serde(rename = "sourceSpeaker")]
    source_speaker: Option<String>,
    speaker: Option<String>,
    #[serde(rename = "speakerCorrected")]
    speaker_corrected: bool,
    start: f64,
    text: String,
    // Withheld rows are positional only: the gate's decision is visible, the
    // withheld words never leave the artifact, so `text` stays empty.
    withheld: bool,
}

struct RestoredTranscriptProjection {
    meeting_id: String,
    turns: Vec<TranscriptTurn>,
    current_transcript_sha256: String,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryTranscriptProjection {
    turns: Vec<TranscriptTurn>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryComparisonResponse {
    meeting_id: Uuid,
    operation_id: Uuid,
    source_transcript_sha256: String,
    candidate_transcript_sha256: String,
    current: RetryTranscriptProjection,
    candidate: RetryTranscriptProjection,
    quality: local_meeting_notes_session_core::capture_quality::CaptureQualityProjection,
    recording_device: local_meeting_notes_session_core::capture_quality::RecordingDeviceProjection,
    /// Whether this recording was held open with nothing being captured. It
    /// rides beside the quality evidence because a gap in the audio changes how
    /// a transcript should be read, whichever transcript wins the comparison.
    pauses: local_meeting_notes_session_core::capture_quality::CapturePauseProjection,
    /// D6: the word-level delta between the two sides, so the reader sees
    /// what differs before choosing keep or promote. See
    /// `transcript_retry_diff` for the algorithm and its bounds.
    diff: RetryDiffProjection,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryWordSpan {
    start_word: u32,
    end_word: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryTurnDiffSpans {
    turn_index: u32,
    /// The word count this side used when it built `spans`. The browser
    /// tokenizes the same turn text independently and must compare its own
    /// count against this one before trusting a span's word indices — see
    /// `transcript_retry_diff`'s tokenization contract.
    word_count: u32,
    spans: Vec<RetryWordSpan>,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum RetryDiffState {
    Computed,
    Skipped,
}

/// Reader-safe wire shape for [`local_meeting_notes_session_core::transcript_retry_diff::TranscriptRetryDiff`].
///
/// `state: "skipped"` means the comparison hit a bound (either side's word
/// count, or the edit-script search budget) and carries no spans; it is
/// deliberately not the same shape as "computed with zero spans"
/// (identical), so the browser can tell "not checked" from "checked, same"
/// apart and word the quiet sentence above the columns accordingly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryDiffProjection {
    state: RetryDiffState,
    current: Vec<RetryTurnDiffSpans>,
    candidate: Vec<RetryTurnDiffSpans>,
}

fn retry_diff_turn_inputs(turns: &[TranscriptTurn]) -> Vec<DiffTurnInput<'_>> {
    turns
        .iter()
        .map(|turn| {
            if turn.withheld {
                DiffTurnInput::withheld()
            } else {
                DiffTurnInput::visible(turn.text.as_str())
            }
        })
        .collect()
}

fn retry_diff_projection(
    current_turns: &[TranscriptTurn],
    candidate_turns: &[TranscriptTurn],
) -> RetryDiffProjection {
    let current_inputs = retry_diff_turn_inputs(current_turns);
    let candidate_inputs = retry_diff_turn_inputs(candidate_turns);
    let diff = diff_transcript_turns(&current_inputs, &candidate_inputs);
    let to_wire =
        |side: Vec<local_meeting_notes_session_core::transcript_retry_diff::TurnDiffSpans>| {
            side.into_iter()
                .map(|turn| RetryTurnDiffSpans {
                    turn_index: turn.turn_index,
                    word_count: turn.word_count,
                    spans: turn
                        .spans
                        .into_iter()
                        .map(|span| RetryWordSpan {
                            start_word: span.start_word,
                            end_word: span.end_word,
                        })
                        .collect(),
                })
                .collect()
        };
    RetryDiffProjection {
        state: match diff.state {
            TranscriptRetryDiffState::Computed => RetryDiffState::Computed,
            TranscriptRetryDiffState::Skipped => RetryDiffState::Skipped,
        },
        current: to_wire(diff.current),
        candidate: to_wire(diff.candidate),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RetryDecisionInput {
    KeepCurrent,
    UseRetry,
}

#[derive(Debug, Serialize)]
struct RetryDecisionResponse {
    outcome: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewLibraryTranscript {
    state: &'static str,
    meeting_id: Option<String>,
    /// A fresh one-use capability to open this same checked artifact in the
    /// system app. It is opaque to the webview and never carries a path.
    transcript_file_handle: Option<String>,
    /// The digest the projection was verified against — the exact value the
    /// frozen restore-withheld-turn shape requires back, so a restore request
    /// can only name the transcript the operator was actually reading.
    #[serde(rename = "currentTranscriptSha256")]
    current_transcript_sha256: Option<String>,
    turns: Vec<TranscriptTurn>,
    warnings: Vec<String>,
    message: String,
}

/// Content-free voice-setup status cached for Settings.
///
/// `profile_present` and `profile_active` are deliberately separate, because
/// the lifecycle distinguishes them and the operator consequence is opposite.
/// Preserved legacy bytes are present and inactive: Preview will not activate
/// them, and saying so is the whole point of the migration-review path. An
/// enrolled profile is present and active. Collapsing the two would let the
/// surface describe a live profile as "stored material Preview will not
/// activate", which is the exact reassurance this product must not get wrong.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewProfileSnapshot {
    state: &'static str,
    profile_present: Option<bool>,
    profile_active: Option<bool>,
    guided_enrollment: GuidedEnrollmentStatus,
}

impl PreviewProfileSnapshot {
    /// The empty-evidence evaluation used wherever the lifecycle could not
    /// answer: it truthfully reports `blocked` with the first enforced step.
    /// The expected encoder is `None` because there is no evidence to
    /// mislabel; real evidence flows through `baseline_from_store` instead.
    fn guidance() -> GuidedEnrollmentStatus {
        evaluate_enrollment_evidence(&EnrollmentEvidence::default(), None)
    }

    fn unavailable() -> Self {
        Self {
            state: "unavailable",
            profile_present: None,
            profile_active: None,
            guided_enrollment: Self::guidance(),
        }
    }

    /// The lifecycle answered, so both profile facts are known.
    fn baseline(profile_present: bool, profile_active: bool) -> Self {
        Self {
            state: "baseline-ready",
            profile_present: Some(profile_present),
            profile_active: Some(profile_active),
            guided_enrollment: Self::guidance(),
        }
    }

    /// The lifecycle answered and the sitting evidence store was read. The
    /// expected encoder digest comes from the verified runtime manifest, via
    /// the runtime identity captured at worker spawn.
    ///
    /// With recorded evidence and no verified encoder identity the snapshot
    /// refuses (`needs-attention`) instead of evaluating: evaluated with
    /// `None`, a uniformly stale checkpoint would read as a working choice
    /// screen while `load_profile` refuses all of it. Before the worker has
    /// spawned the store is empty in any packaged build, so the honest
    /// empty-evidence evaluation still renders.
    fn baseline_from_store(
        profile_present: bool,
        profile_active: bool,
        evidence: EnrollmentEvidence,
        expected_encoder_sha256: Option<&str>,
    ) -> Self {
        let evidence_empty = evidence.sittings.is_empty() && evidence.negative_sources.is_empty();
        if expected_encoder_sha256.is_none() && !evidence_empty {
            return Self::lifecycle_unreadable("needs-attention");
        }
        Self {
            state: "baseline-ready",
            profile_present: Some(profile_present),
            profile_active: Some(profile_active),
            guided_enrollment: evaluate_enrollment_evidence(&evidence, expected_encoder_sha256),
        }
    }

    /// The lifecycle refused to answer. Neither profile fact is known, and
    /// neither may be guessed: an unread profile is not an absent one.
    fn lifecycle_unreadable(state: &'static str) -> Self {
        Self {
            state,
            profile_present: None,
            profile_active: None,
            guided_enrollment: Self::guidance(),
        }
    }
}

/// Content-free sentences for why the dedicated sitting recorder cannot
/// start. Each names the actual boundary; none invites retrying around it.
const RECORDER_REASON_RUNTIME_UNKNOWN: &str =
    "The verified runtime identity is not available yet, so a setup recording cannot start.";
const RECORDER_REASON_NO_ENCODER: &str = "This build does not yet include an approved \
     voice-measurement model, so a setup recording cannot be saved. Recording opens in a \
     build where that model has passed its checks.";
const RECORDER_REASON_STATUS_UNAVAILABLE: &str =
    "Voice profile status is unavailable, so a setup recording cannot start.";
const RECORDER_REASON_SITTING_ACTIVE: &str = "A setup recording is already in progress.";
const RECORDER_REASON_DERIVING: &str =
    "The recording finished. The app is deriving voice material from it now.";

/// Content-free completion sentences for the most recent setup recording.
/// Each states only the evidence store's own lifecycle fact; none carries
/// audio, timing, or transcript-derived content.
const SITTING_OUTCOME_SAVED: &str =
    "The recording was saved: voice material is stored and the temporary recording was deleted.";
const SITTING_OUTCOME_CLEANUP_PENDING: &str =
    "Voice material is stored. The app still has to delete the temporary recording.";
const SITTING_OUTCOME_RAW_RETAINED: &str = "The recording finished. The app could not derive \
     voice material yet; the temporary recording is kept until that completes.";
const SITTING_OUTCOME_REHEARSAL: &str = "The recording did not finish and was set aside as a \
     rehearsal. It does not count toward setup.";
const SITTING_OUTCOME_NOT_STARTED: &str =
    "The setup recording could not start. Nothing was recorded.";

/// One recorded sitting, content-free: an identifier, what kind of material
/// it is, and where it sits in the evidence lifecycle. No audio digest,
/// timing, or transcript-derived value crosses this surface.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSittingSummary {
    sitting_id: String,
    kind: &'static str,
    source_class: Option<String>,
    state: &'static str,
}

/// The recorder half of the Voice profile screen. Recording opens only in a
/// build whose verified runtime carries the admitted encoder — the boundary
/// sentence names the actual reason in every other lane — and the sittings
/// list is the durable evidence store's projection. `last_outcome` is the
/// content-free completion sentence for the most recent recording attempt.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewEnrollmentSurface {
    recording_available: bool,
    recording_unavailable_reason: Option<&'static str>,
    sittings: Vec<PreviewSittingSummary>,
    last_outcome: Option<&'static str>,
    /// True from the moment a take claims the task slot until its thread's
    /// final refresh. It outlives the recording-in-progress row — capture
    /// closes before derivation — so the surface can stay honest, and keep
    /// polling, through the derive window.
    attempt_active: bool,
}

impl PreviewEnrollmentSurface {
    fn unavailable() -> Self {
        Self {
            recording_available: false,
            recording_unavailable_reason: Some(RECORDER_REASON_STATUS_UNAVAILABLE),
            sittings: Vec::new(),
            last_outcome: None,
            attempt_active: false,
        }
    }
}

/// Control handle for the dedicated-sitting capture thread. The sender is the
/// operator's Stop; the driver treats a vanished stop channel as a
/// control-plane fault, so the handle stays in place until the thread clears
/// it on the way out. Stop deliberately avoids `command_lock`, so it never
/// queues behind whatever command is in flight — the one signal an operator
/// mid-take must always be able to land.
struct SittingTaskControl {
    sitting_id: String,
    sender: mpsc::Sender<()>,
}

fn clear_sitting_task(state: &ApplicationState, sitting_id: &str) {
    if let Ok(mut active) = state.sitting_task.lock() {
        if active
            .as_ref()
            .is_some_and(|control| control.sitting_id == sitting_id)
        {
            *active = None;
        }
    }
}

/// Whether a speech or note model may be swapped right now. The mic and the
/// system tap are released once a transcript is on screen, and opening a
/// past meeting from the library restores the reducer to `TranscriptReady`
/// (see `apply_restored_transcript_projection`), so that state is idle for
/// this purpose. Treating it as "a meeting in progress" disabled Settings
/// with "Finish the current meeting" while the operator was only reading
/// (roadmap D-GATE, found on the 88da6b6 installed captures).
fn model_change_audio_idle(capture: CaptureState, sitting_task_active: bool) -> bool {
    matches!(capture, CaptureState::Idle | CaptureState::TranscriptReady) && !sitting_task_active
}

fn sitting_task_active(state: &ApplicationState) -> bool {
    state
        .sitting_task
        .lock()
        .map(|active| active.is_some())
        .unwrap_or(true)
}

#[cfg(target_os = "macos")]
fn sitting_kind_label(
    kind: local_meeting_notes_session_core::sitting_evidence::SittingKind,
) -> &'static str {
    use local_meeting_notes_session_core::sitting_evidence::SittingKind;
    match kind {
        SittingKind::OperatorSitting => "operator-sitting",
        SittingKind::NegativeSource => "negative-source",
    }
}

#[cfg(target_os = "macos")]
fn sitting_state_label(
    state: local_meeting_notes_session_core::sitting_evidence::SittingLifecycleState,
) -> &'static str {
    use local_meeting_notes_session_core::sitting_evidence::SittingLifecycleState;
    match state {
        SittingLifecycleState::RecordingInProgress => "recording-in-progress",
        SittingLifecycleState::RawRetained => "raw-retained",
        SittingLifecycleState::CleanupPending => "cleanup-pending",
        SittingLifecycleState::Saved => "saved",
        SittingLifecycleState::Rehearsal => "rehearsal",
    }
}

fn apply_restored_transcript_projection(
    model: &mut AppModel,
    projection: RestoredTranscriptProjection,
) -> Result<(), String> {
    model
        .reducer
        .restore_capture_projection(CaptureState::TranscriptReady)
        .map_err(error_text)?;
    model.meeting_id = Some(projection.meeting_id);
    model.turns = projection.turns;
    model.current_transcript_sha256 = Some(projection.current_transcript_sha256);
    model.warnings = projection.warnings;
    Ok(())
}

#[derive(Clone)]
struct StorageContext {
    storage: StorageRoot,
    resource_root: PathBuf,
    manifest_path: PathBuf,
    diagnostics: PathBuf,
}

fn with_meeting_storage_sequence<T>(
    coordination: &MeetingStorageCoordination,
    operation: impl FnOnce(&HashSet<String>) -> T,
) -> Result<T, ()> {
    let sequence = coordination.lock_sequence().map_err(|_| ())?;
    let active_meeting_ids = sequence.active_meeting_ids().map_err(|_| ())?;
    let response = operation(&active_meeting_ids);
    drop(sequence);
    Ok(response)
}

fn preview_storage_clone(state: &ApplicationState) -> Result<StorageRoot, ()> {
    state
        .storage
        .lock()
        .map_err(|_| ())?
        .as_ref()
        .map(|context| context.storage.clone())
        .ok_or(())
}

/// The admitted `note.project` transport for library rebuilds, resolved per
/// the wiring recipe in `docs/note-runtime-decision.md`: catalog from
/// `verified_model_catalog`, manifest paths from the resource root, the
/// admission decision cached (successes only) off the hot rebuild path.
/// Every failure shape degrades to `UnavailableProjector`, which is also
/// today's steady state — the bundle ships no generate manifest and the
/// signed catalog carries no note-model role yet.
fn admitted_note_projector(state: &ApplicationState) -> Arc<dyn NoteProjector> {
    if let Ok(cached) = state.note_projector.lock()
        && let Some(projector) = cached.as_ref()
    {
        return Arc::clone(projector);
    }
    let unavailable: Arc<dyn NoteProjector> = Arc::new(UnavailableProjector);
    let Ok(guard) = state.storage.lock() else {
        return unavailable;
    };
    let Some(context) = guard.clone() else {
        return unavailable;
    };
    drop(guard);
    let Ok(manifest) = RuntimeManifest::load_and_verify(&context.manifest_path) else {
        return unavailable;
    };
    let Ok(Some(catalog)) = verified_model_catalog(&context.manifest_path, &manifest) else {
        return unavailable;
    };
    let Some(projector) = admit_note_projector(
        &context.storage,
        &catalog,
        &context.resource_root.join(PROJECT_MANIFEST_FILE),
        &context.resource_root.join(GENERATE_MANIFEST_FILE),
    ) else {
        return unavailable;
    };
    if let Ok(mut cached) = state.note_projector.lock() {
        *cached = Some(Arc::clone(&projector));
    }
    projector
}

fn with_preview_library_invalidated<T>(
    state: &ApplicationState,
    operation: impl FnOnce() -> T,
) -> Result<T, ()> {
    let mut library = state.preview_library.lock().map_err(|_| ())?;
    *library = None;
    drop(library);
    Ok(operation())
}

impl ApplicationState {
    fn remember_permissions(
        &self,
        received: first_run::FirstRunPermissions,
    ) -> first_run::FirstRunPermissions {
        if received.probe_unavailable {
            return received;
        }
        let mut observation = self
            .permission_observation
            .lock()
            .expect("permission observation lock");
        let merged = first_run::merge_permissions(observation.as_ref(), received);
        *observation = Some(merged.clone());
        merged
    }

    #[allow(dead_code)]
    fn manual_audio_deletion_facade(&self) -> manual_delete_facade::ManualAudioDeletionFacade<'_> {
        manual_delete_facade::ManualAudioDeletionFacade::new(&self.app_data_writer_lock)
    }

    /// Still a real capability (it is what a trash purge ultimately runs
    /// through, via `meeting_trash::purge_trashed_meeting`), but no command in
    /// this app calls it directly anymore — "Delete meeting" now reaches only
    /// [`Self::meeting_trash_facade`].
    #[allow(dead_code)]
    fn whole_meeting_deletion_facade(
        &self,
    ) -> manual_delete_facade::WholeMeetingDeletionFacade<'_> {
        manual_delete_facade::WholeMeetingDeletionFacade::new(&self.app_data_writer_lock)
    }

    fn meeting_trash_facade(&self) -> manual_delete_facade::MeetingTrashFacade<'_> {
        manual_delete_facade::MeetingTrashFacade::new(&self.app_data_writer_lock)
    }

    fn meeting_restore_facade(&self) -> manual_delete_facade::MeetingRestoreFacade<'_> {
        manual_delete_facade::MeetingRestoreFacade::new(&self.app_data_writer_lock)
    }

    fn transcript_deletion_facade(&self) -> manual_delete_facade::TranscriptDeletionFacade<'_> {
        manual_delete_facade::TranscriptDeletionFacade::new(&self.app_data_writer_lock)
    }

    fn meeting_storage_coordination(&self) -> Result<Arc<MeetingStorageCoordination>, String> {
        self.app_data_writer_lock
            .lock()
            .map_err(|_| "the app-data writer lock is unavailable".to_string())?
            .as_ref()
            .map(|writer| writer.coordination())
            .ok_or_else(|| "the app-data writer lock is unavailable".to_string())
    }

    fn with_preview_library<T>(
        &self,
        unavailable: impl Fn() -> T,
        operation: impl FnOnce(&mut library_reader::LibraryReader, &HashSet<String>) -> T,
    ) -> T {
        let coordination = match self.meeting_storage_coordination() {
            Ok(coordination) => coordination,
            Err(_) => return unavailable(),
        };
        with_meeting_storage_sequence(&coordination, |active_meeting_ids| {
            let mut library = match self.preview_library.lock() {
                Ok(library) => library,
                Err(_) => return unavailable(),
            };
            let Some(reader) = library.as_mut() else {
                return unavailable();
            };
            operation(reader, active_meeting_ids)
        })
        .unwrap_or_else(|_| unavailable())
    }
}

#[derive(Clone)]
struct RuntimeIdentity {
    admission: String,
    worker_build_sha256: String,
    worker_executable_sha256: String,
    transcript_model_identity: String,
    tap_build_sha256: String,
    tap_path: PathBuf,
    /// The digest the verified manifest records for the speaker encoder.
    /// Today that is the `encoder-unavailable.identity` placeholder; guided
    /// enrolment compares derived material against exactly this value, so a
    /// build without a real encoder truthfully refuses rather than guessing.
    encoder_sha256: String,
    /// Every `models[]` entry the verified manifest records, as `(id, sha256)`.
    ///
    /// Carried so the embedding pass can ask whether the model this build
    /// packages is the model its vectors describe. That comparison is the whole
    /// reason `EmbedderIdentity` holds digests, and until 2026-08-08 nothing
    /// made it — the identity asserted a fact instead of testing one.
    packaged_models: Vec<(String, String)>,
    /// Whether the manifest names a real encoder resource at all.
    /// `worker/build_runtime.sh` deliberately records the
    /// `encoder-unavailable.identity` placeholder file when no speaker
    /// encoder is packaged — the file name is the build's own declared
    /// signal, so the recorder surface derives its honest boundary from it
    /// instead of hardcoding the placeholder's digest.
    encoder_available: bool,
}

struct TranscriptionExecutorControl {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

struct CaptureTaskControl {
    meeting_id: String,
    sender: mpsc::SyncSender<CaptureTaskCommand>,
}

struct CaptureTaskRegistration {
    app: AppHandle,
    meeting_id: String,
}

impl Drop for CaptureTaskRegistration {
    fn drop(&mut self) {
        let state = self.app.state::<ApplicationState>();
        clear_capture_task(&state, &self.meeting_id);
    }
}

enum CaptureTaskCommand {
    /// Release both audio sources without ending the take.
    Pause,
    /// Reacquire both sources into the same take.
    Resume,
    Stop,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartAttestation {
    participants_consented: bool,
    headphones: bool,
    operator_alone: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CaptureAttemptReceipt {
    schema: String,
    meeting_id: String,
    attempt_id: String,
    created_at_epoch_seconds: u64,
    application_build_sha256: String,
    participant_notice_version: String,
    operator_attestation: StartAttestation,
    retention_policy_sha256: String,
}

struct AttemptContext {
    meeting_dir: PathBuf,
    application_build_sha256: String,
}

#[derive(Debug)]
enum WorkerCallError {
    Rejected,
    Supervisor(String),
}

/// The strict-loader bridge behind `preview_enrollment_build_profile`,
/// registered 2026-08-05 with the operator's profile-build decision: the
/// worker validates the canonical profile semantics and answers with
/// digests; the lifecycle publishes only descriptor-reopened bytes.
#[cfg(target_os = "macos")]
struct StrictProfileEnrollmentWorker<'a> {
    state: &'a ApplicationState,
}

#[cfg(target_os = "macos")]
impl ProfileEnrollmentWorker for StrictProfileEnrollmentWorker<'_> {
    fn inspect_candidate(
        &self,
        operation_id: &str,
    ) -> Result<String, ProfileEnrollmentWorkerError> {
        profile_worker_digest(
            self.state,
            Operation::ProfileInspect,
            json!({ "profile_id": operation_id }),
        )
    }

    fn discard_candidate(
        &self,
        operation_id: &str,
        profile_sha256: &str,
    ) -> Result<String, ProfileEnrollmentWorkerError> {
        profile_worker_digest(
            self.state,
            Operation::ProfileDiscard,
            json!({
                "profile_id": operation_id,
                "profile_sha256": profile_sha256,
            }),
        )
    }
}

#[cfg(target_os = "macos")]
fn profile_worker_digest(
    state: &ApplicationState,
    operation: Operation,
    arguments: Value,
) -> Result<String, ProfileEnrollmentWorkerError> {
    let values =
        request_worker(state, operation, arguments, WORKER_REQUEST_TIMEOUT).map_err(|error| {
            match error {
                WorkerCallError::Rejected => ProfileEnrollmentWorkerError::Refused,
                WorkerCallError::Supervisor(_) => ProfileEnrollmentWorkerError::Unavailable,
            }
        })?;
    let values =
        exact_digests(&values, &["profile"]).map_err(|_| ProfileEnrollmentWorkerError::Refused)?;
    values
        .get("profile")
        .cloned()
        .ok_or(ProfileEnrollmentWorkerError::Refused)
}

impl WorkerCallError {
    fn is_supervisor(&self) -> bool {
        matches!(self, Self::Supervisor(_))
    }
}

impl std::fmt::Display for WorkerCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected => formatter.write_str("worker rejected the operation"),
            Self::Supervisor(detail) => formatter.write_str(detail),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CaptureEvent {
    /// The helper's pre-start safe state, before it has opened any hardware.
    /// Distinct from `Suspended`, which is an operator pause mid-take.
    Paused,
    Recording,
    /// Both sources released mid-take; nothing is reaching the audio files.
    Suspended,
    /// Both sources reacquired into the same audio files.
    Resumed,
    Finalized {
        mic_samples: u64,
        system_samples: u64,
    },
    /// The dedicated-sitting helper mode records the mic leg only, so its
    /// finalized receipt carries exactly one leg. A meeting capture must
    /// never accept this shape, and a sitting capture must never accept the
    /// two-leg shape.
    FinalizedMicOnly {
        mic_samples: u64,
    },
    Failed {
        code: String,
    },
    Interrupted,
}

enum CaptureStreamItem {
    Event(CaptureEvent),
    ProtocolFailure,
    Closed,
}

struct CaptureProcess {
    child: Option<Child>,
    control: Option<File>,
    liveness: Option<File>,
    events: mpsc::Receiver<CaptureStreamItem>,
    reader_thread: Option<JoinHandle<()>>,
    process_group_id: i32,
    finished: bool,
}

impl CaptureProcess {
    fn spawn(
        executable: &Path,
        capture_directory: &Path,
        process_group_id: i32,
    ) -> Result<Self, String> {
        if !executable.is_file() || process_group_id <= 0 {
            return Err("capture helper is unavailable".into());
        }
        let capture_directory_file = File::open(capture_directory).map_err(error_text)?;
        Self::spawn_with_mode(
            executable,
            "--capture-dir-fd",
            capture_directory_file,
            process_group_id,
        )
    }

    /// Spawns the helper in dedicated-sitting mode and returns the read end
    /// of its mic PCM stream alongside the process. The write end lives only
    /// in the child after spawn, so end-of-stream arrives exactly when the
    /// helper process exits — never earlier.
    #[cfg(target_os = "macos")]
    fn spawn_sitting(executable: &Path, process_group_id: i32) -> Result<(Self, File), String> {
        let (audio_read, audio_write) = cloexec_pipe().map_err(error_text)?;
        let process = Self::spawn_with_mode(
            executable,
            "--sitting-audio-fd",
            audio_write,
            process_group_id,
        )?;
        Ok((process, audio_read))
    }

    fn spawn_with_mode(
        executable: &Path,
        mode_flag: &'static str,
        mode_file: File,
        process_group_id: i32,
    ) -> Result<Self, String> {
        if !executable.is_file() || process_group_id <= 0 {
            return Err("capture helper is unavailable".into());
        }
        let (control_read, control_write) = cloexec_pipe().map_err(error_text)?;
        let (event_read, event_write) = cloexec_pipe().map_err(error_text)?;
        let (liveness_read, liveness_write) = cloexec_pipe().map_err(error_text)?;
        let inherited = [
            mode_file.as_raw_fd(),
            control_read.as_raw_fd(),
            event_write.as_raw_fd(),
            liveness_read.as_raw_fd(),
        ];
        let mut command = Command::new(executable);
        command
            .arg(mode_flag)
            .arg(inherited[0].to_string())
            .arg("--control-fd")
            .arg(inherited[1].to_string())
            .arg("--event-fd")
            .arg(inherited[2].to_string())
            .arg("--parent-liveness-fd")
            .arg(inherited[3].to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, process_group_id) != 0 {
                    return Err(io::Error::last_os_error());
                }
                for descriptor in inherited {
                    set_close_on_exec(descriptor, false)?;
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(error_text)?;
        drop(mode_file);
        drop(control_read);
        drop(event_write);
        drop(liveness_read);

        let (sender, events) = mpsc::channel();
        let reader_thread = match std::thread::Builder::new()
            .name("meeting-capture-events".into())
            .spawn(move || read_capture_events(event_read, sender))
        {
            Ok(thread) => thread,
            Err(error) => {
                drop(control_write);
                drop(liveness_write);
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        };
        Ok(Self {
            child: Some(child),
            control: Some(control_write),
            liveness: Some(liveness_write),
            events,
            reader_thread: Some(reader_thread),
            process_group_id,
            finished: false,
        })
    }

    fn pid(&self) -> Result<u32, String> {
        self.child
            .as_ref()
            .map(Child::id)
            .ok_or_else(|| "capture helper is no longer running".into())
    }

    fn send(&mut self, command: u8) -> Result<(), String> {
        self.control
            .as_mut()
            .ok_or_else(|| "capture control channel is closed".to_string())?
            .write_all(&[command])
            .map_err(error_text)
    }

    fn receive_until(&self, deadline: Instant) -> Result<CaptureEvent, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("capture helper timed out".into());
        }
        match self.events.recv_timeout(remaining) {
            Ok(CaptureStreamItem::Event(event)) => Ok(event),
            Ok(CaptureStreamItem::ProtocolFailure) => {
                Err("capture helper returned an invalid event".into())
            }
            Ok(CaptureStreamItem::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("capture helper exited before completing".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err("capture helper timed out".into()),
        }
    }

    fn receive_briefly(&self, timeout: Duration) -> Result<Option<CaptureEvent>, String> {
        match self.events.recv_timeout(timeout) {
            Ok(CaptureStreamItem::Event(event)) => Ok(Some(event)),
            Ok(CaptureStreamItem::ProtocolFailure) => {
                Err("capture helper returned an invalid event".into())
            }
            Ok(CaptureStreamItem::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("capture helper exited while recording".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        }
    }

    fn finish_cleanly(&mut self, deadline: Instant) -> Result<(), String> {
        let Some(status) = self.wait_for_exit(deadline).map_err(error_text)? else {
            self.cleanup();
            return Err("capture helper did not exit after finalization".into());
        };
        if !status.success() {
            self.cleanup();
            return Err("capture helper reported an unsuccessful exit".into());
        }
        self.control.take();
        self.liveness.take();
        self.join_reader()?;
        self.finished = true;
        Ok(())
    }

    fn wait_for_exit(&mut self, deadline: Instant) -> io::Result<Option<ExitStatus>> {
        loop {
            let Some(child) = self.child.as_mut() else {
                return Ok(None);
            };
            if let Some(status) = child.try_wait()? {
                let _ = child.wait();
                self.child.take();
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn join_reader(&mut self) -> Result<(), String> {
        if let Some(thread) = self.reader_thread.take() {
            thread
                .join()
                .map_err(|_| "capture event reader failed".to_string())?;
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        self.control.take();
        self.liveness.take();
        let first_deadline = Instant::now() + Duration::from_millis(750);
        if self.wait_for_exit(first_deadline).ok().flatten().is_none() {
            let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
            let second_deadline = Instant::now() + Duration::from_millis(750);
            if self.wait_for_exit(second_deadline).ok().flatten().is_none() {
                let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
                if let Some(mut child) = self.child.take() {
                    let _ = child.wait();
                }
            }
        }
        let _ = self.join_reader();
        self.finished = true;
    }
}

impl Drop for CaptureProcess {
    fn drop(&mut self) {
        if !self.finished {
            self.cleanup();
        }
    }
}

/// A completed dedicated-sitting capture: the evidence store holds the
/// finalized raw recording, and the helper's finalized receipt attested the
/// same sample count the parent actually drained from the stream.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
struct SittingCaptureReceipt {
    mic_samples: u64,
}

/// Content-free sitting capture failure. `code` mirrors the meeting capture
/// failure codes (or relays the helper's own code); `detail` never carries
/// audio or transcript content.
#[cfg(target_os = "macos")]
#[derive(Debug)]
#[allow(dead_code)] // the registered surface stays content-free; fields feed tests and diagnostics
struct SittingCaptureFailure {
    code: String,
    detail: String,
}

#[cfg(target_os = "macos")]
fn sitting_failure(code: &str, detail: impl std::fmt::Display) -> SittingCaptureFailure {
    SittingCaptureFailure {
        code: code.into(),
        detail: detail.to_string(),
    }
}

/// Records one dedicated enrolment sitting through the capture helper's
/// mic-only mode and admits it into the sitting evidence store.
///
/// The store stays the only writer of durable sitting bytes: the helper
/// streams PCM over a pipe and every drained chunk goes through
/// `SittingEvidenceAuthority::append_raw_audio`. Admission authority is the
/// helper's `finalized` receipt — the capture is finalized only when the
/// receipt's sample count matches the bytes actually drained. End-of-stream
/// and a clean helper exit are never treated as completion, and a vanished
/// stop channel is a control-plane fault rather than an implied Stop; every
/// outcome other than a matching receipt after an explicit Stop abandons the
/// sitting, which the store labels a rehearsal.
///
/// Registered behind `preview_enrollment_start_sitting` on 2026-08-04 by the
/// operator's guided-enrollment registration decision; helper identity
/// attestation beyond the meeting path's checks remains future work.
#[cfg(target_os = "macos")]
fn run_sitting_capture(
    authority: &SittingEvidenceAuthority<'_>,
    executable: &Path,
    process_group_id: i32,
    sitting_id: &str,
    kind: local_meeting_notes_session_core::sitting_evidence::SittingKind,
    source_class: Option<&str>,
    stop: &mpsc::Receiver<()>,
) -> Result<SittingCaptureReceipt, SittingCaptureFailure> {
    authority
        .begin_sitting(sitting_id, kind, source_class, now_epoch_seconds())
        .map_err(|error| sitting_failure("sitting_begin_refused", error))?;
    match drive_sitting_helper(authority, executable, process_group_id, sitting_id, stop) {
        Ok(receipt) => match authority.finalize_capture(sitting_id, now_epoch_seconds()) {
            Ok(()) => Ok(receipt),
            Err(error) => {
                let _ = authority.abandon_sitting(sitting_id, now_epoch_seconds());
                Err(sitting_failure("sitting_finalize_refused", error))
            }
        },
        Err(failure) => {
            let _ = authority.abandon_sitting(sitting_id, now_epoch_seconds());
            Err(failure)
        }
    }
}

#[cfg(target_os = "macos")]
fn drive_sitting_helper(
    authority: &SittingEvidenceAuthority<'_>,
    executable: &Path,
    process_group_id: i32,
    sitting_id: &str,
    stop: &mpsc::Receiver<()>,
) -> Result<SittingCaptureReceipt, SittingCaptureFailure> {
    let (mut helper, audio) = CaptureProcess::spawn_sitting(executable, process_group_id)
        .map_err(|error| sitting_failure("sitting_helper_spawn_failed", error))?;
    match helper.receive_until(Instant::now() + Duration::from_secs(10)) {
        Ok(CaptureEvent::Paused) => {}
        Ok(_) => {
            return Err(sitting_failure(
                "sitting_helper_bad_pause",
                "capture helper did not begin in paused state",
            ));
        }
        Err(error) => return Err(sitting_failure("sitting_helper_pause_failed", error)),
    }
    helper
        .send(b'S')
        .map_err(|error| sitting_failure("sitting_start_signal_failed", error))?;
    match helper.receive_until(Instant::now() + Duration::from_secs(10)) {
        Ok(CaptureEvent::Recording) => {}
        Ok(CaptureEvent::Failed { code }) => {
            return Err(sitting_failure(
                &code,
                "capture helper failed before recording",
            ));
        }
        Ok(_) => {
            return Err(sitting_failure(
                "sitting_helper_bad_arm",
                "capture helper skipped the recording event",
            ));
        }
        Err(error) => return Err(sitting_failure("sitting_helper_arm_failed", error)),
    }
    set_nonblocking(audio.as_raw_fd())
        .map_err(|error| sitting_failure("sitting_stream_setup_failed", error))?;

    let mut drained: u64 = 0;
    let mut receipt: Option<u64> = None;
    let mut events_open = true;
    let mut stream_open = true;
    let mut stop_deadline: Option<Instant> = None;
    let mut buffer = vec![0_u8; 64 * 1024];
    while stream_open {
        let mut readiness = libc::pollfd {
            fd: audio.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut readiness, 1, 100) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(sitting_failure("sitting_stream_poll_failed", error));
            }
        } else if ready > 0 {
            loop {
                match (&audio).read(&mut buffer) {
                    Ok(0) => {
                        stream_open = false;
                        break;
                    }
                    Ok(read) => {
                        drained += read as u64;
                        if drained > SITTING_STREAM_MAX_BYTES {
                            return Err(sitting_failure(
                                "sitting_stream_overlong",
                                "the sitting stream exceeded the supported duration",
                            ));
                        }
                        authority
                            .append_raw_audio(sitting_id, &buffer[..read])
                            .map_err(|error| {
                                sitting_failure("sitting_store_append_refused", error)
                            })?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        return Err(sitting_failure("sitting_stream_read_failed", error));
                    }
                }
            }
        }
        while events_open && stream_open {
            match helper.events.try_recv() {
                Ok(CaptureStreamItem::Event(event)) => {
                    handle_sitting_event(event, &mut receipt, stop_deadline.is_some())?;
                }
                Ok(CaptureStreamItem::ProtocolFailure) => {
                    return Err(sitting_failure(
                        "sitting_event_invalid",
                        "capture helper emitted an invalid event",
                    ));
                }
                Ok(CaptureStreamItem::Closed) | Err(mpsc::TryRecvError::Disconnected) => {
                    events_open = false;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        if stream_open {
            match stop_deadline {
                None => {
                    let requested = match stop.try_recv() {
                        Ok(()) => true,
                        // A vanished stop channel is a control-plane fault,
                        // not an operator Stop: admitting the take here would
                        // let a panicked caller turn a partial recording into
                        // completed enrolment evidence. The meeting loop
                        // refuses the identical condition
                        // (capture_control_disconnected).
                        Err(mpsc::TryRecvError::Disconnected) => {
                            return Err(sitting_failure(
                                "sitting_control_disconnected",
                                "the sitting stop channel vanished before Stop",
                            ));
                        }
                        Err(mpsc::TryRecvError::Empty) => false,
                    };
                    if requested {
                        // Flag before byte: the receipt-ordering refusal in
                        // handle_sitting_event documents that the stop flag
                        // is set before the helper can observe X, so keep
                        // the assignment ahead of the send that makes X
                        // observable. (A send failure returns immediately;
                        // the already-set deadline leaks nothing.)
                        stop_deadline = Some(Instant::now() + CAPTURE_STOP_TIMEOUT);
                        helper.send(b'X').map_err(|error| {
                            sitting_failure("sitting_stop_signal_failed", error)
                        })?;
                    }
                }
                Some(deadline) if Instant::now() >= deadline => {
                    return Err(sitting_failure(
                        "sitting_stop_timeout",
                        "capture helper did not close the stream after Stop",
                    ));
                }
                Some(_) => {}
            }
        }
    }

    // The stream is closed, but the finalized receipt may still be in flight
    // on the event pipe.
    let receipt_deadline = Instant::now() + Duration::from_secs(5);
    while receipt.is_none() && events_open {
        let remaining = receipt_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match helper.events.recv_timeout(remaining) {
            Ok(CaptureStreamItem::Event(event)) => {
                handle_sitting_event(event, &mut receipt, stop_deadline.is_some())?
            }
            Ok(CaptureStreamItem::ProtocolFailure) => {
                return Err(sitting_failure(
                    "sitting_event_invalid",
                    "capture helper emitted an invalid event",
                ));
            }
            Ok(CaptureStreamItem::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                events_open = false;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
        }
    }
    helper
        .finish_cleanly(Instant::now() + Duration::from_secs(5))
        .map_err(|error| sitting_failure("sitting_helper_exit_failed", error))?;
    // The stream must not end before the parent requested Stop. A receipt
    // arriving pre-Stop is already refused at the event itself, so reaching
    // here with no Stop sent means the helper simply walked away — also not
    // its call to make.
    if stop_deadline.is_none() {
        return Err(sitting_failure(
            "sitting_finalize_without_stop",
            "the stream ended before Stop was requested; a self-finalizing helper is not admission authority",
        ));
    }
    let Some(mic_samples) = receipt else {
        return Err(sitting_failure(
            "sitting_finalize_receipt_missing",
            "the stream ended without a finalized receipt; end of stream is not completion authority",
        ));
    };
    if mic_samples == 0 || Some(drained) != mic_samples.checked_mul(2) {
        return Err(sitting_failure(
            "sitting_sample_count_mismatch",
            format!(
                "the finalized receipt attests {mic_samples} samples but {drained} bytes were streamed"
            ),
        ));
    }
    Ok(SittingCaptureReceipt { mic_samples })
}

/// The only events a sitting helper may emit after recording begins: its own
/// mic-only finalized receipt (once, and only after the parent sent Stop),
/// or a fault. The meeting-shaped two-leg receipt and repeated lifecycle
/// events are protocol violations.
///
/// A receipt observed while `stop_requested` is false is refused outright:
/// the driver sets its stop flag locally before the helper can possibly
/// observe the X byte, so a legitimate finalize can never precede it — but a
/// helper finalizing on its own initiative could truncate the take, hold the
/// stream open until the operator's eventual Stop, and exit with every
/// end-of-stream gate green. The receipt itself is where that ordering is
/// enforceable race-free.
#[cfg(target_os = "macos")]
fn handle_sitting_event(
    event: CaptureEvent,
    receipt: &mut Option<u64>,
    stop_requested: bool,
) -> Result<(), SittingCaptureFailure> {
    match event {
        CaptureEvent::FinalizedMicOnly { mic_samples } => {
            if !stop_requested {
                return Err(sitting_failure(
                    "sitting_finalize_before_stop",
                    "the helper finalized before Stop was requested; a self-finalizing helper is not admission authority",
                ));
            }
            if receipt.replace(mic_samples).is_some() {
                return Err(sitting_failure(
                    "sitting_helper_protocol_violation",
                    "capture helper finalized twice",
                ));
            }
            Ok(())
        }
        CaptureEvent::Failed { code } => Err(sitting_failure(
            &code,
            "capture helper reported a recording fault",
        )),
        CaptureEvent::Interrupted => Err(sitting_failure(
            "sitting_capture_interrupted",
            "capture helper reported interruption",
        )),
        // A setup sitting has no pause control, so Suspended and Resumed are
        // as much a protocol violation here as a two-leg finalize would be.
        CaptureEvent::Paused
        | CaptureEvent::Recording
        | CaptureEvent::Suspended
        | CaptureEvent::Resumed
        | CaptureEvent::Finalized { .. } => Err(sitting_failure(
            "sitting_helper_protocol_violation",
            "capture helper emitted an event outside the sitting protocol",
        )),
    }
}

#[tauri::command]
fn app_snapshot(state: State<'_, ApplicationState>) -> AppSnapshot {
    state
        .model
        .lock()
        .expect("application model lock")
        .snapshot()
}

#[tauri::command]
fn start_meeting(
    app: AppHandle,
    retention_days: u64,
    attestation: StartAttestation,
    journey_timing: capture_timing::JourneyTiming,
) -> Result<AppSnapshot, String> {
    validate_start_request(retention_days, &attestation)?;
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    stop_owned_audio_playback(&state);
    if state.model_install_active.load(Ordering::SeqCst) {
        return Err("Wait for the speech model change to finish before starting a meeting.".into());
    }
    if transcription_capacity_full(&state) {
        return Err("The local transcription queue is full. Wait for the previous meeting to finish processing.".into());
    }
    let meeting_id = Uuid::new_v4().to_string();
    let attempt_id = Uuid::new_v4().to_string();
    // Room for a Stop to queue behind a pause change the helper has not
    // confirmed yet. Ending a meeting must never be refused because the
    // operator paused a moment earlier.
    let (sender, receiver) = mpsc::sync_channel(4);
    let snapshot = {
        let mut model = state.model.lock().expect("application model lock");
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("A meeting cannot start from the current state.".into());
        }
        if !model.retention_operational {
            return Err("Audio retention needs attention before another meeting can start.".into());
        }
        let mut active = state.capture_task.lock().expect("capture task lock");
        if active.is_some() {
            return Err("Another capture attempt is already active.".into());
        }
        // A meeting starting mid-take would make the take's remaining
        // evidence operations refuse (the store excludes active meetings),
        // so the meeting is what refuses here.
        if sitting_task_active(&state) {
            return Err("Finish the setup recording before starting a meeting.".into());
        }
        transition_capture(&mut model, CaptureState::Arming)?;
        model.clear_meeting_projection();
        model.meeting_id = Some(meeting_id.clone());
        *active = Some(CaptureTaskControl {
            meeting_id: meeting_id.clone(),
            sender,
        });
        model.snapshot()
    };
    let task_app = app.clone();
    let spawn_failure_meeting_id = meeting_id.clone();
    // t3: this command returning (Arming accepted) is the boundary packet
    // W7-C's app-latency span is measured from the other side of -- see
    // `capture_timing`. Stamped here, before the spawn can fail, so a spawn
    // failure (which never reaches `CaptureState::Recording`) simply never
    // gets to use it.
    let t3_epoch_ms = capture_timing::now_epoch_millis();
    std::thread::Builder::new()
        .name("meeting-capture-attempt".into())
        .spawn(move || {
            run_capture_task(
                task_app,
                meeting_id,
                attempt_id,
                retention_days,
                attestation,
                receiver,
                journey_timing,
                t3_epoch_ms,
            )
        })
        .map_err(|error| {
            fail_capture_task(
                &app,
                None,
                false,
                true,
                "capture_task_spawn_failed",
                &error.to_string(),
                "The recording task could not start.",
            );
            clear_capture_task(&app.state::<ApplicationState>(), &spawn_failure_meeting_id);
            "The recording task could not start.".to_string()
        })?;
    // Arms the note-capture hotkey for the lifetime of this attempt. Framed
    // against the attempt starting (Arming), not the later CaptureState::
    // Recording transition inside run_capture_task's own thread, since that
    // transition is internal to a sibling packet's owned code. Any failure
    // between here and Recording still ends the attempt through
    // fail_capture_task, which disarms it.
    capture_shortcut::activate(&app);
    Ok(snapshot)
}

fn transcription_capacity_full(state: &ApplicationState) -> bool {
    let storage = state
        .storage
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|context| context.storage.clone()));
    let Some(storage) = storage else {
        return true;
    };
    TranscriptionQueue::open(&storage)
        .and_then(|queue| queue.discover())
        .map(|discovery| {
            discovery
                .items
                .into_iter()
                .filter(|item| {
                    item.terminal.is_none() && item.commit.is_none() && item.result.is_none()
                })
                .count()
                >= 2
        })
        .unwrap_or(true)
}

/// Releases both audio sources without ending the meeting.
///
/// The reducer is not moved here. The capture task moves it when the helper
/// confirms that both sources are released, so the recorder never shows
/// "Paused" over a microphone that is still open.
#[tauri::command]
fn pause_meeting(app: AppHandle) -> Result<AppSnapshot, String> {
    send_pause_change(app, CaptureTaskCommand::Pause)
}

/// Reacquires both audio sources into the same meeting.
///
/// The reducer moves back to recording only once the helper reports audio
/// flowing again, for the same reason pausing waits.
#[tauri::command]
fn resume_meeting(app: AppHandle) -> Result<AppSnapshot, String> {
    send_pause_change(app, CaptureTaskCommand::Resume)
}

fn send_pause_change(app: AppHandle, command: CaptureTaskCommand) -> Result<AppSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command_lock = state.command_lock.lock().expect("command lock");
    let mut model = state.model.lock().expect("application model lock");
    let (required, refusal) = match command {
        CaptureTaskCommand::Pause => (CaptureState::Recording, "No recording is ready to pause."),
        CaptureTaskCommand::Resume => (CaptureState::Paused, "No recording is ready to resume."),
        CaptureTaskCommand::Stop => return Err("Use Stop to end this meeting.".into()),
    };
    if model.reducer.capture() != required {
        return Err(refusal.into());
    }
    if model.capture_pause_change_pending {
        return Err("Wait for the last pause change to finish.".into());
    }
    let send_result = state
        .capture_task
        .lock()
        .expect("capture task lock")
        .as_ref()
        .ok_or_else(|| "The recording task is unavailable.".to_string())?
        .sender
        .try_send(command);
    if send_result.is_err() {
        model.error = Some("The recording did not accept that change.".into());
        return Err("The recording did not accept that change.".into());
    }
    model.capture_pause_change_pending = true;
    Ok(model.snapshot())
}

#[tauri::command]
fn stop_meeting(app: AppHandle) -> Result<AppSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    stop_owned_audio_playback(&state);
    let mut model = state.model.lock().expect("application model lock");
    // Ending the meeting while paused is ordinary operator behavior: the audio
    // already captured is a real take and finalizes the same way.
    if !matches!(
        model.reducer.capture(),
        CaptureState::Recording | CaptureState::Paused
    ) {
        return Err("No recording is ready to stop.".into());
    }
    transition_capture(&mut model, CaptureState::Stopping)?;
    model.capture_pause_change_pending = false;
    // The operator explicitly asked to stop; disarm now rather than waiting
    // for the eventual Idle transition deep in the capture task.
    capture_shortcut::deactivate(&app);
    let send_result = state
        .capture_task
        .lock()
        .expect("capture task lock")
        .as_ref()
        .ok_or_else(|| "The recording task is unavailable.".to_string())?
        .sender
        .try_send(CaptureTaskCommand::Stop);
    if send_result.is_err() {
        transition_capture(&mut model, CaptureState::RecoveredInterrupted)?;
        model.error = Some("The recording task ended before Stop completed.".into());
        return Err("The recording task ended before Stop completed.".into());
    }
    Ok(model.snapshot())
}

#[tauri::command]
fn dismiss_meeting(app: AppHandle) -> Result<AppSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    stop_owned_audio_playback(&state);
    let mut model = state.model.lock().expect("application model lock");
    if model.reducer.startup() != StartupState::Ready {
        return Err("Finish the installation check before starting another meeting.".into());
    }
    if state
        .capture_task
        .lock()
        .expect("capture task lock")
        .is_some()
    {
        return Err("The current meeting is still finishing.".into());
    }
    if !matches!(
        model.reducer.capture(),
        CaptureState::TranscriptReady
            | CaptureState::TranscriptionFailed
            | CaptureState::RecoveredInterrupted
    ) {
        return Err("The current meeting cannot be dismissed yet.".into());
    }
    transition_capture(&mut model, CaptureState::Idle)?;
    model.clear_meeting_projection();
    // Leaving a meeting always returns to the library. If retention is degraded,
    // that is surfaced as a home-screen condition (Record is held with its
    // reason), never by transitioning startup into the needs-attention blocker
    // — dismissing a meeting must not route the operator into a screen with no
    // way back to their meetings (Order 4: a recovery action must not trap).
    if !model.retention_operational && model.reducer.startup() == StartupState::Ready {
        model.error =
            Some("Audio retention needs attention before another meeting can start.".into());
    }
    Ok(model.snapshot())
}

/// § A menubar presentation: one glyph and one sentence per state, from the
/// same reducer facts the window renders. The load-bearing rule is that
/// `recording` and `degraded` are distinguishable at a glance — the filled
/// glyph gains a persistent mark, never a silent "recording". `detected`
/// and `armed` belong to the future microphone-use detection path and stay
/// dormant; the accent-colored designed glyph waits on a template icon, so
/// the internal alpha renders text glyphs.
fn tray_presentation(
    startup: StartupState,
    capture: CaptureState,
    degraded: bool,
) -> (&'static str, &'static str) {
    match startup {
        StartupState::Ready => match capture {
            // Captured sits after Stopping: the take is committed and
            // nothing is recording, so the filled glyph would be the exact
            // inversion § A forbids. It renders as processing instead.
            CaptureState::Recording | CaptureState::Stopping => {
                if degraded {
                    ("●!", "Recording — one audio channel needs attention")
                } else {
                    ("●", "Recording")
                }
            }
            // The hollow glyph is the load-bearing part: a paused meeting is
            // still open, but nothing is being captured, and the filled glyph
            // would assert the opposite.
            CaptureState::Paused => ("○", "Paused — the meeting is open, nothing is recording"),
            CaptureState::Arming => ("○", "Preparing to record. Nothing is recording yet"),
            CaptureState::Captured | CaptureState::Transcribing | CaptureState::Summarizing => {
                ("◐", "Transcribing the finished recording")
            }
            // A finished meeting the operator has not opened yet is a ready
            // note, not idle silence. Falling into the idle arm below would
            // be true ("nothing is recording") but misleading — it hides the
            // fact that something is waiting to be read. Distinct from the
            // hollow idle glyph and from "◐" (still in progress).
            CaptureState::TranscriptReady | CaptureState::Ready => {
                ("◍", "Your meeting is ready to read.")
            }
            // SummaryFailed persists until the operator acts — its only exit
            // is an explicit retry — so it must carry the error mark, not
            // the all-clear glyph.
            CaptureState::TranscriptionFailed
            | CaptureState::SummaryFailed
            | CaptureState::RecoveredInterrupted => ("×", "The last recording needs attention"),
            _ => ("○", "Nothing is recording"),
        },
        StartupState::ShellRendered | StartupState::Checking | StartupState::Retrying => {
            ("○", "Checking the local runtime. Nothing is recording")
        }
        // First-run model selection is a one-time setup step, not a failure.
        // It gets its own calm arm so it doesn't share the alarming "×" with
        // genuine startup failures (RuntimeMissing, ServiceTimeout, …).
        StartupState::ModelRequired => ("◌", "One-time setup: choose a speech model."),
        _ => ("×", "The app needs attention. Nothing is recording"),
    }
}

/// Whether the tray's "Stop recording" item should be present — the same
/// two `CaptureState` values `stop_meeting` itself requires (see its
/// refusal there: "No recording is ready to stop."). Kept separate from
/// `tray_presentation` (glyph/tooltip) so the two stay independently owned
/// and independently testable.
fn tray_shows_stop_recording(capture: CaptureState) -> bool {
    matches!(capture, CaptureState::Recording | CaptureState::Paused)
}

/// Maps a native File/View menu item id to the frontend event it emits, so
/// the exact event names are unit-testable without a live menu bar. Kept
/// separate from `main`'s `on_menu_event` closure, which calls this for every
/// id besides "open-settings" (that one drives `show_settings_window`
/// directly and never reaches the frontend as an event). The frontend is the
/// only thing that turns "menu:new-recording"/"menu:stop" into an actual
/// start or stop — this function only names the wire, it does not call
/// `stop_meeting` or anything else.
fn menu_event_name(id: &str) -> Option<&'static str> {
    match id {
        "new-recording" => Some("menu:new-recording"),
        "stop-recording" => Some("menu:stop"),
        "toggle-sidebar" => Some("menu:toggle-sidebar"),
        "open-transcript" => Some("menu:open-transcript"),
        _ => None,
    }
}

/// Keeps the always-present menubar item current from the reducer. A 1 s
/// poll over the model is deliberate: every state change already lands in
/// the model under its lock, and the menubar only needs to follow it, not
/// participate in it. AppKit updates run on the main thread.
///
/// W8-A: the same tick keeps the tray's "Stop recording" item in sync too,
/// via `tray_shows_stop_recording`. It is inserted into or removed from
/// `menu` only on the tick where that fact actually changes — the menu is
/// never rebuilt from scratch, and `tray_presentation`'s own glyph/tooltip
/// update below is untouched by this.
fn spawn_tray_updater(
    app: AppHandle,
    tray: tauri::tray::TrayIcon,
    menu: tauri::menu::Menu<tauri::Wry>,
    stop_item: tauri::menu::MenuItem<tauri::Wry>,
    // File > Stop Recording (native window menu, not the tray's own item
    // above). Enabled/disabled from the same `tray_shows_stop_recording`
    // read of `CaptureState` on the same tick, so the tray and the File menu
    // never disagree about whether a capture is live to stop.
    file_stop_item: tauri::menu::MenuItem<tauri::Wry>,
) {
    let _ = std::thread::Builder::new()
        .name("menubar-state".into())
        .spawn(move || {
            let mut last: Option<(&'static str, &'static str)> = None;
            // The menu is built without "Stop recording" (see `main`'s
            // setup): seeding this `Some(false)` instead of `None` matches
            // that starting shape and skips a first-tick `menu.remove()` on
            // an item that was never inserted.
            let mut last_shows_stop: Option<bool> = Some(false);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let state = app.state::<ApplicationState>();
                let (presentation, shows_stop) = {
                    let Ok(model) = state.model.lock() else {
                        continue;
                    };
                    (
                        tray_presentation(
                            model.reducer.startup(),
                            model.reducer.capture(),
                            model.degraded,
                        ),
                        tray_shows_stop_recording(model.reducer.capture()),
                    )
                };
                if last != Some(presentation) {
                    last = Some(presentation);
                    let tray = tray.clone();
                    let _ = app.run_on_main_thread(move || {
                        let (glyph, words) = presentation;
                        let _ = tray.set_title(Some(glyph));
                        let _ = tray.set_tooltip(Some(words));
                    });
                }
                if last_shows_stop != Some(shows_stop) {
                    last_shows_stop = Some(shows_stop);
                    let menu = menu.clone();
                    let stop_item = stop_item.clone();
                    let file_stop_item = file_stop_item.clone();
                    let _ = app.run_on_main_thread(move || {
                        if shows_stop {
                            let _ = menu.insert(&stop_item, 1);
                        } else {
                            let _ = menu.remove(&stop_item);
                        }
                        let _ = file_stop_item.set_enabled(shows_stop);
                    });
                }
            }
        });
}

fn prepare_startup_retry(model: &mut AppModel) -> Result<(), String> {
    transition_startup(model, StartupState::Retrying)?;
    model.startup_message = "Restarting local checks.".into();
    if model.reducer.capture() == CaptureState::Idle {
        model.error = None;
    }
    Ok(())
}

#[tauri::command]
fn retry_startup(app: AppHandle) -> Result<AppSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    // A startup retry re-runs reconciliation over the same stores the take
    // is writing; it lands after the take, by refusal.
    if sitting_task_active(&state) {
        return Err("Finish the setup recording first.".into());
    }
    if state
        .capture_task
        .lock()
        .expect("capture task lock")
        .is_some()
    {
        return Err("Startup cannot be retried while a meeting is active.".into());
    }
    let snapshot = {
        let mut model = state.model.lock().expect("application model lock");
        if !matches!(
            model.reducer.startup(),
            StartupState::RuntimeMissing
                | StartupState::ServiceTimeout
                | StartupState::DiagnosticWritten
        ) {
            // "Check again" clicked when the check is not waiting for a retry —
            // a double-click, or a click racing a state that already cleared.
            // There is nothing to tell the operator and nothing to redo, so
            // this is a no-op returning the current state, not an error whose
            // internal-state wording ("not waiting for a retry") means nothing
            // to a reader. The honest surface is whatever screen the snapshot
            // already describes.
            return Ok(model.snapshot());
        }
        prepare_startup_retry(&mut model)?;
        model.snapshot()
    };
    let task_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("meeting-runtime-retry".into())
        .spawn(move || initialize_application(task_app, true));
    if let Err(error) = spawned {
        write_diagnostic(&state, "startup_retry_spawn_failed", &error.to_string());
        finish_startup_failure(
            &state,
            true,
            StartupFailure::Diagnostic,
            "the installation retry could not start",
        );
        return Err("The installation retry could not start.".into());
    }
    Ok(snapshot)
}

/// A verified `RuntimeManifest`, reused across calls instead of re-reading
/// and re-hashing every declared runtime resource from disk each time.
///
/// Root cause of the Settings row freezing at "Checking speech model" for
/// over a minute in the installed preview, both appearances (cold review:
/// docs/evidence/screen-reviews/all-surfaces-bfa0a80-installed-cold.md):
/// `transcript_model_settings` and `note_model_settings` each called
/// `RuntimeManifest::load_and_verify` fresh on every invocation — every time
/// the Settings window opened, and again on every 500ms poll while a change
/// was active. `load_and_verify` SHA-256-hashes every resource the manifest
/// declares, including any bundled model weight files it lists (see
/// `worker/build_manifest.py`'s `resources`/`models`), with no caching
/// anywhere in the eight call sites in this file. The frontend was never
/// seeing an error — `renderModels` only ever shows the static "Checking…"
/// row while `models` is null, so a slow-but-eventually-successful call and
/// a genuinely hung one render identically — it was still waiting on this
/// verification to finish. `settings.js`'s `scheduleModelPoll` also never
/// retried after a failed check (a real, separate defect, fixed there too),
/// but that path only matters once an attempt has actually failed; this is
/// what made a single attempt itself take so long.
///
/// The manifest is a bundled resource nothing in this process rewrites at
/// runtime, so verifying it once per distinct path and trusting that result
/// for the rest of the process's life is correct, not merely fast. This
/// cache is deliberately used only by the two read-only Settings snapshot
/// functions below (`transcript_model_settings_for`, `note_model_settings_for`)
/// — every call site that gates spawning a privileged process or installing
/// a model still calls `RuntimeManifest::load_and_verify` directly, unchanged,
/// so nothing here weakens verification immediately before a security-
/// sensitive action.
fn cached_verified_manifest(
    state: &ApplicationState,
    path: &Path,
) -> Result<Arc<RuntimeManifest>, String> {
    let mut cache = state
        .verified_manifest_cache
        .lock()
        .map_err(|_| "the runtime manifest cache is unavailable".to_string())?;
    if let Some(manifest) = cache.get(path) {
        return Ok(manifest.clone());
    }
    let manifest = Arc::new(RuntimeManifest::load_and_verify(path).map_err(error_text)?);
    cache.insert(path.to_path_buf(), manifest.clone());
    Ok(manifest)
}

fn verified_model_catalog(
    manifest_path: &Path,
    manifest: &RuntimeManifest,
) -> Result<Option<ModelCatalog>, String> {
    let Some(resource) = manifest.model_catalog.as_ref() else {
        return Ok(None);
    };
    let path = manifest
        .model_catalog_path(manifest_path)
        .ok_or_else(|| "the signed model catalog path is unavailable".to_string())?;
    ModelCatalog::load_and_verify(&path, &resource.sha256)
        .map(Some)
        .map_err(error_text)
}

fn transcript_model_settings_for(
    state: &ApplicationState,
) -> Result<TranscriptModelSettingsSnapshot, String> {
    let storage_context = state
        .storage
        .lock()
        .map_err(|_| "the private workspace is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "the private workspace is unavailable".to_string())?;
    let manifest = cached_verified_manifest(state, &storage_context.manifest_path)?;
    let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
        .ok_or_else(|| "this build does not use downloadable speech models".to_string())?;
    let active = active_model(&storage_context.storage, &catalog).map_err(error_text)?;
    let active_model_id = active.as_ref().map(|entry| entry.id.clone());
    let options = catalog
        .models
        .iter()
        .map(|entry| {
            let stored = model_is_stored(&storage_context.storage, entry).map_err(error_text)?;
            Ok(TranscriptModelSettingsOption {
                id: entry.id.clone(),
                title: entry.title.clone(),
                detail: entry.detail.clone(),
                download_bytes: entry.download_bytes,
                installed_bytes: entry.installed_bytes,
                stored,
                active: active_model_id.as_deref() == Some(entry.id.as_str()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let installing = state.model_install_active.load(Ordering::SeqCst);
    let (setup, startup, capture) = {
        let model = state.model.lock().expect("application model lock");
        (
            model.model_setup.clone(),
            model.reducer.startup(),
            model.reducer.capture(),
        )
    };
    let startup_ready = matches!(startup, StartupState::Ready | StartupState::ModelRequired);
    let audio_idle = model_change_audio_idle(capture, sitting_task_active(state));
    let can_change = startup_ready && audio_idle && !installing;
    let unavailable_reason = if installing {
        Some("Wait for the current model change to finish.".into())
    } else if !audio_idle {
        Some("Finish the current meeting before changing speech models.".into())
    } else if !startup_ready {
        Some("Yawn must finish starting before speech models can be changed.".into())
    } else {
        None
    };
    Ok(TranscriptModelSettingsSnapshot {
        state: setup.state,
        options,
        active_model_id,
        selected_model_id: setup.selected_model_id,
        downloaded_bytes: setup.downloaded_bytes,
        total_bytes: setup.total_bytes,
        error: setup.error,
        change_active: installing,
        can_change,
        unavailable_reason,
    })
}

#[tauri::command]
fn transcript_model_settings(
    state: State<'_, ApplicationState>,
) -> Result<TranscriptModelSettingsSnapshot, String> {
    transcript_model_settings_for(&state)
}

fn finish_model_selection(
    state: &ApplicationState,
    retry: bool,
    catalog: &ModelCatalog,
    error: Option<String>,
) {
    let mut model = state.model.lock().expect("application model lock");
    model.model_setup = ModelSetupSnapshot {
        state: if error.is_some() { "failed" } else { "choose" }.into(),
        options: catalog.models.iter().map(ModelSetupOption::from).collect(),
        selected_model_id: None,
        downloaded_bytes: 0,
        total_bytes: 0,
        error,
    };
    model.startup_message = "Choose the speech model to keep on this Mac.".into();
    model.error = None;
    if transition_startup(&mut model, StartupState::ModelRequired).is_err() {
        model.error = Some(
            if retry {
                "The model setup retry stopped in an invalid state."
            } else {
                "Model setup stopped in an invalid state."
            }
            .into(),
        );
    }
}

#[tauri::command]
fn install_transcript_model(app: AppHandle, model_id: String) -> Result<AppSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    let capture_active = state
        .model
        .lock()
        .expect("application model lock")
        .reducer
        .capture()
        != CaptureState::Idle;
    if capture_active || sitting_task_active(&state) {
        return Err("A speech model cannot be installed while audio work is active.".into());
    }
    if state
        .model_install_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A speech model is already downloading.".into());
    }

    let preparation = (|| {
        let storage_context = state
            .storage
            .lock()
            .map_err(|_| "the private workspace is unavailable".to_string())?
            .clone()
            .ok_or_else(|| "the private workspace is unavailable".to_string())?;
        let manifest =
            RuntimeManifest::load_and_verify(&storage_context.manifest_path).map_err(error_text)?;
        let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
            .ok_or_else(|| "this build does not use downloadable speech models".to_string())?;
        let selected = catalog.model(&model_id).map_err(error_text)?.clone();
        let previous_active =
            active_model(&storage_context.storage, &catalog).map_err(error_text)?;
        {
            let mut model = state.model.lock().expect("application model lock");
            if !matches!(
                model.reducer.startup(),
                StartupState::ModelRequired | StartupState::Ready
            ) {
                return Err("Speech models cannot be changed from the current state.".into());
            }
            model.model_setup = ModelSetupSnapshot {
                state: "downloading".into(),
                options: catalog.models.iter().map(ModelSetupOption::from).collect(),
                selected_model_id: Some(selected.id.clone()),
                downloaded_bytes: 0,
                total_bytes: selected.download_bytes,
                error: None,
            };
            model.startup_message = format!("Downloading {}.", selected.title);
            model.error = None;
        }
        Ok::<_, String>((storage_context, selected, previous_active))
    })();

    let (storage_context, selected, previous_active) = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            state.model_install_active.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };
    let snapshot = state
        .model
        .lock()
        .expect("application model lock")
        .snapshot();
    let task_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("meeting-model-download".into())
        .spawn(move || {
            let install_result = model_download::install(
                &storage_context.storage,
                &selected,
                |downloaded_bytes| {
                    let state = task_app.state::<ApplicationState>();
                    let mut model = state.model.lock().expect("application model lock");
                    if model.model_setup.selected_model_id.as_deref()
                        == Some(selected.id.as_str())
                    {
                        model.model_setup.downloaded_bytes = downloaded_bytes;
                    }
                },
            );
            let state = task_app.state::<ApplicationState>();
            match install_result {
                Ok(()) => {
                    let _command = state.command_lock.lock().expect("command lock");
                    {
                        let mut model = state.model.lock().expect("application model lock");
                        model.model_setup.state = "verifying".into();
                        model.model_setup.downloaded_bytes = selected.download_bytes;
                        model.startup_message = "Verifying the downloaded speech model.".into();
                        if let Err(error) = transition_startup(&mut model, StartupState::Retrying) {
                            if let Some(previous) = previous_active.as_ref() {
                                let _ = activate_model(&storage_context.storage, previous);
                            }
                            model.model_setup.state = "failed".into();
                            model.model_setup.error = Some(error);
                            state.model_install_active.store(false, Ordering::SeqCst);
                            return;
                        }
                    }
                    initialize_application(task_app.clone(), true);
                    state.model_install_active.store(false, Ordering::SeqCst);
                }
                Err(error) => {
                    state.model_install_active.store(false, Ordering::SeqCst);
                    let detail = error.to_string();
                    let _ = write_private_diagnostic(
                        &storage_context.diagnostics,
                        "model_download_failed",
                        &detail,
                    );
                    let mut model = state.model.lock().expect("application model lock");
                    model.model_setup.state = "failed".into();
                    model.model_setup.error = Some(
                        "The speech model could not be downloaded and verified. Choose it again to retry."
                            .into(),
                    );
                    model.startup_message = "Speech model setup needs attention.".into();
                }
            }
        });
    if let Err(error) = spawned {
        state.model_install_active.store(false, Ordering::SeqCst);
        let mut model = state.model.lock().expect("application model lock");
        model.model_setup.state = "failed".into();
        model.model_setup.error = Some("The speech model download could not start.".into());
        write_diagnostic(&state, "model_download_spawn_failed", &error.to_string());
        return Err("The speech model download could not start.".into());
    }
    Ok(snapshot)
}

#[tauri::command]
fn remove_transcript_model(
    model_id: String,
    state: State<'_, ApplicationState>,
) -> Result<TranscriptModelSettingsSnapshot, String> {
    let _command = state.command_lock.lock().expect("command lock");
    if state.model_install_active.load(Ordering::SeqCst) {
        return Err("Wait for the current model change to finish.".into());
    }
    let ready = {
        let model = state.model.lock().expect("application model lock");
        matches!(
            model.reducer.startup(),
            StartupState::Ready | StartupState::ModelRequired
        ) && model.reducer.capture() == CaptureState::Idle
    };
    if !ready || sitting_task_active(&state) {
        return Err("Finish the current meeting before removing a speech model.".into());
    }
    let storage_context = state
        .storage
        .lock()
        .map_err(|_| "the private workspace is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "the private workspace is unavailable".to_string())?;
    let manifest =
        RuntimeManifest::load_and_verify(&storage_context.manifest_path).map_err(error_text)?;
    let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
        .ok_or_else(|| "this build does not use downloadable speech models".to_string())?;
    remove_inactive_model(&storage_context.storage, &catalog, &model_id).map_err(|error| {
        match error {
            local_meeting_notes_session_core::model_store::ModelStoreError::ActiveModel => {
                "Switch to the other speech model before removing this one.".into()
            }
            other => error_text(other),
        }
    })?;
    transcript_model_settings_for(&state)
}

fn note_model_settings_for(state: &ApplicationState) -> Result<NoteModelSettingsSnapshot, String> {
    let storage_context = state
        .storage
        .lock()
        .map_err(|_| "the private workspace is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "the private workspace is unavailable".to_string())?;
    let manifest = cached_verified_manifest(state, &storage_context.manifest_path)?;
    let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
        .ok_or_else(|| "this build does not use downloadable note models".to_string())?;
    let active = active_note_model(&storage_context.storage, &catalog).map_err(error_text)?;
    let active_model_id = active.as_ref().map(|entry| entry.id.clone());
    let options = catalog
        .note_models
        .iter()
        .map(|entry| {
            let stored =
                note_model_is_stored(&storage_context.storage, entry).map_err(error_text)?;
            Ok(NoteModelSettingsOption {
                id: entry.id.clone(),
                title: entry.title.clone(),
                detail: entry.detail.clone(),
                download_bytes: entry.download_bytes,
                installed_bytes: entry.installed_bytes,
                stored,
                active: active_model_id.as_deref() == Some(entry.id.as_str()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let installing = state.note_model_install_active.load(Ordering::SeqCst);
    let setup = state
        .note_model_setup
        .lock()
        .expect("note model setup lock")
        .clone();
    let (startup, capture) = {
        let model = state.model.lock().expect("application model lock");
        (model.reducer.startup(), model.reducer.capture())
    };
    let startup_ready = matches!(startup, StartupState::Ready | StartupState::ModelRequired);
    let audio_idle = model_change_audio_idle(capture, sitting_task_active(state));
    let can_change = startup_ready && audio_idle && !installing;
    let unavailable_reason = if installing {
        Some("Wait for the current note model change to finish.".into())
    } else if !audio_idle {
        Some("Finish the current meeting before changing the note model.".into())
    } else if !startup_ready {
        Some("Yawn must finish starting before the note model can be changed.".into())
    } else {
        None
    };
    Ok(NoteModelSettingsSnapshot {
        state: setup.state,
        options,
        active_model_id,
        selected_model_id: setup.selected_model_id,
        downloaded_bytes: setup.downloaded_bytes,
        total_bytes: setup.total_bytes,
        error: setup.error,
        change_active: installing,
        can_change,
        unavailable_reason,
    })
}

#[tauri::command]
fn note_model_settings(
    state: State<'_, ApplicationState>,
) -> Result<NoteModelSettingsSnapshot, String> {
    note_model_settings_for(&state)
}

#[tauri::command]
fn install_note_model(
    app: AppHandle,
    model_id: String,
) -> Result<NoteModelSettingsSnapshot, String> {
    let state = app.state::<ApplicationState>();
    let _command = state.command_lock.lock().expect("command lock");
    let capture_active = state
        .model
        .lock()
        .expect("application model lock")
        .reducer
        .capture()
        != CaptureState::Idle;
    if capture_active || sitting_task_active(&state) {
        return Err("The note model cannot be installed while audio work is active.".into());
    }
    if state.model_install_active.load(Ordering::SeqCst) {
        return Err("Wait for the speech model change to finish.".into());
    }
    if state
        .note_model_install_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("The note model is already downloading.".into());
    }
    let preparation = (|| {
        let storage_context = state
            .storage
            .lock()
            .map_err(|_| "the private workspace is unavailable".to_string())?
            .clone()
            .ok_or_else(|| "the private workspace is unavailable".to_string())?;
        let manifest =
            RuntimeManifest::load_and_verify(&storage_context.manifest_path).map_err(error_text)?;
        let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
            .ok_or_else(|| "this build does not use downloadable note models".to_string())?;
        let selected = catalog.note_model(&model_id).map_err(error_text)?.clone();
        {
            let mut setup = state
                .note_model_setup
                .lock()
                .expect("note model setup lock");
            *setup = NoteModelSetup {
                state: "downloading".into(),
                selected_model_id: Some(selected.id.clone()),
                downloaded_bytes: 0,
                total_bytes: selected.download_bytes,
                error: None,
            };
        }
        Ok::<_, String>((storage_context, selected))
    })();
    let (storage_context, selected) = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            state
                .note_model_install_active
                .store(false, Ordering::SeqCst);
            return Err(error);
        }
    };
    let task_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("note-model-download".into())
        .spawn(move || {
            let install_result = model_download::install(
                &storage_context.storage,
                &selected,
                |downloaded_bytes| {
                    let state = task_app.state::<ApplicationState>();
                    let mut setup = state.note_model_setup.lock().expect("note model setup lock");
                    if setup.selected_model_id.as_deref() == Some(selected.id.as_str()) {
                        setup.downloaded_bytes = downloaded_bytes;
                    }
                },
            );
            let state = task_app.state::<ApplicationState>();
            match install_result {
                Ok(()) => {
                    {
                        let mut setup =
                            state.note_model_setup.lock().expect("note model setup lock");
                        *setup = NoteModelSetup {
                            state: "idle".into(),
                            ..NoteModelSetup::default()
                        };
                    }
                    // The next library read must rebuild: admission caches
                    // only success, so a rebuild against the newly installed
                    // model admits the projector without a restart.
                    if let Ok(mut library) = state.preview_library.lock() {
                        *library = None;
                    }
                    state.note_model_install_active.store(false, Ordering::SeqCst);
                }
                Err(error) => {
                    state.note_model_install_active.store(false, Ordering::SeqCst);
                    let detail = error.to_string();
                    let _ = write_private_diagnostic(
                        &storage_context.diagnostics,
                        "note_model_download_failed",
                        &detail,
                    );
                    let mut setup = state.note_model_setup.lock().expect("note model setup lock");
                    setup.state = "failed".into();
                    setup.error = Some(
                        "The note model could not be downloaded and verified. Choose it again to retry."
                            .into(),
                    );
                }
            }
        });
    if let Err(error) = spawned {
        state
            .note_model_install_active
            .store(false, Ordering::SeqCst);
        {
            let mut setup = state
                .note_model_setup
                .lock()
                .expect("note model setup lock");
            setup.state = "failed".into();
            setup.error = Some("The note model download could not start.".into());
        }
        write_diagnostic(
            &state,
            "note_model_download_spawn_failed",
            &error.to_string(),
        );
        return Err("The note model download could not start.".into());
    }
    note_model_settings_for(&state)
}

#[tauri::command]
fn remove_note_model(
    model_id: String,
    state: State<'_, ApplicationState>,
) -> Result<NoteModelSettingsSnapshot, String> {
    let _command = state.command_lock.lock().expect("command lock");
    if state.note_model_install_active.load(Ordering::SeqCst) {
        return Err("Wait for the current note model change to finish.".into());
    }
    let ready = {
        let model = state.model.lock().expect("application model lock");
        model.reducer.capture() == CaptureState::Idle
    };
    if !ready || sitting_task_active(&state) {
        return Err("Finish the current meeting before removing the note model.".into());
    }
    let storage_context = state
        .storage
        .lock()
        .map_err(|_| "the private workspace is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "the private workspace is unavailable".to_string())?;
    let manifest =
        RuntimeManifest::load_and_verify(&storage_context.manifest_path).map_err(error_text)?;
    let catalog = verified_model_catalog(&storage_context.manifest_path, &manifest)?
        .ok_or_else(|| "this build does not use downloadable note models".to_string())?;
    // Unlike a speech model, the note model may be removed while active:
    // "no note model" is an ordinary state the library renders honestly, so
    // deactivate first and then remove. The cached projector admission is
    // dropped so the next rebuild re-derives against the emptied store.
    if active_note_model(&storage_context.storage, &catalog)
        .map_err(error_text)?
        .is_some_and(|active| active.id == model_id)
    {
        deactivate_note_model(&storage_context.storage).map_err(error_text)?;
    }
    remove_inactive_note_model(&storage_context.storage, &catalog, &model_id)
        .map_err(error_text)?;
    if let Ok(mut cached) = state.note_projector.lock() {
        *cached = None;
    }
    if let Ok(mut library) = state.preview_library.lock() {
        *library = None;
    }
    note_model_settings_for(&state)
}

/// Runs one organization mutation under the held process writer lock.
///
/// Every command below is this function with a different closure, which is the
/// point: the lock acquisition, the capture refusal and the snapshot
/// invalidation are written once. A command that forgot the invalidation would
/// leave the operator looking at their old title with no error to explain it.
fn with_library_organization(
    state: &ApplicationState,
    action: impl FnOnce(
        &local_meeting_notes_session_core::retention::LibraryOrganizationAuthority<'_>,
    ) -> Result<
        local_meeting_notes_session_core::library_metadata::OrganizationOutcome,
        local_meeting_notes_session_core::library_metadata::OrganizationError,
    >,
) -> library_organization::OrganizationResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return library_organization::OrganizationResponse::unavailable(
            "Renaming is unavailable. Reopen the app and try again.",
        );
    };
    // Organization writes to the same storage root a take's evidence writes
    // sit beside, so like every mutating command it refuses during a take
    // rather than interleaving with it.
    if sitting_task_active(state) {
        return library_organization::OrganizationResponse::unavailable(
            "Finish the setup recording before changing folders or titles.",
        );
    }
    let writer = match state.app_data_writer_lock.lock() {
        Ok(held) => held.clone(),
        Err(_) => None,
    };
    let Some(writer) = writer else {
        return library_organization::OrganizationResponse::unavailable(
            "Renaming is unavailable. Reopen the app and try again.",
        );
    };
    let response =
        library_organization::from_result(action(&writer.library_organization_authority()));
    if response.changed {
        // The snapshot the operator is looking at was built against the old
        // revision, and every handle in it binds that revision. Dropping it
        // here means the next read rebuilds rather than serving a title that
        // is no longer what the record says.
        if let Ok(mut library) = state.preview_library.lock() {
            *library = None;
        }
    }
    response
}

/// One invocation's ceiling. At the measured 32 ms per 24-window batch this is
/// well under a second of work, and a corpus larger than it simply takes another
/// call — which is the honest shape while nothing shows progress.
const CORPUS_EMBED_BUDGET_WINDOWS: usize = 512;

/// What one embedding pass did. Counts and a reason; never a meeting ID, never
/// text.
#[derive(serde::Serialize)]
struct CorpusEmbedResponse {
    requested: usize,
    embedded: usize,
    batches: usize,
    /// Windows in the corpus, and how many this embedder has answered for.
    windows: usize,
    covered: usize,
    /// Why the pass ended, in a fixed vocabulary.
    stop: &'static str,
}

impl CorpusEmbedResponse {
    fn unavailable(stop: &'static str) -> Self {
        Self {
            requested: 0,
            embedded: 0,
            batches: 0,
            windows: 0,
            covered: 0,
            stop,
        }
    }
}

/// Fills the corpus's vector column from the packaged embedding model.
///
/// Registered rather than run automatically, and that is a decision rather than
/// a stopgap: **no surface asks a semantic question yet**, so there is nothing to
/// keep warm for. Wiring it into the library-read path would put 34 worker round
/// trips on a path a person waits on, and `ProcessWorkerPort` holds the worker
/// mutex for each one — capture would queue behind search-index work nobody
/// asked for.
///
/// It refuses during a take for the same reason every mutating command does.
///
/// `(async)` because it waits on a child process. The default execution context
/// is `Blocking`, which runs the body inline in the IPC handler — and one pass
/// is up to twenty-one worker round trips, which left blocking would freeze the
/// window for seconds at a time. It was registered without the attribute on
/// 2026-08-08 and nothing called it, so nothing froze; the defect became
/// reachable the moment a control rendered for it.
#[tauri::command(async)]
fn corpus_embed_pending(state: State<'_, ApplicationState>) -> CorpusEmbedResponse {
    use local_meeting_notes_session_core::corpus_embedding::{FillStop, fill_vectors};
    use local_meeting_notes_session_core::corpus_index::CorpusIndex;
    use local_meeting_notes_session_core::corpus_window::EmbedderIdentity;

    let Ok(_command) = state.command_lock.lock() else {
        return CorpusEmbedResponse::unavailable("unavailable");
    };
    if sitting_task_active(&state) {
        return CorpusEmbedResponse::unavailable("busy");
    }
    let storage = {
        let guard = state.storage.lock().expect("storage context lock");
        match guard.as_ref().map(|context| context.storage.clone()) {
            Some(storage) => storage,
            None => return CorpusEmbedResponse::unavailable("unavailable"),
        }
    };
    let packaged: Vec<(String, String)> = {
        let guard = state.runtime.lock().expect("runtime identity lock");
        match guard.as_ref() {
            Some(runtime) => runtime.packaged_models.clone(),
            None => return CorpusEmbedResponse::unavailable("unavailable"),
        }
    };
    let borrowed: Vec<(&str, &str)> = packaged
        .iter()
        .map(|(id, digest)| (id.as_str(), digest.as_str()))
        .collect();

    let Ok(mut index) = CorpusIndex::open(&storage) else {
        return CorpusEmbedResponse::unavailable("unavailable");
    };
    let identity = EmbedderIdentity::measured();
    let embedder = corpus_embedder::WorkerWindowEmbedder::new(
        Arc::new(product_coordinator::ProcessWorkerPort::new(
            state.worker.clone(),
        )),
        identity.dimension,
    );
    let Ok(outcome) = fill_vectors(
        &mut index,
        &identity,
        &embedder,
        &borrowed,
        CORPUS_EMBED_BUDGET_WINDOWS,
    ) else {
        return CorpusEmbedResponse::unavailable("unavailable");
    };
    let coverage = index.vector_coverage(&identity).unwrap_or(
        local_meeting_notes_session_core::corpus_index::VectorCoverage {
            windows: 0,
            embedded: 0,
        },
    );
    CorpusEmbedResponse {
        requested: outcome.requested,
        embedded: outcome.embedded,
        batches: outcome.batches,
        windows: coverage.windows,
        covered: coverage.embedded,
        // Deliberately not the error text. `EmbedderUnavailable` carries a
        // transport message, and a transport message can name a path.
        stop: match outcome.stop {
            FillStop::Complete => "complete",
            FillStop::BudgetReached => "budget",
            FillStop::EmbedderUnavailable(_) => "worker-unavailable",
            FillStop::ReplyIncomplete => "reply-incomplete",
            FillStop::ModelMismatch => "model-mismatch",
        },
    }
}

/// One meeting's best passage for a question, as the shell renders it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusSearchAnswer {
    meeting_id: String,
    title: Option<String>,
    folder: Option<String>,
    /// An untitled meeting is named by when it happened, exactly as the Library
    /// names it. The shell formats it; Rust sends no second, UTC copy.
    created_at_epoch_seconds: u64,
    /// The words the vector was computed from. This is the evidence, and it is
    /// why a hit is a passage rather than a percentage.
    quote: String,
    similarity: f32,
    /// Store-numbered turns, from zero. The shell adds one, exactly as it does
    /// for an exact hit, so the two searches never number the same transcript
    /// differently.
    first_turn_index: u64,
    last_turn_index: u64,
    /// Single-use, minted against the current projection. `None` for a meeting
    /// the library cannot currently open — the row renders unopenable rather
    /// than failing when it is clicked.
    transcript_handle: Option<String>,
}

/// A ranking, what it searched, and why it stopped where it did.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusSearchResponse {
    /// Fixed vocabulary, never a transport message — the same rule
    /// `corpus_embed_pending` follows, for the same reason: an
    /// `EmbedderUnavailable` reason can name a path.
    state: &'static str,
    answers: Vec<CorpusSearchAnswer>,
    /// Windows in the corpus against windows this model has answered for.
    /// Reported in **every** state, including the failures: "we searched
    /// nothing" and "nothing matched" are different sentences and the shell
    /// cannot tell them apart without these two numbers.
    windows: usize,
    covered: usize,
    /// Meetings within the measured tie band of the top score, counted before
    /// [`ANSWER_LIMIT`] truncated anything.
    near_ties: usize,
    /// Whether transcript handles were mintable at all: `"minted"` or
    /// `"unavailable"`. Two words rather than a bare absence, because a row with
    /// no handle would otherwise render "No transcript to open" — which tells
    /// the operator these meetings have no transcript when the truth is the
    /// library reader was gone. A row does not claim something untrue.
    handles: &'static str,
    message: String,
}

impl CorpusSearchResponse {
    fn stopped(state: &'static str, message: &str) -> Self {
        Self {
            state,
            answers: Vec::new(),
            windows: 0,
            covered: 0,
            near_ties: 0,
            handles: "unavailable",
            message: message.into(),
        }
    }
}

/// Asks the corpus a question in words and answers with quoted passages.
///
/// # What it searches, and when that stopped being true
///
/// The corpus index is synced by `LibraryReader::rebuild`, so this searches the
/// corpus as of the last time the library was opened or invalidated. The shell
/// initialises the reader before it reaches this command, and a meeting
/// recorded since then arrives as an unembedded window — which shows up
/// honestly as `covered` below `windows` rather than as a meeting that
/// silently does not match.
///
/// # Lock order
///
/// `command_lock` first, then `with_preview_library`'s storage sequence and
/// reader mutex. That is the order every other command in this file takes —
/// checked, not assumed: all thirteen acquisitions of `command_lock` are at the
/// top of their function and none sits inside a `with_preview_library` closure.
/// The reader's own doc names the sequence-before-reader half; this is the tier
/// above it, and this is the first command to hold both.
///
/// The ask itself happens **outside** that closure, deliberately. Minting
/// handles takes the meeting-storage sequence, and holding it across a worker
/// round trip would make a question block capture for as long as the model
/// takes.
///
/// # Why it refuses during a take
///
/// One question is one worker round trip of about 30 ms, which sounds harmless.
/// It is not: `ProcessWorkerPort` holds the worker mutex for it, and during a
/// sitting that worker is transcribing. Search is a convenience and capture is
/// the product.
///
/// `(async)` for the same reason as [`corpus_embed_pending`]: it waits on a
/// child process, and the default `Blocking` context would run that wait inside
/// the IPC handler.
#[tauri::command(async)]
fn corpus_search(question: String, state: State<'_, ApplicationState>) -> CorpusSearchResponse {
    use local_meeting_notes_session_core::corpus_index::CorpusIndex;
    use local_meeting_notes_session_core::corpus_question::{ANSWER_LIMIT, AskStop, ask};
    use local_meeting_notes_session_core::corpus_window::EmbedderIdentity;

    let unavailable = "Semantic search is unavailable right now. Reopen the app and try again.";
    let Ok(_command) = state.command_lock.lock() else {
        return CorpusSearchResponse::stopped("unavailable", unavailable);
    };
    if sitting_task_active(&state) {
        return CorpusSearchResponse::stopped(
            "busy",
            "Finish the current recording before searching by meaning. \
             Exact word search still works.",
        );
    }
    let storage = {
        let guard = state.storage.lock().expect("storage context lock");
        match guard.as_ref().map(|context| context.storage.clone()) {
            Some(storage) => storage,
            None => return CorpusSearchResponse::stopped("unavailable", unavailable),
        }
    };
    let packaged: Vec<(String, String)> = {
        let guard = state.runtime.lock().expect("runtime identity lock");
        match guard.as_ref() {
            Some(runtime) => runtime.packaged_models.clone(),
            None => return CorpusSearchResponse::stopped("unavailable", unavailable),
        }
    };
    let borrowed: Vec<(&str, &str)> = packaged
        .iter()
        .map(|(id, digest)| (id.as_str(), digest.as_str()))
        .collect();
    let Ok(index) = CorpusIndex::open(&storage) else {
        return CorpusSearchResponse::stopped("unavailable", unavailable);
    };
    let identity = EmbedderIdentity::measured();
    let embedder = corpus_embedder::WorkerWindowEmbedder::new(
        Arc::new(product_coordinator::ProcessWorkerPort::new(
            state.worker.clone(),
        )),
        identity.dimension,
    );
    let Ok(outcome) = ask(
        &index,
        &identity,
        &embedder,
        &borrowed,
        &question,
        ANSWER_LIMIT,
    ) else {
        return CorpusSearchResponse::stopped("unavailable", unavailable);
    };

    let windows = outcome.coverage.windows;
    let covered = outcome.coverage.embedded;
    let (state_name, message) = match &outcome.stop {
        AskStop::Answered if outcome.answers.is_empty() => (
            "empty",
            "No meeting was close enough to that description.".to_string(),
        ),
        AskStop::Answered => (
            "answered",
            format!(
                "{} {} from {} of {} passages.",
                outcome.answers.len(),
                if outcome.answers.len() == 1 {
                    "meeting"
                } else {
                    "meetings"
                },
                covered,
                windows,
            ),
        ),
        AskStop::NothingToSearch => (
            "nothing-to-search",
            "There are no retained transcripts to search yet.".to_string(),
        ),
        AskStop::NothingPrepared => (
            "nothing-prepared",
            format!(
                "None of your {windows} passages are prepared for meaning search yet. \
                 Preparing runs entirely on this Mac.",
            ),
        ),
        AskStop::QuestionBlank => ("blank", "Describe what the meeting was about.".to_string()),
        AskStop::QuestionTooLong => (
            "too-long",
            "That description is longer than any passage it could match. \
             Shorten it to a sentence or two."
                .to_string(),
        ),
        AskStop::QuestionUnanswered | AskStop::EmbedderUnavailable(_) => (
            "worker-unavailable",
            "The local model did not answer. Reopen the app and try again.".to_string(),
        ),
        AskStop::ModelMismatch => (
            "model-mismatch",
            "This installation packages a different model than the stored passages were \
             prepared with. Reinstall to search by meaning."
                .to_string(),
        ),
    };

    // Handles are minted in a second pass, deliberately. `with_preview_library`
    // holds the meeting-storage sequence, and the ask above is a worker round
    // trip — holding that sequence across it would make a question block
    // capture for as long as the model takes.
    let handles: Option<HashMap<String, String>> = if outcome.answers.is_empty() {
        Some(HashMap::new())
    } else {
        let ids: Vec<String> = outcome
            .answers
            .iter()
            .map(|answer| answer.meeting_id.clone())
            .collect();
        state.with_preview_library(
            || None,
            |reader, _active| {
                Some(
                    ids.iter()
                        .filter_map(|id| {
                            reader
                                .retain_transcript_handle(id)
                                .map(|handle| (id.clone(), handle))
                        })
                        .collect(),
                )
            },
        )
    };
    let handles_state = if handles.is_some() {
        "minted"
    } else {
        "unavailable"
    };
    let handles = handles.unwrap_or_default();

    CorpusSearchResponse {
        state: state_name,
        answers: outcome
            .answers
            .into_iter()
            .map(|answer| CorpusSearchAnswer {
                transcript_handle: handles.get(&answer.meeting_id).cloned(),
                meeting_id: answer.meeting_id,
                title: answer.title,
                folder: answer.folder,
                created_at_epoch_seconds: answer.created_at_epoch_seconds,
                quote: answer.quote,
                similarity: answer.similarity,
                first_turn_index: answer.first_turn_index,
                last_turn_index: answer.last_turn_index,
            })
            .collect(),
        windows,
        covered,
        near_ties: outcome.near_ties,
        handles: handles_state,
        message,
    }
}

#[tauri::command]
fn library_create_folder(
    expected_revision: u64,
    name: String,
    state: State<'_, ApplicationState>,
) -> library_organization::OrganizationResponse {
    with_library_organization(&state, |authority| {
        authority.create_folder(expected_revision, &name)
    })
}

#[tauri::command]
fn library_rename_folder(
    expected_revision: u64,
    folder_id: String,
    name: String,
    state: State<'_, ApplicationState>,
) -> library_organization::OrganizationResponse {
    with_library_organization(&state, |authority| {
        authority.rename_folder(expected_revision, &folder_id, &name)
    })
}

#[tauri::command]
fn library_delete_folder(
    expected_revision: u64,
    folder_id: String,
    state: State<'_, ApplicationState>,
) -> library_organization::OrganizationResponse {
    with_library_organization(&state, |authority| {
        authority.delete_folder(expected_revision, &folder_id)
    })
}

#[tauri::command]
fn library_assign_meeting_folder(
    expected_revision: u64,
    meeting_id: String,
    folder_id: Option<String>,
    state: State<'_, ApplicationState>,
) -> library_organization::OrganizationResponse {
    with_library_organization(&state, |authority| {
        authority.assign_meeting_folder(expected_revision, &meeting_id, folder_id.as_deref())
    })
}

/// Null clears the operator's title and restores the derived one.
#[tauri::command]
fn library_set_meeting_title(
    expected_revision: u64,
    meeting_id: String,
    title: Option<String>,
    state: State<'_, ApplicationState>,
) -> library_organization::OrganizationResponse {
    with_library_organization(&state, |authority| {
        authority.set_meeting_title(expected_revision, &meeting_id, title.as_deref())
    })
}

/// Roadmap intake W8-B: the frontend has no other way to learn whether the
/// operator's local probe marker exists, and it needs to know before it
/// renders anything -- flag off must mean zero new UI, not a button that
/// invokes and then refuses. `library_snapshot` is the one call the Home
/// screen already makes on every load and on every search-box debounce, so
/// piggybacking the flag bit here costs one more `Path::is_file` next to a
/// storage-backed library rebuild that already runs on this exact call,
/// rather than adding a third command whose only job is one boolean.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySnapshotResponse {
    #[serde(flatten)]
    library: library_reader::LibrarySnapshot,
    search_probe_enabled: bool,
    /// Roadmap packet W10: whether the operator has already dismissed the
    /// once-only first-run sheet. Same reasoning as `search_probe_enabled`
    /// above -- `library_snapshot` is the one call the Home screen always
    /// makes before it renders anything, so it is also the one place that can
    /// hand the frontend this fact without a second round trip. Defaults to
    /// `true` (already seen) when storage is unavailable: an app that cannot
    /// read its own storage should not also show a teaching sheet it cannot
    /// durably dismiss.
    first_run_sheet_seen: bool,
}

#[tauri::command]
fn library_snapshot(
    filter: Option<library_reader::LibraryFilterArgs>,
    state: State<'_, ApplicationState>,
) -> LibrarySnapshotResponse {
    library_snapshot_response_for(filter.unwrap_or_default(), &state)
}

// Split out from the `#[tauri::command]` wrapper so a test can call it with a
// plain `&ApplicationState` -- the same "_for" idiom `library_snapshot_with`
// and `preview_library_search_for` already use, needed because
// `tauri::State<'_, T>` cannot be constructed outside a running Tauri app.
fn library_snapshot_response_for(
    filter: library_reader::LibraryFilterArgs,
    state: &ApplicationState,
) -> LibrarySnapshotResponse {
    let library = library_snapshot_with(filter, state);
    let search_probe_enabled = preview_storage_clone(state)
        .map(|storage| search_probe::enabled(&storage))
        .unwrap_or(false);
    let first_run_sheet_seen = preview_storage_clone(state)
        .map(|storage| onboarding::seen(&storage))
        .unwrap_or(true);
    LibrarySnapshotResponse {
        library,
        search_probe_enabled,
        first_run_sheet_seen,
    }
}

/// Roadmap packet W10: writes the first-run sheet's dismissal marker. See
/// `onboarding::mark_seen` for why this is idempotent and best-effort --
/// nothing here needs to distinguish "already marked" from "just marked," and
/// a write failure must never surface as an error over what is, at most, the
/// sheet reappearing on a later cold boot.
#[tauri::command]
fn dismiss_first_run_sheet(state: State<'_, ApplicationState>) {
    dismiss_first_run_sheet_for(&state);
}

fn dismiss_first_run_sheet_for(state: &ApplicationState) {
    if let Ok(storage) = preview_storage_clone(state) {
        onboarding::mark_seen(&storage);
    }
}

fn library_snapshot_for(state: &ApplicationState) -> library_reader::LibrarySnapshot {
    library_snapshot_with(library_reader::LibraryFilterArgs::default(), state)
}

fn library_snapshot_with(
    filter: library_reader::LibraryFilterArgs,
    state: &ApplicationState,
) -> library_reader::LibrarySnapshot {
    let Ok(storage) = preview_storage_clone(state) else {
        return library_reader::LibraryReader::unavailable_snapshot();
    };
    let coordination = match state.meeting_storage_coordination() {
        Ok(coordination) => coordination,
        Err(_) => return library_reader::LibraryReader::unavailable_snapshot(),
    };
    let projector = admitted_note_projector(state);
    with_meeting_storage_sequence(&coordination, |active_meeting_ids| {
        let mut reader = match library_reader::LibraryReader::rebuild_with_projector(
            storage,
            active_meeting_ids,
            projector,
        ) {
            Ok(reader) => reader,
            Err(_) => return library_reader::LibraryReader::unavailable_snapshot(),
        };
        let snapshot = reader.snapshot_filtered(active_meeting_ids, &filter.to_filter());
        let Ok(mut library) = state.preview_library.lock() else {
            return library_reader::LibraryReader::unavailable_snapshot();
        };
        *library = Some(reader);
        snapshot
    })
    .unwrap_or_else(|_| library_reader::LibraryReader::unavailable_snapshot())
}

/// One §K retention row: content-free retention facts joined to the rendered
/// library list by meeting id. No handle is minted here — the overview must
/// never invalidate the snapshot generation the operator is navigating —
/// and no title or transcript-derived value crosses this surface: a date, a
/// size, a deadline, and the store's own state vocabulary.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRetentionRow {
    meeting_id: String,
    created_at_epoch_seconds: u64,
    audio_state: &'static str,
    policy: &'static str,
    deadline_epoch_seconds: Option<u64>,
    retained_bytes: Option<u64>,
}

/// The standing §K statement: what is held, how much of it, and until when.
/// `holding` and `nothing-held` are the spec's own states; deletion itself
/// stays behind each meeting's reviewed two-step path.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRetentionOverview {
    state: &'static str,
    total_retained_bytes: u64,
    retained_count: usize,
    unavailable_count: usize,
    rows: Vec<PreviewRetentionRow>,
    message: String,
}

impl PreviewRetentionOverview {
    fn unavailable() -> Self {
        Self {
            state: "unavailable",
            total_retained_bytes: 0,
            retained_count: 0,
            unavailable_count: 0,
            rows: Vec::new(),
            message: "Retention status is unavailable. Reopen Meetings and try again.".into(),
        }
    }
}

/// The manifest path, or `None` before storage exists.
///
/// First run's commands are reachable from the startup-failure screen, where
/// storage may legitimately be absent. They report `probe_unavailable` in that
/// case rather than erroring, because "we could not ask" is a state § H has to
/// render and not a fault to surface as an exception.
fn first_run_manifest_path(state: &ApplicationState) -> Option<std::path::PathBuf> {
    state
        .storage
        .lock()
        .ok()?
        .as_ref()
        .map(|context| context.manifest_path.clone())
}

// An absent storage context is a legitimate first-run state, not an error: these
// commands are reachable from the startup-failure screen. The empty path it
// resolves to fails verification, so every one of these reports
// `probe_unavailable` — which is the state § H renders — instead of raising.
//
// All three carry `(async)`, which on a synchronous function moves the body to
// Tauri's threadpool (`tauri-macros`'s `ExecutionContext::Async` arm; the default
// is `Blocking`, which runs inline in the IPC handler). Every one of them waits on
// a child process, and the microphone request waits on the operator answering a
// system dialog — up to the probe's own 120 s ceiling. Left blocking, walking away
// from that dialog freezes the window. Reported by review on 5f54376; these are the
// first blocking children on a UI path in this crate, which is why the rest of the
// file has no precedent for the attribute.
#[tauri::command(async)]
fn first_run_permissions(state: State<'_, ApplicationState>) -> first_run::FirstRunPermissions {
    state.remember_permissions(first_run::permissions_status(
        &first_run_manifest_path(&state).unwrap_or_default(),
    ))
}

#[tauri::command(async)]
fn first_run_request_microphone(
    state: State<'_, ApplicationState>,
) -> first_run::FirstRunPermissions {
    state.remember_permissions(first_run::request_microphone(
        &first_run_manifest_path(&state).unwrap_or_default(),
    ))
}

#[tauri::command(async)]
fn first_run_request_system_audio(
    state: State<'_, ApplicationState>,
) -> first_run::FirstRunPermissions {
    state.remember_permissions(first_run::request_system_audio(
        &first_run_manifest_path(&state).unwrap_or_default(),
    ))
}

/// Rebuild the live model's transcript projection from storage.
///
/// A restoration publishes a *new* current transcript, so the projection the
/// screen shown right after a recording is holding goes stale the instant one
/// succeeds — including the digest a second restore would have to name, which
/// would then be refused as a changed source. The Library route survives this
/// by re-walking its reader from a fresh snapshot; the recording screen has no
/// reader to re-walk, so it asks for its projection to be rebuilt in place.
///
/// It re-derives everything rather than patching the turn it was told about:
/// the transcript pointer, the artifact digest, and the withheld set all come
/// from disk, and a restored view resolves through the same audited chain
/// walker a cold open uses. Nothing the shell sent is trusted, because the
/// shell is not what holds the meeting.
///
/// Returns nothing. The snapshot poll carries the result, so there is one path
/// by which a projection reaches a screen rather than two that can disagree.
/// The meeting directory of whatever meeting is currently projected.
///
/// § D's commands take no meeting identifier from the shell. The note belongs to
/// the meeting the operator is in, and letting a surface name a different one
/// would make the note addressable by a caller that has no business addressing
/// it.
fn current_meeting_dir(state: &ApplicationState) -> Result<std::path::PathBuf, String> {
    let meeting_id = state
        .model
        .lock()
        .expect("application model lock")
        .meeting_id
        .clone()
        .ok_or_else(|| "There is no meeting open.".to_string())?;
    let storage = {
        let guard = state.storage.lock().expect("storage context lock");
        guard
            .as_ref()
            .map(|context| context.storage.clone())
            .ok_or_else(|| "Local storage is unavailable.".to_string())?
    };
    meeting_dir(&storage, &meeting_id).map_err(error_text)
}

/// Reading is separate from the snapshot on purpose: the note can be long, the
/// snapshot is polled every 400 ms while recording, and putting operator prose on
/// that path would carry it through every tick for no reader.
#[tauri::command(async)]
fn operator_note(
    state: State<'_, ApplicationState>,
) -> Result<operator_note::OperatorNote, String> {
    Ok(operator_note::read(&current_meeting_dir(&state)?))
}

#[tauri::command(async)]
fn save_operator_note(
    state: State<'_, ApplicationState>,
    text: String,
) -> Result<operator_note::OperatorNote, String> {
    let directory = current_meeting_dir(&state)?;
    operator_note::write(&directory, &text)?;
    // Answer with what is now on disk rather than echoing the argument, so the
    // surface's "saved" is a statement about storage and not about the request.
    Ok(operator_note::read(&directory))
}

/// Roadmap intake I3. Mirrors `operator_note` exactly: reading is separate
/// from the snapshot for the same reason, and the meeting this resolves is
/// whatever the shell is currently in, never a caller-named one.
#[tauri::command(async)]
fn meeting_context(
    state: State<'_, ApplicationState>,
) -> Result<meeting_context::MeetingContext, String> {
    Ok(meeting_context::read(&current_meeting_dir(&state)?))
}

#[tauri::command(async)]
fn save_meeting_context(
    state: State<'_, ApplicationState>,
    text: String,
) -> Result<meeting_context::MeetingContext, String> {
    let directory = current_meeting_dir(&state)?;
    meeting_context::write(&directory, &text)?;
    // Same reasoning as `save_operator_note`: answer with what is now on disk.
    Ok(meeting_context::read(&directory))
}

/// Resolves the one verified transcript artifact already selected by a meeting
/// view. This stays entirely on the native side: the webview receives no path,
/// digest, or filesystem authority, and Finder/TextEdit only receives a file
/// the existing transcript reader has checked against `meeting.json`.
fn verified_transcript_file(
    storage: &StorageRoot,
    meeting_id: &str,
    expected: &ArtifactRef,
) -> Result<PathBuf, CommandError> {
    let directory = meeting_dir(storage, meeting_id).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    if meeting.artifacts.current_transcript.as_ref() != Some(expected) {
        return Err(CommandError::coded(
            error_codes::TRANSCRIPT_CHANGED,
            "That transcript changed. Reopen the meeting and try again.",
        ));
    }
    let path = resolve_artifact(&directory, &expected.relative_path).map_err(error_text)?;
    let bytes = read_private_bytes(&path, TRANSCRIPT_MAX_BYTES).map_err(error_text)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected.sha256 {
        return Err(CommandError::coded(
            error_codes::TRANSCRIPT_CHANGED,
            "That transcript changed. Reopen the meeting and try again.",
        ));
    }
    let current = load_meeting(&directory).map_err(error_text)?;
    if current.artifacts.current_transcript.as_ref() != Some(expected)
        || artifact_ref(&directory, &expected.relative_path).map_err(error_text)? != *expected
    {
        return Err(CommandError::coded(
            error_codes::TRANSCRIPT_CHANGED,
            "That transcript changed. Reopen the meeting and try again.",
        ));
    }
    Ok(path)
}

/// Opens one verified local transcript in the system's default text-capable
/// app. This is intentionally a fixed native executable and a checked artifact
/// path, not general shell or filesystem access from the webview.
fn open_verified_transcript_file(
    storage: &StorageRoot,
    meeting_id: &str,
    expected: &ArtifactRef,
) -> Result<(), CommandError> {
    let path = verified_transcript_file(storage, meeting_id, expected)?;
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/open")
            .arg(&path)
            .status()
            .map_err(|_| CommandError::from("Yawn could not open this transcript file."))?;
        if status.success() {
            Ok(())
        } else {
            Err("Yawn could not open this transcript file.".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("Opening transcript files is available only on macOS.".into())
    }
}

/// Opens the transcript belonging to the active completed meeting. Unlike the
/// library route, this path has no browser-supplied meeting identifier: it uses
/// the settled capture projection and re-checks that exact artifact under the
/// storage coordination lock before opening it.
#[tauri::command]
fn open_current_transcript_file(state: State<'_, ApplicationState>) -> Result<(), CommandError> {
    let (meeting_id, current_transcript_sha256) = {
        let model = state.model.lock().expect("application model lock");
        if model.reducer.capture() != CaptureState::TranscriptReady {
            return Err("There is no finished transcript open.".into());
        }
        (
            model
                .meeting_id
                .clone()
                .ok_or_else(|| "There is no finished transcript open.".to_string())?,
            model
                .current_transcript_sha256
                .clone()
                .ok_or_else(|| "There is no finished transcript open.".to_string())?,
        )
    };
    let storage = {
        let guard = state.storage.lock().expect("storage context lock");
        guard
            .as_ref()
            .map(|context| context.storage.clone())
            .ok_or_else(|| "Local storage is unavailable.".to_string())?
    };
    let coordination = state.meeting_storage_coordination()?;
    with_meeting_storage_sequence(&coordination, |_| {
        let directory = meeting_dir(&storage, &meeting_id).map_err(error_text)?;
        let meeting = load_meeting(&directory).map_err(error_text)?;
        let transcript = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or_else(|| "That meeting has no current transcript.".to_string())?;
        if transcript.sha256 != current_transcript_sha256 {
            return Err(CommandError::coded(
                error_codes::TRANSCRIPT_CHANGED,
                "That transcript changed. Reopen the meeting and try again.",
            ));
        }
        open_verified_transcript_file(&storage, &meeting_id, transcript)
    })
    .map_err(|_| {
        CommandError::coded(
            error_codes::TRANSCRIPT_UNAVAILABLE,
            "The local transcript is unavailable. Reopen the meeting and try again.",
        )
    })?
}

#[tauri::command(async)]
fn refresh_current_transcript(state: State<'_, ApplicationState>) -> Result<(), String> {
    let meeting_id = {
        let model = state.model.lock().expect("application model lock");
        // Only a settled transcript has anything to rebuild. Refusing here keeps
        // this from touching a meeting that is still recording.
        if model.reducer.capture() != CaptureState::TranscriptReady {
            return Err("There is no finished transcript to refresh.".into());
        }
        model.meeting_id.clone()
    }
    .ok_or_else(|| "There is no finished transcript to refresh.".to_string())?;

    let storage = {
        let guard = state.storage.lock().expect("storage context lock");
        guard
            .as_ref()
            .map(|context| context.storage.clone())
            .ok_or_else(|| "Local storage is unavailable.".to_string())?
    };

    let directory = meeting_dir(&storage, &meeting_id).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    let transcript = meeting
        .artifacts
        .current_transcript
        .clone()
        .ok_or_else(|| "That meeting has no current transcript.".to_string())?;
    let (turns, mut warnings) = load_transcript_projection(&directory, &meeting_id, &transcript)?;
    // Same clause the startup path appends, for the same reason: a transcript
    // whose audio is gone must say so wherever it is read, not only where it
    // was first opened.
    if meeting.retention.state == AudioState::Released {
        warnings.push(
            "Meeting audio was deleted under the selected retention period. The transcript remains."
                .into(),
        );
    }

    let mut model = state.model.lock().expect("application model lock");
    // The meeting can change under a slow read — a dismissal, or another
    // recording. Publishing a projection onto a different meeting would put one
    // meeting's words under another's heading.
    if model.meeting_id.as_deref() != Some(meeting_id.as_str())
        || model.reducer.capture() != CaptureState::TranscriptReady
    {
        return Err("That transcript is no longer open.".into());
    }
    model.turns = turns;
    model.current_transcript_sha256 = Some(transcript.sha256);
    model.warnings = warnings;
    Ok(())
}

#[tauri::command]
fn preview_retention_overview(state: State<'_, ApplicationState>) -> PreviewRetentionOverview {
    preview_retention_overview_for(&state)
}

fn preview_retention_overview_for(state: &ApplicationState) -> PreviewRetentionOverview {
    let Ok(storage) = preview_storage_clone(state) else {
        return PreviewRetentionOverview::unavailable();
    };
    state.with_preview_library(PreviewRetentionOverview::unavailable, |reader, active| {
        let Some(identities) = reader.retention_identities(active) else {
            return PreviewRetentionOverview::unavailable();
        };
        let mut rows = Vec::new();
        let mut total_retained_bytes: u64 = 0;
        let mut retained_count = 0_usize;
        let mut unavailable_count = 0_usize;
        for (meeting_id, created_at_epoch_seconds) in identities {
            if active.contains(&meeting_id) {
                continue;
            }
            let retention =
                library_reader::LibraryReader::read_audio_retention(&storage, &meeting_id);
            if retention.state == "unavailable" {
                unavailable_count += 1;
                continue;
            }
            if retention.state == "retained" {
                retained_count += 1;
                total_retained_bytes = total_retained_bytes
                    .saturating_add(retention.retained_bytes.unwrap_or_default());
            }
            rows.push(PreviewRetentionRow {
                meeting_id,
                created_at_epoch_seconds,
                audio_state: retention.state,
                policy: retention.policy,
                deadline_epoch_seconds: retention.deadline_epoch_seconds,
                retained_bytes: retention.retained_bytes,
            });
        }
        // Soonest deadline first, then newest capture: the §K promise is
        // that deletion is never a surprise, so what expires next leads.
        rows.sort_by_key(|row| {
            (
                row.deadline_epoch_seconds.is_none(),
                row.deadline_epoch_seconds,
                u64::MAX - row.created_at_epoch_seconds,
            )
        });
        let (state, message) = if retained_count > 0 {
            (
                "holding",
                "Recording audio held on this Mac, deleted on the schedule below.".to_string(),
            )
        } else if rows.is_empty() && unavailable_count == 0 {
            (
                "nothing-held",
                "No recording audio is held. Meetings you record keep their audio here until \
                 their chosen deletion time."
                    .to_string(),
            )
        } else {
            (
                "nothing-held",
                "No recording audio is held. Retained transcripts and notes remain readable."
                    .to_string(),
            )
        };
        PreviewRetentionOverview {
            state,
            total_retained_bytes,
            retained_count,
            unavailable_count,
            rows,
            message,
        }
    })
}

#[tauri::command]
fn preview_profile_snapshot(state: State<'_, ApplicationState>) -> PreviewProfileSnapshot {
    preview_profile_snapshot_for(&state)
}

/// Read-only recorder surface: the cached evidence-store projection plus the
/// honest recording boundary. Preview gains no enrolment mutation through
/// this — starting, deriving, or abandoning a sitting stays ungranted.
#[tauri::command]
fn preview_enrollment_surface(state: State<'_, ApplicationState>) -> PreviewEnrollmentSurface {
    preview_enrollment_surface_for(&state)
}

fn preview_enrollment_surface_for(state: &ApplicationState) -> PreviewEnrollmentSurface {
    state
        .preview_enrollment
        .lock()
        .map(|cached| cached.clone())
        .unwrap_or_else(|_| PreviewEnrollmentSurface::unavailable())
}

#[tauri::command]
fn preview_profile_preserve_legacy(
    state: State<'_, ApplicationState>,
) -> Result<PreviewProfileSnapshot, String> {
    preview_profile_preserve_legacy_for(&state)
}

#[tauri::command]
fn preview_profile_reset(
    confirmed: bool,
    state: State<'_, ApplicationState>,
) -> Result<PreviewProfileSnapshot, String> {
    preview_profile_reset_for(&state, confirmed)
}

fn preview_profile_snapshot_for(state: &ApplicationState) -> PreviewProfileSnapshot {
    state
        .preview_profile
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_else(|_| PreviewProfileSnapshot::unavailable())
}

#[cfg(target_os = "macos")]
fn preview_profile_preserve_legacy_for(
    state: &ApplicationState,
) -> Result<PreviewProfileSnapshot, String> {
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| "the profile action is unavailable".to_string())?;
    // A dedicated sitting is one exclusive writer session over the profile
    // and evidence stores; a profile action lands after the take, by
    // refusal rather than by queueing.
    if sitting_task_active(state) {
        return Err("Finish the setup recording first.".into());
    }
    let current = preview_profile_snapshot_for(state);
    if current.state != "migration-review-required" {
        return Err("No legacy profile is awaiting review.".into());
    }
    {
        let model = state
            .model
            .lock()
            .map_err(|_| "the application state is unavailable".to_string())?;
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("Finish the current app or recording operation first.".into());
        }
    }
    let result = {
        let held = state
            .app_data_writer_lock
            .lock()
            .map_err(|_| "the app-data writer lock is unavailable".to_string())?;
        let writer = held
            .as_ref()
            .ok_or_else(|| "the app-data writer lock is unavailable".to_string())?;
        writer
            .profile_lifecycle_authority()
            .preserve_legacy_for_review()
    };
    match result {
        Ok(baseline) => {
            let snapshot = PreviewProfileSnapshot::baseline(
                baseline.profile_present(),
                baseline.profile_active(),
            );
            *state
                .preview_profile
                .lock()
                .map_err(|_| "the cached profile status is unavailable".to_string())? =
                snapshot.clone();
            Ok(snapshot)
        }
        Err(ProfileLifecycleAdmissionError::Lifecycle(ProfileLifecycleError::Quarantined)) => {
            let snapshot = PreviewProfileSnapshot::lifecycle_unreadable("needs-attention");
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = snapshot.clone();
            }
            Ok(snapshot)
        }
        Err(ProfileLifecycleAdmissionError::Lifecycle(
            ProfileLifecycleError::MigrationReviewRequired,
        )) => Err("The legacy profile still requires review.".into()),
        Err(ProfileLifecycleAdmissionError::Lifecycle(_)) => {
            let snapshot = PreviewProfileSnapshot::lifecycle_unreadable("needs-attention");
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = snapshot;
            }
            Err("The stored profile could not be preserved safely.".into())
        }
        Err(ProfileLifecycleAdmissionError::ActiveMeeting) => {
            Err("Finish the current recording before preserving this profile.".into())
        }
        Err(ProfileLifecycleAdmissionError::AuthorityLost)
        | Err(ProfileLifecycleAdmissionError::Coordination(_)) => {
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = PreviewProfileSnapshot::unavailable();
            }
            if let Ok(mut model) = state.model.lock() {
                if model.reducer.startup() == StartupState::Ready {
                    let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
                }
                model.error = Some("Private storage needs attention before recording.".into());
            }
            Err("Private storage needs attention before preserving this profile.".into())
        }
    }
}

#[cfg(target_os = "macos")]
fn preview_profile_reset_for(
    state: &ApplicationState,
    confirmed: bool,
) -> Result<PreviewProfileSnapshot, String> {
    if !confirmed {
        return Err("Confirm profile reset before continuing.".into());
    }
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| "the profile action is unavailable".to_string())?;
    // Same exclusivity refusal as preserve-legacy: a profile reset must
    // not interleave with an active take's evidence writes.
    if sitting_task_active(state) {
        return Err("Finish the setup recording first.".into());
    }
    let current = preview_profile_snapshot_for(state);
    if current.state != "baseline-ready" || current.profile_present != Some(true) {
        return Err("No stored profile is available to reset.".into());
    }
    {
        let model = state
            .model
            .lock()
            .map_err(|_| "the application state is unavailable".to_string())?;
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("Finish the current app or recording operation first.".into());
        }
    }
    let requested_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "the profile action time is unavailable".to_string())?
        .as_secs();
    let operation_id = Uuid::new_v4().to_string();
    let result = {
        let held = state
            .app_data_writer_lock
            .lock()
            .map_err(|_| "the app-data writer lock is unavailable".to_string())?;
        let writer = held
            .as_ref()
            .ok_or_else(|| "the app-data writer lock is unavailable".to_string())?;
        writer
            .profile_lifecycle_authority()
            .reset_profile(&operation_id, requested_at)
    };
    match result {
        Ok(_) => {
            let snapshot = PreviewProfileSnapshot::baseline(false, false);
            *state
                .preview_profile
                .lock()
                .map_err(|_| "the cached profile status is unavailable".to_string())? =
                snapshot.clone();
            Ok(snapshot)
        }
        Err(ProfileLifecycleAdmissionError::ActiveMeeting) => {
            Err("Finish the current recording before resetting this profile.".into())
        }
        Err(ProfileLifecycleAdmissionError::Lifecycle(ProfileLifecycleError::SwapUnsupported)) => {
            Err("This storage volume cannot safely reset the profile. Nothing was deleted.".into())
        }
        Err(ProfileLifecycleAdmissionError::Lifecycle(_)) => {
            let snapshot = PreviewProfileSnapshot::lifecycle_unreadable("needs-attention");
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = snapshot;
            }
            Err("The stored profile needs attention before it can be reset.".into())
        }
        Err(ProfileLifecycleAdmissionError::AuthorityLost)
        | Err(ProfileLifecycleAdmissionError::Coordination(_)) => {
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = PreviewProfileSnapshot::unavailable();
            }
            if let Ok(mut model) = state.model.lock() {
                if model.reducer.startup() == StartupState::Ready {
                    let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
                }
                model.error = Some("Private storage needs attention before recording.".into());
            }
            Err("Private storage needs attention before resetting this profile.".into())
        }
    }
}

#[cfg(target_os = "macos")]
fn reconcile_preview_profile_lifecycle(state: &ApplicationState) -> Result<(), &'static str> {
    // Reset first so any refusal below leaves the recorder surface honestly
    // unavailable instead of retaining a stale sittings list.
    if let Ok(mut cached) = state.preview_enrollment.lock() {
        *cached = PreviewEnrollmentSurface::unavailable();
    }
    let held = state
        .app_data_writer_lock
        .lock()
        .map_err(|_| "the app-data writer lock is unavailable")?;
    let writer = held
        .as_ref()
        .ok_or("the app-data writer lock is unavailable")?;
    let lifecycle = match writer.profile_lifecycle_authority().initialize_or_open() {
        Ok(baseline) => {
            // The sitting evidence store reconciles under the same held
            // authority: crashed recordings become labelled rehearsals and
            // interrupted cleanups resume before anything is projected. The
            // expected encoder digest is the verified manifest's, via the
            // runtime identity captured at worker spawn; before spawn it is
            // unknown and `baseline_from_store` refuses to evaluate recorded
            // evidence against nothing.
            let (expected_encoder, encoder_available) = {
                let runtime = state
                    .runtime
                    .lock()
                    .map_err(|_| "the runtime identity is unavailable")?;
                (
                    runtime
                        .as_ref()
                        .map(|runtime| runtime.encoder_sha256.clone()),
                    runtime.as_ref().map(|runtime| runtime.encoder_available),
                )
            };
            match writer
                .sitting_evidence_authority()
                .reconcile_and_read(now_epoch_seconds())
            {
                Ok((evidence, summaries)) => {
                    let surface = enrollment_surface_from_summaries(
                        &summaries,
                        encoder_available,
                        None,
                        None,
                    );
                    if let Ok(mut cached) = state.preview_enrollment.lock() {
                        *cached = surface;
                    }
                    Ok(PreviewProfileSnapshot::baseline_from_store(
                        baseline.profile_present(),
                        baseline.profile_active(),
                        evidence,
                        expected_encoder.as_deref(),
                    ))
                }
                Err(SittingEvidenceAdmissionError::Evidence(_)) => Ok(
                    PreviewProfileSnapshot::lifecycle_unreadable("needs-attention"),
                ),
                Err(SittingEvidenceAdmissionError::AuthorityLost) => {
                    Err("the app-data writer authority changed")
                }
                Err(SittingEvidenceAdmissionError::ActiveMeeting) => {
                    Err("sitting evidence startup overlapped an active meeting")
                }
                Err(SittingEvidenceAdmissionError::Coordination(_)) => {
                    Err("sitting evidence storage coordination is unavailable")
                }
            }
        }
        Err(ProfileLifecycleAdmissionError::Lifecycle(
            ProfileLifecycleError::MigrationReviewRequired,
        )) => Ok(PreviewProfileSnapshot::lifecycle_unreadable(
            "migration-review-required",
        )),
        Err(ProfileLifecycleAdmissionError::Lifecycle(ProfileLifecycleError::Quarantined)) => Ok(
            PreviewProfileSnapshot::lifecycle_unreadable("needs-attention"),
        ),
        Err(ProfileLifecycleAdmissionError::Lifecycle(_)) => Ok(
            PreviewProfileSnapshot::lifecycle_unreadable("needs-attention"),
        ),
        Err(ProfileLifecycleAdmissionError::AuthorityLost) => {
            Err("the app-data writer authority changed")
        }
        Err(ProfileLifecycleAdmissionError::ActiveMeeting) => {
            Err("profile lifecycle startup overlapped an active meeting")
        }
        Err(ProfileLifecycleAdmissionError::Coordination(_)) => {
            Err("profile lifecycle storage coordination is unavailable")
        }
    };
    drop(held);
    match lifecycle {
        Ok(profile) => {
            *state
                .preview_profile
                .lock()
                .map_err(|_| "the cached profile status is unavailable")? = profile;
            Ok(())
        }
        Err(error) => {
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = PreviewProfileSnapshot::unavailable();
            }
            Err(error)
        }
    }
}

/// Builds the recorder surface from the evidence store's projection. The
/// availability ladder names the actual boundary: an active take, an unknown
/// runtime, a build without the admitted encoder — and opens only when the
/// verified manifest carries the encoder the store will bind evidence to.
#[cfg(target_os = "macos")]
fn enrollment_surface_from_summaries(
    summaries: &[local_meeting_notes_session_core::sitting_evidence::SittingRecordSummary],
    encoder_available: Option<bool>,
    hold: Option<&'static str>,
    last_outcome: Option<&'static str>,
) -> PreviewEnrollmentSurface {
    let (recording_available, recording_unavailable_reason, attempt_active) = match hold {
        Some(reason) => (false, Some(reason), true),
        None => match encoder_available {
            None => (false, Some(RECORDER_REASON_RUNTIME_UNKNOWN), false),
            Some(false) => (false, Some(RECORDER_REASON_NO_ENCODER), false),
            Some(true) => (true, None, false),
        },
    };
    PreviewEnrollmentSurface {
        recording_available,
        recording_unavailable_reason,
        sittings: summaries
            .iter()
            .map(|summary| PreviewSittingSummary {
                sitting_id: summary.sitting_id.clone(),
                kind: sitting_kind_label(summary.kind),
                source_class: summary.source_class.clone(),
                state: sitting_state_label(summary.state),
            })
            .collect(),
        last_outcome,
        attempt_active,
    }
}

/// Recomputes both cached Preview projections while the caller already holds
/// the app-data writer. The sitting thread holds that lock for the whole
/// take, so it must not re-enter `reconcile_preview_profile_lifecycle`,
/// which locks it itself. Mid-session the migration-review state cannot
/// appear — startup already reconciled it — so lifecycle refusals collapse
/// to the honest needs-attention projection.
#[cfg(target_os = "macos")]
fn refresh_preview_caches_with_writer(
    state: &ApplicationState,
    writer: &AppDataWriterLock,
    hold: Option<&'static str>,
    last_outcome: Option<&'static str>,
) {
    let (expected_encoder, encoder_available) = match state.runtime.lock() {
        Ok(runtime) => (
            runtime
                .as_ref()
                .map(|runtime| runtime.encoder_sha256.clone()),
            runtime.as_ref().map(|runtime| runtime.encoder_available),
        ),
        Err(_) => (None, None),
    };
    let baseline = writer.profile_lifecycle_authority().initialize_or_open();
    let evidence = writer
        .sitting_evidence_authority()
        .reconcile_and_read(now_epoch_seconds());
    match (baseline, evidence) {
        (Ok(baseline), Ok((evidence, summaries))) => {
            let surface = enrollment_surface_from_summaries(
                &summaries,
                encoder_available,
                hold,
                last_outcome,
            );
            if let Ok(mut cached) = state.preview_enrollment.lock() {
                *cached = surface;
            }
            let snapshot = PreviewProfileSnapshot::baseline_from_store(
                baseline.profile_present(),
                baseline.profile_active(),
                evidence,
                expected_encoder.as_deref(),
            );
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = snapshot;
            }
        }
        _ => {
            if let Ok(mut cached) = state.preview_enrollment.lock() {
                *cached = PreviewEnrollmentSurface::unavailable();
            }
            if let Ok(mut cached) = state.preview_profile.lock() {
                *cached = PreviewProfileSnapshot::lifecycle_unreadable("needs-attention");
            }
        }
    }
}

/// Validates the operator's recording request against the closed kind and
/// source-class vocabulary before anything is spawned or stored. Sentences
/// stay content-free and name the boundary, not a retry.
#[cfg(target_os = "macos")]
fn parse_sitting_request(
    kind: &str,
    source_class: Option<&str>,
) -> Result<
    (
        local_meeting_notes_session_core::sitting_evidence::SittingKind,
        Option<String>,
    ),
    String,
> {
    use local_meeting_notes_session_core::enrollment_guidance::PERMITTED_NEGATIVE_SOURCE_CLASSES;
    use local_meeting_notes_session_core::sitting_evidence::SittingKind;
    match kind {
        "operator-sitting" => {
            if source_class.is_some() {
                return Err("A voice session does not name a comparison source.".into());
            }
            Ok((SittingKind::OperatorSitting, None))
        }
        "negative-source" => {
            let class = source_class.ok_or_else(|| {
                "A comparison recording needs its permitted source named.".to_string()
            })?;
            if !PERMITTED_NEGATIVE_SOURCE_CLASSES.contains(&class) {
                return Err("That comparison source is not permitted.".into());
            }
            Ok((SittingKind::NegativeSource, Some(class.to_string())))
        }
        _ => Err("The setup recording kind is not recognized.".into()),
    }
}

/// Everything the start command must refuse before spawning the sitting
/// thread. Split from the command so refusal paths are testable without a
/// Tauri runtime. Holds no lock on return; the caller re-checks nothing —
/// the sitting-task slot is claimed here, and the optimistic surface is
/// written inside the same claim, before any thread exists, so the thread's
/// own refreshes always come later and can never be overwritten by it.
#[cfg(target_os = "macos")]
fn claim_sitting_start(
    state: &ApplicationState,
    sitting_id: &str,
    kind_label: &'static str,
    source_class: Option<String>,
    sender: mpsc::Sender<()>,
) -> Result<(), String> {
    let model = state
        .model
        .lock()
        .map_err(|_| "the application state is unavailable".to_string())?;
    if model.reducer.startup() != StartupState::Ready {
        return Err("Finish the installation check before a setup recording.".into());
    }
    if model.reducer.capture() != CaptureState::Idle {
        return Err("Finish the current meeting before a setup recording.".into());
    }
    if state
        .capture_task
        .lock()
        .map_err(|_| "the application state is unavailable".to_string())?
        .is_some()
    {
        return Err("Finish the current meeting before a setup recording.".into());
    }
    let encoder_available = state
        .runtime
        .lock()
        .map_err(|_| "the application state is unavailable".to_string())?
        .as_ref()
        .map(|runtime| runtime.encoder_available);
    match encoder_available {
        None => return Err(RECORDER_REASON_RUNTIME_UNKNOWN.into()),
        Some(false) => return Err(RECORDER_REASON_NO_ENCODER.into()),
        Some(true) => {}
    }
    let mut active = state
        .sitting_task
        .lock()
        .map_err(|_| "the application state is unavailable".to_string())?;
    if active.is_some() {
        return Err(RECORDER_REASON_SITTING_ACTIVE.into());
    }
    *active = Some(SittingTaskControl {
        sitting_id: sitting_id.to_string(),
        sender,
    });
    if let Ok(mut cached) = state.preview_enrollment.lock() {
        cached.recording_available = false;
        cached.recording_unavailable_reason = Some(RECORDER_REASON_SITTING_ACTIVE);
        cached.attempt_active = true;
        cached.last_outcome = None;
        cached.sittings.push(PreviewSittingSummary {
            sitting_id: sitting_id.to_string(),
            kind: kind_label,
            source_class,
            state: "recording-in-progress",
        });
    }
    Ok(())
}

/// Clears the task slot on every exit from the sitting thread — including a
/// panic, which would otherwise strand the slot and permanently refuse every
/// command `sitting_task_active` gates. On the panic path it also strips the
/// optimistic projection, since no refresh will ever arrive to correct it.
#[cfg(target_os = "macos")]
struct SittingTaskGuard<'a> {
    state: &'a ApplicationState,
    sitting_id: &'a str,
    completed: bool,
}

#[cfg(target_os = "macos")]
impl Drop for SittingTaskGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            if let Ok(mut cached) = self.state.preview_enrollment.lock() {
                cached.recording_available = false;
                cached.recording_unavailable_reason = Some(RECORDER_REASON_STATUS_UNAVAILABLE);
                cached.attempt_active = false;
                cached.last_outcome = Some(SITTING_OUTCOME_REHEARSAL);
                cached
                    .sittings
                    .retain(|sitting| sitting.state != "recording-in-progress");
            }
        }
        clear_sitting_task(self.state, self.sitting_id);
    }
}

/// Worker derivation of one finalized sitting transcribes and embeds the
/// take, so its budget is minutes, not the interactive request timeout.
#[cfg(target_os = "macos")]
const SITTING_DERIVE_TIMEOUT: Duration = Duration::from_secs(300);

/// Terminal cache write for a finished (or never-started) attempt. Prefers a
/// full store refresh through the writer; without a writer it still strips
/// the optimistic projection, so a failed attempt can never leave the
/// surface claiming a live take.
#[cfg(target_os = "macos")]
fn finish_sitting_attempt(state: &ApplicationState, outcome: &'static str) -> &'static str {
    let writer = state
        .app_data_writer_lock
        .lock()
        .ok()
        .and_then(|held| held.clone());
    match writer {
        Some(writer) => refresh_preview_caches_with_writer(state, &writer, None, Some(outcome)),
        None => {
            if let Ok(mut cached) = state.preview_enrollment.lock() {
                let mut surface = PreviewEnrollmentSurface::unavailable();
                surface.last_outcome = Some(outcome);
                *cached = surface;
            }
        }
    }
    outcome
}

/// One full sitting attempt: capture through the helper's mic-only mode,
/// then worker derivation, then admission of the derived rows. The take
/// holds the writer by Arc — not the slot's mutex guard — so commands that
/// only need the coordination handle stay unblocked for its whole duration;
/// exclusivity against mutating commands is the `sitting_task_active`
/// refusal set. Returns the content-free outcome sentence for the surface.
/// Every path, including the ones that fail before any capture, ends in a
/// terminal cache write.
#[cfg(target_os = "macos")]
fn run_sitting_attempt(
    state: &ApplicationState,
    sitting_id: &str,
    kind: local_meeting_notes_session_core::sitting_evidence::SittingKind,
    source_class: Option<&str>,
    stop: &mpsc::Receiver<()>,
) -> &'static str {
    use local_meeting_notes_session_core::sitting_evidence::SittingLifecycleState;
    let runtime = match state.runtime.lock() {
        Ok(runtime) => runtime.clone(),
        Err(_) => None,
    };
    let Some(runtime) = runtime else {
        return finish_sitting_attempt(state, SITTING_OUTCOME_NOT_STARTED);
    };
    let process_group_id = match inspect_worker(state, &runtime) {
        Ok((process_group_id, _)) => process_group_id,
        Err(_) => return finish_sitting_attempt(state, SITTING_OUTCOME_NOT_STARTED),
    };
    let writer = match state.app_data_writer_lock.lock() {
        Ok(held) => held.clone(),
        Err(_) => None,
    };
    let Some(writer) = writer else {
        return finish_sitting_attempt(state, SITTING_OUTCOME_NOT_STARTED);
    };
    let outcome = {
        let authority = writer.sitting_evidence_authority();
        match run_sitting_capture(
            &authority,
            &runtime.tap_path,
            process_group_id,
            sitting_id,
            kind,
            source_class,
            stop,
        ) {
            Ok(_receipt) => {
                // The capture is closed; the surface must stop claiming a
                // live take while the worker derives, which can run to the
                // full SITTING_DERIVE_TIMEOUT.
                refresh_preview_caches_with_writer(
                    state,
                    &writer,
                    Some(RECORDER_REASON_DERIVING),
                    None,
                );
                let derived = request_worker(
                    state,
                    Operation::SittingDerive,
                    json!({ "sitting_id": sitting_id }),
                    SITTING_DERIVE_TIMEOUT,
                )
                .and_then(|_digests| {
                    authority
                        .admit_derived_material(
                            sitting_id,
                            &runtime.encoder_sha256,
                            now_epoch_seconds(),
                        )
                        .map_err(|error| WorkerCallError::Supervisor(error.to_string()))
                });
                match derived {
                    Ok(SittingLifecycleState::Saved) => SITTING_OUTCOME_SAVED,
                    Ok(SittingLifecycleState::CleanupPending) => SITTING_OUTCOME_CLEANUP_PENDING,
                    Ok(_) | Err(_) => SITTING_OUTCOME_RAW_RETAINED,
                }
            }
            Err(_failure) => SITTING_OUTCOME_REHEARSAL,
        }
    };
    refresh_preview_caches_with_writer(state, &writer, None, Some(outcome));
    outcome
}

#[cfg(target_os = "macos")]
fn run_sitting_task(
    app: AppHandle,
    sitting_id: String,
    kind: local_meeting_notes_session_core::sitting_evidence::SittingKind,
    source_class: Option<String>,
    stop: mpsc::Receiver<()>,
) {
    let state = app.state::<ApplicationState>();
    let mut guard = SittingTaskGuard {
        state: &state,
        sitting_id: &sitting_id,
        completed: false,
    };
    let _ = run_sitting_attempt(
        guard.state,
        &sitting_id,
        kind,
        source_class.as_deref(),
        &stop,
    );
    guard.completed = true;
}

/// Starts one dedicated enrolment sitting. Registered 2026-08-04 by the
/// operator's guided-enrollment registration decision — the slice
/// `run_sitting_capture`'s contract reserved for "the future registration
/// slice". Profile build and activation stay unregistered.
#[tauri::command]
fn preview_enrollment_start_sitting(
    app: AppHandle,
    kind: String,
    source_class: Option<String>,
) -> Result<PreviewEnrollmentSurface, String> {
    let (parsed_kind, parsed_class) = parse_sitting_request(&kind, source_class.as_deref())?;
    let state = app.state::<ApplicationState>();
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| "the recording action is unavailable".to_string())?;
    let sitting_id = Uuid::new_v4().to_string();
    let (sender, receiver) = mpsc::channel();
    claim_sitting_start(
        &state,
        &sitting_id,
        sitting_kind_label(parsed_kind),
        parsed_class.clone(),
        sender,
    )?;
    let task_app = app.clone();
    let task_sitting_id = sitting_id.clone();
    let spawn = std::thread::Builder::new()
        .name("sitting-capture-attempt".into())
        .spawn(move || {
            run_sitting_task(
                task_app,
                task_sitting_id,
                parsed_kind,
                parsed_class,
                receiver,
            )
        });
    if spawn.is_err() {
        clear_sitting_task(&state, &sitting_id);
        finish_sitting_attempt(&state, SITTING_OUTCOME_NOT_STARTED);
        return Err("The setup recording could not start.".into());
    }
    // The claim already wrote the optimistic projection; whatever is cached
    // now — that projection, or a fast-failing thread's honest outcome — is
    // the freshest truth to return.
    Ok(preview_enrollment_surface_for(&state))
}

/// Requests Stop for the active sitting. Deliberately takes no
/// `command_lock` — see `SittingTaskControl` — and never blocks: it only
/// reads the control slot and signals the thread that owns every refusal.
#[tauri::command]
fn preview_enrollment_stop_sitting(state: State<'_, ApplicationState>) -> Result<(), String> {
    preview_enrollment_stop_sitting_for(&state)
}

fn preview_enrollment_stop_sitting_for(state: &ApplicationState) -> Result<(), String> {
    let active = state
        .sitting_task
        .lock()
        .map_err(|_| "the recording action is unavailable".to_string())?;
    let control = active
        .as_ref()
        .ok_or_else(|| "No setup recording is in progress.".to_string())?;
    control
        .sender
        .send(())
        .map_err(|_| "The setup recording ended before Stop completed.".to_string())
}

/// One measured operating point, exactly as the canonical arithmetic
/// produced it. Deserialized from the worker's relay document (snake_case)
/// and serialized to the shell (camelCase); no field is invented or renamed
/// in between, so the surface can only show what was measured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
#[serde(deny_unknown_fields)]
struct MeasuredOperatingPoint {
    target_frr: f64,
    threshold: f64,
    measured_frr: f64,
    n_operator: u32,
    false_admit_rate: Option<f64>,
    n_other: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoicesEvidenceRef {
    #[allow(dead_code)]
    sitting_id: String,
    #[allow(dead_code)]
    audio_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoicesEvidence {
    #[allow(dead_code)]
    sittings: Vec<ChoicesEvidenceRef>,
    #[allow(dead_code)]
    negative_sources: Vec<ChoicesEvidenceRef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoicesDocument {
    schema: String,
    encoder_sha256: String,
    #[allow(dead_code)]
    evidence: ChoicesEvidence,
    #[allow(dead_code)]
    n_operator_scores: u64,
    #[allow(dead_code)]
    n_negative_scores: u64,
    #[allow(dead_code)]
    negative_scorable_seconds: f64,
    choices: Vec<MeasuredOperatingPoint>,
}

/// The measured choices as the shell receives them. `choices_sha256` is the
/// deterministic digest of the relay document — identical evidence yields an
/// identical digest — and the build command refuses a selection whose digest
/// no longer matches, so the operator can only build the row they reviewed.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewOperatingPointsResponse {
    state: &'static str,
    choices_sha256: Option<String>,
    points: Vec<MeasuredOperatingPoint>,
    message: String,
}

impl PreviewOperatingPointsResponse {
    fn refused(message: &str) -> Self {
        Self {
            state: "refused",
            choices_sha256: None,
            points: Vec::new(),
            message: message.into(),
        }
    }

    fn unavailable() -> Self {
        Self {
            state: "unavailable",
            choices_sha256: None,
            points: Vec::new(),
            message: "The measured options are unavailable right now. Try again.".into(),
        }
    }
}

/// Runs one `profile.choices` exchange and returns the parsed, digest-bound
/// document. The relay file is deleted after reading on every path — it is a
/// transport, never a record.
#[cfg(target_os = "macos")]
fn request_measured_choices(
    state: &ApplicationState,
) -> Result<(String, ChoicesDocument), PreviewOperatingPointsResponse> {
    let operation_id = Uuid::new_v4().to_string();
    let digests = match request_worker(
        state,
        Operation::ProfileChoices,
        json!({ "operation_id": operation_id }),
        WORKER_REQUEST_TIMEOUT,
    ) {
        Ok(digests) => digests,
        Err(WorkerCallError::Rejected) => {
            return Err(PreviewOperatingPointsResponse::refused(
                "The measured options are not ready. Finish the recording steps first.",
            ));
        }
        Err(WorkerCallError::Supervisor(_)) => {
            return Err(PreviewOperatingPointsResponse::unavailable());
        }
    };
    let digests = exact_digests(&digests, &["choices"])
        .map_err(|_| PreviewOperatingPointsResponse::unavailable())?;
    let expected = digests["choices"].clone();
    let storage =
        preview_storage_clone(state).map_err(|_| PreviewOperatingPointsResponse::unavailable())?;
    let path = storage
        .resolve(&Path::new("enrollment-choices").join(format!("{operation_id}.json")))
        .map_err(|_| PreviewOperatingPointsResponse::unavailable())?;
    let bytes = fs::read(&path).map_err(|_| PreviewOperatingPointsResponse::unavailable())?;
    let _ = fs::remove_file(&path);
    if format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err(PreviewOperatingPointsResponse::unavailable());
    }
    let document: ChoicesDocument = serde_json::from_slice(&bytes)
        .map_err(|_| PreviewOperatingPointsResponse::unavailable())?;
    if document.schema != "profile-choices/1" {
        return Err(PreviewOperatingPointsResponse::unavailable());
    }
    let manifest_encoder = state.runtime.lock().ok().and_then(|runtime| {
        runtime
            .as_ref()
            .map(|runtime| runtime.encoder_sha256.clone())
    });
    if manifest_encoder.as_deref() != Some(document.encoder_sha256.as_str()) {
        return Err(PreviewOperatingPointsResponse::unavailable());
    }
    Ok((expected, document))
}

/// § I `choosing-operating-point`: the one screen that presents a trade-off
/// rather than a reading. This command only reports — two or three measured
/// rows, no default, nothing written — and the digest it returns is what the
/// separate build command demands back.
#[tauri::command]
fn preview_enrollment_operating_points(
    state: State<'_, ApplicationState>,
) -> PreviewOperatingPointsResponse {
    preview_enrollment_operating_points_for(&state)
}

#[cfg(target_os = "macos")]
fn preview_enrollment_operating_points_for(
    state: &ApplicationState,
) -> PreviewOperatingPointsResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return PreviewOperatingPointsResponse::unavailable();
    };
    if sitting_task_active(state) {
        return PreviewOperatingPointsResponse::refused("Finish the setup recording first.");
    }
    {
        let Ok(model) = state.model.lock() else {
            return PreviewOperatingPointsResponse::unavailable();
        };
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return PreviewOperatingPointsResponse::refused(
                "Finish the current app or recording operation first.",
            );
        }
    }
    match request_measured_choices(state) {
        Ok((digest, document)) => PreviewOperatingPointsResponse {
            state: "choices",
            choices_sha256: Some(digest),
            points: document.choices,
            message: "Options measured from your own recordings. None is chosen for you.".into(),
        },
        Err(response) => response,
    }
}

/// Builds and publishes the profile for one explicitly selected measured
/// row. The worker recomputes the choices and refuses a target the evidence
/// no longer supports; this command additionally refuses when the recomputed
/// document's digest differs from the one the operator reviewed, then runs
/// the strict-loader bridge: worker-validated candidate, descriptor-reopened
/// digest, lifecycle publication, candidate cleanup.
#[tauri::command]
fn preview_enrollment_build_profile(
    selected_target: f64,
    choices_sha256: String,
    state: State<'_, ApplicationState>,
) -> Result<PreviewProfileSnapshot, String> {
    preview_enrollment_build_profile_for(&state, selected_target, &choices_sha256)
}

#[cfg(target_os = "macos")]
fn preview_enrollment_build_profile_for(
    state: &ApplicationState,
    selected_target: f64,
    choices_sha256: &str,
) -> Result<PreviewProfileSnapshot, String> {
    if !valid_sha256(choices_sha256) {
        return Err("The reviewed options could not be identified. Review them again.".into());
    }
    if !(selected_target.is_finite() && 0.0 < selected_target && selected_target < 1.0) {
        return Err("The selected option is not one of the measured choices.".into());
    }
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| "the profile action is unavailable".to_string())?;
    if sitting_task_active(state) {
        return Err("Finish the setup recording first.".into());
    }
    {
        let model = state
            .model
            .lock()
            .map_err(|_| "the application state is unavailable".to_string())?;
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("Finish the current app or recording operation first.".into());
        }
    }
    // The selection binds to the reviewed measurements, not to hope: the
    // choices are recomputed and must hash to exactly what the operator saw.
    let (current_digest, _document) =
        request_measured_choices(state).map_err(|response| response.message)?;
    if current_digest != choices_sha256 {
        return Err("The measured options changed. Review them again.".into());
    }
    let profile_id = Uuid::new_v4().to_string();
    let built = request_worker(
        state,
        Operation::ProfileBuild,
        json!({ "profile_id": profile_id, "selected_target": selected_target }),
        WORKER_REQUEST_TIMEOUT,
    )
    .map_err(|error| match error {
        WorkerCallError::Rejected => {
            "The profile could not be built from the current evidence.".to_string()
        }
        WorkerCallError::Supervisor(_) => "the local worker is unavailable".to_string(),
    })?;
    let built = exact_digests(&built, &["profile"])?;
    let built_sha256 = built["profile"].clone();

    let completion = {
        let held = state
            .app_data_writer_lock
            .lock()
            .map_err(|_| "the app-data writer lock is unavailable".to_string())?;
        let writer = held
            .as_ref()
            .ok_or_else(|| "the app-data writer lock is unavailable".to_string())?;
        let bridge = StrictProfileEnrollmentWorker { state };
        let completion = writer
            .profile_lifecycle_authority()
            .enroll_profile_candidate(&bridge, &profile_id, now_epoch_seconds())
            .map_err(|error| match error {
                ProfileEnrollmentAdmissionError::ActiveMeeting => {
                    "Finish the current recording before completing setup.".to_string()
                }
                ProfileEnrollmentAdmissionError::Worker(_) => {
                    "The built profile did not pass its final check. Nothing was stored."
                        .to_string()
                }
                ProfileEnrollmentAdmissionError::CandidateUnsafe => {
                    "The built profile did not pass its final check. Nothing was stored."
                        .to_string()
                }
                _ => "Profile storage needs attention. Nothing was stored.".to_string(),
            })?;
        refresh_preview_caches_with_writer(state, writer, None, None);
        completion
    };
    let stored_sha256 = match &completion {
        ProfileEnrollmentCompletion::Enrolled { profile_sha256, .. }
        | ProfileEnrollmentCompletion::CleanupPending { profile_sha256, .. } => profile_sha256,
    };
    if stored_sha256 != &built_sha256 {
        return Err(
            "The stored profile disagrees with the built candidate. Review the profile status."
                .into(),
        );
    }
    Ok(preview_profile_snapshot_for(state))
}

/// Roadmap intake W8-B. Registered, unlike the rest of `library_reader`'s
/// surface, but its very first act is the probe flag check: with
/// `search-probe.flag` absent from the storage root, this refuses exactly the
/// way an unregistered command would have -- no library read, no handle
/// mint, and (per the module doc on `search_probe`) no log line, because
/// nothing ran. Only once the flag is present does this delegate to
/// `LibraryReader::search`, the same W5-B-hardened path
/// `library_reader.rs`'s own tests already prove excludes a locked meeting's
/// every hit kind (see `search_current`'s doc comment there); this command
/// adds the flag gate and the probe's own log line, and changes nothing about
/// that exclusion.
#[tauri::command]
fn preview_library_search(
    query: String,
    state: State<'_, ApplicationState>,
) -> library_reader::LibrarySearchResponse {
    preview_library_search_for(&query, &state)
}

fn preview_library_search_for(
    query: &str,
    state: &ApplicationState,
) -> library_reader::LibrarySearchResponse {
    let Ok(storage) = preview_storage_clone(state) else {
        return library_reader::LibraryReader::unavailable_search();
    };
    if !search_probe::enabled(&storage) {
        return library_reader::LibrarySearchResponse {
            state: "probe-disabled",
            results: Vec::new(),
            total_matches: 0,
            unavailable_count: 0,
            message: search_probe::REFUSAL_MESSAGE.into(),
        };
    }
    search_probe::record(&storage, search_probe::ProbeEvent::Invoked);
    let mut response = state.with_preview_library(
        library_reader::LibraryReader::unavailable_search,
        |reader, active| reader.search(query, active),
    );
    // Preview's active reader is deliberately transcript/title metadata only.
    // The broader projection contract also supports future claim readers, but
    // this surface cannot make claim text a search destination yet.
    response
        .results
        .retain(|result| matches!(result.kind, "transcript" | "withheld" | "meeting"));
    if matches!(response.state, "results" | "results-incomplete") && response.results.is_empty() {
        if response.unavailable_count == 0 {
            response.state = "no-results";
            response.message =
                "No retained transcript, title, or folder matched that search.".into();
        } else {
            response.state = "incomplete";
            response.message = format!(
                "No retained transcript, title, or folder match was found among readable meetings. {} could not be searched.",
                response.unavailable_count
            );
        }
    }
    response
}

/// Roadmap intake W8-B. Same flag-gate shape as `preview_library_search`
/// above; see that command's doc comment. `open_search_result` already
/// re-checks the lock at open time (roadmap intake I5's race guard -- a
/// meeting unlocked when the search sealed this handle and relocked before it
/// was opened), so this adds only the probe gate and its own log line, never
/// a second exclusion decision.
#[tauri::command]
fn preview_library_open_search_result(
    handle: String,
    state: State<'_, ApplicationState>,
) -> library_reader::LibrarySearchOpenResponse {
    preview_library_open_search_result_for(&handle, &state)
}

fn preview_library_open_search_result_for(
    handle: &str,
    state: &ApplicationState,
) -> library_reader::LibrarySearchOpenResponse {
    let Ok(storage) = preview_storage_clone(state) else {
        return library_reader::LibrarySearchOpenResponse {
            state: "unavailable",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            message: "The local Preview library is unavailable. Reopen the app and try again."
                .into(),
        };
    };
    if !search_probe::enabled(&storage) {
        return library_reader::LibrarySearchOpenResponse {
            state: "probe-disabled",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            message: search_probe::REFUSAL_MESSAGE.into(),
        };
    }
    search_probe::record(&storage, search_probe::ProbeEvent::Opened);
    state.with_preview_library(
        || library_reader::LibrarySearchOpenResponse {
            state: "unavailable",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            message: "The local Preview library is unavailable. Reopen the app and try again."
                .into(),
        },
        |reader, active| reader.open_search_result(handle, active),
    )
}

/// `lockToken` is roadmap intake I5's read gate: a single-use confirmation
/// minted by `authorize_locked_action` for this exact meeting and the `open`
/// action. It is absent for every unlocked meeting, which is the ordinary
/// case, and a locked meeting without it comes back as `state: "locked"`
/// holding no content and no capability.
#[tauri::command]
fn library_open_note(
    handle: String,
    lock_token: Option<String>,
    state: State<'_, ApplicationState>,
) -> library_reader::LibraryNoteResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return library_reader::LibraryReader::unavailable_note("");
    };
    // A detail view is a playback boundary: reopening it must never leave an
    // earlier recording playing behind a different meeting.
    stop_owned_audio_playback(&state);
    // Spent before the library is touched, so a token is consumed exactly once
    // whether or not the open then succeeds.
    let unlocked = consume_locked_action(
        &state,
        lock_token.as_deref(),
        meeting_lock::LockedAction::Open,
    );
    let mut response = state.with_preview_library(
        || library_reader::LibraryReader::unavailable_note(""),
        |reader, active| reader.open_note(&handle, active, unlocked.as_deref()),
    );
    // Spend and re-issue, exactly as `library_open_transcript_file` and
    // `library_export_meeting` do with the transcript handle. This is the
    // "unlocked for reading" session made concrete: one confirmation opened
    // the meeting, and each successful read hands the next one its authority.
    // The chain is rooted in that single confirmation and can only ever open
    // this same meeting — it authorizes no export and no playback, it dies the
    // moment the shell drops it by leaving the meeting, and locking the
    // meeting revokes it outright.
    // A fact about this Mac, not this meeting. See the field's docs for why it
    // rides the note response and what it may and may not decide.
    response.can_confirm_operator = state.confirmation.available();

    // Compute note generation availability. This reflects live storage state
    // so a model installed or removed since startup changes the answer.
    let bridge = product_coordinator::WorkerProcessNoteGenerationBridge::new(
        Arc::new(product_coordinator::ProcessWorkerPort::new(state.worker.clone())),
        state.storage.clone(),
    );
    let (available, reason) = bridge.admission_check();
    response.note_generation_available = available;
    response.note_generation_unavailable_reason = reason;

    if response.lock.locked && response.state != "locked" {
        if let Ok(mut authority) = state.locked_actions.lock() {
            response.lock_token =
                Some(authority.mint(&response.meeting_id, meeting_lock::LockedAction::Open));
        }
    }
    response
}

/// Starts the only retained-audio playback route. `handle` is an opaque,
/// source-bound Library capability; the browser cannot select a path, source,
/// executable, or any other child-process argument.
#[tauri::command]
fn library_play_retained_audio(
    handle: String,
    lock_token: Option<String>,
    state: State<'_, ApplicationState>,
) -> RetainedAudioPlaybackResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return audio_playback_response_coded(
            "unavailable",
            None,
            "Retained audio is unavailable. Reopen Library and try again.",
            Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
        );
    };
    // A new source always owns the sole player slot. Reap the former child
    // before consuming the freshly revalidated capability for this launch.
    stop_owned_audio_playback(&state);
    let unlocked = consume_locked_action(
        &state,
        lock_token.as_deref(),
        meeting_lock::LockedAction::Playback,
    );
    let grant = match state.with_preview_library(
        || {
            Err(library_reader::LibraryAudioPlaybackAccess {
                state: "unavailable",
                message: "Retained audio is unavailable. Reopen Library and try again.".into(),
            })
        },
        |reader, active| reader.authorize_audio_playback(&handle, active, unlocked.as_deref()),
    ) {
        Ok(grant) => grant,
        Err(access) if access.state == "locked" => {
            // Roadmap intake I5. Said with the reader's own sentence rather
            // than the generic unavailable one: nothing is out of date here,
            // and "reopen it" is the wrong next step.
            return audio_playback_response("locked", None, library_reader::LOCKED_MESSAGE);
        }
        Err(access) if access.state == "stale" => {
            return audio_playback_response_coded(
                "unavailable",
                None,
                "That view is no longer current. Reopen it and try again.",
                Some(error_codes::VIEW_STALE),
            );
        }
        Err(_) => {
            return audio_playback_response_coded(
                "unavailable",
                None,
                "Retained audio is unavailable. Reopen Library and try again.",
                Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
            );
        }
    };
    let source = match grant.source() {
        library_reader::RetainedAudioSource::Microphone => "microphone",
        library_reader::RetainedAudioSource::System => "system",
    };
    let playback = match RetainedAudioPlayback::spawn(grant) {
        Ok(playback) => playback,
        Err(_) => {
            return audio_playback_response_coded(
                "unavailable",
                None,
                "Retained audio is unavailable. Reopen Library and try again.",
                Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
            );
        }
    };
    let Ok(mut slot) = state.audio_playback.lock() else {
        let mut playback = playback;
        playback.stop_and_reap();
        return audio_playback_response_coded(
            "unavailable",
            None,
            "Retained audio is unavailable. Reopen Library and try again.",
            Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
        );
    };
    *slot = Some(playback);
    audio_playback_response("playing", Some(source), "Playing retained audio.")
}

#[tauri::command]
fn library_retained_audio_playback_status(
    state: State<'_, ApplicationState>,
) -> RetainedAudioPlaybackResponse {
    owned_audio_playback_status(&state)
}

#[tauri::command]
fn library_stop_retained_audio(
    state: State<'_, ApplicationState>,
) -> RetainedAudioPlaybackResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return audio_playback_response_coded(
            "unavailable",
            None,
            "Retained audio is unavailable. Reopen Library and try again.",
            Some(error_codes::RETAINED_AUDIO_UNAVAILABLE),
        );
    };
    stop_owned_audio_playback(&state);
    audio_playback_response("idle", None, "No recording is playing.")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewAudioDeletionResponse {
    state: &'static str,
    audio_retention: Option<library_reader::LibraryAudioRetention>,
    message: String,
    /// See `RetainedAudioPlaybackResponse::code`; this response is re-thrown
    /// as an `Error` the same way (see `main.js`'s delete-recording handler).
    code: Option<&'static str>,
}

fn unavailable_preview_audio_deletion() -> PreviewAudioDeletionResponse {
    PreviewAudioDeletionResponse {
        state: "unavailable",
        audio_retention: None,
        message: "Recording deletion is unavailable. Reopen Library and try again.".into(),
        code: Some(error_codes::RECORDING_DELETION_UNAVAILABLE),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PreviewAudioDeletionGateRefusal {
    state: &'static str,
    message: &'static str,
}

fn with_preview_audio_deletion_gate<T>(
    startup: StartupState,
    capture: CaptureState,
    operation: impl FnOnce() -> T,
) -> Result<T, PreviewAudioDeletionGateRefusal> {
    if startup != StartupState::Ready {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "not-ready",
            message: "Recording deletion is available after the installation check is ready.",
        });
    }
    if !matches!(capture, CaptureState::Idle | CaptureState::TranscriptReady) {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "capture-active",
            message: "Recording deletion is unavailable while a meeting is active or recovering.",
        });
    }
    Ok(operation())
}

#[tauri::command]
fn preview_delete_meeting_audio(
    handle: String,
    state: State<'_, ApplicationState>,
) -> PreviewAudioDeletionResponse {
    preview_delete_meeting_audio_for(handle, &state)
}

fn preview_delete_meeting_audio_for(
    handle: String,
    state: &ApplicationState,
) -> PreviewAudioDeletionResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return unavailable_preview_audio_deletion();
    };
    stop_owned_audio_playback(state);
    // Deletion mutates retained-audio state the take's evidence writes sit
    // beside; like every mutating command it refuses during a take instead
    // of interleaving with it.
    if sitting_task_active(state) {
        return PreviewAudioDeletionResponse {
            state: "capture-active",
            audio_retention: None,
            message: "Finish the setup recording before deleting a recording.".into(),
            code: None,
        };
    }
    let (startup, capture) = match state.model.lock() {
        Ok(model) => (model.reducer.startup(), model.reducer.capture()),
        Err(_) => return unavailable_preview_audio_deletion(),
    };
    let prepared = match with_preview_audio_deletion_gate(startup, capture, || {
        let storage = preview_storage_clone(state).ok()?;
        let access = state.with_preview_library(
            || library_reader::LibraryAudioDeletionAccess {
                state: "unavailable",
                meeting_id: None,
                message: "Recording deletion is unavailable. Reopen Library and try again.".into(),
                code: Some(error_codes::RECORDING_DELETION_UNAVAILABLE),
            },
            |reader, active| reader.authorize_audio_deletion(&handle, active),
        );
        Some((storage, access))
    }) {
        Ok(prepared) => prepared,
        Err(refusal) => {
            return PreviewAudioDeletionResponse {
                state: refusal.state,
                audio_retention: None,
                message: refusal.message.into(),
                code: None,
            };
        }
    };
    let Some((storage, access)) = prepared else {
        return unavailable_preview_audio_deletion();
    };
    let retention_for = |meeting_id: &str| {
        let coordination = state.meeting_storage_coordination().ok()?;
        with_meeting_storage_sequence(&coordination, |active| {
            (!active.contains(meeting_id))
                .then(|| library_reader::LibraryReader::read_audio_retention(&storage, meeting_id))
        })
        .ok()
        .flatten()
    };

    // Any attempted mutation boundary invalidates every retained reader handle.
    if with_preview_library_invalidated(state, || ()).is_err() {
        return unavailable_preview_audio_deletion();
    }

    let Some(meeting_id) = access.meeting_id else {
        return PreviewAudioDeletionResponse {
            state: access.state,
            audio_retention: None,
            message: access.message,
            code: access.code,
        };
    };
    if access.state != "authorized" {
        return PreviewAudioDeletionResponse {
            state: access.state,
            audio_retention: retention_for(&meeting_id),
            message: access.message,
            code: access.code,
        };
    }

    let result = state
        .manual_audio_deletion_facade()
        .delete_audio(ManualAudioDeletionUiArgs {
            meeting_id: meeting_id.clone(),
            review: AudioDeletionReview::Reviewed,
        });
    let (response_state, message, code) = match result {
        Ok(ManualAudioDeletionFacadeOutcome::AudioReleased) => (
            "released",
            "The meeting recording was permanently deleted from this Mac.",
            None,
        ),
        Ok(ManualAudioDeletionFacadeOutcome::RecoveredRemoval) => (
            "released",
            "The interrupted recording deletion was recovered and completed.",
            None,
        ),
        Ok(ManualAudioDeletionFacadeOutcome::AlreadyReleased) => (
            "already-released",
            "This meeting recording was already deleted.",
            None,
        ),
        Ok(ManualAudioDeletionFacadeOutcome::DeferredActive) => (
            "deferred-active",
            "Recording deletion was deferred because this meeting is still active.",
            None,
        ),
        Err(ManualAudioDeletionFacadeError::MeetingActionInProgress) => (
            "action-in-progress",
            "Another action for this meeting is in progress. Reopen Library and try again.",
            Some(error_codes::MEETING_ACTION_IN_PROGRESS),
        ),
        Err(
            ManualAudioDeletionFacadeError::ConfirmationRequired
            | ManualAudioDeletionFacadeError::WriterLockUnavailable
            | ManualAudioDeletionFacadeError::StorageUnavailable,
        ) => (
            "unavailable",
            "Recording deletion could not complete. Reopen Library and try again.",
            Some(error_codes::RECORDING_DELETION_FAILED),
        ),
    };
    PreviewAudioDeletionResponse {
        state: response_state,
        audio_retention: retention_for(&meeting_id),
        message: message.into(),
        code,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewTranscriptDeletionResponse {
    state: &'static str,
    message: String,
    /// See `RetainedAudioPlaybackResponse::code`; this response is re-thrown
    /// as an `Error` the same way (see `main.js`'s delete-transcript handler).
    code: Option<&'static str>,
}

fn unavailable_preview_transcript_deletion() -> PreviewTranscriptDeletionResponse {
    PreviewTranscriptDeletionResponse {
        state: "unavailable",
        message: "Transcript deletion is unavailable. Reopen Library and try again.".into(),
        code: Some(error_codes::TRANSCRIPT_DELETION_UNAVAILABLE),
    }
}

fn with_preview_transcript_deletion_gate<T>(
    startup: StartupState,
    capture: CaptureState,
    operation: impl FnOnce() -> T,
) -> Result<T, PreviewAudioDeletionGateRefusal> {
    if startup != StartupState::Ready {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "not-ready",
            message: "Transcript deletion is available after the installation check is ready.",
        });
    }
    if !matches!(capture, CaptureState::Idle | CaptureState::TranscriptReady) {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "capture-active",
            message: "Transcript deletion is unavailable while a meeting is active or recovering.",
        });
    }
    Ok(operation())
}

/// Removes the transcript and generated notes after the local shell reports a
/// reviewed confirmation. The separately typed facade token keeps this act
/// distinct from freeing raw audio or deleting the entire meeting.
#[tauri::command(async)]
fn preview_delete_meeting_transcript(
    handle: String,
    confirmed: bool,
    state: State<'_, ApplicationState>,
) -> PreviewTranscriptDeletionResponse {
    preview_delete_meeting_transcript_for(handle, confirmed, &state)
}

fn preview_delete_meeting_transcript_for(
    handle: String,
    confirmed: bool,
    state: &ApplicationState,
) -> PreviewTranscriptDeletionResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return unavailable_preview_transcript_deletion();
    };
    stop_owned_audio_playback(state);
    if sitting_task_active(state) {
        return PreviewTranscriptDeletionResponse {
            state: "capture-active",
            message: "Finish the setup recording before deleting a transcript.".into(),
            code: None,
        };
    }
    let (startup, capture) = match state.model.lock() {
        Ok(model) => (model.reducer.startup(), model.reducer.capture()),
        Err(_) => return unavailable_preview_transcript_deletion(),
    };
    let prepared = match with_preview_transcript_deletion_gate(startup, capture, || {
        state.with_preview_library(
            || library_reader::LibraryTranscriptDeletionAccess {
                state: "unavailable",
                meeting_id: None,
                message: "Transcript deletion is unavailable. Reopen Library and try again.".into(),
                code: Some(error_codes::TRANSCRIPT_DELETION_UNAVAILABLE),
            },
            |reader, active| reader.authorize_transcript_deletion(&handle, active),
        )
    }) {
        Ok(prepared) => prepared,
        Err(refusal) => {
            return PreviewTranscriptDeletionResponse {
                state: refusal.state,
                message: refusal.message.into(),
                code: None,
            };
        }
    };

    if with_preview_library_invalidated(state, || ()).is_err() {
        return unavailable_preview_transcript_deletion();
    }

    let Some(meeting_id) = prepared.meeting_id else {
        return PreviewTranscriptDeletionResponse {
            state: prepared.state,
            message: prepared.message,
            code: prepared.code,
        };
    };
    if prepared.state != "authorized" {
        return PreviewTranscriptDeletionResponse {
            state: prepared.state,
            message: prepared.message,
            code: prepared.code,
        };
    }

    let review = if confirmed {
        manual_delete_facade::TranscriptDeletionReview::Reviewed
    } else {
        manual_delete_facade::TranscriptDeletionReview::NotReviewed
    };
    let result = state
        .transcript_deletion_facade()
        .delete_transcript(manual_delete_facade::TranscriptDeletionUiArgs { meeting_id, review });
    let (response_state, message, code) = match result {
        Ok(manual_delete_facade::TranscriptDeletionFacadeOutcome::TranscriptRemoved) => (
            "removed",
            "The transcript and generated notes were permanently deleted from this Mac.",
            None,
        ),
        Ok(manual_delete_facade::TranscriptDeletionFacadeOutcome::RecoveredRemoval) => (
            "removed",
            "The interrupted transcript deletion was recovered and completed.",
            None,
        ),
        Ok(manual_delete_facade::TranscriptDeletionFacadeOutcome::AlreadyRemoved) => (
            "already-removed",
            "This meeting transcript was already deleted.",
            None,
        ),
        Ok(manual_delete_facade::TranscriptDeletionFacadeOutcome::DeferredActive) => (
            "deferred-active",
            "Transcript deletion was deferred because this meeting is still active.",
            None,
        ),
        Err(manual_delete_facade::TranscriptDeletionFacadeError::ConfirmationRequired) => (
            "confirmation-required",
            "Deleting a transcript removes its generated notes permanently. Confirm to continue.",
            None,
        ),
        Err(manual_delete_facade::TranscriptDeletionFacadeError::MeetingActionInProgress) => (
            "action-in-progress",
            "Another action for this meeting is in progress. Reopen Library and try again.",
            Some(error_codes::MEETING_ACTION_IN_PROGRESS),
        ),
        Err(manual_delete_facade::TranscriptDeletionFacadeError::NoTranscript) => (
            "no-transcript",
            "This meeting has no retained transcript to delete.",
            None,
        ),
        Err(manual_delete_facade::TranscriptDeletionFacadeError::NoSuchMeeting) => {
            ("already-removed", "This meeting was already deleted.", None)
        }
        Err(
            manual_delete_facade::TranscriptDeletionFacadeError::WriterLockUnavailable
            | manual_delete_facade::TranscriptDeletionFacadeError::StorageUnavailable,
        ) => (
            "unavailable",
            "Transcript deletion could not complete. Reopen Library and try again.",
            Some(error_codes::TRANSCRIPT_DELETION_FAILED),
        ),
    };
    PreviewTranscriptDeletionResponse {
        state: response_state,
        message: message.into(),
        code,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewMeetingDeletionResponse {
    state: &'static str,
    message: String,
    /// See `RetainedAudioPlaybackResponse::code`; this response is re-thrown
    /// as an `Error` the same way (see `main.js`'s delete-meeting handler).
    code: Option<&'static str>,
}

fn unavailable_preview_meeting_deletion() -> PreviewMeetingDeletionResponse {
    PreviewMeetingDeletionResponse {
        state: "unavailable",
        message: "Meeting deletion is unavailable. Reopen Library and try again.".into(),
        code: Some(error_codes::MEETING_DELETION_UNAVAILABLE),
    }
}

/// The same startup and capture conditions as audio release, worded for the
/// larger act. The wording is not cosmetic: a refusal that says "recording"
/// when the operator asked to delete a whole meeting leaves them unsure which
/// of the two did not happen.
fn with_preview_meeting_deletion_gate<T>(
    startup: StartupState,
    capture: CaptureState,
    operation: impl FnOnce() -> T,
) -> Result<T, PreviewAudioDeletionGateRefusal> {
    if startup != StartupState::Ready {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "not-ready",
            message: "Meeting deletion is available after the installation check is ready.",
        });
    }
    if !matches!(capture, CaptureState::Idle | CaptureState::TranscriptReady) {
        return Err(PreviewAudioDeletionGateRefusal {
            state: "capture-active",
            message: "Meeting deletion is unavailable while a meeting is active or recovering.",
        });
    }
    Ok(operation())
}

/// Removes one whole meeting after the operator confirmed it twice.
///
/// `confirmed` is the shell reporting that its § G confirmation panel was
/// answered. Be precise about what that buys: this process cannot verify the
/// operator actually saw the panel, so the flag is a report from the trusted
/// local shell, not a proof. It is stricter than the audio path, which hardcodes
/// `Reviewed` and takes no flag at all.
///
/// What the closed `MeetingDeletionReview` token does protect is the in-process
/// boundary. The facade is the only route to storage authority and it refuses
/// without the token, so no future Rust caller reaches whole-meeting removal
/// without stating a reviewed decision — and because the token is a distinct
/// type from `AudioDeletionReview`, it cannot state the wrong one by accident.
#[tauri::command(async)]
fn preview_delete_meeting(
    handle: String,
    confirmed: bool,
    state: State<'_, ApplicationState>,
) -> PreviewMeetingDeletionResponse {
    preview_delete_meeting_for(handle, confirmed, &state)
}

fn preview_delete_meeting_for(
    handle: String,
    confirmed: bool,
    state: &ApplicationState,
) -> PreviewMeetingDeletionResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return unavailable_preview_meeting_deletion();
    };
    stop_owned_audio_playback(state);
    if sitting_task_active(state) {
        return PreviewMeetingDeletionResponse {
            state: "capture-active",
            message: "Finish the setup recording before deleting a meeting.".into(),
            code: None,
        };
    }
    let (startup, capture) = match state.model.lock() {
        Ok(model) => (model.reducer.startup(), model.reducer.capture()),
        Err(_) => return unavailable_preview_meeting_deletion(),
    };
    let prepared = match with_preview_meeting_deletion_gate(startup, capture, || {
        state.with_preview_library(
            || library_reader::LibraryMeetingDeletionAccess {
                state: "unavailable",
                meeting_id: None,
                message: "Meeting deletion is unavailable. Reopen Library and try again.".into(),
                code: Some(error_codes::MEETING_DELETION_UNAVAILABLE),
            },
            |reader, active| reader.authorize_meeting_deletion(&handle, active),
        )
    }) {
        Ok(prepared) => prepared,
        Err(refusal) => {
            return PreviewMeetingDeletionResponse {
                state: refusal.state,
                message: refusal.message.into(),
                code: None,
            };
        }
    };

    // Any attempted mutation boundary invalidates every retained reader handle.
    if with_preview_library_invalidated(state, || ()).is_err() {
        return unavailable_preview_meeting_deletion();
    }

    let Some(meeting_id) = prepared.meeting_id else {
        return PreviewMeetingDeletionResponse {
            state: prepared.state,
            message: prepared.message,
            code: prepared.code,
        };
    };
    if prepared.state != "authorized" {
        return PreviewMeetingDeletionResponse {
            state: prepared.state,
            message: prepared.message,
            code: prepared.code,
        };
    }

    let review = if confirmed {
        manual_delete_facade::MeetingTrashReview::Reviewed
    } else {
        manual_delete_facade::MeetingTrashReview::NotReviewed
    };
    let result = state.meeting_trash_facade().trash_meeting(
        manual_delete_facade::MeetingTrashUiArgs { meeting_id, review },
        now_epoch_seconds(),
    );
    let (response_state, message, code) = match result {
        Ok(manual_delete_facade::MeetingTrashFacadeOutcome::Trashed) => (
            "trashed",
            "The meeting moved to Trash. It stays recoverable there for 30 days, then Yawn removes it permanently and it cannot be recovered.",
            None,
        ),
        Ok(manual_delete_facade::MeetingTrashFacadeOutcome::RecoveredTrash) => (
            "trashed",
            "The interrupted move to Trash was recovered and completed.",
            None,
        ),
        Ok(manual_delete_facade::MeetingTrashFacadeOutcome::AlreadyTrashed) => {
            ("already-trashed", "This meeting is already in Trash.", None)
        }
        Ok(manual_delete_facade::MeetingTrashFacadeOutcome::DeferredActive) => (
            "deferred-active",
            "Moving this meeting to Trash was deferred because it is still active.",
            None,
        ),
        Err(manual_delete_facade::MeetingTrashFacadeError::ConfirmationRequired) => (
            "confirmation-required",
            "Deleting a meeting moves it to Trash for 30 days, then removes it permanently. Confirm to continue.",
            None,
        ),
        Err(manual_delete_facade::MeetingTrashFacadeError::MeetingActionInProgress) => (
            "action-in-progress",
            "Another action for this meeting is in progress. Reopen Library and try again.",
            Some(error_codes::MEETING_ACTION_IN_PROGRESS),
        ),
        Err(manual_delete_facade::MeetingTrashFacadeError::NoSuchMeeting) => {
            ("already-trashed", "This meeting is already in Trash.", None)
        }
        Err(
            manual_delete_facade::MeetingTrashFacadeError::WriterLockUnavailable
            | manual_delete_facade::MeetingTrashFacadeError::StorageUnavailable,
        ) => (
            "unavailable",
            // NOTE: this text has no entry in error-codes.json. It reads
            // like "Meeting deletion could not complete..." but is not that
            // string byte-for-byte, and the frontend's legacy array names
            // the *other* wording -- already a dead match before this
            // packet. See error_codes.rs's module doc for why this is left
            // uncoded rather than "fixed" by coding this message instead.
            "Moving this meeting to Trash could not complete. Reopen Library and try again.",
            None,
        ),
    };
    PreviewMeetingDeletionResponse {
        state: response_state,
        message: message.into(),
        code,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashEntryPresentation {
    meeting_id: String,
    label: String,
    deleted_at_epoch_seconds: u64,
    purge_after_epoch_seconds: u64,
}

/// A quiet, read-only listing of what is currently in Trash. Not
/// handle-based like the library snapshot: a trash entry carries no
/// transcript, note, or audio access, only the identity and dates the Trash
/// row itself shows, so there is nothing here that widening access would
/// expose.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashListResponse {
    state: &'static str,
    entries: Vec<TrashEntryPresentation>,
}

#[tauri::command]
fn preview_list_trash(state: State<'_, ApplicationState>) -> TrashListResponse {
    preview_list_trash_for(&state)
}

fn preview_list_trash_for(state: &ApplicationState) -> TrashListResponse {
    let Ok(storage) = preview_storage_clone(state) else {
        return TrashListResponse {
            state: "unavailable",
            entries: Vec::new(),
        };
    };
    let Ok(entries) = list_trash_entries(&storage) else {
        return TrashListResponse {
            state: "unavailable",
            entries: Vec::new(),
        };
    };
    TrashListResponse {
        state: "ok",
        entries: entries
            .into_iter()
            .map(|entry| TrashEntryPresentation {
                label: entry
                    .title
                    .clone()
                    .unwrap_or_else(|| {
                        format!("Meeting · {}", entry.meeting_id.chars().take(8).collect::<String>())
                    }),
                meeting_id: entry.meeting_id,
                deleted_at_epoch_seconds: entry.deleted_at_epoch_seconds,
                purge_after_epoch_seconds: entry.purge_after_epoch_seconds,
            })
            .collect(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreMeetingResponse {
    state: &'static str,
    message: String,
}

/// Restores one meeting from Trash back into the ordinary library. No review
/// token: restoring is additive, and the operator already committed to it by
/// clicking Restore in the Trash list this command re-reads authority from.
#[tauri::command(async)]
fn restore_meeting_from_trash_command(
    meeting_id: String,
    state: State<'_, ApplicationState>,
) -> RestoreMeetingResponse {
    restore_meeting_from_trash_for(meeting_id, &state)
}

fn restore_meeting_from_trash_for(
    meeting_id: String,
    state: &ApplicationState,
) -> RestoreMeetingResponse {
    let Ok(_command) = state.command_lock.lock() else {
        return RestoreMeetingResponse {
            state: "unavailable",
            message: "Restore is unavailable. Reopen Trash and try again.".into(),
        };
    };
    let access = state.with_preview_library(
        || library_reader::LibraryTrashRestoreAccess {
            state: "unavailable",
            message: "Restore is unavailable. Reopen Trash and try again.".into(),
        },
        |reader, _active| reader.authorize_trash_restore(&meeting_id),
    );
    // Any attempted mutation boundary invalidates every retained reader
    // handle, exactly like every other mutating preview command.
    if with_preview_library_invalidated(state, || ()).is_err() {
        return RestoreMeetingResponse {
            state: "unavailable",
            message: "Restore is unavailable. Reopen Trash and try again.".into(),
        };
    }
    if access.state != "authorized" {
        return RestoreMeetingResponse {
            state: access.state,
            message: access.message,
        };
    }

    let result = state
        .meeting_restore_facade()
        .restore_meeting(
            manual_delete_facade::MeetingRestoreUiArgs {
                meeting_id: meeting_id.clone(),
            },
            now_epoch_seconds(),
        );
    let (response_state, message) = match result {
        Ok(manual_delete_facade::MeetingRestoreFacadeOutcome::Restored) => (
            "restored",
            "The meeting was restored from Trash. It is back in Meetings.",
        ),
        Ok(manual_delete_facade::MeetingRestoreFacadeOutcome::RecoveredRestore) => (
            "restored",
            "The interrupted restore was recovered and completed. The meeting is back in Meetings.",
        ),
        Err(
            manual_delete_facade::MeetingRestoreFacadeError::NoSuchTrashEntry
            | manual_delete_facade::MeetingRestoreFacadeError::AlreadyPurged,
        ) => (
            "not-found",
            "This meeting is no longer in Trash.",
        ),
        Err(manual_delete_facade::MeetingRestoreFacadeError::DestinationExists) => (
            "unavailable",
            "Yawn could not restore this meeting because a meeting already occupies its place. Reopen Trash and try again.",
        ),
        Err(manual_delete_facade::MeetingRestoreFacadeError::PurgeInProgress) => (
            "unavailable",
            "This meeting is being permanently removed and can no longer be restored.",
        ),
        Err(
            manual_delete_facade::MeetingRestoreFacadeError::WriterLockUnavailable
            | manual_delete_facade::MeetingRestoreFacadeError::StorageUnavailable,
        ) => (
            "unavailable",
            "Restore could not complete. Reopen Trash and try again.",
        ),
    };
    RestoreMeetingResponse {
        state: response_state,
        message: message.into(),
    }
}

#[tauri::command]
fn preview_library_open_evidence(
    handle: String,
    locator_ordinal: usize,
    state: State<'_, ApplicationState>,
) -> library_reader::LibraryEvidenceResponse {
    state.with_preview_library(
        library_reader::LibraryReader::unavailable_evidence,
        |reader, active| reader.open_evidence(&handle, locator_ordinal, active),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryOperatorNoteSaveResponse {
    operator_note: operator_note::OperatorNote,
    /// The next one-use edit capability. A saved note may be edited again, but
    /// each save requires a new bounded authority rather than keeping a path or
    /// general meeting-write capability in the webview.
    operator_note_handle: Option<String>,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryTranscriptFileOpenResponse {
    /// Reissued after opening so an operator can reopen the same selected file
    /// without falling back to a general filesystem chooser.
    transcript_file_handle: Option<String>,
}

/// Replaces the personal note for one meeting the operator already opened in
/// Library. The opaque handle is single-use and only resolves through the
/// current verified library projection.
#[tauri::command(async)]
fn library_save_operator_note(
    handle: String,
    text: String,
    state: State<'_, ApplicationState>,
) -> Result<LibraryOperatorNoteSaveResponse, CommandError> {
    state.with_preview_library(
        || {
            Err(CommandError::coded(
                error_codes::MEETING_LIBRARY_UNAVAILABLE,
                "The local meeting library is unavailable. Reopen the app and try again.",
            ))
        },
        |reader, active| {
            let saved = reader.open_operator_note_bound(&handle, active, |storage, meeting_id| {
                let directory = meeting_dir(storage, meeting_id).map_err(error_text)?;
                operator_note::write(&directory, &text)?;
                Ok::<_, CommandError>((meeting_id.to_owned(), operator_note::read(&directory)))
            });
            match saved {
                Ok(Ok((meeting_id, operator_note))) => Ok(LibraryOperatorNoteSaveResponse {
                    operator_note_handle: if operator_note.unreadable {
                        None
                    } else {
                        reader.retain_operator_note_handle(&meeting_id)
                    },
                    operator_note,
                    message: "Your notes were saved on this Mac.".into(),
                }),
                Ok(Err(error)) => Err(error),
                Err(access) => Err(CommandError {
                    code: access.code,
                    message: access.message,
                }),
            }
        },
    )
}

/// Roadmap intake I5. What one lock command did, and the meeting's lock now.
///
/// It carries no meeting content and no capability. The shell reopens the
/// meeting after a change rather than being handed a fresh set of handles
/// here, so a lock action can never be the thing that grants access.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingLockResponse {
    /// `locked`, `unlocked`, `declined`, `unavailable`, or `stale`.
    state: &'static str,
    lock: meeting_lock::MeetingLock,
    message: String,
}

/// Roadmap intake I5. A fresh single-use confirmation for one action on one
/// locked meeting, or the honest reason there is none.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LockedActionAuthorizationResponse {
    /// `authorized`, `declined`, `unavailable`, `not-locked`, or `stale`.
    state: &'static str,
    /// Present only on `authorized`. Opaque, single-use, and scoped to one
    /// meeting and one action; the native side holds its meaning.
    token: Option<String>,
    message: String,
}

/// Said whenever this Mac cannot run the device-owner check at all.
///
/// The second sentence is the packet's whole honesty burden in one line: an
/// unavailable check does not open the meeting. There is no bypass here, and
/// building one would make the lock decorative. What remains true — and is
/// stated in the module docs rather than shouted at the operator — is that the
/// lock is a plain file, so a person with this Mac's filesystem can still get
/// at the meeting. That is the deterrent's honest ceiling, not a loophole this
/// command should offer.
const CONFIRMATION_UNAVAILABLE_MESSAGE: &str =
    "This Mac cannot confirm it's you (no Touch ID or password available). The lock stays on.";

/// Said when the person cancelled or the check did not pass. Deliberately not
/// the sentence above: "not this time" and "not on this Mac" are different
/// facts, and collapsing them would tell someone to go buy hardware they have.
const CONFIRMATION_DECLINED_MESSAGE: &str =
    "Yawn could not confirm it's you. This meeting stays locked.";

/// What macOS renders under the prompt when the operator asks to take the lock
/// off. Removing a lock is not one of the three gated actions -- it changes the
/// meeting rather than reaching into it -- so it carries its own sentence
/// instead of borrowing `LockedAction::Open`'s, which would say "open a locked
/// meeting" to someone who clicked Remove lock.
const UNLOCK_CONFIRMATION_REASON: &str = "remove the lock from a meeting on this Mac";

/// Runs the device-owner check off every lock this app holds.
///
/// The prompt blocks on a person for as long as they take. `with_preview_library`
/// holds the meeting-storage sequence and the library mutex for its whole
/// closure, and `command_lock` serializes commands, so confirming inside any of
/// them would stall the app behind a panel the operator may never answer. Every
/// caller here therefore follows the same three steps: resolve the meeting
/// (holding locks), release, then confirm.
///
/// `reason` is product copy: macOS shows it inside its own panel, so it names
/// the act the operator just asked for and never a meeting's title.
fn confirm_operator(
    state: &State<'_, ApplicationState>,
    reason: &str,
) -> operator_confirmation::Confirmation {
    state.confirmation.confirm(reason)
}

/// Locks one meeting the operator already opened in Library.
///
/// Locking needs no confirmation — it only ever removes this app's own access,
/// and asking to prove identity before giving something up is friction with no
/// safety in it. Bear's per-note lock behaves the same way.
///
/// Two things happen besides the write, and both are the point rather than
/// housekeeping. Every outstanding confirmation is revoked, and the library
/// reader is dropped so its handle maps go with it. Without them a meeting
/// opened a moment ago would leave live transcript, export, playback, and
/// personal-note capabilities in the webview that outlived the lock — writing
/// a sidecar does not reach into a handle map.
#[tauri::command(async)]
fn lock_meeting(
    handle: String,
    state: State<'_, ApplicationState>,
) -> Result<MeetingLockResponse, CommandError> {
    let Ok(_command) = state.command_lock.lock() else {
        return Err(CommandError::coded(
            error_codes::MEETING_LIBRARY_UNAVAILABLE,
            "The local meeting library is unavailable. Reopen the app and try again.",
        ));
    };
    let resolved = state
        .with_preview_library(
            || None,
            |reader, _active| {
                reader.with_locked_meeting(&handle, |storage, meeting_id, _lock| {
                    meeting_dir(storage, meeting_id).ok()
                })
            },
        )
        .flatten();
    let Some(directory) = resolved else {
        return Err(CommandError::coded(
            error_codes::VIEW_STALE,
            "That view is no longer current. Reopen it and try again.",
        ));
    };
    meeting_lock::write(&directory, true)?;
    if let Ok(mut authority) = state.locked_actions.lock() {
        authority.revoke_all();
    }
    // Drop the reader with every capability it minted for this meeting while
    // it was open.
    let _ = with_preview_library_invalidated(&state, || ());
    Ok(MeetingLockResponse {
        state: "locked",
        lock: meeting_lock::read(&directory),
        message: "This meeting is locked on this Mac.".into(),
    })
}

/// Removes the lock from one meeting, after the device-owner check passes.
///
/// This is the only command that clears the flag, and it is the reason the
/// check exists: without it, anything that could set `locked: false` would be
/// the bypass. An unreadable lock file is unlocked the same way — the check
/// still runs first, so "unreadable" never becomes the path that skips it.
#[tauri::command(async)]
fn unlock_meeting(
    handle: String,
    state: State<'_, ApplicationState>,
) -> Result<MeetingLockResponse, CommandError> {
    // Resolve while holding the locks, then release them before the prompt.
    let resolved = {
        let Ok(_command) = state.command_lock.lock() else {
            return Err(CommandError::coded(
                error_codes::MEETING_LIBRARY_UNAVAILABLE,
                "The local meeting library is unavailable. Reopen the app and try again.",
            ));
        };
        state
            .with_preview_library(
                || None,
                |reader, _active| {
                    reader.with_locked_meeting(&handle, |storage, meeting_id, lock| {
                        meeting_dir(storage, meeting_id)
                            .ok()
                            .map(|directory| (directory, lock))
                    })
                },
            )
            .flatten()
    };
    let Some((directory, lock)) = resolved else {
        return Err(CommandError::coded(
            error_codes::VIEW_STALE,
            "That view is no longer current. Reopen it and try again.",
        ));
    };
    if !lock.locked {
        return Ok(MeetingLockResponse {
            state: "unlocked",
            lock,
            message: "This meeting is not locked.".into(),
        });
    }
    // No lock held from here to the answer.
    match confirm_operator(&state, UNLOCK_CONFIRMATION_REASON) {
        operator_confirmation::Confirmation::Confirmed => {}
        operator_confirmation::Confirmation::Declined => {
            return Ok(MeetingLockResponse {
                state: "declined",
                lock,
                message: CONFIRMATION_DECLINED_MESSAGE.into(),
            });
        }
        operator_confirmation::Confirmation::Unavailable => {
            return Ok(MeetingLockResponse {
                state: "unavailable",
                lock,
                message: CONFIRMATION_UNAVAILABLE_MESSAGE.into(),
            });
        }
    }
    let Ok(_command) = state.command_lock.lock() else {
        return Err(CommandError::coded(
            error_codes::MEETING_LIBRARY_UNAVAILABLE,
            "The local meeting library is unavailable. Reopen the app and try again.",
        ));
    };
    meeting_lock::write(&directory, false)?;
    Ok(MeetingLockResponse {
        state: "unlocked",
        lock: meeting_lock::read(&directory),
        message: "This meeting is no longer locked.".into(),
    })
}

/// Runs the device-owner check for one action on one locked meeting and, on
/// success, mints the single-use token that action's command requires.
///
/// One confirmation authorizes exactly one act. There is no session grant and
/// no window to expire, which is why nothing here reads a clock: the token dies
/// when it is spent, when another is minted, or when the meeting is locked.
/// Reading a locked meeting is itself one of the three acts, so reopening it
/// asks again rather than trading on the last answer.
#[tauri::command(async)]
fn authorize_locked_action(
    handle: String,
    action: String,
    state: State<'_, ApplicationState>,
) -> Result<LockedActionAuthorizationResponse, CommandError> {
    let Some(action) = meeting_lock::LockedAction::parse(&action) else {
        return Err("That action cannot be confirmed.".into());
    };
    let resolved = {
        let Ok(_command) = state.command_lock.lock() else {
            return Err(CommandError::coded(
                error_codes::MEETING_LIBRARY_UNAVAILABLE,
                "The local meeting library is unavailable. Reopen the app and try again.",
            ));
        };
        state.with_preview_library(
            || None,
            |reader, _active| {
                reader.with_locked_meeting(&handle, |_storage, meeting_id, lock| {
                    (meeting_id.to_owned(), lock)
                })
            },
        )
    };
    let Some((meeting_id, lock)) = resolved else {
        return Ok(LockedActionAuthorizationResponse {
            state: "stale",
            token: None,
            message: "That view is no longer current. Reopen it and try again.".into(),
        });
    };
    if !lock.locked {
        // Decision 3: no friction on the default path. An unlocked meeting is
        // told plainly that it needs nothing, rather than being handed a token
        // that would only be ignored.
        return Ok(LockedActionAuthorizationResponse {
            state: "not-locked",
            token: None,
            message: "This meeting is not locked.".into(),
        });
    }
    match confirm_operator(&state, action.reason()) {
        operator_confirmation::Confirmation::Confirmed => {}
        operator_confirmation::Confirmation::Declined => {
            return Ok(LockedActionAuthorizationResponse {
                state: "declined",
                token: None,
                message: CONFIRMATION_DECLINED_MESSAGE.into(),
            });
        }
        operator_confirmation::Confirmation::Unavailable => {
            return Ok(LockedActionAuthorizationResponse {
                state: "unavailable",
                token: None,
                message: CONFIRMATION_UNAVAILABLE_MESSAGE.into(),
            });
        }
    }
    let Ok(mut authority) = state.locked_actions.lock() else {
        return Err(CommandError::coded(
            error_codes::MEETING_LIBRARY_UNAVAILABLE,
            "The local meeting library is unavailable. Reopen the app and try again.",
        ));
    };
    Ok(LockedActionAuthorizationResponse {
        state: "authorized",
        token: Some(authority.mint(&meeting_id, action)),
        message: "Confirmed on this Mac.".into(),
    })
}

/// Spends a presented confirmation for `action`, returning the meeting it
/// authorizes. `None` covers every failure the gate treats identically: no
/// token, an unknown one, one already spent, one minted for another action.
fn consume_locked_action(
    state: &State<'_, ApplicationState>,
    token: Option<&str>,
    action: meeting_lock::LockedAction,
) -> Option<String> {
    state
        .locked_actions
        .lock()
        .ok()
        .and_then(|mut authority| authority.consume(token, action))
}

/// Roadmap intake I5's refusal for the two commands that reach a meeting
/// through a bounded callback rather than through the reader's own access
/// types. Both receive the storage root and an already-verified meeting id, so
/// the gate runs on exactly the meeting about to be acted on.
///
/// A meeting whose directory cannot be resolved is *not* treated as locked:
/// the surrounding command's own failure is the honest answer, and refusing
/// with a lock message would name a barrier that is not what stopped it.
fn refuse_locked_meeting(
    storage: &StorageRoot,
    meeting_id: &str,
    unlocked_meeting_id: Option<&str>,
) -> Result<(), String> {
    let Ok(directory) = meeting_dir(storage, meeting_id) else {
        return Ok(());
    };
    if meeting_lock::permits(
        meeting_lock::read(&directory),
        unlocked_meeting_id,
        meeting_id,
    ) {
        return Ok(());
    }
    Err(library_reader::LOCKED_MESSAGE.into())
}

/// Opens the exact transcript artifact selected by a fresh Library handle. The
/// handle is spent before the file is checked and launched, so this does not
/// turn Library into generic filesystem access.
///
/// `lockToken` is roadmap intake I5's gate, and this command takes the same
/// `export` scope as `library_export_meeting` rather than a scope of its own.
/// Both do the same escalating thing: they put a locked meeting's transcript
/// somewhere outside Yawn, where nothing about the lock reaches it. The
/// in-app transcript read (`library_open_transcript`) is not gated here,
/// because its handle is minted only by a `library_open_note` that already
/// passed the gate, and locking a meeting invalidates the reader that holds
/// every such handle.
#[tauri::command]
fn library_open_transcript_file(
    handle: String,
    lock_token: Option<String>,
    state: State<'_, ApplicationState>,
) -> Result<LibraryTranscriptFileOpenResponse, CommandError> {
    let unlocked = consume_locked_action(
        &state,
        lock_token.as_deref(),
        meeting_lock::LockedAction::Export,
    );
    state.with_preview_library(
        || {
            Err(CommandError::coded(
                error_codes::MEETING_LIBRARY_UNAVAILABLE,
                "The local meeting library is unavailable. Reopen the app and try again.",
            ))
        },
        |reader, active| match reader.open_transcript_bound(
            &handle,
            active,
            |storage, meeting_id, artifact| {
                refuse_locked_meeting(storage, meeting_id, unlocked.as_deref())?;
                open_verified_transcript_file(storage, meeting_id, artifact)?;
                Ok::<_, CommandError>(meeting_id.to_owned())
            },
        ) {
            Ok(Ok(meeting_id)) => Ok(LibraryTranscriptFileOpenResponse {
                transcript_file_handle: reader.retain_transcript_handle(&meeting_id),
            }),
            Ok(Err(error)) => Err(error),
            Err(access) => Err(CommandError {
                code: access.code,
                message: access.message,
            }),
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryExportMeetingResponse {
    state: &'static str,
    /// Every artifact this export could not include, each named with why —
    /// never a silent gap. Empty when every eligible artifact exported.
    withheld: Vec<String>,
    /// A fresh single-use authority so the operator can open the transcript
    /// file or export again without reopening the meeting. Same authority
    /// class `library_open_transcript_file` reissues under this same field
    /// name — export spends and reissues the identical handle.
    transcript_file_handle: Option<String>,
    message: String,
}

/// Roadmap intake I7+I8: exports a reviewed meeting as plain per-item files
/// plus one compact archive of the same content, written only inside that
/// meeting's own directory — no save dialog, no path picker, no new
/// filesystem scope. The handle spent here is the meeting's existing
/// transcript-file authority (`transcriptFileHandle`), not a new capability —
/// `meeting_export` does the actual verification, assembly, and writing over
/// the values `open_export_bound` hands it.
#[tauri::command(async)]
fn library_export_meeting(
    handle: String,
    lock_token: Option<String>,
    state: State<'_, ApplicationState>,
) -> Result<LibraryExportMeetingResponse, CommandError> {
    let unlocked = consume_locked_action(
        &state,
        lock_token.as_deref(),
        meeting_lock::LockedAction::Export,
    );
    state.with_preview_library(
        || {
            Err(CommandError::coded(
                error_codes::MEETING_LIBRARY_UNAVAILABLE,
                "The local meeting library is unavailable. Reopen the app and try again.",
            ))
        },
        |reader, active| match reader.open_export_bound(
            &handle,
            active,
            |storage, meeting_id, label, created_at_epoch_seconds, claims| {
                // Roadmap intake I5, checked before a single file is written.
                let outcome = refuse_locked_meeting(storage, meeting_id, unlocked.as_deref())
                    .and_then(|()| {
                        meeting_export::export_meeting(
                            storage,
                            meeting_id,
                            label,
                            created_at_epoch_seconds,
                            claims,
                        )
                    });
                (meeting_id.to_owned(), outcome)
            },
        ) {
            Ok((meeting_id, Ok(outcome))) => Ok(LibraryExportMeetingResponse {
                state: "exported",
                withheld: outcome.withheld,
                transcript_file_handle: reader.retain_transcript_handle(&meeting_id),
                message: "Exported to a folder next to this meeting on this Mac.".into(),
            }),
            Ok((_, Err(error))) => Err(error.into()),
            Err(access) => Err(CommandError {
                code: access.code,
                message: access.message,
            }),
        },
    )
}

#[tauri::command]
fn library_open_transcript(
    handle: String,
    state: State<'_, ApplicationState>,
) -> PreviewLibraryTranscript {
    state.with_preview_library(
        || PreviewLibraryTranscript {
            state: "unavailable",
            meeting_id: None,
            transcript_file_handle: None,
            current_transcript_sha256: None,
            turns: Vec::new(),
            warnings: Vec::new(),
            message: "The local Preview library is unavailable. Reopen the app and try again."
                .into(),
        },
        |reader, active| match reader.open_transcript_bound(
            &handle,
            active,
            |storage, meeting_id, artifact| {
                (
                    meeting_id.to_owned(),
                    artifact.sha256.clone(),
                    load_bound_preview_transcript_projection(storage, meeting_id, artifact),
                )
            },
        ) {
            Ok((meeting_id, transcript_sha256, Ok((turns, warnings)))) => {
                let transcript_file_handle = reader.retain_transcript_handle(&meeting_id);
                PreviewLibraryTranscript {
                    state: "transcript",
                    meeting_id: Some(meeting_id),
                    transcript_file_handle,
                    current_transcript_sha256: Some(transcript_sha256),
                    turns,
                    warnings,
                    message: "Retained transcript from this Preview meeting.".into(),
                }
            }
            Ok((_, _, Err(_))) => PreviewLibraryTranscript {
                state: "stale",
                meeting_id: None,
                transcript_file_handle: None,
                current_transcript_sha256: None,
                turns: Vec::new(),
                warnings: Vec::new(),
                message: "That transcript is no longer available. Reopen Library and try again."
                    .into(),
            },
            Err(opened) => PreviewLibraryTranscript {
                state: opened.state,
                meeting_id: None,
                transcript_file_handle: None,
                current_transcript_sha256: None,
                turns: Vec::new(),
                warnings: Vec::new(),
                message: opened.message,
            },
        },
    )
}

fn load_bound_preview_transcript_projection(
    storage: &StorageRoot,
    meeting_id: &str,
    expected: &ArtifactRef,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    let directory = meeting_dir(storage, meeting_id).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    if meeting.artifacts.current_transcript.as_ref() != Some(expected) {
        return Err("the selected transcript pointer changed".into());
    }
    let path = resolve_artifact(&directory, &expected.relative_path).map_err(error_text)?;
    let bytes = read_private_bytes(&path, TRANSCRIPT_MAX_BYTES).map_err(error_text)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected.sha256 {
        return Err("the selected transcript bytes changed".into());
    }
    let (turns, mut warnings) =
        project_current_transcript(&directory, meeting_id, expected, &bytes, true)?;
    let current = load_meeting(&directory).map_err(error_text)?;
    if current.artifacts.current_transcript.as_ref() != Some(expected)
        || artifact_ref(&directory, &expected.relative_path).map_err(error_text)? != *expected
    {
        return Err("the selected transcript changed while opening".into());
    }
    if current.retention.state == AudioState::Released {
        warnings.push(
            "Meeting audio was deleted under the selected retention period. The transcript remains."
                .into(),
        );
    }
    Ok((turns, warnings))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpeakerCorrectionResponse {
    operation_id: Uuid,
    applied_turn_count: usize,
    message: String,
}

/// A private, exact vocabulary row together with its use in the meeting the
/// operator is currently reviewing. The count is derived from the verified
/// projection below; it never records a new transcript derivative.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalVocabularyEntryResponse {
    id: Uuid,
    source_phrase: String,
    preferred_replacement: String,
    enabled: bool,
    applied_turn_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalVocabularySheetResponse {
    entries: Vec<LocalVocabularyEntryResponse>,
}

fn local_vocabulary_error(
    error: local_meeting_notes_session_core::local_vocabulary::LocalVocabularyError,
) -> CommandError {
    use local_meeting_notes_session_core::local_vocabulary::LocalVocabularyError;

    match error {
        LocalVocabularyError::DuplicateSourcePhrase => {
            "That Before phrase already has a replacement. Edit the existing row instead.".into()
        }
        LocalVocabularyError::NotFound => {
            "That replacement is no longer available. Reopen Vocabulary and try again.".into()
        }
        LocalVocabularyError::TooManyEntries => {
            "You can save up to 256 local replacements.".into()
        }
        LocalVocabularyError::InvalidEntry(_) => {
            "Use two different single-line phrases, up to 256 characters each.".into()
        }
        LocalVocabularyError::TextTooLarge | LocalVocabularyError::TooManyRangeReplacements => {
            CommandError::coded(
                error_codes::VOCABULARY_CHECK_UNSAFE,
                "Yawn could not safely check this transcript against local vocabulary. Nothing changed. Reopen the meeting and try again.",
            )
        }
        LocalVocabularyError::DocumentTooLarge => {
            "Your saved vocabulary is over its 256 KB limit. Nothing changed.".into()
        }
        LocalVocabularyError::InvalidPrivateStorage
        | LocalVocabularyError::Malformed(_)
        | LocalVocabularyError::Io(_)
        | LocalVocabularyError::Json(_) => {
            CommandError::coded(
                error_codes::VOCABULARY_READ_FAILED,
                "Your saved vocabulary could not be read. Nothing changed. Reopen the meeting and try again.",
            )
        }
    }
}

fn with_current_local_vocabulary<T>(
    meeting_id: Uuid,
    source_transcript_sha256: &str,
    state: &ApplicationState,
    operation: impl FnOnce(
        &local_meeting_notes_session_core::local_vocabulary::LocalVocabularyStore,
        &[TranscriptTurn],
    ) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let _command = state.command_lock.lock().map_err(|_| {
        CommandError::coded(
            error_codes::VOCABULARY_UNAVAILABLE,
            "Vocabulary is unavailable. Reopen the meeting and try again.",
        )
    })?;
    if sitting_task_active(state) {
        return Err("Finish the setup recording before changing local vocabulary.".into());
    }
    {
        let model = state.model.lock().map_err(|_| {
            CommandError::coded(
                error_codes::VOCABULARY_UNAVAILABLE,
                "Vocabulary is unavailable. Reopen the meeting and try again.",
            )
        })?;
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("Finish the current recording before changing local vocabulary.".into());
        }
    }
    let storage = preview_storage_clone(state)
        .map_err(|_| "Local meeting storage is unavailable. Reopen the app and try again.")?;
    let coordination = state.meeting_storage_coordination()?;
    let _lease = coordination.acquire(&meeting_id.to_string()).map_err(|_| {
        CommandError::coded(
            error_codes::MEETING_ACTION_IN_USE,
            "Another action is using this meeting. Reopen it and try again.",
        )
    })?;
    let directory = meeting_dir(&storage, &meeting_id.to_string()).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    let current = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .ok_or_else(|| "This meeting no longer has a retained transcript.".to_string())?;
    if meeting.meeting_id != meeting_id.to_string() || current.sha256 != source_transcript_sha256 {
        return Err(CommandError::coded(
            error_codes::TRANSCRIPT_CHANGED_RETRY,
            "The transcript changed. Reopen the meeting and try again.",
        ));
    }
    let (turns, _) = load_transcript_projection(&directory, &meeting_id.to_string(), current)?;
    let vocabulary =
        local_meeting_notes_session_core::local_vocabulary::LocalVocabularyStore::open(&storage)
            .map_err(local_vocabulary_error)?;
    operation(&vocabulary, &turns)
}

fn local_vocabulary_sheet_response(
    vocabulary: &local_meeting_notes_session_core::local_vocabulary::LocalVocabularyStore,
    turns: &[TranscriptTurn],
) -> Result<LocalVocabularySheetResponse, CommandError> {
    let entries = vocabulary.list().map_err(local_vocabulary_error)?;
    let mut applied_by_entry = HashMap::<Uuid, usize>::new();
    // This is a review-only count, not a note-generation frame. Project each
    // retained turn so the count preserves its entry id without imposing the
    // worker transport's 64-range ceiling. Withheld rows are never supplied to
    // the store, even if a later parser accidentally carries text in memory.
    for turn in turns.iter().filter(|turn| !turn.withheld) {
        for application in vocabulary
            .project(&turn.text)
            .map_err(local_vocabulary_error)?
            .applied
        {
            *applied_by_entry.entry(application.entry_id).or_default() += 1;
        }
    }
    Ok(LocalVocabularySheetResponse {
        entries: entries
            .into_iter()
            .map(|entry| LocalVocabularyEntryResponse {
                id: entry.id,
                source_phrase: entry.source_phrase,
                preferred_replacement: entry.preferred_replacement,
                enabled: entry.enabled,
                applied_turn_count: applied_by_entry.remove(&entry.id).unwrap_or(0),
            })
            .collect(),
    })
}

#[tauri::command(rename_all = "camelCase")]
fn local_vocabulary_list(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    state: State<'_, ApplicationState>,
) -> Result<LocalVocabularySheetResponse, CommandError> {
    with_current_local_vocabulary(
        meeting_id,
        &source_transcript_sha256,
        &state,
        |vocabulary, turns| local_vocabulary_sheet_response(vocabulary, turns),
    )
}

#[tauri::command(rename_all = "camelCase")]
fn local_vocabulary_add(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    source_phrase: String,
    preferred_replacement: String,
    state: State<'_, ApplicationState>,
) -> Result<LocalVocabularySheetResponse, CommandError> {
    with_current_local_vocabulary(
        meeting_id,
        &source_transcript_sha256,
        &state,
        |vocabulary, turns| {
            vocabulary
                .add(&source_phrase, &preferred_replacement)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        },
    )
}

#[tauri::command(rename_all = "camelCase")]
fn local_vocabulary_edit(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    id: Uuid,
    source_phrase: String,
    preferred_replacement: String,
    state: State<'_, ApplicationState>,
) -> Result<LocalVocabularySheetResponse, CommandError> {
    with_current_local_vocabulary(
        meeting_id,
        &source_transcript_sha256,
        &state,
        |vocabulary, turns| {
            vocabulary
                .edit(id, &source_phrase, &preferred_replacement)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        },
    )
}

#[tauri::command(rename_all = "camelCase")]
fn local_vocabulary_set_enabled(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    id: Uuid,
    enabled: bool,
    state: State<'_, ApplicationState>,
) -> Result<LocalVocabularySheetResponse, CommandError> {
    with_current_local_vocabulary(
        meeting_id,
        &source_transcript_sha256,
        &state,
        |vocabulary, turns| {
            vocabulary
                .set_enabled(id, enabled)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        },
    )
}

#[tauri::command(rename_all = "camelCase")]
fn local_vocabulary_delete(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    id: Uuid,
    state: State<'_, ApplicationState>,
) -> Result<LocalVocabularySheetResponse, CommandError> {
    with_current_local_vocabulary(
        meeting_id,
        &source_transcript_sha256,
        &state,
        |vocabulary, turns| {
            vocabulary.delete(id).map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        },
    )
}

/// Saves one meeting-local speaker-name correction without rewriting the
/// retained transcript. The source digest and original source label are both
/// rechecked under the meeting lease, so a stale screen cannot rename a newer
/// projection or broaden a correction to a group it did not display.
#[tauri::command(rename_all = "camelCase")]
fn correct_speaker_name(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    source_speaker: Option<String>,
    replacement: String,
    state: State<'_, ApplicationState>,
) -> Result<SpeakerCorrectionResponse, CommandError> {
    correct_speaker_name_for(
        meeting_id,
        source_transcript_sha256,
        source_speaker,
        replacement,
        &state,
    )
}

fn correct_speaker_name_for(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    source_speaker: Option<String>,
    replacement: String,
    state: &ApplicationState,
) -> Result<SpeakerCorrectionResponse, CommandError> {
    let _command = state.command_lock.lock().map_err(|_| {
        CommandError::coded(
            error_codes::SPEAKER_CORRECTION_UNAVAILABLE,
            "Speaker correction is unavailable. Reopen the meeting and try again.",
        )
    })?;
    if sitting_task_active(&state) {
        return Err("Finish the setup recording before correcting a speaker name.".into());
    }
    {
        let model = state.model.lock().map_err(|_| {
            CommandError::coded(
                error_codes::SPEAKER_CORRECTION_UNAVAILABLE,
                "Speaker correction is unavailable. Reopen the meeting and try again.",
            )
        })?;
        if model.reducer.startup() != StartupState::Ready
            || model.reducer.capture() != CaptureState::Idle
        {
            return Err("Finish the current recording before correcting a speaker name.".into());
        }
    }
    let storage = preview_storage_clone(&state)
        .map_err(|_| "Local meeting storage is unavailable. Reopen the app and try again.")?;
    let coordination = state.meeting_storage_coordination()?;
    let _lease = coordination.acquire(&meeting_id.to_string()).map_err(|_| {
        CommandError::coded(
            error_codes::MEETING_ACTION_IN_USE,
            "Another action is using this meeting. Reopen it and try again.",
        )
    })?;
    let directory = meeting_dir(&storage, &meeting_id.to_string()).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    let current = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .ok_or_else(|| "This meeting no longer has a retained transcript.".to_string())?;
    if current.sha256 != source_transcript_sha256 {
        return Err(CommandError::coded(
            error_codes::TRANSCRIPT_CHANGED_RETRY,
            "The transcript changed. Reopen the meeting and try again.",
        ));
    }
    let (turns, _) = load_transcript_projection(&directory, &meeting_id.to_string(), current)?;
    let applied_turn_count = turns
        .iter()
        .filter(|turn| !turn.withheld && turn.source_speaker == source_speaker)
        .count();
    if applied_turn_count == 0 {
        return Err(CommandError::coded(
            error_codes::SPEAKER_GROUP_UNAVAILABLE,
            "That speaker group is no longer available. Reopen the meeting and try again.",
        ));
    }
    let replacement = speaker_correction::normalize_replacement(&replacement)?;
    let operation_id = Uuid::new_v4();
    speaker_correction::append(
        &directory,
        meeting_id,
        speaker_correction::SpeakerCorrectionOperation {
            operation_id,
            source_transcript_sha256: source_transcript_sha256.clone(),
            source_speaker: source_speaker.clone(),
            replacement: replacement.clone(),
            applied_at_epoch_seconds: now_epoch_seconds(),
        },
    )?;
    let stored =
        speaker_correction::current_labels(&directory, meeting_id, &source_transcript_sha256)
            .map_err(|_| "The saved speaker correction could not be verified.".to_string())?;
    let restored_source_label = replacement == source_speaker.as_deref().unwrap_or("Unattributed");
    if (restored_source_label && stored.contains_key(&source_speaker))
        || (!restored_source_label && stored.get(&source_speaker) != Some(&replacement))
    {
        return Err("The saved speaker correction could not be verified.".into());
    }
    with_preview_library_invalidated(&state, || ())
        .map_err(|_| "The meeting was corrected, but the library could not refresh.".to_string())?;
    Ok(SpeakerCorrectionResponse {
        operation_id,
        applied_turn_count,
        message: format!(
            "Speaker name updated for {applied_turn_count} transcript {}. The retained transcript was not changed.",
            if applied_turn_count == 1 {
                "turn"
            } else {
                "turns"
            }
        ),
    })
}

/// Derive the note-generation context overlay from the same sidecar the
/// pre-capture and capture surfaces edit. Unlike the two overlays below, this
/// one is not bound to a transcript digest -- context is written before a
/// transcript exists at all -- so there is no staleness check here beyond the
/// unreadable-file protection `meeting_context::read` already carries. The
/// coordinator re-reads and re-attests this same value under the meeting lease
/// (`product_coordinator.rs::accept_regeneration`) before it reaches a prompt.
pub(crate) fn pre_meeting_context_for(
    meeting_id: Uuid,
    state: &ApplicationState,
) -> Result<Option<String>, String> {
    let storage = preview_storage_clone(state).map_err(|_| {
        "Local meeting storage is unavailable. Reopen the app and try again.".to_string()
    })?;
    let directory = meeting_dir(&storage, &meeting_id.to_string()).map_err(error_text)?;
    let context = meeting_context::read(&directory);
    if context.unreadable {
        return Err(
            "Saved meeting context could not be read, so the note was not replaced.".into(),
        );
    }
    Ok((!context.text.is_empty()).then_some(context.text))
}

/// Derive the note-generation overlay from the same digest-bound correction
/// sidecar that supplies the transcript screen. A malformed or stale sidecar
/// has no fallback: the existing note remains current until a later request
/// can pass this check.
pub(crate) fn speaker_label_overrides_for(
    meeting_id: Uuid,
    source_transcript_sha256: &str,
    state: &ApplicationState,
) -> Result<Vec<local_meeting_notes_session_core::operations::SpeakerLabelOverride>, String> {
    let storage = preview_storage_clone(state).map_err(|_| {
        "Local meeting storage is unavailable. Reopen the app and try again.".to_string()
    })?;
    let directory = meeting_dir(&storage, &meeting_id.to_string()).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    let current = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .ok_or_else(|| "This meeting no longer has a retained transcript.".to_string())?;
    if current.sha256 != source_transcript_sha256 {
        return Err("The transcript changed. Refresh the meeting and try again.".into());
    }
    speaker_correction::current_label_overrides(&directory, meeting_id, source_transcript_sha256)
        .map_err(|_| {
            "Saved speaker corrections could not be read, so the note was not replaced.".into()
        })
}

/// Derive the prompt-only vocabulary overlay from the same verified meeting
/// projection the review screen uses. Withheld rows carry empty text, so their
/// source indices stay stable without exposing or replacing withheld words.
pub(crate) fn vocabulary_replacements_for(
    meeting_id: Uuid,
    source_transcript_sha256: &str,
    state: &ApplicationState,
) -> Result<
    Vec<local_meeting_notes_session_core::local_vocabulary::VocabularyRangeReplacement>,
    String,
> {
    let storage = preview_storage_clone(state).map_err(|_| {
        "Local meeting storage is unavailable. Reopen the app and try again.".to_string()
    })?;
    let directory = meeting_dir(&storage, &meeting_id.to_string()).map_err(error_text)?;
    current_vocabulary_replacements(&storage, &directory, meeting_id, source_transcript_sha256)
}

pub(crate) fn current_vocabulary_replacements(
    storage: &StorageRoot,
    meeting_directory: &Path,
    meeting_id: Uuid,
    source_transcript_sha256: &str,
) -> Result<
    Vec<local_meeting_notes_session_core::local_vocabulary::VocabularyRangeReplacement>,
    String,
> {
    let meeting = load_meeting(meeting_directory).map_err(error_text)?;
    let current = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .ok_or_else(|| "This meeting no longer has a retained transcript.".to_string())?;
    if current.sha256 != source_transcript_sha256 || meeting.meeting_id != meeting_id.to_string() {
        return Err("The transcript changed. Refresh the meeting and try again.".into());
    }
    let (turns, _) =
        load_transcript_projection(meeting_directory, &meeting_id.to_string(), current)?;
    let source_turns = turns
        .iter()
        .map(|turn| turn.text.as_str())
        .collect::<Vec<_>>();
    local_meeting_notes_session_core::local_vocabulary::LocalVocabularyStore::open(storage)
        .and_then(|vocabulary| vocabulary.project_turns(&source_turns))
        .map_err(|_| "Local vocabulary could not be verified, so the note was not replaced.".into())
}

fn retry_comparison_response(
    state: &ApplicationState,
    operation: &product_facade::TranscriptRetryOperation,
) -> Result<RetryComparisonResponse, CommandError> {
    let storage = preview_storage_clone(state)
        .map_err(|_| "Local meeting storage is unavailable. Reopen the app and try again.")?;
    let coordination = state.meeting_storage_coordination()?;
    let _lease = coordination
        .acquire(&operation.meeting_id.to_string())
        .map_err(|_| {
            CommandError::coded(
                error_codes::MEETING_ACTION_IN_USE,
                "Another action is using this meeting. Reopen it and try again.",
            )
        })?;
    let directory = meeting_dir(&storage, &operation.meeting_id.to_string()).map_err(error_text)?;
    let meeting = load_meeting(&directory).map_err(error_text)?;
    verify_record_artifacts(&directory, &meeting).map_err(error_text)?;
    let current = meeting
        .artifacts
        .current_transcript
        .as_ref()
        .ok_or_else(|| "This meeting no longer has a retained transcript.".to_string())?;
    if current.sha256 != operation.source_transcript_sha256 {
        return Err("The transcript changed. Refresh the meeting and try again.".into());
    }
    let (current_turns, current_warnings) =
        load_transcript_projection(&directory, &operation.meeting_id.to_string(), current)?;
    let (candidate_turns, candidate_warnings) = load_transcript_projection_without_corrections(
        &directory,
        &operation.meeting_id.to_string(),
        &operation.candidate_transcript,
    )?;
    let quality = local_meeting_notes_session_core::capture_quality::project_capture_quality(
        &directory,
        &meeting,
    )
    .map_err(|_| {
        CommandError::coded(
            error_codes::RETRY_QUALITY_EVIDENCE_CHANGED,
            "Recording-quality evidence changed while opening this retry. Reopen the meeting and try again.",
        )
    })?;
    let recording_device =
        local_meeting_notes_session_core::capture_quality::project_recording_device(
            &directory,
            &meeting,
        )
        .map_err(|_| {
            CommandError::coded(
                error_codes::RETRY_DEVICE_EVIDENCE_CHANGED,
                "Recording-device evidence changed while opening this retry. Reopen the meeting and try again.",
            )
        })?;
    let pauses = local_meeting_notes_session_core::capture_quality::project_capture_pauses(
        &directory,
        &meeting,
    )
    .map_err(|_| {
        CommandError::coded(
            error_codes::RETRY_PAUSE_EVIDENCE_CHANGED,
            "Pause evidence changed while opening this retry. Reopen the meeting and try again.",
        )
    })?;
    let diff = retry_diff_projection(&current_turns, &candidate_turns);
    Ok(RetryComparisonResponse {
        meeting_id: operation.meeting_id,
        operation_id: operation.operation_id,
        source_transcript_sha256: operation.source_transcript_sha256.clone(),
        candidate_transcript_sha256: operation.candidate_transcript.sha256.clone(),
        current: RetryTranscriptProjection {
            turns: current_turns,
            warnings: current_warnings,
        },
        candidate: RetryTranscriptProjection {
            turns: candidate_turns,
            warnings: candidate_warnings,
        },
        quality,
        recording_device,
        pauses,
        diff,
    })
}

fn retry_command_is_available(state: &ApplicationState) -> Result<(), String> {
    if sitting_task_active(state) {
        return Err("Finish the setup recording first.".into());
    }
    let capture = state
        .model
        .lock()
        .map_err(|_| "Finish the current recording first.".to_string())?
        .reducer
        .capture();
    if capture != CaptureState::Idle {
        return Err("Finish the current recording first.".into());
    }
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn transcript_retry_start(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    facade: State<'_, product_facade::ProductOperationFacade>,
    state: State<'_, ApplicationState>,
) -> Result<RetryComparisonResponse, CommandError> {
    retry_command_is_available(&state)?;
    let operation = facade
        .start_transcript_retry(TranscriptRetryUiArgs {
            meeting_id,
            source_transcript_sha256,
        })
        .map_err(product_facade::ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    facade.finish(operation.operation_id);
    retry_comparison_response(&state, &operation)
}

#[tauri::command(rename_all = "camelCase")]
fn transcript_retry_pending(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    facade: State<'_, product_facade::ProductOperationFacade>,
    state: State<'_, ApplicationState>,
) -> Result<Option<RetryComparisonResponse>, CommandError> {
    retry_command_is_available(&state)?;
    let operation = facade
        .pending_transcript_retry(TranscriptRetryUiArgs {
            meeting_id,
            source_transcript_sha256,
        })
        .map_err(product_facade::ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    operation
        .as_ref()
        .map(|operation| retry_comparison_response(&state, operation))
        .transpose()
}

/// The exact operator copy for a settled retry decision.
///
/// Promotion always leaves the meeting without a generated note, but it cannot
/// report having *cleared* one. The meeting may never have had a note, and a
/// replayed promotion clears nothing because the pointer already moved. So the
/// success copy states what is now true and names the next step, instead of
/// claiming an event this command has no fact for.
fn retry_decision_response(
    outcome: TranscriptRetryOutcome,
) -> Result<RetryDecisionResponse, CommandError> {
    match outcome {
        TranscriptRetryOutcome::CurrentKept => Ok(RetryDecisionResponse {
            outcome: "current-kept",
            message: "The current transcript was kept.",
        }),
        TranscriptRetryOutcome::CandidatePromoted => Ok(RetryDecisionResponse {
            outcome: "candidate-promoted",
            message: "The retry transcript is now current. Generate a new note when you're ready.",
        }),
        TranscriptRetryOutcome::CandidateAvailableForComparison => {
            Err("The retry candidate is still awaiting a decision.".into())
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
fn transcript_retry_decide(
    meeting_id: Uuid,
    operation_id: Uuid,
    source_transcript_sha256: String,
    candidate_transcript_sha256: String,
    decision: RetryDecisionInput,
    facade: State<'_, product_facade::ProductOperationFacade>,
    state: State<'_, ApplicationState>,
) -> Result<RetryDecisionResponse, CommandError> {
    retry_command_is_available(&state)?;
    if !valid_sha256(&candidate_transcript_sha256) {
        return Err(CommandError::coded(
            error_codes::RETRY_CANDIDATE_CHANGED,
            "The retry candidate changed. Reopen the meeting and try again.",
        ));
    }
    let operation = product_facade::TranscriptRetryOperation {
        operation_id,
        meeting_id,
        source_transcript_sha256,
        candidate_transcript: ArtifactRef {
            relative_path: format!("transcript/{candidate_transcript_sha256}.json"),
            sha256: candidate_transcript_sha256,
        },
    };
    let outcome = facade
        .decide_transcript_retry(
            operation,
            match decision {
                RetryDecisionInput::KeepCurrent => {
                    product_facade::TranscriptRetryDecision::KeepCurrent
                }
                RetryDecisionInput::UseRetry => product_facade::TranscriptRetryDecision::UseRetry,
            },
        )
        .map_err(product_facade::ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    let response = retry_decision_response(outcome)?;
    if matches!(outcome, TranscriptRetryOutcome::CandidatePromoted) {
        with_preview_library_invalidated(&state, || ()).map_err(|_| {
            "The retry transcript was selected, but the library could not refresh.".to_string()
        })?;
    }
    Ok(response)
}

fn main() {
    let state = ApplicationState::default();
    // Managed now so registering the facade commands later is one move; the
    // commands themselves stay out of the handler until the operator's
    // admission decision.
    let product_operations = product_facade::ProductOperationFacade::new(Arc::new(
        product_coordinator::DesktopProductCoordinator::new(
            state.storage.clone(),
            state.app_data_writer_lock.clone(),
            Arc::new(product_coordinator::ProcessWorkerPort::new(
                state.worker.clone(),
            )),
        ),
    ));
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_and_focus_active_window(app);
        }))
        // § A: the menubar item is the primary UI and must survive the most
        // ordinary window action. Closing the window hides it instead of
        // destroying it — a destroyed last window exits the process and
        // takes the tray with it. Quit (⌘Q) still exits honestly.
        .on_window_event(|window, event| {
            if window.label() == ACTIVE_WINDOW_LABEL {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .manage(state)
        .manage(product_operations)
        .invoke_handler(tauri::generate_handler![
            app_snapshot,
            open_settings_window,
            start_meeting,
            pause_meeting,
            resume_meeting,
            stop_meeting,
            dismiss_meeting,
            retry_startup,
            install_transcript_model,
            transcript_model_settings,
            remove_transcript_model,
            note_model_settings,
            install_note_model,
            remove_note_model,
            first_run_permissions,
            first_run_request_microphone,
            first_run_request_system_audio,
            operator_note,
            save_operator_note,
            meeting_context,
            save_meeting_context,
            open_current_transcript_file,
            library_snapshot,
            // Roadmap packet W10: the once-only first-run sheet's dismissal
            // marker. `library_snapshot` above already tells the frontend
            // whether it has been seen; this is the only other half.
            dismiss_first_run_sheet,
            library_set_meeting_title,
            library_open_note,
            library_play_retained_audio,
            library_retained_audio_playback_status,
            library_stop_retained_audio,
            library_open_transcript,
            library_open_transcript_file,
            library_export_meeting,
            // Roadmap intake I5: the per-meeting local-access barrier and its
            // action-scoped confirmations.
            lock_meeting,
            unlock_meeting,
            authorize_locked_action,
            correct_speaker_name,
            local_vocabulary_list,
            local_vocabulary_add,
            local_vocabulary_edit,
            local_vocabulary_set_enabled,
            local_vocabulary_delete,
            library_save_operator_note,
            preview_delete_meeting_audio,
            preview_delete_meeting_transcript,
            preview_delete_meeting,
            preview_list_trash,
            restore_meeting_from_trash_command,
            preview_library_open_evidence,
            product_facade::restore_withheld_turn,
            product_facade::regenerate_note,
            transcript_retry_start,
            transcript_retry_pending,
            transcript_retry_decide,
            // Roadmap intake W8-B: the one-week local usage probe. Both
            // commands refuse with a quiet, honest message whenever
            // `search-probe.flag` is absent from the storage root -- see
            // `search_probe.rs` -- so registering them here changes nothing
            // about today's behavior until the operator creates that file by
            // hand.
            preview_library_search,
            preview_library_open_search_result
        ])
        .setup(|app| {
            // Only teaches the plugin what to do if the note-capture
            // shortcut is ever registered; it is not armed at launch. See
            // capture_shortcut::activate, called from start_meeting.
            capture_shortcut::install(app.handle())?;
            let settings = tauri::menu::MenuItemBuilder::with_id("open-settings", "Settings…")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;
            let app_menu = tauri::menu::SubmenuBuilder::new(app, "Yawn")
                .about(None)
                .separator()
                .item(&settings)
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;
            // Concept-A window chrome (docs/design-direction-decision.md):
            // Record is a toolbar control with a File menu counterpart. New
            // Recording always dispatches to the same start flow the toolbar
            // Record control uses (the Start sheet's attestations still gate
            // an actual start; this item only opens that path). Stop Recording
            // starts disabled — `main`'s CaptureState is Idle at launch — and
            // is kept in sync with the tray's own "Stop recording" item via
            // `tray_shows_stop_recording`, the single source of truth for
            // "is a capture live" (see `spawn_tray_updater` below). Neither
            // item calls `stop_meeting` itself: both emit a `menu:*` event the
            // frontend handles the same way it already handles the in-window
            // Record/Stop controls, so there is exactly one place (the
            // frontend's existing `stopRecording`) that ever calls the
            // `stop_meeting` command for a window-driven stop. The tray's own
            // "Stop recording" item is unrelated — it calls `stop_meeting`
            // directly, unchanged.
            let new_recording = tauri::menu::MenuItemBuilder::with_id("new-recording", "New Recording")
                .accelerator("CmdOrCtrl+R")
                .build(app)?;
            let file_stop_recording =
                tauri::menu::MenuItemBuilder::with_id("stop-recording", "Stop Recording")
                    .accelerator("CmdOrCtrl+.")
                    .enabled(false)
                    .build(app)?;
            let file_menu = tauri::menu::SubmenuBuilder::new(app, "File")
                .item(&new_recording)
                .separator()
                .item(&file_stop_recording)
                .build()?;
            // D9 native-text audit: without an Edit menu, macOS has no
            // key-equivalent route for cmd-Z/X/C/V/A into the webview, so
            // undo is unreachable from the keyboard even though the editor
            // preserves its undo stack (confirmed live against the packaged
            // preview bundle, 2026-09-01). Predefined items only — they bind
            // the native selectors and need no menu-event handling.
            let edit_menu = tauri::menu::SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            // Concept-A: the sidebar toggle and the transcript inspector both
            // get a View menu entry alongside their existing shortcuts and
            // toolbar/inline controls. Rust owns no sidebar-visibility or
            // open-claim state, so these are plain event emits the frontend
            // interprets exactly as it does the matching keyboard shortcut.
            let toggle_sidebar =
                tauri::menu::MenuItemBuilder::with_id("toggle-sidebar", "Show/Hide Sidebar")
                    .accelerator("CmdOrCtrl+Shift+S")
                    .build(app)?;
            let open_transcript =
                tauri::menu::MenuItemBuilder::with_id("open-transcript", "Open Full Transcript")
                    .accelerator("CmdOrCtrl+Alt+T")
                    .build(app)?;
            let view_menu = tauri::menu::SubmenuBuilder::new(app, "View")
                .item(&toggle_sidebar)
                .item(&open_transcript)
                .build()?;
            // W6-B: a standard Window submenu. Predefined items only, same as
            // Edit above. `.maximize()` is Tauri/muda's builder name for the
            // item macOS itself labels "Zoom" (⌘W's `close_window()` sends
            // the native `performClose:` action, which macOS routes through
            // the same `windowShouldClose:` delegate call as the traffic-light
            // close button — the existing CloseRequested handler below
            // already intercepts that into a hide, so ⌘W composes with it
            // for free and needs no menu-event handling here).
            let window_menu = tauri::menu::SubmenuBuilder::new(app, "Window")
                .minimize()
                .maximize()
                .separator()
                .close_window()
                .build()?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&app_menu)
                .item(&file_menu)
                .item(&edit_menu)
                .item(&view_menu)
                .item(&window_menu)
                .build()?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                let id = event.id().0.as_str();
                if id == "open-settings" {
                    let _ = show_settings_window(app);
                    return;
                }
                if let Some(name) = menu_event_name(id) {
                    let _ = app.emit(name, ());
                }
            });

            // § A: the menubar item is always present — most sessions never
            // open a window. Built before the startup thread so the first
            // rendered state is the honest hollow glyph, never a gap.
            let open = tauri::menu::MenuItem::with_id(
                app,
                "open-window",
                "Open Yawn",
                true,
                None::<&str>,
            )?;
            // W8-A: the tray becomes minimally state-aware. "Stop recording"
            // is built once here but starts OUT of `menu` below — it is
            // inserted or removed by `spawn_tray_updater` as CaptureState
            // crosses the Recording/Paused boundary (`tray_shows_stop_recording`),
            // so it is truly absent otherwise, not merely disabled (muda has
            // no per-item visibility toggle, only membership). Its handler
            // below calls `stop_meeting` directly — same validation, same
            // capture-task Stop command, same honest failure handling as the
            // in-window Stop action, which carries no confirmation sheet
            // either (see `stopRecording` in apps/desktop/ui/main.js).
            let stop = tauri::menu::MenuItem::with_id(
                app,
                "stop-recording",
                "Stop recording",
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&open)
                .separator()
                // The tray's Quit is not a second path that has to be kept
                // in sync with ⌘Q — `.quit_with_text` builds the same
                // native `muda::PredefinedMenuItem::quit` the app menu's
                // `.quit()` above already uses, bound directly to macOS's
                // `terminate:` action. No on_menu_event arm fires for it:
                // the OS handles it before a Tauri menu event ever exists,
                // so quit-during-recording behaves exactly as ⌘Q already
                // does (interrupted-capture recovery on the next launch;
                // see `scan_and_recover`) — this packet adds no new
                // quit-time confirmation.
                .quit_with_text("Quit Yawn")
                .build()?;
            let tray = tauri::tray::TrayIconBuilder::with_id("menubar-item")
                .title("○")
                .tooltip("Nothing is recording")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id() == "open-window" {
                        show_and_focus_active_window(app);
                    } else if event.id() == "stop-recording" {
                        // Native menu clicks land on the main thread, but
                        // stop_meeting takes command_lock/model locks and
                        // calls capture_shortcut::deactivate, which reaches
                        // into the global-shortcut plugin's own OS-level
                        // unregister, so dispatch it off the main thread
                        // rather than assuming the plugin's internals
                        // tolerate a main-thread caller. (The parenthetical
                        // here used to say the frontend reaches this command
                        // off-main because Tauri runs commands on an async
                        // task pool. It does not: a command without `async`
                        // in its attribute runs on the main thread, which is
                        // what froze the window during note generation. The
                        // spawn below is still right; only the reason was.)
                        let app = app.clone();
                        std::thread::spawn(move || {
                            let _ = stop_meeting(app);
                        });
                    }
                })
                .build(app)?;
            spawn_tray_updater(
                app.handle().clone(),
                tray,
                menu,
                stop,
                file_stop_recording,
            );
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("meeting-runtime-startup".into())
                .spawn(move || initialize_application(handle, false))
                .map_err(io_error)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Local Meeting Notes shell failed")
        // `Builder::run` is a shorthand for `build` followed by `App::run(|_,
        // _| {})` — it discards every RunEvent, including macOS's Reopen.
        // That gap left Dock-clicking the app with no visible window front
        // nothing (the tray's "Open Yawn" was the only recovery), so this
        // app runs its own callback instead of the shorthand.
        .run(|app_handle, event| handle_run_event(app_handle, event));
}

// `RunEvent::Reopen` only exists on macOS (`applicationShouldHandleReopen`),
// so the match lives behind its own cfg — a plain `if let` on the variant
// would not compile on a non-macOS target. This app only ships for macOS,
// but Cargo.toml carries no target_os restriction, so keep the split honest
// rather than relying on that.
#[cfg(target_os = "macos")]
fn handle_run_event(app_handle: &AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Reopen {
        has_visible_windows,
        ..
    } = event
    {
        if should_show_on_reopen(has_visible_windows) {
            show_and_focus_active_window(app_handle);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_run_event(_app_handle: &AppHandle, _event: tauri::RunEvent) {}

fn initialize_application(app: AppHandle, retry: bool) {
    let state = app.state::<ApplicationState>();
    stop_owned_audio_playback(&state);
    if let Ok(mut profile) = state.preview_profile.lock() {
        *profile = PreviewProfileSnapshot::unavailable();
    }
    {
        let mut model = state.model.lock().expect("application model lock");
        model.retention_operational = false;
        model.startup_message = "Checking your private workspace.".into();
        if !retry && transition_startup(&mut model, StartupState::Checking).is_err() {
            return;
        }
    }

    state.runtime.lock().expect("runtime identity lock").take();
    let worker_cleanup = state
        .worker
        .lock()
        .expect("worker process lock")
        .take()
        .map(|mut worker| worker.stop_and_wait(Duration::from_millis(750)));
    let worker_cleanup_failed = if let Some(Err(error)) = worker_cleanup {
        write_diagnostic(&state, "worker_cleanup_failed", &error.to_string());
        true
    } else {
        false
    };
    let transcription_worker_cleanup = state
        .transcription_worker
        .lock()
        .expect("transcription worker process lock")
        .take()
        .map(|mut worker| worker.stop_and_wait(Duration::from_millis(750)));
    let transcription_worker_cleanup_failed = if let Some(Err(error)) = transcription_worker_cleanup
    {
        write_diagnostic(
            &state,
            "transcription_worker_cleanup_failed",
            &error.to_string(),
        );
        true
    } else {
        false
    };
    stop_transcription_queue_executor(&state);
    if worker_cleanup_failed || transcription_worker_cleanup_failed {
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "a previous worker could not be stopped safely",
        );
        return;
    }

    let storage_context = match create_storage_context(&app) {
        Ok(context) => context,
        Err(error) => {
            finish_startup_failure(&state, retry, StartupFailure::Diagnostic, &error);
            return;
        }
    };
    if let Err(error) = ensure_app_data_writer_lock(&state, &storage_context.storage) {
        finish_startup_failure(&state, retry, StartupFailure::Diagnostic, &error);
        return;
    }
    *state.storage.lock().expect("storage context lock") = Some(storage_context.clone());
    set_startup_message(&state, "Checking saved meetings on this Mac.");

    let coordination = match state.meeting_storage_coordination() {
        Ok(coordination) => coordination,
        Err(_) => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Diagnostic,
                "meeting storage coordination is unavailable",
            );
            return;
        }
    };
    if let Err(error) = reconcile_pending_meeting_deletions(&storage_context.storage, &coordination)
    {
        let _ = write_private_diagnostic(
            &storage_context.diagnostics,
            "meeting_deletion_reconciliation_failed",
            &error.to_string(),
        );
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "an interrupted meeting deletion requires attention",
        );
        return;
    }
    // Ahead of any library read, exactly like the permanent-deletion
    // reconciliation above: a meeting mid-move to trash or mid-restore must
    // never be read as anything but what it will finish as.
    if let Err(error) =
        reconcile_pending_trash(&storage_context.storage, &coordination, now_epoch_seconds())
    {
        let _ = write_private_diagnostic(
            &storage_context.diagnostics,
            "meeting_trash_reconciliation_failed",
            &error.to_string(),
        );
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "an interrupted trash move or restore requires attention",
        );
        return;
    }
    if let Err(error) =
        reconcile_pending_transcript_deletions(&storage_context.storage, &coordination)
    {
        let _ = write_private_diagnostic(
            &storage_context.diagnostics,
            "transcript_deletion_reconciliation_failed",
            &error.to_string(),
        );
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "an interrupted transcript deletion requires attention",
        );
        return;
    }
    let storage_sequence = match coordination.lock_sequence() {
        Ok(sequence) => sequence,
        Err(_) => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Diagnostic,
                "meeting storage coordination is unavailable",
            );
            return;
        }
    };

    let recovery = scan_and_recover(
        &storage_context.storage,
        now_epoch_seconds(),
        &SystemProcessInspector,
        &SystemGroupSignaler,
        Duration::from_millis(750),
    );
    let (recovery_ready, restorable_meetings) = match recovery {
        Ok(report) => {
            let mut retention_ready = true;
            let mut restorable_meetings = Vec::new();
            for meeting in &report.meetings {
                match meeting.disposition {
                    RecoveryDisposition::Valid | RecoveryDisposition::RecoveredAudioDeletion => {
                        restorable_meetings.push(meeting.meeting_id.clone());
                    }
                    RecoveryDisposition::Quarantined(code) => {
                        if code == RecoveryCode::RetentionMismatch {
                            retention_ready = false;
                        }
                        let _ = write_private_diagnostic(
                            &storage_context.diagnostics,
                            code.as_str(),
                            &format!(
                                "meeting {} was quarantined without mutation",
                                meeting.meeting_id
                            ),
                        );
                    }
                    RecoveryDisposition::OwnershipAmbiguous => {
                        let _ = write_private_diagnostic(
                            &storage_context.diagnostics,
                            "meeting_recovery_ownership_ambiguous",
                            &format!(
                                "meeting {} blocks capture because child identity is uncertain",
                                meeting.meeting_id
                            ),
                        );
                    }
                    _ => {}
                }
            }
            (
                !report.blocks_capture && retention_ready,
                restorable_meetings,
            )
        }
        Err(error) => {
            let _ = state
                .worker
                .lock()
                .expect("worker process lock")
                .take()
                .map(|mut worker| worker.stop_and_wait(Duration::from_millis(750)));
            let _ = write_private_diagnostic(
                &storage_context.diagnostics,
                "meeting_recovery_failed",
                &error.to_string(),
            );
            (false, Vec::new())
        }
    };
    if !recovery_ready {
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "meeting recovery requires attention",
        );
        return;
    }
    let restored_transcript =
        match load_latest_transcript_projection(&storage_context.storage, &restorable_meetings) {
            Ok(projection) => projection,
            Err(error) => {
                let _ = write_private_diagnostic(
                    &storage_context.diagnostics,
                    "transcript_restore_failed",
                    &error,
                );
                finish_startup_failure(
                    &state,
                    retry,
                    StartupFailure::Diagnostic,
                    "a retained transcript could not be reopened safely",
                );
                return;
            }
        };
    drop(storage_sequence);
    {
        let mut model = state.model.lock().expect("application model lock");
        model.retention_operational = true;
    }
    start_retention_executor(&app, &storage_context);

    set_startup_message(&state, "Verifying on-device speech models.");
    let manifest = match RuntimeManifest::load_and_verify(&storage_context.manifest_path) {
        Ok(manifest) if manifest.is_internal_alpha() => manifest,
        _ => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Runtime,
                "the packaged runtime is missing or changed",
            );
            return;
        }
    };
    let catalog = match verified_model_catalog(&storage_context.manifest_path, &manifest) {
        Ok(catalog) => catalog,
        Err(_) => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Runtime,
                "the signed model catalog is missing or changed",
            );
            return;
        }
    };
    let installed_transcript_model: Option<InstalledTranscriptModel> = match catalog.as_ref() {
        Some(catalog) => match installed_model(&storage_context.storage, catalog) {
            Ok(Some(installed)) => {
                let mut model = state.model.lock().expect("application model lock");
                model.model_setup = ModelSetupSnapshot {
                    state: "installed".into(),
                    options: catalog.models.iter().map(ModelSetupOption::from).collect(),
                    selected_model_id: Some(installed.entry.id.clone()),
                    downloaded_bytes: installed.entry.download_bytes,
                    total_bytes: installed.entry.download_bytes,
                    error: None,
                };
                Some(installed)
            }
            Ok(None) => {
                finish_model_selection(&state, retry, catalog, None);
                return;
            }
            Err(_) => {
                finish_model_selection(
                    &state,
                    retry,
                    catalog,
                    Some(
                        "The previous speech model is incomplete or changed. Choose a model to repair it."
                            .into(),
                    ),
                );
                return;
            }
        },
        None => {
            let mut model = state.model.lock().expect("application model lock");
            model.model_setup = ModelSetupSnapshot {
                state: "bundled".into(),
                ..ModelSetupSnapshot::default()
            };
            None
        }
    };
    set_startup_message(&state, "Starting the local transcription engine.");
    let worker_path = storage_context.resource_root.join(&manifest.runtime.path);
    let mut command = Command::new(&worker_path);
    command
        .args(["-E", "-s", "-B", "-m", "worker.main"])
        .arg("--app-data-root")
        .arg(storage_context.storage.path())
        .arg("--runtime-manifest")
        .arg(&storage_context.manifest_path);
    if let Some(installed) = installed_transcript_model.as_ref() {
        command
            .arg("--transcript-model-receipt")
            .arg(&installed.receipt_path);
    }
    command.current_dir(&storage_context.resource_root);
    let mut worker = match OwnedChild::spawn(&mut command) {
        Ok(worker) => worker,
        Err(SupervisionError::MissingChild) => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Runtime,
                "the packaged worker is missing",
            );
            return;
        }
        Err(error) => {
            let _ = write_private_diagnostic(
                &storage_context.diagnostics,
                "worker_spawn_failed",
                &error.to_string(),
            );
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Diagnostic,
                "the packaged worker could not start",
            );
            return;
        }
    };
    let ready = worker.wait_ready(Duration::from_secs(10), &internal_alpha_operations());
    let external_models = installed_transcript_model
        .as_ref()
        .map(InstalledTranscriptModel::runtime_models)
        .unwrap_or_default();
    match ready {
        Ok(ready) if manifest.matches_ready_with_external_models(&ready, &external_models) => {}
        Err(SupervisionError::ReadyTimeout) => {
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Timeout,
                "the packaged worker did not answer",
            );
            return;
        }
        Ok(_) => {
            if let Err(error) = worker.stop_and_wait(Duration::from_millis(750)) {
                let _ = write_private_diagnostic(
                    &storage_context.diagnostics,
                    "worker_identity_mismatch_cleanup_failed",
                    &error.to_string(),
                );
                finish_startup_failure(
                    &state,
                    retry,
                    StartupFailure::Diagnostic,
                    "the mismatched worker could not be stopped safely",
                );
                return;
            }
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Runtime,
                "the packaged worker identity does not match the application",
            );
            return;
        }
        Err(error) => {
            let _ = write_private_diagnostic(
                &storage_context.diagnostics,
                "worker_startup_failed",
                &error.to_string(),
            );
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Diagnostic,
                "the packaged worker failed its startup check",
            );
            return;
        }
    }

    let retention_ready = state
        .model
        .lock()
        .expect("application model lock")
        .retention_operational;
    if !retention_ready {
        if let Err(error) = worker.stop_and_wait(Duration::from_millis(750)) {
            let _ = write_private_diagnostic(
                &storage_context.diagnostics,
                "worker_retention_block_cleanup_failed",
                &error.to_string(),
            );
        }
        finish_startup_failure(
            &state,
            retry,
            StartupFailure::Diagnostic,
            "audio retention needs attention before startup can finish",
        );
        return;
    }

    let mut packaged_models: Vec<(String, String)> = manifest
        .models
        .iter()
        .map(|model| (model.id.clone(), model.sha256.clone()))
        .collect();
    packaged_models.extend(external_models.clone());
    let runtime = RuntimeIdentity {
        admission: "internal-alpha".into(),
        worker_build_sha256: manifest.worker.sha256.clone(),
        worker_executable_sha256: manifest.runtime.sha256.clone(),
        transcript_model_identity: installed_transcript_model
            .as_ref()
            .map(|model| format!("{}@{}", model.entry.id, model.entry.revision))
            .or_else(|| manifest.models.first().map(|model| model.id.clone()))
            .unwrap_or_else(|| "bundled-transcript-model".into()),
        tap_build_sha256: manifest.tap.sha256.clone(),
        tap_path: storage_context.resource_root.join(&manifest.tap.path),
        encoder_sha256: manifest.encoder.sha256.clone(),
        packaged_models,
        encoder_available: manifest.encoder.path.file_name()
            != Some(std::ffi::OsStr::new("encoder-unavailable.identity")),
    };
    *state.worker.lock().expect("worker process lock") = Some(worker);
    let mut transcription_command = Command::new(&worker_path);
    transcription_command
        .args(["-E", "-s", "-B", "-m", "worker.main"])
        .arg("--app-data-root")
        .arg(storage_context.storage.path())
        .arg("--runtime-manifest")
        .arg(&storage_context.manifest_path);
    if let Some(installed) = installed_transcript_model.as_ref() {
        transcription_command
            .arg("--transcript-model-receipt")
            .arg(&installed.receipt_path);
    }
    transcription_command.current_dir(&storage_context.resource_root);
    let mut transcription_worker = match OwnedChild::spawn(&mut transcription_command) {
        Ok(worker) => worker,
        Err(error) => {
            let _ = state
                .worker
                .lock()
                .expect("worker process lock")
                .take()
                .map(|mut worker| worker.stop_and_wait(Duration::from_millis(750)));
            let _ = write_private_diagnostic(
                &storage_context.diagnostics,
                "transcription_worker_spawn_failed",
                &error.to_string(),
            );
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Diagnostic,
                "the dedicated transcription worker could not start",
            );
            return;
        }
    };
    let transcription_operations = internal_alpha_operations();
    match transcription_worker.wait_ready(Duration::from_secs(10), &transcription_operations) {
        Ok(ready) if manifest.matches_ready_with_external_models(&ready, &external_models) => {}
        _ => {
            let mut worker = transcription_worker;
            let _ = worker.stop_and_wait(Duration::from_millis(750));
            let _ = state
                .worker
                .lock()
                .expect("worker process lock")
                .take()
                .map(|mut worker| worker.stop_and_wait(Duration::from_millis(750)));
            finish_startup_failure(
                &state,
                retry,
                StartupFailure::Runtime,
                "the dedicated transcription worker failed its startup check",
            );
            return;
        }
    }
    *state
        .transcription_worker
        .lock()
        .expect("transcription worker process lock") = Some(transcription_worker);
    *state.runtime.lock().expect("runtime identity lock") = Some(runtime.clone());
    let mut model = state.model.lock().expect("application model lock");
    model.admission = runtime.admission.clone();
    model.startup_message = "Finishing your private workspace.".into();
    if let Some(projection) = restored_transcript
        && let Err(error) = apply_restored_transcript_projection(&mut model, projection)
    {
        model.error = Some(error);
        let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
        return;
    }
    if let Err(error) = transition_startup(&mut model, StartupState::Ready) {
        model.error = Some(error);
    } else {
        model.startup_message = "Yawn is ready.".into();
    }
    drop(model);
    start_transcription_queue_executor(&app, &storage_context.storage, &runtime);
}

enum StartupFailure {
    Runtime,
    Timeout,
    Diagnostic,
}

fn finish_startup_failure(
    state: &ApplicationState,
    retry: bool,
    failure: StartupFailure,
    detail: &str,
) {
    let target = match (retry, failure) {
        (_, StartupFailure::Timeout) => StartupState::ServiceTimeout,
        (false, StartupFailure::Runtime) => StartupState::RuntimeMissing,
        (false, StartupFailure::Diagnostic) => StartupState::DiagnosticWritten,
        (true, StartupFailure::Runtime) => StartupState::ReinstallRequired,
        (true, StartupFailure::Diagnostic) => StartupState::DiagnosticWritten,
    };
    let mut model = state.model.lock().expect("application model lock");
    model.startup_message = "Startup needs attention.".into();
    if transition_startup(&mut model, target).is_err() {
        model.error = Some("The installation check stopped in an invalid state.".into());
    } else {
        model.error = Some(detail.into());
    }
}

fn set_startup_message(state: &ApplicationState, message: &'static str) {
    state
        .model
        .lock()
        .expect("application model lock")
        .startup_message = message.into();
}

fn create_storage_context(app: &AppHandle) -> Result<StorageContext, String> {
    let app_data = app.path().app_data_dir().map_err(error_text)?;
    let resource_root = app.path().resource_dir().map_err(error_text)?;
    #[cfg(debug_assertions)]
    let protected_root = repository_root();
    #[cfg(not(debug_assertions))]
    let protected_root = resource_root.clone();
    let storage = StorageRoot::create(&app_data, &protected_root).map_err(error_text)?;
    Ok(StorageContext {
        manifest_path: resource_root.join("app-runtime.json"),
        diagnostics: storage.path().join("diagnostics"),
        storage,
        resource_root,
    })
}

fn ensure_app_data_writer_lock(
    state: &ApplicationState,
    storage: &StorageRoot,
) -> Result<(), String> {
    let mut held = state
        .app_data_writer_lock
        .lock()
        .map_err(|_| "the app-data writer lock is unavailable".to_string())?;
    if held.is_some() {
        return Ok(());
    }
    *held = Some(Arc::new(acquire_app_data_writer_lock(storage)?));
    Ok(())
}

fn acquire_app_data_writer_lock(storage: &StorageRoot) -> Result<AppDataWriterLock, String> {
    AppDataWriterLock::acquire(storage).map_err(error_text)
}

#[cfg(debug_assertions)]
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root above apps/desktop/src-tauri")
        .to_path_buf()
}

fn start_retention_executor(app: &AppHandle, context: &StorageContext) {
    let state = app.state::<ApplicationState>();
    if state.retention_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let storage = context.storage.clone();
    let diagnostics = context.diagnostics.clone();
    std::thread::spawn(move || {
        let mut reported_quarantines = HashSet::new();
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let state = app.state::<ApplicationState>();
            let retention_result =
                execute_scheduled_retention(&state, &storage, now_epoch_seconds());
            // Decided before the match consumes the result, and never from a
            // per-meeting outcome: only a genuine retention-machine failure may
            // refuse app-wide readiness.
            let holds_readiness = scheduled_retention_holds_app_readiness(&retention_result);
            match retention_result {
                Ok(None) => {}
                Ok(Some(report)) => {
                    // A per-meeting quarantine is a per-meeting condition: it is
                    // logged and surfaced on that meeting, but it never marks
                    // retention app-wide unavailable. One un-releasable meeting
                    // blocking every future recording was the D-LOCK hard lock;
                    // only a genuine retention-machine failure (the two Err arms
                    // below) may refuse app-wide readiness. This mirrors the
                    // trash-purge rule already documented below — separate
                    // promises, kept separate.
                    for outcome in report.retention {
                        if let RetentionOutcome::Quarantined(meeting_id) = outcome
                            && reported_quarantines.insert(meeting_id.clone())
                        {
                            let _ = write_private_diagnostic(
                                &diagnostics,
                                "retention_meeting_quarantined",
                                &format!("meeting {meeting_id} was quarantined without mutation"),
                            );
                        }
                    }
                    // A trash entry that fails to purge is logged the same way
                    // a quarantined meeting is, but it never marks retention
                    // itself unavailable — the two are separate promises, and
                    // a stuck purge should not also block audio release.
                    for outcome in report.trash_purge {
                        if let TrashPurgeOutcome::Quarantined(meeting_id) = outcome
                            && reported_quarantines.insert(format!("trash-purge:{meeting_id}"))
                        {
                            let _ = write_private_diagnostic(
                                &diagnostics,
                                "trash_purge_quarantined",
                                &format!("trash entry {meeting_id} could not be purged"),
                            );
                        }
                    }
                }
                Err(ScheduledRetentionError::Coordination(message)) => {
                    let _ = write_private_diagnostic(
                        &diagnostics,
                        "retention_coordination_failed",
                        message,
                    );
                }
                Err(ScheduledRetentionError::Retention) => {
                    let _ = write_private_diagnostic(
                        &diagnostics,
                        "retention_tick_failed",
                        "scheduled retained-audio release could not complete",
                    );
                }
            }
            if holds_readiness {
                mark_retention_unavailable(&state);
            }
        }
    });
}

/// Whether one scheduled retention tick may refuse app-wide readiness.
///
/// Only a genuine failure of the retention machine itself — its coordination
/// lock or the release pass — holds readiness. A completed tick never does, no
/// matter what its per-meeting outcomes were: a single quarantined or
/// deferred meeting is a per-meeting condition, surfaced on that meeting, and
/// must never block every future recording. Collapsing the two was the D-LOCK
/// hard lock.
fn scheduled_retention_holds_app_readiness(
    result: &Result<Option<ScheduledRetentionReport>, ScheduledRetentionError>,
) -> bool {
    matches!(
        result,
        Err(ScheduledRetentionError::Coordination(_)) | Err(ScheduledRetentionError::Retention)
    )
}

#[derive(Debug)]
enum ScheduledRetentionError {
    Coordination(&'static str),
    Retention,
}

/// What one scheduled tick did, across the two independent promises it
/// reconciles: audio retention (a privacy deadline) and the trash purge
/// window (a recoverability deadline). They share a tick because both are
/// "sweep storage for something whose time has come," but neither outcome
/// list is folded into the other's meaning.
#[derive(Debug, PartialEq, Eq)]
struct ScheduledRetentionReport {
    retention: Vec<RetentionOutcome>,
    trash_purge: Vec<TrashPurgeOutcome>,
}

/// Runs one scheduled retention pass under the same first-tier command lock
/// as playback launch and explicit deletion. A live owned child defers the
/// whole pass before it can acquire the storage sequence; a completed child is
/// reaped before the pass continues. The order is deliberately `command_lock`,
/// playback slot, then meeting-storage sequence; no storage lock is held while
/// acquiring an earlier tier.
///
/// The trash purge runs in this same locked pass rather than a second timer,
/// reusing retention's own scheduling idiom exactly as the roadmap intake
/// calls for.
fn execute_scheduled_retention(
    state: &ApplicationState,
    storage: &StorageRoot,
    now: u64,
) -> Result<Option<ScheduledRetentionReport>, ScheduledRetentionError> {
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| ScheduledRetentionError::Coordination("command lock is unavailable"))?;
    if scheduled_retention_playback_is_active(state)? {
        return Ok(None);
    }

    let coordination = state.meeting_storage_coordination().map_err(|_| {
        ScheduledRetentionError::Coordination("meeting storage coordination is unavailable")
    })?;
    let storage_sequence = coordination.lock_sequence().map_err(|_| {
        ScheduledRetentionError::Coordination("meeting storage sequence lock is unavailable")
    })?;
    let active_meetings = storage_sequence.active_meeting_ids().map_err(|_| {
        ScheduledRetentionError::Coordination("active meeting registry is unavailable")
    })?;

    let retention = execute_due_retention_excluding(storage, now, &active_meetings)
        .map_err(|_| ScheduledRetentionError::Retention)?;
    let trash_purge = execute_due_trash_purge(storage, &coordination, now)
        .map_err(|_| ScheduledRetentionError::Retention)?;
    Ok(Some(ScheduledRetentionReport {
        retention,
        trash_purge,
    }))
}

/// Polls only Yawn's retained child while the command lock is held. A live
/// child keeps the reviewed artifact in place; an exited or failed child is
/// reaped before retention obtains storage authority.
fn scheduled_retention_playback_is_active(
    state: &ApplicationState,
) -> Result<bool, ScheduledRetentionError> {
    let mut slot = state.audio_playback.lock().map_err(|_| {
        ScheduledRetentionError::Coordination("retained-audio playback is unavailable")
    })?;
    let Some(playback) = slot.as_mut() else {
        return Ok(false);
    };
    match playback.poll() {
        Ok(true) => Ok(true),
        Ok(false) => {
            slot.take();
            Ok(false)
        }
        Err(_) => {
            if let Some(mut playback) = slot.take() {
                playback.stop_and_reap();
            }
            Ok(false)
        }
    }
}

fn mark_retention_unavailable(state: &ApplicationState) {
    let Ok(_command) = state.command_lock.lock() else {
        return;
    };
    let Ok(mut model) = state.model.lock() else {
        return;
    };
    model.retention_operational = false;
    model.error = Some("Audio retention needs attention before another meeting can start.".into());
    if model.reducer.capture() == CaptureState::Idle
        && model.reducer.startup() == StartupState::Ready
    {
        let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
    }
}

fn run_capture_task(
    app: AppHandle,
    meeting_id: String,
    attempt_id: String,
    retention_days: u64,
    attestation: StartAttestation,
    commands: mpsc::Receiver<CaptureTaskCommand>,
    journey_timing: capture_timing::JourneyTiming,
    t3_epoch_ms: u64,
) {
    let state = app.state::<ApplicationState>();
    let _task_registration = CaptureTaskRegistration {
        app: app.clone(),
        meeting_id: meeting_id.clone(),
    };
    let storage = match state.storage.lock().expect("storage context lock").clone() {
        Some(storage) => storage,
        None => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_storage_unavailable",
                "storage context is missing",
                "Private meeting storage is unavailable.",
            );
            return;
        }
    };
    let runtime = match state.runtime.lock().expect("runtime identity lock").clone() {
        Some(runtime) => runtime,
        None => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_runtime_unavailable",
                "runtime identity is missing",
                "The local runtime is unavailable.",
            );
            return;
        }
    };
    let (process_group_id, initial_worker_identity) = match inspect_worker(&state, &runtime) {
        Ok(identity) => identity,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_worker_unavailable",
                &error,
                "The local worker stopped before recording began.",
            );
            return;
        }
    };
    let coordination = match state.meeting_storage_coordination() {
        Ok(coordination) => coordination,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_storage_coordination_failed",
                &error,
                "Private meeting storage could not be reserved safely.",
            );
            return;
        }
    };
    let _active_meeting_lease = match coordination.acquire(&meeting_id) {
        Ok(lease) => lease,
        Err(error) => {
            let error = error.to_string();
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_storage_coordination_failed",
                &error,
                "Private meeting storage could not be reserved safely.",
            );
            return;
        }
    };
    let attempt = match create_attempt(
        &storage.storage,
        &meeting_id,
        &attempt_id,
        retention_days,
        attestation,
    ) {
        Ok(attempt) => attempt,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_attempt_write_failed",
                &error,
                "The private attempt receipt could not be saved.",
            );
            return;
        }
    };

    let mut helper = match CaptureProcess::spawn(
        &runtime.tap_path,
        &attempt.meeting_dir.join("capture"),
        process_group_id,
    ) {
        Ok(helper) => helper,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                false,
                "capture_helper_spawn_failed",
                &error,
                "The audio helper could not start.",
            );
            return;
        }
    };
    match helper.receive_until(Instant::now() + Duration::from_secs(10)) {
        Ok(CaptureEvent::Paused) => {}
        Ok(_) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                false,
                "capture_helper_bad_pause",
                "capture helper did not begin in paused state",
                "The audio helper did not start safely.",
            );
            return;
        }
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                false,
                "capture_helper_pause_failed",
                &error,
                "The audio helper did not reach its safe paused state.",
            );
            return;
        }
    }

    let helper_identity = match helper.pid().and_then(inspect_process) {
        Ok(identity) if identity.executable_sha256 == runtime.tap_build_sha256 => identity,
        _ => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_helper_identity_failed",
                "capture helper identity could not be established",
                "The audio helper identity could not be verified.",
            );
            return;
        }
    };
    let current_worker_identity = match inspect_process(initial_worker_identity.pid) {
        Ok(identity)
            if identity == initial_worker_identity
                && identity.executable_sha256 == runtime.worker_executable_sha256 =>
        {
            identity
        }
        _ => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_worker_identity_changed",
                "worker identity changed before ownership commit",
                "The local worker changed before recording began.",
            );
            return;
        }
    };
    let ownership = OwnershipReceipt {
        schema: OwnershipSchema::V1,
        process_group_id,
        application_build_sha256: attempt.application_build_sha256.clone(),
        worker_build_sha256: runtime.worker_build_sha256.clone(),
        tap_build_sha256: runtime.tap_build_sha256.clone(),
        children: vec![current_worker_identity, helper_identity],
    };
    if write_ownership_receipt(&attempt.meeting_dir, &ownership).is_err() {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            false,
            true,
            "capture_ownership_write_failed",
            "capture ownership receipt could not become durable",
            "The capture ownership receipt could not be saved.",
        );
        return;
    }
    let ownership_ref = match artifact_ref(&attempt.meeting_dir, "ownership.json") {
        Ok(reference) => reference,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_ownership_verify_failed",
                &error.to_string(),
                "The capture ownership receipt could not be verified.",
            );
            return;
        }
    };
    let mut meeting = match load_meeting(&attempt.meeting_dir) {
        Ok(meeting) => meeting,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "capture_meeting_load_failed",
                &error.to_string(),
                "The meeting record could not be reopened.",
            );
            return;
        }
    };
    meeting.artifacts.ownership = Some(ownership_ref);
    meeting.lifecycle = MeetingLifecycle::Incomplete;
    if let Err(error) = write_meeting(&attempt.meeting_dir, &meeting) {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            false,
            true,
            "capture_meeting_ownership_commit_failed",
            &error.to_string(),
            "The meeting ownership record could not be committed.",
        );
        return;
    }
    let recovery_required = true;
    if let Err(error) = helper.send(b'S') {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            recovery_required,
            true,
            "capture_start_signal_failed",
            &error,
            "Recording could not begin.",
        );
        return;
    }
    match helper.receive_until(Instant::now() + CAPTURE_ARM_TIMEOUT) {
        Ok(CaptureEvent::Recording) => {}
        Ok(CaptureEvent::Failed { code }) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_hardware_failed",
                &format!("capture helper failed with code {code}"),
                capture_user_message(&code),
            );
            return;
        }
        Ok(_) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_recording_event_invalid",
                "capture helper emitted an unexpected event before recording",
                "Both audio channels did not become ready.",
            );
            return;
        }
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_recording_event_failed",
                &error,
                "Both audio channels did not become ready.",
            );
            return;
        }
    }
    let recording_started = Instant::now();
    let started_at_epoch_seconds = now_epoch_seconds();
    {
        let mut model = state.model.lock().expect("application model lock");
        if transition_capture(&mut model, CaptureState::Recording).is_err() {
            drop(model);
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_recording_transition_failed",
                "application state changed during capture arming",
                "The recording state could not be confirmed.",
            );
            return;
        }
        model.started_at_epoch_seconds = Some(started_at_epoch_seconds);
        model.mic_state = Some("Active".into());
        model.system_state = Some("Active".into());
    }

    // Packet W7-C: t4, the moment `CaptureState::Recording` is confirmed
    // above -- the other end of the app's own start-latency span (t2 -> t4).
    // Written once, here, and nowhere else; every earlier return in this
    // function leaves no receipt, which is the "absence means unmeasured"
    // rule from `capture_timing`'s module doc. Best-effort: a write failure
    // must never unwind a recording that has already, correctly, started.
    let t4_epoch_ms = capture_timing::now_epoch_millis();
    let app_version = app.package_info().version.to_string();
    let _ = capture_timing::write(
        &attempt.meeting_dir,
        journey_timing,
        t3_epoch_ms,
        t4_epoch_ms,
        &app_version,
    );

    let mut ledger = PauseLedger::new(recording_started);
    // A pause or resume the helper has not confirmed yet. The reducer is not
    // moved until it does, so the recorder never claims a state the audio has
    // not reached; if the helper goes quiet the capture fails rather than
    // leaving the operator looking at a request that will never land.
    let mut pause_change_deadline: Option<Instant> = None;
    // Stop is honored, but not mid-handshake: the helper still owes a suspended
    // or resumed event, and consuming it as if it were the finalize would fail
    // an otherwise healthy take. The deadline above bounds the wait.
    let mut stop_requested = false;
    loop {
        if stop_requested && pause_change_deadline.is_none() {
            break;
        }
        match commands.try_recv() {
            Ok(CaptureTaskCommand::Pause) => {
                pause_change_deadline = Some(Instant::now() + CAPTURE_PAUSE_TIMEOUT);
                if let Err(error) = helper.send(b'P') {
                    fail_capture_task(
                        &app,
                        Some(&meeting_id),
                        recovery_required,
                        true,
                        "capture_pause_signal_failed",
                        &error,
                        "The recording could not be paused.",
                    );
                    return;
                }
            }
            Ok(CaptureTaskCommand::Resume) => {
                pause_change_deadline = Some(Instant::now() + CAPTURE_PAUSE_TIMEOUT);
                if let Err(error) = helper.send(b'R') {
                    fail_capture_task(
                        &app,
                        Some(&meeting_id),
                        recovery_required,
                        true,
                        "capture_resume_signal_failed",
                        &error,
                        "The recording could not be resumed.",
                    );
                    return;
                }
            }
            Ok(CaptureTaskCommand::Stop) => stop_requested = true,
            Err(mpsc::TryRecvError::Disconnected) => {
                fail_capture_task(
                    &app,
                    Some(&meeting_id),
                    recovery_required,
                    true,
                    "capture_control_disconnected",
                    "application capture control closed",
                    "The recording control closed unexpectedly.",
                );
                return;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if stop_requested && pause_change_deadline.is_none() {
            break;
        }
        if pause_change_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_pause_change_timeout",
                "capture helper did not confirm the pause change",
                "The recording did not confirm the pause. Nothing was marked complete.",
            );
            return;
        }
        match helper.receive_briefly(Duration::from_millis(100)) {
            Ok(None) => {}
            Ok(Some(CaptureEvent::Suspended)) => {
                pause_change_deadline = None;
                // The ledger records what the audio did regardless of what the
                // operator did next: the gap is real even if Stop is already
                // waiting behind it.
                ledger.begin_pause(Instant::now());
                if stop_requested {
                    // The operator stopped while the pause was still settling,
                    // so the reducer is already on Stopping. Moving it to
                    // Paused here would refuse and discard a healthy take.
                    continue;
                }
                let mut model = state.model.lock().expect("application model lock");
                if transition_capture(&mut model, CaptureState::Paused).is_err() {
                    drop(model);
                    fail_capture_task(
                        &app,
                        Some(&meeting_id),
                        recovery_required,
                        true,
                        "capture_pause_transition_failed",
                        "application state changed while the recording was pausing",
                        "The paused state could not be confirmed.",
                    );
                    return;
                }
                model.capture_pause_change_pending = false;
                // Both channels are released, so neither is active. Saying
                // "Paused" rather than blanking the fact keeps the recorder
                // surface describing what the hardware is actually doing.
                model.mic_state = Some("Paused".into());
                model.system_state = Some("Paused".into());
            }
            Ok(Some(CaptureEvent::Resumed)) => {
                pause_change_deadline = None;
                ledger.end_pause(Instant::now());
                if stop_requested {
                    continue;
                }
                let mut model = state.model.lock().expect("application model lock");
                if transition_capture(&mut model, CaptureState::Recording).is_err() {
                    drop(model);
                    fail_capture_task(
                        &app,
                        Some(&meeting_id),
                        recovery_required,
                        true,
                        "capture_resume_transition_failed",
                        "application state changed while the recording was resuming",
                        "The recording state could not be confirmed.",
                    );
                    return;
                }
                model.capture_pause_change_pending = false;
                model.mic_state = Some("Active".into());
                model.system_state = Some("Active".into());
            }
            Ok(Some(CaptureEvent::Failed { code })) => {
                fail_capture_task(
                    &app,
                    Some(&meeting_id),
                    recovery_required,
                    true,
                    "capture_failed_while_recording",
                    &format!("capture helper failed with code {code}"),
                    capture_user_message(&code),
                );
                return;
            }
            Ok(Some(CaptureEvent::Interrupted)) => {
                fail_capture_task(
                    &app,
                    Some(&meeting_id),
                    recovery_required,
                    true,
                    "capture_interrupted",
                    "capture helper reported interruption",
                    "Recording was interrupted before both files were finalized.",
                );
                return;
            }
            Ok(Some(_)) | Err(_) => {
                fail_capture_task(
                    &app,
                    Some(&meeting_id),
                    recovery_required,
                    true,
                    "capture_event_invalid",
                    "capture helper emitted an invalid event sequence",
                    "The audio helper stopped following the recording protocol.",
                );
                return;
            }
        }
    }

    ledger.finish(Instant::now());
    // Recorded elapsed time, not wall clock. The audio files hold nothing for a
    // paused span, so wall clock would read as both legs having ended early and
    // `capture-health` would refuse anything past its startup-skew allowance.
    let capture_elapsed_samples = match elapsed_samples(ledger.recorded_elapsed()) {
        Ok(samples) => samples,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_elapsed_time_invalid",
                &error,
                "The recording exceeded the supported duration.",
            );
            return;
        }
    };
    if let Err(error) = helper.send(b'X') {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            recovery_required,
            true,
            "capture_stop_signal_failed",
            &error,
            "The audio helper did not receive Stop.",
        );
        return;
    }
    let finalized = match helper.receive_until(Instant::now() + CAPTURE_STOP_TIMEOUT) {
        Ok(CaptureEvent::Finalized {
            mic_samples,
            system_samples,
        }) if mic_samples > 0 && system_samples > 0 => (mic_samples, system_samples),
        Ok(CaptureEvent::Failed { code }) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_finalize_failed",
                &format!("capture helper failed with code {code}"),
                capture_user_message(&code),
            );
            return;
        }
        Ok(_) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_finalize_event_invalid",
                "capture helper did not return a valid finalized event",
                "Both audio files could not be finalized.",
            );
            return;
        }
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_finalize_event_failed",
                &error,
                "Both audio files could not be finalized.",
            );
            return;
        }
    };
    if finalized.0 > 16_000 * 60 * 60 * 24 || finalized.1 > 16_000 * 60 * 60 * 24 {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            recovery_required,
            true,
            "capture_sample_count_invalid",
            "capture helper returned an out-of-range sample count",
            "The finalized audio timing was invalid.",
        );
        return;
    }
    if let Err(error) = helper.finish_cleanly(Instant::now() + Duration::from_secs(5)) {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            recovery_required,
            true,
            "capture_helper_exit_failed",
            &error,
            "The audio helper did not close cleanly.",
        );
        return;
    }
    drop(helper);

    let pauses = match ledger.receipt() {
        Ok(pauses) => pauses,
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                true,
                "capture_pause_record_invalid",
                &error,
                "The record of when this recording was paused could not be written.",
            );
            return;
        }
    };
    let capture_result = request_worker(
        &state,
        Operation::CaptureFinalize,
        json!({
            "meeting_id": meeting_id,
            "started_at_epoch_seconds": started_at_epoch_seconds,
            "capture_elapsed_samples": capture_elapsed_samples,
            "pauses": pauses,
        }),
        WORKER_REQUEST_TIMEOUT,
    );
    let capture_digests = match capture_result {
        Ok(result) => match exact_digests(
            &result,
            &["capture-session", "capture-mic", "capture-system"],
        ) {
            Ok(digests) => digests,
            Err(error) => {
                fail_capture_task(
                    &app,
                    Some(&meeting_id),
                    recovery_required,
                    true,
                    "capture_digest_set_invalid",
                    &error,
                    "The finalized capture could not be verified.",
                );
                return;
            }
        },
        Err(error) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                recovery_required,
                error.is_supervisor(),
                "capture_worker_finalize_failed",
                &error.to_string(),
                "The finalized capture did not pass its integrity check.",
            );
            return;
        }
    };
    if let Err(error) = commit_captured_meeting(&attempt.meeting_dir, &capture_digests) {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            recovery_required,
            true,
            "capture_meeting_commit_failed",
            &error,
            "The validated capture could not be committed.",
        );
        return;
    }
    let runtime = state
        .runtime
        .lock()
        .expect("runtime identity lock")
        .clone()
        .ok_or_else(|| "verified runtime identity is unavailable".to_string());
    let storage = state
        .storage
        .lock()
        .expect("storage context lock")
        .as_ref()
        .map(|context| context.storage.clone())
        .ok_or_else(|| "private storage is unavailable".to_string());
    let request = match (runtime, storage) {
        (Ok(runtime), Ok(_storage)) => {
            let meeting = load_meeting(&attempt.meeting_dir).map_err(error_text);
            match meeting {
                Ok(meeting) => TranscriptionRequest {
                    schema: local_meeting_notes_session_core::transcription_queue::TranscriptionRequestSchema::V1,
                    request_id: Uuid::new_v4(),
                    meeting_id: meeting_id.clone(),
                    capture_session_sha256: meeting.artifacts.capture_session.as_ref().unwrap().sha256.clone(),
                    microphone_audio_sha256: meeting.artifacts.microphone_audio.as_ref().unwrap().sha256.clone(),
                    system_audio_sha256: meeting.artifacts.system_audio.as_ref().unwrap().sha256.clone(),
                    model_identity: runtime.transcript_model_identity.clone(),
                    worker_runtime_identity: runtime.worker_executable_sha256.clone(),
                    enqueued_at_epoch_seconds: now_epoch_seconds(),
                },
                Err(error) => {
                    fail_capture_task(&app, Some(&meeting_id), false, true, "transcription_queue_meeting_load_failed", &error, "The capture could not be queued for local transcription.");
                    return;
                }
            }
        }
        (Err(error), _) | (_, Err(error)) => {
            fail_capture_task(
                &app,
                Some(&meeting_id),
                false,
                true,
                "transcription_queue_context_missing",
                &error,
                "The capture could not be queued for local transcription.",
            );
            return;
        }
    };
    let storage = state
        .storage
        .lock()
        .expect("storage context lock")
        .as_ref()
        .unwrap()
        .storage
        .clone();
    if let Err(error) = TranscriptionQueue::open(&storage).and_then(|queue| queue.enqueue(request))
    {
        fail_capture_task(
            &app,
            Some(&meeting_id),
            false,
            true,
            "transcription_queue_enqueue_failed",
            &error.to_string(),
            "The capture could not be queued for local transcription.",
        );
        return;
    }
    {
        let mut model = state.model.lock().expect("application model lock");
        if transition_capture(&mut model, CaptureState::Captured).is_err()
            || transition_capture(&mut model, CaptureState::Idle).is_err()
        {
            model.error =
                Some("The capture was saved, but the recorder could not return to idle.".into());
        } else {
            model.clear_meeting_projection();
        }
    }
}

fn write_ownership_receipt(meeting_dir: &Path, ownership: &OwnershipReceipt) -> Result<(), String> {
    if !ownership.validate() {
        return Err("capture ownership receipt is invalid".into());
    }
    let encoded = serde_json::to_vec_pretty(ownership).map_err(error_text)?;
    durable_create_new(&meeting_dir.join("ownership.json"), &encoded).map_err(error_text)
}

fn validate_start_request(days: u64, attestation: &StartAttestation) -> Result<(), String> {
    if !matches!(days, 1 | 7 | 30) {
        return Err("Choose one of the available audio-retention periods.".into());
    }
    if !attestation.participants_consented {
        return Err("Confirm that everyone agreed before recording.".into());
    }
    if !attestation.headphones {
        return Err("This alpha requires headphones.".into());
    }
    if !attestation.operator_alone {
        return Err("This alpha requires one person near the microphone.".into());
    }
    Ok(())
}

fn create_attempt(
    storage: &StorageRoot,
    meeting_id: &str,
    attempt_id: &str,
    retention_days: u64,
    attestation: StartAttestation,
) -> Result<AttemptContext, String> {
    let meeting_dir = meeting_dir(storage, meeting_id).map_err(error_text)?;
    let capture_dir = meeting_dir.join("capture");
    create_private_dir(&meeting_dir).map_err(error_text)?;
    create_private_dir(&capture_dir).map_err(error_text)?;
    let result = (|| {
        let created_at_epoch_seconds = now_epoch_seconds();
        let seconds = retention_days
            .checked_mul(24 * 60 * 60)
            .ok_or_else(|| "retention period overflowed".to_string())?;
        let rule = AudioRetentionRule::DeleteAfter { seconds };
        let policy_sha256 = retention_policy_sha256(&rule);
        let application_build_sha256 =
            sha256_file(&std::env::current_exe().map_err(error_text)?).map_err(error_text)?;
        let attempt = CaptureAttemptReceipt {
            schema: "capture-attempt/1".into(),
            meeting_id: meeting_id.into(),
            attempt_id: attempt_id.into(),
            created_at_epoch_seconds,
            application_build_sha256: application_build_sha256.clone(),
            participant_notice_version: PARTICIPANT_NOTICE_VERSION.into(),
            operator_attestation: attestation,
            retention_policy_sha256: policy_sha256.clone(),
        };
        durable_create_new(
            &meeting_dir.join("attempt.json"),
            &serde_json::to_vec_pretty(&attempt).map_err(error_text)?,
        )
        .map_err(error_text)?;
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: meeting_id.into(),
            lifecycle: MeetingLifecycle::RecoveredInterrupted,
            retention: AudioRetention {
                rule,
                policy_sha256,
                next_deletion_at_epoch_seconds: Some(
                    created_at_epoch_seconds
                        .checked_add(seconds)
                        .ok_or_else(|| "retention deadline overflowed".to_string())?,
                ),
                state: AudioState::NeverCreated,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&meeting_dir, "attempt.json").map_err(error_text)?,
                ownership: None,
                capture_session: None,
                microphone_audio: None,
                system_audio: None,
                current_transcript: None,
                current_note: None,
            },
            pending_storage_operation: None,
        };
        meeting.validate(meeting_id).map_err(error_text)?;
        durable_create_new(
            &meeting_dir.join("meeting.json"),
            &serde_json::to_vec_pretty(&meeting).map_err(error_text)?,
        )
        .map_err(error_text)?;
        sync_directory(&meeting_dir).map_err(error_text)?;
        Ok(AttemptContext {
            meeting_dir: meeting_dir.clone(),
            application_build_sha256,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(meeting_dir.join("meeting.json"));
        let _ = fs::remove_file(meeting_dir.join("attempt.json"));
        let _ = fs::remove_dir(&capture_dir);
        let _ = fs::remove_dir(&meeting_dir);
        if let Some(meetings) = meeting_dir.parent() {
            let _ = sync_directory(meetings);
        }
    }
    result
}

fn inspect_worker(
    state: &ApplicationState,
    runtime: &RuntimeIdentity,
) -> Result<(i32, ProcessIdentity), String> {
    let worker = state.worker.lock().expect("worker process lock");
    let worker = worker
        .as_ref()
        .ok_or_else(|| "worker process is unavailable".to_string())?;
    worker.check_health().map_err(error_text)?;
    let process_group_id = worker.process_group_id();
    let identity = inspect_process(worker.pid())?;
    if identity.executable_sha256 != runtime.worker_executable_sha256 {
        return Err("worker executable identity does not match the runtime manifest".into());
    }
    Ok((process_group_id, identity))
}

fn inspect_process(pid: u32) -> Result<ProcessIdentity, String> {
    match SystemProcessInspector.inspect(pid).map_err(error_text)? {
        ProcessInspection::Identity(identity) => Ok(identity),
        ProcessInspection::Absent => Err("owned process is absent".into()),
        ProcessInspection::Unavailable => Err("owned process identity is unavailable".into()),
    }
}

fn request_worker(
    state: &ApplicationState,
    operation: Operation,
    arguments: Value,
    timeout: Duration,
) -> Result<HashMap<String, String>, WorkerCallError> {
    let port = product_coordinator::ProcessWorkerPort::new(state.worker.clone());
    let result: WorkerResult = port
        .request_with_progress(operation, arguments, timeout, |progress| {
            record_transcription_heartbeat(state, progress)
        })
        .map_err(|unavailable| WorkerCallError::Supervisor(unavailable.0))?;
    if !result.ok {
        return Err(WorkerCallError::Rejected);
    }
    Ok(result.artifact_digests)
}

fn request_worker_on(
    worker: Arc<Mutex<Option<OwnedChild>>>,
    operation: Operation,
    arguments: Value,
    timeout: Duration,
    mut heartbeat: impl FnMut(&WorkerProgress) -> Result<(), ProtocolError>,
) -> Result<HashMap<String, String>, WorkerCallError> {
    let port = product_coordinator::ProcessWorkerPort::new(worker);
    let result: WorkerResult = port
        .request_with_progress(operation, arguments, timeout, |progress| {
            heartbeat(progress)
        })
        .map_err(|unavailable| WorkerCallError::Supervisor(unavailable.0))?;
    if !result.ok {
        return Err(WorkerCallError::Rejected);
    }
    Ok(result.artifact_digests)
}

fn start_transcription_queue_executor(
    app: &AppHandle,
    storage: &StorageRoot,
    runtime: &RuntimeIdentity,
) {
    let state = app.state::<ApplicationState>();
    let mut executor = state
        .transcription_executor
        .lock()
        .expect("transcription executor lock");
    if executor.is_some() {
        return;
    }
    let app = app.clone();
    let storage = storage.clone();
    let runtime = runtime.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let handle = match std::thread::Builder::new()
        .name("background-transcription-queue".into())
        .spawn(move || loop {
            if thread_stop.load(Ordering::SeqCst) { break; }
            let state = app.state::<ApplicationState>();
            let queue = match TranscriptionQueue::open(&storage) {
                Ok(queue) => queue,
                Err(error) => {
                    write_diagnostic(&state, "transcription_queue_open_failed", &error.to_string());
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };
            let item = match recover_transcription_queue(&state, &storage, &runtime, &queue) {
                Ok(discovery) => {
                    let eligible = discovery
                        .items
                        .into_iter()
                        .filter(queue_item_is_eligible)
                        .collect::<Vec<_>>();
                    let queued_count = eligible.len();
                    let item = eligible.into_iter().next();
                    if let Ok(mut model) = state.model.lock() {
                        model.background_transcription_queued_count = queued_count;
                        model.background_transcription_active = item.is_some();
                    }
                    item
                }
                Err(error) => {
                    write_diagnostic(&state, "transcription_queue_recovery_failed", &error);
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };
            let Some(item) = item else {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            };
            let request_id = item.request.request_id;
            let claim = match queue.claim(request_id, runtime.worker_executable_sha256.clone(), now_epoch_seconds()) {
                Ok(item) => item.claim,
                Err(error) => {
                    write_diagnostic(&state, "transcription_queue_claim_failed", &error.to_string());
                    continue;
                }
            };
            let Some(claim) = claim else { continue };
            let expected_meeting = match Uuid::parse_str(&item.request.meeting_id) {
                Ok(id) => id,
                Err(_) => {
                    let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds());
                    continue;
                }
            };
            let worker = state.transcription_worker.clone();
            let worker_request_id = Arc::new(Mutex::new(None::<Uuid>));
            let heartbeat_id = worker_request_id.clone();
            let result = request_worker_on(
                worker,
                Operation::TranscriptCreate,
                json!({"meeting_id": item.request.meeting_id}),
                TRANSCRIPT_REQUEST_TIMEOUT,
                |progress| {
                    if progress.event != ProgressEvent::CaptureState || progress.state != CaptureProgressState::Transcribing || progress.meeting_id != expected_meeting {
                        return Err(ProtocolError::InvalidEvent);
                    }
                    let mut id = heartbeat_id.lock().map_err(|_| ProtocolError::InvalidEvent)?;
                    if let Some(existing) = *id {
                        if existing != progress.request_id { return Err(ProtocolError::InvalidEvent); }
                    } else { *id = Some(progress.request_id); }
                    Ok(())
                },
            );
            match result {
                Ok(digests) => {
                    let digest = match exact_digests(&digests, &["transcript"]) { Ok(digests) => digests["transcript"].clone(), Err(error) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds()); write_diagnostic(&state, "transcription_queue_digest_invalid", &error); continue; } };
                    let meeting_dir = match storage.resolve(Path::new("meetings").join(&item.request.meeting_id).as_path()) { Ok(path) => path, Err(_) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds()); continue; } };
                    let reference = match verified_artifact(&meeting_dir, &format!("transcript/{digest}.json"), &digest) { Ok(reference) => reference, Err(error) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds()); write_diagnostic(&state, "transcription_queue_artifact_invalid", &error); continue; } };
                    if let Err(error) = load_transcript_projection(&meeting_dir, &item.request.meeting_id, &reference) {
                        let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds());
                        write_diagnostic(&state, "transcription_queue_projection_invalid", &error);
                        continue;
                    }
                    let path = match resolve_artifact(&meeting_dir, &reference.relative_path) { Ok(path) => path, Err(_) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds()); continue; } };
                    let bytes = match read_private_bytes(&path, TRANSCRIPT_MAX_BYTES) { Ok(bytes) => bytes, Err(_) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds()); continue; } };
                    if queue.admit_result(request_id, &bytes, reference, now_epoch_seconds()).is_err() || queue.commit(request_id, now_epoch_seconds()).is_err() {
                        let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds());
                    }
                }
                Err(WorkerCallError::Rejected) => { let _ = queue.fail(request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Failed, now_epoch_seconds()); }
                Err(WorkerCallError::Supervisor(_)) => { let _ = queue.release_claim(request_id, claim.claim_id, now_epoch_seconds()); }
            }
        }) {
        Ok(handle) => handle,
        Err(error) => {
            write_diagnostic(&state, "transcription_executor_spawn_failed", &error.to_string());
            return;
        }
    };
    *executor = Some(TranscriptionExecutorControl { stop, handle });
}

fn recover_transcription_queue(
    state: &ApplicationState,
    storage: &StorageRoot,
    runtime: &RuntimeIdentity,
    queue: &TranscriptionQueue<'_>,
) -> Result<local_meeting_notes_session_core::transcription_queue::QueueDiscovery, String> {
    let discovery = queue.discover().map_err(|error| error.to_string())?;
    for item in &discovery.items {
        if item.terminal.is_some() {
            continue;
        }
        if let Some(claim) = &item.claim {
            queue
                .release_claim(item.request.request_id, claim.claim_id, now_epoch_seconds())
                .map_err(|error| error.to_string())?;
        }
        if queue_item_is_repairable(item) {
            let meeting_dir = storage
                .resolve(
                    Path::new("meetings")
                        .join(&item.request.meeting_id)
                        .as_path(),
                )
                .map_err(error_text)?;
            let reference = item.result.as_ref().unwrap().transcript.clone();
            if load_transcript_projection(&meeting_dir, &item.request.meeting_id, &reference)
                .is_ok()
            {
                queue
                    .commit(item.request.request_id, now_epoch_seconds())
                    .map_err(|error| error.to_string())?;
            } else {
                let _ = queue.fail(item.request.request_id, local_meeting_notes_session_core::transcription_queue::TranscriptionTerminalKind::Quarantined, now_epoch_seconds());
                write_diagnostic(
                    state,
                    "transcription_queue_projection_invalid",
                    "stored transcript projection could not be revalidated",
                );
            }
        }
    }
    let discovery = queue.discover().map_err(|error| error.to_string())?;
    for meeting_id in discovery.orphan_captured_meetings {
        let meeting_dir = storage
            .resolve(Path::new("meetings").join(&meeting_id).as_path())
            .map_err(error_text)?;
        let meeting = load_meeting(&meeting_dir).map_err(error_text)?;
        let request = TranscriptionRequest {
            schema: local_meeting_notes_session_core::transcription_queue::TranscriptionRequestSchema::V1,
            request_id: Uuid::new_v4(),
            meeting_id: meeting_id.clone(),
            capture_session_sha256: meeting.artifacts.capture_session.as_ref().ok_or_else(|| "captured meeting has no session artifact".to_string())?.sha256.clone(),
            microphone_audio_sha256: meeting.artifacts.microphone_audio.as_ref().ok_or_else(|| "captured meeting has no microphone artifact".to_string())?.sha256.clone(),
            system_audio_sha256: meeting.artifacts.system_audio.as_ref().ok_or_else(|| "captured meeting has no system artifact".to_string())?.sha256.clone(),
            model_identity: runtime.transcript_model_identity.clone(),
            worker_runtime_identity: runtime.worker_executable_sha256.clone(),
            enqueued_at_epoch_seconds: now_epoch_seconds(),
        };
        queue.enqueue(request).map_err(|error| error.to_string())?;
    }
    queue.discover().map_err(|error| error.to_string())
}

fn queue_item_is_repairable(
    item: &local_meeting_notes_session_core::transcription_queue::TranscriptionQueueItem,
) -> bool {
    item.terminal.is_none() && item.result.is_some() && item.commit.is_none()
}

fn queue_item_is_eligible(
    item: &local_meeting_notes_session_core::transcription_queue::TranscriptionQueueItem,
) -> bool {
    item.terminal.is_none()
        && item.result.is_none()
        && item.commit.is_none()
        && item.claim.is_none()
}

fn stop_transcription_queue_executor(state: &ApplicationState) {
    let control = state
        .transcription_executor
        .lock()
        .expect("transcription executor lock")
        .take();
    if let Some(control) = control {
        control.stop.store(true, Ordering::SeqCst);
        if control.handle.join().is_err() {
            write_diagnostic(
                state,
                "transcription_executor_join_failed",
                "background transcription executor did not stop cleanly",
            );
        }
    }
}

fn record_transcription_heartbeat(
    state: &ApplicationState,
    progress: &WorkerProgress,
) -> Result<(), ProtocolError> {
    if progress.event != ProgressEvent::CaptureState
        || progress.state != CaptureProgressState::Transcribing
    {
        return Err(ProtocolError::InvalidEvent);
    }
    let progress_meeting_id = progress.meeting_id.to_string();
    let mut model = state
        .model
        .lock()
        .map_err(|_| ProtocolError::InvalidEvent)?;
    if model.reducer.capture() != CaptureState::Transcribing
        || model.meeting_id.as_deref() != Some(progress_meeting_id.as_str())
    {
        return Err(ProtocolError::InvalidEvent);
    }
    model.transcription_last_worker_heartbeat_at_epoch_seconds = Some(now_epoch_seconds());
    Ok(())
}

fn exact_digests(
    values: &HashMap<String, String>,
    expected: &[&str],
) -> Result<HashMap<String, String>, String> {
    let actual = values.keys().map(String::as_str).collect::<HashSet<_>>();
    let expected = expected.iter().copied().collect::<HashSet<_>>();
    if actual != expected || values.values().any(|digest| !valid_sha256(digest)) {
        return Err("worker artifact digest set is invalid".into());
    }
    Ok(values.clone())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn commit_captured_meeting(
    meeting_dir: &Path,
    digests: &HashMap<String, String>,
) -> Result<(), String> {
    let capture_session = verified_artifact(
        meeting_dir,
        "capture/session.json",
        &digests["capture-session"],
    )?;
    let microphone_audio =
        verified_artifact(meeting_dir, "capture/mic.wav", &digests["capture-mic"])?;
    let system_audio = verified_artifact(
        meeting_dir,
        "capture/system.wav",
        &digests["capture-system"],
    )?;
    let mut meeting = load_meeting(meeting_dir).map_err(error_text)?;
    if meeting.lifecycle != MeetingLifecycle::Incomplete || meeting.artifacts.ownership.is_none() {
        return Err("meeting is not awaiting a finalized capture".into());
    }
    meeting.lifecycle = MeetingLifecycle::Captured;
    meeting.retention.state = AudioState::Retained;
    meeting.artifacts.capture_session = Some(capture_session);
    meeting.artifacts.microphone_audio = Some(microphone_audio);
    meeting.artifacts.system_audio = Some(system_audio);
    write_meeting(meeting_dir, &meeting).map_err(error_text)
}

fn verified_artifact(
    meeting_dir: &Path,
    relative_path: &str,
    expected_digest: &str,
) -> Result<ArtifactRef, String> {
    let reference = artifact_ref(meeting_dir, relative_path).map_err(error_text)?;
    if reference.sha256 != expected_digest {
        return Err("worker digest does not match the private artifact".into());
    }
    Ok(reference)
}

/// What the microphone gate did, as the capture recorded it.
///
/// Read rather than discarded because two of these fields are operator-facing
/// obligations, and until 2026-08-05 the
/// whole block was parsed into `_voiceprint` and dropped. The gate could
/// therefore delete a colleague's speech from a meeting that cannot be re-run
/// and tell nobody: the alert's only route to a human ran through a note, and no
/// note generator is admitted.
///
/// Not `deny_unknown_fields`, unlike its parent. The capture side writes the
/// full provenance block — encoder fingerprints, profile digests, versions — and
/// this screen needs four values out of it. Pinning the whole shape here would
/// make every future capture-side field a transcript that will not open.
#[derive(Deserialize)]
struct VoiceprintReport {
    #[serde(default)]
    applied: bool,
    /// The dropped speech keeps returning as one voice: somebody sitting beside
    /// the operator is being removed, rather than scattered noise.
    #[serde(default)]
    persistent_other: bool,
    #[serde(default)]
    rejected_seconds: Option<f64>,
    /// The share of dropped speech that was the one recurring voice.
    /// `rejected_seconds` alone is everything the gate dropped, scattered noise
    /// included, so quoting it beside "someone next to you is being removed"
    /// overstates that person's loss — by up to 2x, since the flag fires at
    /// `share > 0.5`. `notes/transcript.py` already multiplies these two; this
    /// screen must not disagree with the note about the same capture.
    #[serde(default)]
    coherent_share: Option<f64>,
    /// Derived from leave-one-sitting-out enrolment evidence, never from live
    /// meeting audio. The number is real; what it was measured on is not the
    /// thing being gated.
    #[serde(default)]
    measured_frr: Option<f64>,
    #[serde(default)]
    n_sittings: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptDocument {
    schema: String,
    #[serde(rename = "source")]
    _source: String,
    attribution: String,
    #[serde(rename = "bleed")]
    _bleed: Option<Value>,
    voiceprint: Option<VoiceprintReport>,
    #[serde(rename = "capture_health")]
    _capture_health: Value,
    turns: Vec<TranscriptInputTurn>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptInputTurn {
    start: f64,
    end: f64,
    speaker: Option<String>,
    text: String,
    gated: Option<bool>,
    gate_score: Option<f64>,
    gate_reason: Option<String>,
}

fn load_latest_transcript_projection(
    storage: &StorageRoot,
    eligible_meeting_ids: &[String],
) -> Result<Option<RestoredTranscriptProjection>, String> {
    let mut latest: Option<(u64, String, PathBuf, ArtifactRef, AudioState)> = None;
    for meeting_id in eligible_meeting_ids {
        let directory = meeting_dir(storage, meeting_id).map_err(error_text)?;
        let meeting = load_meeting(&directory).map_err(error_text)?;
        if !matches!(
            meeting.lifecycle,
            MeetingLifecycle::TranscriptReady
                | MeetingLifecycle::SummaryFailed
                | MeetingLifecycle::Ready
        ) {
            continue;
        }
        let transcript = meeting
            .artifacts
            .current_transcript
            .clone()
            .ok_or_else(|| "transcript-bearing meeting has no current transcript".to_string())?;
        let created_at_epoch_seconds = load_attempt_created_at(&directory, &meeting)?;
        let replace = match &latest {
            Some((latest_created, latest_id, ..)) => {
                created_at_epoch_seconds > *latest_created
                    || (created_at_epoch_seconds == *latest_created
                        && meeting.meeting_id > *latest_id)
            }
            None => true,
        };
        if replace {
            latest = Some((
                created_at_epoch_seconds,
                meeting.meeting_id,
                directory,
                transcript,
                meeting.retention.state,
            ));
        }
    }

    let Some((_created, meeting_id, directory, transcript, audio_state)) = latest else {
        return Ok(None);
    };
    let (turns, mut warnings) = load_transcript_projection(&directory, &meeting_id, &transcript)?;
    if audio_state == AudioState::Released {
        warnings.push(
            "Meeting audio was deleted under the selected retention period. The transcript remains."
                .into(),
        );
    }
    Ok(Some(RestoredTranscriptProjection {
        meeting_id,
        turns,
        current_transcript_sha256: transcript.sha256,
        warnings,
    }))
}

fn load_attempt_created_at(meeting_dir: &Path, meeting: &MeetingRecord) -> Result<u64, String> {
    let actual =
        artifact_ref(meeting_dir, &meeting.artifacts.attempt.relative_path).map_err(error_text)?;
    if actual != meeting.artifacts.attempt {
        return Err("capture attempt no longer matches its meeting record".into());
    }
    let bytes = read_private_bytes(
        &meeting_dir.join(&meeting.artifacts.attempt.relative_path),
        ATTEMPT_MAX_BYTES,
    )
    .map_err(error_text)?;
    let attempt: CaptureAttemptReceipt = serde_json::from_slice(&bytes).map_err(error_text)?;
    if attempt.schema != "capture-attempt/1"
        || attempt.meeting_id != meeting.meeting_id
        || Uuid::parse_str(&attempt.attempt_id).is_err()
        || !valid_sha256(&attempt.application_build_sha256)
        || attempt.participant_notice_version != PARTICIPANT_NOTICE_VERSION
        || !attempt.operator_attestation.participants_consented
        || !attempt.operator_attestation.headphones
        || !attempt.operator_attestation.operator_alone
        || attempt.retention_policy_sha256 != meeting.retention.policy_sha256
    {
        return Err("capture attempt metadata is invalid".into());
    }
    Ok(attempt.created_at_epoch_seconds)
}

fn load_transcript_projection(
    meeting_dir: &Path,
    meeting_id: &str,
    reference: &ArtifactRef,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    load_transcript_projection_with_corrections(meeting_dir, meeting_id, reference, true)
}

fn load_transcript_projection_without_corrections(
    meeting_dir: &Path,
    meeting_id: &str,
    reference: &ArtifactRef,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    load_transcript_projection_with_corrections(meeting_dir, meeting_id, reference, false)
}

fn load_transcript_projection_with_corrections(
    meeting_dir: &Path,
    meeting_id: &str,
    reference: &ArtifactRef,
    apply_speaker_corrections: bool,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    let actual = artifact_ref(meeting_dir, &reference.relative_path).map_err(error_text)?;
    if &actual != reference {
        return Err("retained transcript no longer matches its meeting record".into());
    }
    let path = meeting_dir.join(&reference.relative_path);
    let bytes = read_private_bytes(&path, TRANSCRIPT_MAX_BYTES).map_err(error_text)?;
    if format!("{:x}", Sha256::digest(&bytes)) != reference.sha256 {
        return Err("retained transcript bytes changed while opening".into());
    }
    project_current_transcript(
        meeting_dir,
        meeting_id,
        reference,
        &bytes,
        apply_speaker_corrections,
    )
}

/// Projects the meeting's current transcript pointer whether it names a base
/// capture transcript or a restored `transcript-view/1`. Views resolve through
/// session-core's audited chain walker; restored turns become ordinary rows,
/// still-withheld turns stay content-free.
fn project_current_transcript(
    meeting_dir: &Path,
    meeting_id: &str,
    reference: &ArtifactRef,
    bytes: &[u8],
    apply_speaker_corrections: bool,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    let is_view = serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|document| {
            document
                .get("schema")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        == Some("transcript-view/1");
    let meeting_uuid = Uuid::parse_str(meeting_id).map_err(error_text)?;
    let (mut turns, mut warnings) = if !is_view {
        parse_transcript_projection_with(bytes, &BTreeSet::new())?
    } else {
        let resolved = resolve_stored_transcript_primed(
            meeting_dir,
            meeting_uuid,
            &reference.sha256,
            bytes.to_vec(),
        )
        .map_err(error_text)?;
        let restored: BTreeSet<u32> = resolved
            .inspection
            .restored_source_turn_indices
            .iter()
            .copied()
            .collect();
        parse_transcript_projection_with(&resolved.base_bytes, &restored)?
    };
    if !apply_speaker_corrections {
        return Ok((turns, warnings));
    }
    match speaker_correction::current_labels(meeting_dir, meeting_uuid, &reference.sha256) {
        Ok(labels) => {
            for turn in &mut turns {
                if turn.withheld {
                    continue;
                }
                if let Some(replacement) = labels.get(&turn.source_speaker) {
                    turn.speaker = Some(replacement.clone());
                    turn.speaker_corrected = true;
                }
            }
        }
        Err(()) => warnings.push(
            "Saved speaker corrections could not be read. Source speaker labels are shown.".into(),
        ),
    }
    Ok((turns, warnings))
}

fn parse_transcript_projection_with(
    bytes: &[u8],
    restored: &BTreeSet<u32>,
) -> Result<(Vec<TranscriptTurn>, Vec<String>), String> {
    let document: TranscriptDocument = serde_json::from_slice(bytes).map_err(error_text)?;
    if document.schema != "capture-transcript/1"
        || !matches!(document.attribution.as_str(), "channel" | "none")
        || document.turns.len() > 20_000
    {
        return Err("transcript presentation schema is invalid".into());
    }
    let unattributed = document.attribution == "none";
    let mut turns = Vec::with_capacity(document.turns.len());
    let mut gated = 0_u64;
    for (source_turn_index, turn) in document.turns.into_iter().enumerate() {
        if !turn.start.is_finite()
            || !turn.end.is_finite()
            || turn.start < 0.0
            || turn.end < turn.start
            || turn.text.len() > 100_000
            || turn
                .speaker
                .as_ref()
                .is_some_and(|speaker| speaker.len() > 256)
            || (!unattributed
                && turn
                    .speaker
                    .as_deref()
                    .is_some_and(|speaker| !matches!(speaker, "Me" | "Them")))
            || ((turn.gate_score.is_some() || turn.gate_reason.is_some())
                && turn.gated != Some(true))
            || (!unattributed && turn.gated == Some(true) && turn.speaker.as_deref() != Some("Me"))
        {
            return Err("transcript turn is invalid".into());
        }
        let restored_here = restored.contains(&(source_turn_index as u32));
        if restored_here && turn.gated != Some(true) {
            return Err("a restored turn is not withheld in the base transcript".into());
        }
        if turn.gated == Some(true) && !restored_here {
            gated += 1;
            turns.push(TranscriptTurn {
                source_turn_index: source_turn_index as u32,
                source_speaker: None,
                speaker: None,
                speaker_corrected: false,
                start: turn.start,
                text: String::new(),
                withheld: true,
            });
            continue;
        }
        let source_speaker = if unattributed { None } else { turn.speaker };
        turns.push(TranscriptTurn {
            source_turn_index: source_turn_index as u32,
            speaker: source_speaker.clone(),
            source_speaker,
            speaker_corrected: false,
            start: turn.start,
            text: turn.text,
            withheld: false,
        });
    }
    let mut warnings = Vec::new();
    let report = document.voiceprint.filter(|report| report.applied);
    // First, and before the count, because it is the only one of these that says
    // a person was removed rather than that some audio was.
    if report
        .as_ref()
        .is_some_and(|report| report.persistent_other)
    {
        // The recurring voice's own seconds, not the gate's total. Dropped
        // rather than approximated when either factor is missing, the same way
        // the enrolment basis below is dropped rather than invented.
        let extent = match report.as_ref().and_then(|report| {
            let seconds = report.rejected_seconds?;
            let share = report.coherent_share?;
            (seconds.is_finite() && seconds > 0.0 && share.is_finite() && share > 0.0)
                .then_some((seconds * share, share))
        }) {
            Some((seconds, share)) => format!(
                " About {} seconds of it were withheld — {:.0}% of everything the check dropped.",
                seconds.round(),
                share * 100.0
            ),
            None => String::new(),
        };
        warnings.push(format!(
            "The withheld speech keeps returning as one voice, which is what it looks like when \
             someone next to you is being removed from this record.{extent} Restore any turn that \
             should be here — this meeting cannot be re-run."
        ));
    }
    if document.attribution == "none" {
        warnings.push(
            "Speaker labels are unavailable because the channel split could not be trusted.".into(),
        );
    }
    if gated > 0 {
        warnings.push(format!(
            "The voice check withheld {gated} microphone segment(s); review the retained capture if words appear missing."
        ));
    }
    // Stated whenever the gate ran, not only when it withheld something: a run
    // that withheld nothing was still decided by this threshold, and an operator
    // reading a clean transcript is entitled to know what cleared it.
    if let Some(report) = report {
        let basis = match (report.measured_frr, report.n_sittings) {
            (Some(frr), Some(sittings)) if frr.is_finite() && sittings > 0 => format!(
                " It was set from {sittings} enrolment sitting(s), where it withheld {:.0}% of your own speech.",
                frr * 100.0
            ),
            _ => String::new(),
        };
        warnings.push(format!(
            "The voice check's threshold was measured on your enrolment recordings, not on \
             meeting audio.{basis}"
        ));
    }
    Ok((turns, warnings))
}

fn finish_transcription_failure(
    app: &AppHandle,
    meeting_dir: &Path,
    _meeting_id: &str,
    block_start: bool,
    code: &str,
    detail: &str,
    user_message: &str,
) {
    let mut must_block_start = block_start;
    let persistence_error = match load_meeting(meeting_dir) {
        Ok(mut meeting) if meeting.lifecycle == MeetingLifecycle::Captured => {
            meeting.lifecycle = MeetingLifecycle::TranscriptionFailed;
            write_meeting(meeting_dir, &meeting)
                .err()
                .map(|error| error.to_string())
        }
        Ok(_) => Some("meeting was not in captured state".into()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(error) = persistence_error {
        must_block_start = true;
        write_diagnostic(
            &app.state::<ApplicationState>(),
            "transcript_failure_state_write_failed",
            &error,
        );
    }
    let state = app.state::<ApplicationState>();
    write_diagnostic(&state, code, detail);
    {
        let mut model = state.model.lock().expect("application model lock");
        let _ = transition_capture(&mut model, CaptureState::TranscriptionFailed);
        if must_block_start && model.reducer.startup() == StartupState::Ready {
            let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
        }
        model.error = Some(user_message.into());
        model.mic_state = None;
        model.system_state = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn fail_capture_task(
    app: &AppHandle,
    _meeting_id: Option<&str>,
    recovery_required: bool,
    block_start: bool,
    code: &str,
    detail: &str,
    user_message: &str,
) {
    // A failure anywhere between arming and stopping ends the attempt, so
    // this shared helper is also where the note-capture hotkey comes down —
    // covers every call site, including the ones inside run_capture_task's
    // own thread that this packet does not otherwise touch.
    capture_shortcut::deactivate(app);
    let state = app.state::<ApplicationState>();
    write_diagnostic(&state, code, detail);
    {
        let mut model = state.model.lock().expect("application model lock");
        if matches!(
            model.reducer.capture(),
            CaptureState::Arming
                | CaptureState::Recording
                | CaptureState::Paused
                | CaptureState::Stopping
        ) {
            let _ = transition_capture(&mut model, CaptureState::RecoveredInterrupted);
        }
        if (recovery_required || block_start) && model.reducer.startup() == StartupState::Ready {
            let _ = transition_startup(&mut model, StartupState::DiagnosticWritten);
        }
        model.error = Some(user_message.into());
        model.capture_pause_change_pending = false;
        model.mic_state = None;
        model.system_state = None;
    }
}

fn clear_capture_task(state: &ApplicationState, meeting_id: &str) {
    let mut task = state.capture_task.lock().expect("capture task lock");
    if task
        .as_ref()
        .is_some_and(|task| task.meeting_id == meeting_id)
    {
        task.take();
    }
}

fn write_diagnostic(state: &ApplicationState, code: &str, detail: &str) {
    if let Some(storage) = state.storage.lock().expect("storage context lock").as_ref() {
        let _ = write_private_diagnostic(&storage.diagnostics, code, detail);
    }
}

fn transition_startup(model: &mut AppModel, target: StartupState) -> Result<(), String> {
    model
        .reducer
        .begin(ExclusiveOperation::StartupRecovery)
        .map_err(error_text)?;
    let result = model.reducer.transition_startup(target).map_err(error_text);
    model.reducer.finish(ExclusiveOperation::StartupRecovery);
    result
}

fn transition_capture(model: &mut AppModel, target: CaptureState) -> Result<(), String> {
    model
        .reducer
        .begin(ExclusiveOperation::CaptureTransition)
        .map_err(error_text)?;
    let result = model.reducer.transition_capture(target).map_err(error_text);
    if result.is_ok() {
        model.transcription_last_worker_heartbeat_at_epoch_seconds = None;
        model.capture_state_started_at_epoch_seconds = match target {
            CaptureState::Arming
            | CaptureState::Recording
            // The recorder shows elapsed time per step, and a pause is its own
            // step: the clock restarts so it reads how long this pause has
            // lasted, not how long the meeting has been open.
            | CaptureState::Paused
            | CaptureState::Stopping
            | CaptureState::Captured
            | CaptureState::Transcribing
            | CaptureState::Summarizing => Some(now_epoch_seconds()),
            CaptureState::Idle
            | CaptureState::TranscriptReady
            | CaptureState::Ready
            | CaptureState::TranscriptionFailed
            | CaptureState::SummaryFailed
            | CaptureState::RecoveredInterrupted => None,
        };
    }
    model.reducer.finish(ExclusiveOperation::CaptureTransition);
    result
}

fn read_capture_events(file: File, sender: mpsc::Sender<CaptureStreamItem>) {
    let mut reader = BufReader::new(file);
    loop {
        let mut frame = Vec::new();
        let read = std::io::Read::by_ref(&mut reader)
            .take((CAPTURE_EVENT_MAX_BYTES + 1) as u64)
            .read_until(b'\n', &mut frame);
        match read {
            Ok(0) => {
                let _ = sender.send(CaptureStreamItem::Closed);
                return;
            }
            Ok(_) if frame.len() <= CAPTURE_EVENT_MAX_BYTES && frame.ends_with(b"\n") => {
                frame.pop();
                match parse_capture_event(&frame) {
                    Ok(event) => {
                        if sender.send(CaptureStreamItem::Event(event)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = sender.send(CaptureStreamItem::ProtocolFailure);
                        return;
                    }
                }
            }
            Ok(_) | Err(_) => {
                let _ = sender.send(CaptureStreamItem::ProtocolFailure);
                return;
            }
        }
    }
}

fn parse_capture_event(frame: &[u8]) -> Result<CaptureEvent, String> {
    let value: Value = serde_json::from_slice(frame).map_err(error_text)?;
    let object = value
        .as_object()
        .ok_or_else(|| "capture event is not an object".to_string())?;
    if object.get("schema").and_then(Value::as_str) != Some("capture-event/1") {
        return Err("capture event schema is invalid".into());
    }
    match object.get("event").and_then(Value::as_str) {
        Some("paused") if exact_object_keys(object, &["schema", "event"]) => {
            Ok(CaptureEvent::Paused)
        }
        Some("suspended") if exact_object_keys(object, &["schema", "event"]) => {
            Ok(CaptureEvent::Suspended)
        }
        Some("resumed") if exact_object_keys(object, &["schema", "event"]) => {
            Ok(CaptureEvent::Resumed)
        }
        Some("recording") if exact_object_keys(object, &["schema", "event", "format"]) => {
            let format = object["format"]
                .as_object()
                .ok_or_else(|| "capture format is invalid".to_string())?;
            if !exact_object_keys(format, &["encoding", "sample_rate", "channels"])
                || format.get("encoding").and_then(Value::as_str) != Some("pcm_s16le")
                || format.get("sample_rate").and_then(Value::as_u64) != Some(16_000)
                || format.get("channels").and_then(Value::as_u64) != Some(1)
            {
                return Err("capture format is invalid".into());
            }
            Ok(CaptureEvent::Recording)
        }
        Some("finalized") if exact_object_keys(object, &["schema", "event", "legs"]) => {
            let legs = object["legs"]
                .as_object()
                .ok_or_else(|| "capture legs are invalid".to_string())?;
            let samples = |name: &str| -> Result<u64, String> {
                let leg = legs[name]
                    .as_object()
                    .ok_or_else(|| "capture leg is invalid".to_string())?;
                if !exact_object_keys(leg, &["samples"]) {
                    return Err("capture leg is invalid".into());
                }
                leg["samples"]
                    .as_u64()
                    .ok_or_else(|| "capture sample count is invalid".to_string())
            };
            if exact_object_keys(legs, &["mic", "system"]) {
                Ok(CaptureEvent::Finalized {
                    mic_samples: samples("mic")?,
                    system_samples: samples("system")?,
                })
            } else if exact_object_keys(legs, &["mic"]) {
                Ok(CaptureEvent::FinalizedMicOnly {
                    mic_samples: samples("mic")?,
                })
            } else {
                Err("capture legs are invalid".into())
            }
        }
        Some("failed") => {
            let valid_keys = exact_object_keys(object, &["schema", "event", "code", "detail"])
                || exact_object_keys(object, &["schema", "event", "code", "detail", "leg"]);
            let code = object.get("code").and_then(Value::as_str);
            let detail = object.get("detail").and_then(Value::as_str);
            let leg = object.get("leg").and_then(Value::as_str);
            if !valid_keys
                || code.is_none_or(|code| code.is_empty() || code.len() > 128)
                || detail.is_none_or(|detail| detail.len() > 4_096)
                || leg.is_some_and(|leg| !matches!(leg, "mic" | "system"))
            {
                return Err("capture failure event is invalid".into());
            }
            Ok(CaptureEvent::Failed {
                code: code.expect("validated code").into(),
            })
        }
        Some("interrupted") if exact_object_keys(object, &["schema", "event"]) => {
            Ok(CaptureEvent::Interrupted)
        }
        _ => Err("capture event is outside the closed schema".into()),
    }
}

fn exact_object_keys(object: &serde_json::Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
}

fn cloexec_pipe() -> io::Result<(File, File)> {
    let mut descriptors = [-1; 2];
    if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let result = set_close_on_exec(descriptors[0], true)
        .and_then(|_| set_close_on_exec(descriptors[1], true));
    if let Err(error) = result {
        unsafe {
            libc::close(descriptors[0]);
            libc::close(descriptors[1]);
        }
        return Err(error);
    }
    Ok(unsafe {
        (
            File::from_raw_fd(descriptors[0]),
            File::from_raw_fd(descriptors[1]),
        )
    })
}

fn set_close_on_exec(descriptor: RawFd, enabled: bool) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    let updated = if enabled {
        flags | libc::FD_CLOEXEC
    } else {
        flags & !libc::FD_CLOEXEC
    };
    if unsafe { libc::fcntl(descriptor, libc::F_SETFD, updated) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_nonblocking(descriptor: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn signal_process_group(process_group_id: i32, signal: i32) -> io::Result<()> {
    if unsafe { libc::kill(-process_group_id, signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

/// Wall-clock bookkeeping for one recording that may be paused.
///
/// Two numbers leave this struct and they answer different questions. The
/// recorded elapsed time is how much audio the take actually contains, and it
/// is what `capture.finalize` receives: the audio files hold nothing for a
/// paused span, and `capture-health` refuses a leg that ended materially before
/// capture stopped, so sending wall-clock time would fail any pause longer than
/// its startup-skew allowance. The pause spans are wall-clock offsets from the
/// moment recording began, which is what makes them strictly ordered.
///
/// The two reconcile: wall clock = recorded elapsed + total paused.
#[derive(Debug)]
struct PauseLedger {
    /// Recording segments already closed out, excluding any running segment.
    recorded: Duration,
    /// Pause spans already closed out.
    paused: Duration,
    /// Start of the running recording segment, or `None` while paused.
    segment_started: Option<Instant>,
    /// Start of the running pause, or `None` while recording.
    paused_since: Option<Instant>,
    /// Closed pause spans as wall-clock offsets from the start of recording.
    spans: Vec<(Duration, Duration)>,
}

impl PauseLedger {
    fn new(recording_started: Instant) -> Self {
        Self {
            recorded: Duration::ZERO,
            paused: Duration::ZERO,
            segment_started: Some(recording_started),
            paused_since: None,
            spans: Vec::new(),
        }
    }

    /// Where the wall clock stands, measured from the start of recording, with
    /// neither the running segment nor the running pause counted.
    fn closed_wall_clock(&self) -> Duration {
        self.recorded + self.paused
    }

    fn begin_pause(&mut self, now: Instant) {
        let Some(started) = self.segment_started.take() else {
            return;
        };
        self.recorded += now.saturating_duration_since(started);
        self.paused_since = Some(now);
    }

    fn end_pause(&mut self, now: Instant) {
        let Some(since) = self.paused_since.take() else {
            return;
        };
        let start = self.closed_wall_clock();
        let held = now.saturating_duration_since(since);
        self.spans.push((start, start + held));
        self.paused += held;
        self.segment_started = Some(now);
    }

    /// Closes whichever span is running. Stopping while paused is ordinary
    /// operator behavior, and that trailing pause is still time the operator
    /// held the meeting open with nothing being captured, so it is recorded.
    fn finish(&mut self, now: Instant) {
        if self.paused_since.is_some() {
            self.end_pause(now);
            self.segment_started = None;
        } else if let Some(started) = self.segment_started.take() {
            self.recorded += now.saturating_duration_since(started);
        }
    }

    fn recorded_elapsed(&self) -> Duration {
        self.recorded
    }

    /// The pause record for the capture receipt, or `None` when the recording
    /// was never paused. A capture with no gap writes the receipt it wrote
    /// before pause existed rather than a field asserting an absence.
    fn receipt(&self) -> Result<Option<Value>, String> {
        if self.spans.is_empty() {
            return Ok(None);
        }
        if self.spans.len() > MAX_PAUSE_SPANS {
            return Err("the recording was paused more times than a receipt can record".into());
        }
        let mut spans = Vec::with_capacity(self.spans.len());
        for (start, end) in &self.spans {
            spans.push(json!({
                "paused_at_samples": pause_offset_samples(*start)?,
                "resumed_at_samples": pause_offset_samples(*end)?,
            }));
        }
        Ok(Some(json!({"schema": "capture-pauses/1", "spans": spans})))
    }
}

/// The largest number of pause spans one capture receipt may carry. The worker
/// enforces the same bound at the persistence boundary; this one keeps the
/// application from building a request it knows will be refused.
const MAX_PAUSE_SPANS: usize = 64;

/// A wall-clock offset in samples. Unlike `elapsed_samples` this accepts zero,
/// because the first pause of a recording legitimately begins at offset zero.
fn pause_offset_samples(offset: Duration) -> Result<u64, String> {
    let samples = offset.as_secs_f64() * 16_000.0;
    let maximum = (16_000 * 60 * 60 * 24) as f64;
    if !samples.is_finite() || samples < 0.0 || samples > maximum {
        return Err("a pause offset is outside the supported range".into());
    }
    Ok(samples.round() as u64)
}

fn elapsed_samples(duration: Duration) -> Result<u64, String> {
    let samples = duration.as_secs_f64() * 16_000.0;
    let maximum = (16_000 * 60 * 60 * 24) as f64;
    if !samples.is_finite() || samples < 1.0 || samples > maximum {
        return Err("capture elapsed time is outside the supported range".into());
    }
    Ok(samples.round() as u64)
}

fn capture_user_message(code: &str) -> &'static str {
    match code {
        "microphone_permission_denied" => {
            "Microphone access was not granted. Nothing was marked complete."
        }
        "system_tap_setup_failed" | "system_tap_unavailable" | "system_tap_start_failed" => {
            "System-audio access was unavailable. Nothing was marked complete."
        }
        _ => "An audio channel failed. Nothing was marked complete.",
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn io_error(error: io::Error) -> Box<dyn std::error::Error> {
    Box::new(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meeting_open_for_reading_does_not_block_model_changes() {
        assert!(model_change_audio_idle(CaptureState::Idle, false));
        assert!(model_change_audio_idle(CaptureState::TranscriptReady, false));
        assert!(!model_change_audio_idle(CaptureState::Recording, false));
        assert!(!model_change_audio_idle(CaptureState::Paused, false));
        assert!(!model_change_audio_idle(CaptureState::Transcribing, false));
        assert!(!model_change_audio_idle(CaptureState::TranscriptReady, true));
    }
    use std::sync::Barrier;
    use tempfile::TempDir;

    /// W6-B: the Reopen decision itself can be unit-tested headlessly even
    /// though the event only ever arrives from a live `NSApplicationDelegate`
    /// callback (a real Dock click, which this suite cannot drive). This
    /// pins the one thing that logic is required to get right: don't front a
    /// window that's already visible (which includes Settings being open),
    /// only recover the case the audit found -- zero visible windows.
    #[test]
    fn reopen_shows_the_window_only_when_none_are_visible() {
        assert!(should_show_on_reopen(false));
        assert!(!should_show_on_reopen(true));
    }

    /// Roadmap intake I5, decision 4, verbatim. The sentence is the packet's
    /// honesty burden on the machine that cannot run the check: the lock does
    /// not open, and the reader is told which of the two failures happened.
    #[test]
    fn the_unavailable_confirmation_sentence_is_the_one_the_packet_requires() {
        assert_eq!(
            CONFIRMATION_UNAVAILABLE_MESSAGE,
            "This Mac cannot confirm it's you (no Touch ID or password available). The lock stays on."
        );
        // A decline is a different fact and gets a different sentence. Telling
        // someone who cancelled that their Mac lacks the hardware would send
        // them to buy what they already own.
        assert_ne!(
            CONFIRMATION_DECLINED_MESSAGE,
            CONFIRMATION_UNAVAILABLE_MESSAGE
        );
        assert_eq!(
            CONFIRMATION_DECLINED_MESSAGE,
            "Yawn could not confirm it's you. This meeting stays locked."
        );
    }

    /// The governing constraint says to state the honest claim -- a
    /// local-access deterrent -- unless encryption at rest actually ships. It
    /// has not. No native lock string may claim otherwise.
    #[test]
    fn no_native_lock_sentence_claims_encryption_or_security() {
        for sentence in [
            CONFIRMATION_UNAVAILABLE_MESSAGE,
            CONFIRMATION_DECLINED_MESSAGE,
            library_reader::LOCKED_MESSAGE,
            UNLOCK_CONFIRMATION_REASON,
            meeting_lock::LockedAction::Open.reason(),
            meeting_lock::LockedAction::Export.reason(),
            meeting_lock::LockedAction::Playback.reason(),
        ] {
            let lowered = sentence.to_lowercase();
            for forbidden in ["encrypt", "secure", "protected"] {
                assert!(!lowered.contains(forbidden), "{sentence}");
            }
        }
    }

    /// Each prompt sentence names the act the operator just asked for.
    ///
    /// macOS renders these inside its own panel, so a mismatch is a lie on
    /// screen: someone who clicked "Remove lock" being told Yawn wants to
    /// "open a locked meeting" cannot tell what they are approving. The
    /// forbidden-word test above cannot see this -- it checks what the
    /// sentences must not claim, and this checks that they describe the right
    /// act.
    #[test]
    fn every_confirmation_prompt_names_the_act_it_was_raised_for() {
        assert!(UNLOCK_CONFIRMATION_REASON.contains("remove the lock"));
        assert!(meeting_lock::LockedAction::Open.reason().contains("open"));
        assert!(meeting_lock::LockedAction::Export.reason().contains("export"));
        assert!(meeting_lock::LockedAction::Playback.reason().contains("play"));
        // Removing a lock is not one of the three gated actions and must not
        // borrow one of their sentences.
        for action in [
            meeting_lock::LockedAction::Open,
            meeting_lock::LockedAction::Export,
            meeting_lock::LockedAction::Playback,
        ] {
            assert_ne!(UNLOCK_CONFIRMATION_REASON, action.reason());
        }
        // And the command that removes a lock uses it. Read from source
        // because the prompt itself cannot run here.
        let source = include_str!("main.rs");
        let start = source.find("fn unlock_meeting(").unwrap();
        assert!(
            source[start..start + 2_600].contains("confirm_operator(&state, UNLOCK_CONFIRMATION_REASON)")
        );
    }

    /// The gate in front of export and of opening the transcript file
    /// natively -- two of this packet's four refusal points, and the two that
    /// put a locked meeting's contents somewhere the lock does not reach.
    ///
    /// `refuse_locked_meeting` is where both of them make the decision, so it
    /// is tested directly rather than through the commands, which cannot be
    /// invoked without a Tauri runtime.
    #[test]
    fn export_and_transcript_file_refuse_a_locked_meeting_without_a_confirmation() {
        let temporary = TempDir::new().unwrap();
        let protected = temporary.path().join("protected");
        local_meeting_notes_session_core::storage::create_private_dir(&protected).unwrap();
        let storage =
            StorageRoot::create(&temporary.path().join("app-data"), &protected).unwrap();
        let meeting_id = "11111111-1111-4111-8111-111111111111";
        let directory = storage.path().join("meetings").join(meeting_id);
        local_meeting_notes_session_core::storage::create_private_dir(&directory).unwrap();

        // Decision 3: the unlocked default path takes no friction, with or
        // without a token in hand.
        assert!(refuse_locked_meeting(&storage, meeting_id, None).is_ok());

        meeting_lock::write(&directory, true).unwrap();
        // Refuses without a confirmation, with the sentence every locked
        // refusal uses.
        assert_eq!(
            refuse_locked_meeting(&storage, meeting_id, None).unwrap_err(),
            library_reader::LOCKED_MESSAGE
        );
        // Refuses a confirmation minted for a different meeting. This is what
        // stops a token the operator gave for a meeting they may read from
        // exporting one they may not.
        assert!(refuse_locked_meeting(&storage, meeting_id, Some("another-meeting")).is_err());
        // Allows the meeting its own confirmation names.
        assert!(refuse_locked_meeting(&storage, meeting_id, Some(meeting_id)).is_ok());

        // A meeting whose directory cannot be resolved is not reported as
        // locked: the surrounding command's own failure is the honest answer,
        // and a lock message would name a barrier that is not what stopped it.
        // `..` is what `StorageRoot::resolve` refuses, which is the only way
        // to reach that branch from here.
        assert!(
            meeting_dir(&storage, "../escape").is_err(),
            "this id must be the one storage refuses, or the branch below is untested"
        );
        assert!(refuse_locked_meeting(&storage, "../escape", None).is_ok());
        // And a meeting that simply has no lock file is not locked either --
        // the ordinary case, reached through the other branch.
        assert!(refuse_locked_meeting(&storage, "not-a-meeting", None).is_ok());
    }

    /// The three commands run the device-owner check with no app lock held.
    ///
    /// The prompt blocks on a person, so confirming while holding
    /// `command_lock`, the meeting-storage sequence, or the library mutex
    /// would stall every other command behind a panel that may never be
    /// answered. This reads the source rather than exercising it because the
    /// real prompt cannot run in a test: each command resolves its meeting in
    /// a scope that ends, and only then confirms.
    #[test]
    fn the_confirmation_prompt_is_never_raised_while_a_lock_is_held() {
        let source = include_str!("main.rs");
        for command in ["fn unlock_meeting(", "fn authorize_locked_action("] {
            let start = source.find(command).expect(command);
            let body = &source[start..start + 2_600];
            // The library read is what takes the meeting-storage sequence and
            // the library mutex, and `command_lock` is taken in the same
            // scope. Its enclosing block must close -- releasing all three --
            // strictly before the prompt goes up.
            let resolve = body
                .find("state\n            .with_preview_library(")
                .or_else(|| body.find("state.with_preview_library("))
                .expect("the command must resolve its meeting through the library");
            let resolve_end = resolve
                + body[resolve..]
                    .find("\n    };\n")
                    .expect("the resolving scope must close");
            let confirm = body
                .find("confirm_operator(&state")
                .expect("the command must confirm");
            assert!(
                resolve < resolve_end && resolve_end < confirm,
                "{command} confirms while still holding its resolving scope"
            );
        }
    }

    fn valid_attestation() -> StartAttestation {
        StartAttestation {
            participants_consented: true,
            headphones: true,
            operator_alone: true,
        }
    }

    pub(crate) fn test_storage() -> (TempDir, StorageRoot) {
        let temporary = TempDir::new().unwrap();
        let repository = temporary.path().join("repository");
        fs::create_dir(&repository).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &repository).unwrap();
        (temporary, storage)
    }

    #[test]
    fn retained_audio_stdin_handoff_survives_exec_with_close_on_exec_input() {
        let temporary = TempDir::new().unwrap();
        let wav = temporary.path().join("synthetic.wav");
        // A minimal synthetic WAV prefix is enough for this spawn-level test;
        // no meeting recording is read or played.
        fs::write(&wav, b"RIFF\x24\0\0\0WAVEfmt ").unwrap();
        let file = File::open(wav).unwrap();
        let input = file.try_clone().unwrap();
        set_close_on_exec(input.as_raw_fd(), true).unwrap();
        let output = Command::new("/bin/sh")
            .args(["-c", "dd bs=4 count=1 </dev/stdin 2>/dev/null"])
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"RIFF");
    }

    #[test]
    fn scheduled_retention_defers_active_playback_then_releases_after_stop() {
        let (_temporary, storage) = test_storage();
        let meeting_id = "due-playback";
        write_transcript_fixture(&storage, meeting_id, 0, AudioState::Retained, "synthetic");
        let directory = meeting_dir(&storage, meeting_id).unwrap();
        let mut meeting = load_meeting(&directory).unwrap();
        meeting.retention.next_deletion_at_epoch_seconds = Some(1);
        write_meeting(&directory, &meeting).unwrap();

        let state = ApplicationState::default();
        *state.app_data_writer_lock.lock().unwrap() =
            Some(Arc::new(acquire_app_data_writer_lock(&storage).unwrap()));
        let child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        *state.audio_playback.lock().unwrap() = Some(RetainedAudioPlayback {
            child,
            source: library_reader::RetainedAudioSource::Microphone,
        });

        assert_eq!(
            execute_scheduled_retention(&state, &storage, 1).unwrap(),
            None
        );
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        assert!(directory.join("capture/mic.wav").exists());
        assert!(directory.join("capture/system.wav").exists());
        assert_eq!(
            load_meeting(&directory).unwrap().retention.state,
            AudioState::Retained
        );

        stop_owned_audio_playback(&state);
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));

        let report = execute_scheduled_retention(&state, &storage, 1)
            .unwrap()
            .unwrap();

        assert_eq!(
            report.retention,
            vec![RetentionOutcome::AudioReleased(meeting_id.into())]
        );
        assert!(report.trash_purge.is_empty());
        assert!(state.audio_playback.lock().unwrap().is_none());
        assert!(!directory.join("capture/mic.wav").exists());
        assert!(!directory.join("capture/system.wav").exists());
    }

    #[test]
    fn owned_audio_playback_reports_completion_once_then_reaps_its_child() {
        let state = ApplicationState::default();
        let child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        *state.audio_playback.lock().unwrap() = Some(RetainedAudioPlayback {
            child,
            source: library_reader::RetainedAudioSource::Microphone,
        });
        // The child's exit time is scheduler-dependent, so poll: every status
        // before the exit is observed must read "playing", the observing one
        // "completed", and only then may the slot report "idle".
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match owned_audio_playback_status(&state).state {
                "completed" => break,
                "playing" => {
                    assert!(Instant::now() < deadline, "playback child never exited");
                    std::thread::sleep(Duration::from_millis(5));
                }
                other => panic!("completion may only be reported once, got {other:?}"),
            }
        }
        assert_eq!(owned_audio_playback_status(&state).state, "idle");
    }

    fn storage_tree_bytes(path: &Path) -> Vec<(String, Vec<u8>)> {
        fn visit(root: &Path, path: &Path, entries: &mut Vec<(String, Vec<u8>)>) {
            let mut children = fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                if child.is_dir() {
                    visit(root, &child, entries);
                } else {
                    entries.push((
                        child.strip_prefix(root).unwrap().display().to_string(),
                        fs::read(&child).unwrap(),
                    ));
                }
            }
        }
        let mut entries = Vec::new();
        visit(path, path, &mut entries);
        entries
    }

    /// Preserved legacy bytes and an enrolled profile are both "present", and
    /// the operator consequence is opposite. The snapshot must carry the
    /// lifecycle's own activation fact rather than inferring it from presence,
    /// because the migration path's entire promise is that Preview did not
    /// activate what it found.
    #[test]
    fn preserved_and_absent_profiles_report_activation_separately_from_presence() {
        let (_temporary, storage) = test_storage();
        let legacy = b"stored-profile-sentinel";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.preview_profile.lock().unwrap() =
            PreviewProfileSnapshot::lifecycle_unreadable("migration-review-required");
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }

        let preserved = preview_profile_preserve_legacy_for(&state).unwrap();
        assert_eq!(preserved.profile_present, Some(true));
        assert_eq!(
            preserved.profile_active,
            Some(false),
            "preserve-first migration must never report the bytes it preserved as active"
        );
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );

        let after_reset = preview_profile_reset_for(&state, true).unwrap();
        assert_eq!(after_reset.profile_present, Some(false));
        assert_eq!(after_reset.profile_active, Some(false));

        // An unread lifecycle knows neither fact, and neither may be guessed.
        let unreadable = PreviewProfileSnapshot::lifecycle_unreadable("needs-attention");
        assert_eq!(unreadable.profile_present, None);
        assert_eq!(unreadable.profile_active, None);
        assert_eq!(PreviewProfileSnapshot::unavailable().profile_active, None);
    }

    /// The Settings surface receives the guided-enrolment shortfall in the
    /// terms the capture gate enforces. No sitting recorder exists yet, so the
    /// honest answer is `blocked` with the first enforced step — never a
    /// completion share, and never a claim that setup can start.
    #[test]
    fn profile_snapshot_carries_content_free_guided_enrollment_guidance() {
        let snapshot = PreviewProfileSnapshot::baseline(false, false);
        let guidance = &snapshot.guided_enrollment;
        assert_eq!(
            guidance.state,
            local_meeting_notes_session_core::enrollment_guidance::GuidedEnrollmentState::Blocked
        );
        assert_eq!(guidance.sittings_recorded, 0);
        assert!(guidance.next_step.is_some());
        assert!(
            !guidance.gates.is_empty(),
            "the surface must always carry what this evaluation cannot decide"
        );

        let rendered = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(rendered["profileActive"], serde_json::json!(false));
        assert_eq!(rendered["guidedEnrollment"]["state"], "blocked");
        // Nothing in the delivered payload may read as a progress bar.
        let payload = serde_json::to_string(&rendered).unwrap();
        assert!(!payload.contains('%'), "{payload}");
    }

    #[test]
    fn profile_preview_caches_authoritative_lifecycle_state() {
        let (_fresh_temporary, fresh) = test_storage();
        let fresh_state = ApplicationState::default();
        ensure_app_data_writer_lock(&fresh_state, &fresh).unwrap();
        reconcile_preview_profile_lifecycle(&fresh_state).unwrap();
        assert_eq!(
            preview_profile_snapshot_for(&fresh_state),
            PreviewProfileSnapshot::baseline(false, false)
        );

        let initialized_tree = storage_tree_bytes(fresh.path());
        reconcile_preview_profile_lifecycle(&fresh_state).unwrap();
        assert_eq!(storage_tree_bytes(fresh.path()), initialized_tree);
        assert_eq!(
            preview_profile_snapshot_for(&fresh_state),
            PreviewProfileSnapshot::baseline(false, false)
        );

        let (_legacy_temporary, legacy) = test_storage();
        durable_create_new(
            &legacy.path().join("profile/voiceprint.json"),
            b"stored-profile-sentinel",
        )
        .unwrap();
        let legacy_state = ApplicationState::default();
        ensure_app_data_writer_lock(&legacy_state, &legacy).unwrap();
        let legacy_tree = storage_tree_bytes(legacy.path());
        reconcile_preview_profile_lifecycle(&legacy_state).unwrap();
        assert_eq!(storage_tree_bytes(legacy.path()), legacy_tree);
        assert_eq!(
            preview_profile_snapshot_for(&legacy_state),
            PreviewProfileSnapshot::lifecycle_unreadable("migration-review-required")
        );

        let (_unsafe_temporary, unsafe_storage) = test_storage();
        std::os::unix::fs::symlink(
            unsafe_storage.path().join("elsewhere"),
            unsafe_storage.path().join("profile/voiceprint.json"),
        )
        .unwrap();
        let unsafe_state = ApplicationState::default();
        ensure_app_data_writer_lock(&unsafe_state, &unsafe_storage).unwrap();
        reconcile_preview_profile_lifecycle(&unsafe_state).unwrap();
        assert_eq!(
            preview_profile_snapshot_for(&unsafe_state),
            PreviewProfileSnapshot::lifecycle_unreadable("needs-attention")
        );
    }

    /// With recorded evidence and no runtime identity there is no verified
    /// encoder digest to evaluate against, and guessing would let a uniformly
    /// stale checkpoint read as a working choice screen. The snapshot refuses.
    #[test]
    fn recorded_evidence_without_runtime_identity_refuses_to_evaluate() {
        use local_meeting_notes_session_core::sitting_evidence::{SegmentSpan, SittingKind};
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let sitting_id = "6f7dce4e-93a9-4c05-a9c6-9c1f60ec2c68";
        let spans: Vec<SegmentSpan> = (0..5)
            .map(|index| SegmentSpan {
                start_seconds: index as f64 * 4.0,
                end_seconds: index as f64 * 4.0 + 3.0,
            })
            .collect();
        {
            let held = state.app_data_writer_lock.lock().unwrap();
            let authority = held.as_ref().unwrap().sitting_evidence_authority();
            authority
                .begin_sitting(sitting_id, SittingKind::OperatorSitting, None, 1_000)
                .unwrap();
            authority
                .append_raw_audio(sitting_id, b"synthetic fixture audio bytes")
                .unwrap();
            authority.finalize_capture(sitting_id, 1_100).unwrap();
            authority.record_segments(sitting_id, &spans).unwrap();
        }
        reconcile_preview_profile_lifecycle(&state).unwrap();
        assert_eq!(
            preview_profile_snapshot_for(&state),
            PreviewProfileSnapshot::lifecycle_unreadable("needs-attention")
        );
    }

    /// Once the runtime identity supplies the manifest's encoder digest, the
    /// stored evidence reaches the snapshot's guidance: a raw-retained sitting
    /// reports app-side derivation work, and a saved sitting counts.
    #[test]
    fn sitting_evidence_reaches_snapshot_guidance_with_manifest_encoder() {
        use local_meeting_notes_session_core::sitting_evidence::{SegmentSpan, SittingKind};
        let encoder = "0575cb64845e6b9a10db9bcb74d5ac32b326b8dc90352671d345e2ee3d0126a2";
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.runtime.lock().unwrap() = Some(RuntimeIdentity {
            admission: "internal-alpha".into(),
            worker_build_sha256: "worker-build".into(),
            worker_executable_sha256: "worker-executable".into(),
            transcript_model_identity: "model/v1".into(),
            tap_build_sha256: "tap-build".into(),
            tap_path: PathBuf::from("/nonexistent/tap"),
            packaged_models: Vec::new(),
            encoder_sha256: encoder.into(),
            encoder_available: false,
        });
        let sitting_id = "6f7dce4e-93a9-4c05-a9c6-9c1f60ec2c68";
        let spans: Vec<SegmentSpan> = (0..5)
            .map(|index| SegmentSpan {
                start_seconds: index as f64 * 4.0,
                end_seconds: index as f64 * 4.0 + 3.0,
            })
            .collect();
        {
            let held = state.app_data_writer_lock.lock().unwrap();
            let authority = held.as_ref().unwrap().sitting_evidence_authority();
            authority
                .begin_sitting(sitting_id, SittingKind::OperatorSitting, None, 1_000)
                .unwrap();
            authority
                .append_raw_audio(sitting_id, b"synthetic fixture audio bytes")
                .unwrap();
            authority.finalize_capture(sitting_id, 1_100).unwrap();
            authority.record_segments(sitting_id, &spans).unwrap();
        }
        reconcile_preview_profile_lifecycle(&state).unwrap();
        let raw_retained = preview_profile_snapshot_for(&state);
        assert_eq!(raw_retained.state, "baseline-ready");
        // `sittings_recorded` counts every projected sitting; the derivation
        // ledger is what distinguishes raw-retained from saved.
        assert_eq!(
            raw_retained.guided_enrollment.sittings_awaiting_derivation,
            1
        );
        assert_eq!(raw_retained.guided_enrollment.sittings_recorded, 1);

        // The recorder surface carries the same store read: one raw-retained
        // sitting, recording honestly unavailable because this runtime's
        // encoder is the placeholder.
        let surface = preview_enrollment_surface_for(&state);
        assert!(!surface.recording_available);
        assert_eq!(
            surface.recording_unavailable_reason,
            Some(RECORDER_REASON_NO_ENCODER)
        );
        assert_eq!(surface.sittings.len(), 1);
        assert_eq!(surface.sittings[0].kind, "operator-sitting");
        assert_eq!(surface.sittings[0].state, "raw-retained");
        let rendered = serde_json::to_value(&surface).unwrap();
        assert_eq!(rendered["sittings"][0]["state"], "raw-retained");
        assert_eq!(rendered["recordingAvailable"], serde_json::json!(false));
        let payload = serde_json::to_string(&rendered).unwrap();
        assert!(!payload.contains('%'), "{payload}");

        {
            let held = state.app_data_writer_lock.lock().unwrap();
            let authority = held.as_ref().unwrap().sitting_evidence_authority();
            authority
                .store_derived_material(
                    sitting_id,
                    &vec![7_u8; 5 * 192 * 4],
                    encoder,
                    Some("onnx-artifact-digest"),
                    192,
                    1_200,
                )
                .unwrap();
        }
        reconcile_preview_profile_lifecycle(&state).unwrap();
        let saved = preview_profile_snapshot_for(&state);
        assert_eq!(saved.state, "baseline-ready");
        assert_eq!(saved.guided_enrollment.sittings_recorded, 1);
        assert_eq!(saved.guided_enrollment.sittings_awaiting_derivation, 0);
        let surface = preview_enrollment_surface_for(&state);
        assert_eq!(surface.sittings[0].state, "saved");
    }

    /// Before the worker spawn there is no verified runtime identity; the
    /// recorder surface says so instead of guessing about the encoder. A
    /// reconcile that refuses must also leave the surface unavailable rather
    /// than retaining a stale sittings list.
    #[test]
    fn recorder_surface_reports_runtime_unknown_and_resets_on_refusal() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        reconcile_preview_profile_lifecycle(&state).unwrap();
        let surface = preview_enrollment_surface_for(&state);
        assert!(!surface.recording_available);
        assert_eq!(
            surface.recording_unavailable_reason,
            Some(RECORDER_REASON_RUNTIME_UNKNOWN)
        );
        assert!(surface.sittings.is_empty());

        // Poison the lifecycle by dropping the writer lock: reconcile now
        // refuses, and the cached surface must fall back to unavailable.
        *state.app_data_writer_lock.lock().unwrap() = None;
        assert!(reconcile_preview_profile_lifecycle(&state).is_err());
        let surface = preview_enrollment_surface_for(&state);
        assert_eq!(
            surface.recording_unavailable_reason,
            Some(RECORDER_REASON_STATUS_UNAVAILABLE)
        );
    }

    /// § A's load-bearing rule: recording and degraded are distinguishable
    /// at a glance — the filled glyph gains a persistent mark — and the
    /// filled glyph appears only while capture is actually live. Failure
    /// states never render the live glyph.
    #[test]
    fn tray_states_never_read_as_silently_recording() {
        use StartupState::*;
        assert_eq!(tray_presentation(Ready, CaptureState::Idle, false).0, "○");
        assert_eq!(
            tray_presentation(Ready, CaptureState::Recording, false).0,
            "●"
        );
        assert_eq!(
            tray_presentation(Ready, CaptureState::Recording, true).0,
            "●!"
        );
        assert_eq!(
            tray_presentation(Ready, CaptureState::Stopping, true).0,
            "●!"
        );
        assert_eq!(
            tray_presentation(Ready, CaptureState::Transcribing, false).0,
            "◐"
        );
        // Captured is after Stopping: committed, not recording. The filled
        // glyph there would be the exact inversion § A forbids.
        assert_eq!(
            tray_presentation(Ready, CaptureState::Captured, false).0,
            "◐"
        );
        assert_eq!(
            tray_presentation(Ready, CaptureState::TranscriptionFailed, false).0,
            "×"
        );
        // SummaryFailed persists until an explicit retry; an idle glyph
        // would hide a standing failure in exactly the menubar-only
        // sessions § A exists for.
        assert_eq!(
            tray_presentation(Ready, CaptureState::SummaryFailed, false).0,
            "×"
        );
        assert_eq!(
            tray_presentation(Ready, CaptureState::RecoveredInterrupted, false).0,
            "×"
        );
        assert_eq!(
            tray_presentation(Checking, CaptureState::Idle, false).0,
            "○"
        );
        assert_eq!(
            tray_presentation(RuntimeMissing, CaptureState::Idle, false).0,
            "×"
        );
        // Arming is consent preparation, not capture: the glyph stays
        // hollow and the words say nothing is recording yet.
        let (glyph, words) = tray_presentation(Ready, CaptureState::Arming, false);
        assert_eq!(glyph, "○");
        assert!(words.contains("Nothing is recording yet"));
        // Every non-live, non-finished state says outright that nothing is
        // recording, or names the attention it needs.
        for capture in [CaptureState::Idle, CaptureState::Paused] {
            let (glyph, _) = tray_presentation(Ready, capture, false);
            assert_eq!(glyph, "○");
        }
    }

    /// A finished meeting the operator has not opened yet gets its own
    /// glyph and tooltip — distinct from idle ("nothing is recording", true
    /// but buries the waiting note) and from "◐" (still in progress).
    /// Changed from the prior pinning of TranscriptReady to the idle arm:
    /// old ("○", "Nothing is recording") → new ("◍", "Your meeting is ready
    /// to read.").
    #[test]
    fn finished_meeting_gets_a_ready_glyph_not_idle() {
        for capture in [CaptureState::TranscriptReady, CaptureState::Ready] {
            let (glyph, words) = tray_presentation(StartupState::Ready, capture, false);
            assert_eq!(glyph, "◍");
            assert_eq!(words, "Your meeting is ready to read.");
        }
    }

    /// First-run model selection is a one-time setup step, not a failure.
    /// Before this fix it fell into the catch-all "×" / "The app needs
    /// attention" arm alongside RuntimeMissing and ServiceTimeout. Changed:
    /// old ("×", "The app needs attention. Nothing is recording") → new
    /// ("◌", "One-time setup: choose a speech model.").
    #[test]
    fn model_required_gets_a_calm_setup_glyph_not_a_failure_mark() {
        let (glyph, words) =
            tray_presentation(StartupState::ModelRequired, CaptureState::Idle, false);
        assert_eq!(glyph, "◌");
        assert_eq!(words, "One-time setup: choose a speech model.");
    }

    /// Genuine startup failures keep the alarming mark — only ModelRequired
    /// was carved out.
    #[test]
    fn genuine_startup_failures_still_read_as_failures() {
        for startup in [
            StartupState::RuntimeMissing,
            StartupState::ServiceTimeout,
            StartupState::DiagnosticWritten,
            StartupState::ReinstallRequired,
        ] {
            let (glyph, _) = tray_presentation(startup, CaptureState::Idle, false);
            assert_eq!(glyph, "×");
        }
    }

    /// W8-A: "Stop recording" is present in the tray menu for exactly the
    /// two states `stop_meeting` itself accepts — no more, no less. Every
    /// other `CaptureState` (including `Stopping`, `Arming`, and the two
    /// failure/recovery states) must read false, or the tray would offer a
    /// stop that the command would then refuse.
    #[test]
    fn tray_shows_stop_recording_only_while_live() {
        assert!(tray_shows_stop_recording(CaptureState::Recording));
        assert!(tray_shows_stop_recording(CaptureState::Paused));
        for capture in [
            CaptureState::Idle,
            CaptureState::Arming,
            CaptureState::Stopping,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::TranscriptReady,
            CaptureState::Summarizing,
            CaptureState::Ready,
            CaptureState::TranscriptionFailed,
            CaptureState::SummaryFailed,
            CaptureState::RecoveredInterrupted,
        ] {
            assert!(
                !tray_shows_stop_recording(capture),
                "expected no Stop item for {capture:?}"
            );
        }
    }

    /// The exact event names the frontend listens for (see the brief's
    /// window-chrome packet): a typo here would silently strand a menu
    /// command with no listener, which the frontend side has no way to
    /// detect on its own.
    #[test]
    fn menu_event_names_match_the_frontend_contract() {
        assert_eq!(menu_event_name("new-recording"), Some("menu:new-recording"));
        assert_eq!(menu_event_name("stop-recording"), Some("menu:stop"));
        assert_eq!(menu_event_name("toggle-sidebar"), Some("menu:toggle-sidebar"));
        assert_eq!(
            menu_event_name("open-transcript"),
            Some("menu:open-transcript")
        );
        // "open-settings" is handled before `menu_event_name` is ever
        // called (it drives `show_settings_window` directly) and must not
        // also be wired as an event — a menu item id that means neither
        // must resolve to nothing, not a stray event the frontend never
        // asked for.
        assert_eq!(menu_event_name("open-settings"), None);
        assert_eq!(menu_event_name("unknown-id"), None);
    }

    /// Root-cause regression test for the Settings "Checking speech model"
    /// freeze: `cached_verified_manifest` must hand back the *same* verified
    /// manifest on a second call for the same path, not re-verify from disk.
    /// `Arc::ptr_eq` is the proof — a fresh `RuntimeManifest::load_and_verify`
    /// would produce an equal-looking but distinct value, and this asserts
    /// identity, not equality (the struct has no `PartialEq` to fall back on
    /// anyway).
    #[test]
    fn cached_verified_manifest_reuses_the_same_verified_value() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path();
        let write_resource = |name: &str, bytes: &[u8]| -> String {
            fs::write(root.join(name), bytes).unwrap();
            sha256_file(&root.join(name)).unwrap()
        };
        let runtime = write_resource("runtime", b"runtime");
        let worker = write_resource("main.py", b"worker source");
        let tap = write_resource("tap", b"tap");
        let encoder = write_resource("encoder-unavailable.identity", b"no-encoder");
        let probe = write_resource("permission-probe", b"probe");
        let manifest_path = root.join("app-runtime.json");
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema": "app-runtime/1",
                "admission": "internal-alpha",
                "runtime": {"path": "runtime", "sha256": runtime},
                "worker": {"path": "main.py", "sha256": worker},
                "tap": {"path": "tap", "sha256": tap},
                "encoder": {"path": "encoder-unavailable.identity", "sha256": encoder},
                "permission_probe": {"path": "permission-probe", "sha256": probe},
                "models": [],
            })
            .to_string(),
        )
        .unwrap();

        let state = ApplicationState::default();
        let first = cached_verified_manifest(&state, &manifest_path).unwrap();
        let second = cached_verified_manifest(&state, &manifest_path).unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "expected the cached manifest to be reused, not re-verified"
        );
        assert_eq!(state.verified_manifest_cache.lock().unwrap().len(), 1);
    }

    /// The verified manifest carrying the admitted encoder is what opens the
    /// recorder — the same signal that closes it on the placeholder lane —
    /// and an active take closes it again with its own named reason.
    #[test]
    fn recording_opens_with_the_admitted_encoder_and_closes_during_a_take() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.runtime.lock().unwrap() = Some(RuntimeIdentity {
            admission: "internal-alpha".into(),
            worker_build_sha256: "worker-build".into(),
            worker_executable_sha256: "worker-executable".into(),
            transcript_model_identity: "model/v1".into(),
            tap_build_sha256: "tap-build".into(),
            tap_path: PathBuf::from("/nonexistent/tap"),
            packaged_models: Vec::new(),
            encoder_sha256: "0575cb64845e6b9a10db9bcb74d5ac32b326b8dc90352671d345e2ee3d0126a2"
                .into(),
            encoder_available: true,
        });
        reconcile_preview_profile_lifecycle(&state).unwrap();
        let surface = preview_enrollment_surface_for(&state);
        assert!(surface.recording_available);
        assert_eq!(surface.recording_unavailable_reason, None);
        let rendered = serde_json::to_value(&surface).unwrap();
        assert_eq!(rendered["recordingAvailable"], serde_json::json!(true));
        assert_eq!(rendered["lastOutcome"], serde_json::Value::Null);

        // Claim the take: the surface refuses a second start with the
        // in-progress reason, not the encoder ladder.
        let (sender, _receiver) = mpsc::channel();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
        }
        claim_sitting_start(
            &state,
            "11111111-1111-4111-8111-111111111111",
            "operator-sitting",
            None,
            sender,
        )
        .unwrap();
        // The claim itself writes the optimistic projection — before any
        // thread exists — so a fast-failing thread can never be overwritten
        // by a later command-side write.
        let claimed = preview_enrollment_surface_for(&state);
        assert!(claimed.attempt_active);
        assert_eq!(
            claimed.recording_unavailable_reason,
            Some(RECORDER_REASON_SITTING_ACTIVE)
        );
        assert!(
            claimed
                .sittings
                .iter()
                .any(|sitting| sitting.state == "recording-in-progress")
        );
        let (second, _second_receiver) = mpsc::channel();
        assert_eq!(
            claim_sitting_start(
                &state,
                "22222222-2222-4222-8222-222222222222",
                "operator-sitting",
                None,
                second,
            )
            .unwrap_err(),
            RECORDER_REASON_SITTING_ACTIVE
        );

        // A panicking thread's guard clears the slot and strips the
        // optimistic projection, so the surface never claims a take that no
        // longer exists and the gated commands are not refused forever.
        drop(SittingTaskGuard {
            state: &state,
            sitting_id: "11111111-1111-4111-8111-111111111111",
            completed: false,
        });
        assert!(!sitting_task_active(&state));
        let sanitized = preview_enrollment_surface_for(&state);
        assert!(!sanitized.attempt_active);
        assert_eq!(sanitized.last_outcome, Some(SITTING_OUTCOME_REHEARSAL));
        assert!(
            sanitized
                .sittings
                .iter()
                .all(|sitting| sitting.state != "recording-in-progress")
        );
    }

    /// The start boundary is a closed vocabulary: unknown kinds, an unnamed
    /// or impermissible comparison source, and a source on a voice session
    /// are all refused before anything is spawned or stored.
    #[test]
    fn sitting_request_vocabulary_is_closed() {
        use local_meeting_notes_session_core::sitting_evidence::SittingKind;
        assert_eq!(
            parse_sitting_request("operator-sitting", None).unwrap(),
            (SittingKind::OperatorSitting, None)
        );
        assert_eq!(
            parse_sitting_request("negative-source", Some("public-or-licensed")).unwrap(),
            (
                SittingKind::NegativeSource,
                Some("public-or-licensed".to_string())
            )
        );
        assert_eq!(
            parse_sitting_request("negative-source", Some("consenting-person")).unwrap(),
            (
                SittingKind::NegativeSource,
                Some("consenting-person".to_string())
            )
        );
        assert!(parse_sitting_request("operator-sitting", Some("public-or-licensed")).is_err());
        assert!(parse_sitting_request("negative-source", None).is_err());
        assert!(parse_sitting_request("negative-source", Some("someone-nearby")).is_err());
        assert!(parse_sitting_request("meeting", None).is_err());
    }

    /// Start refuses each boundary in the ladder's own terms, and Stop
    /// refuses when no take is active — the operator is never left signaling
    /// a thread that does not exist.
    #[test]
    fn sitting_start_and_stop_refuse_their_boundaries() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();

        // Startup not Ready.
        let (sender, _receiver) = mpsc::channel();
        assert!(
            claim_sitting_start(
                &state,
                "11111111-1111-4111-8111-111111111111",
                "operator-sitting",
                None,
                sender,
            )
            .unwrap_err()
            .contains("installation check")
        );
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
        }

        // No verified runtime identity yet.
        let (sender, _receiver) = mpsc::channel();
        assert_eq!(
            claim_sitting_start(
                &state,
                "11111111-1111-4111-8111-111111111111",
                "operator-sitting",
                None,
                sender,
            )
            .unwrap_err(),
            RECORDER_REASON_RUNTIME_UNKNOWN
        );

        // Placeholder encoder.
        *state.runtime.lock().unwrap() = Some(RuntimeIdentity {
            admission: "internal-alpha".into(),
            worker_build_sha256: "worker-build".into(),
            worker_executable_sha256: "worker-executable".into(),
            transcript_model_identity: "model/v1".into(),
            tap_build_sha256: "tap-build".into(),
            tap_path: PathBuf::from("/nonexistent/tap"),
            packaged_models: Vec::new(),
            encoder_sha256: "0575cb64845e6b9a10db9bcb74d5ac32b326b8dc90352671d345e2ee3d0126a2"
                .into(),
            encoder_available: false,
        });
        let (sender, _receiver) = mpsc::channel();
        assert_eq!(
            claim_sitting_start(
                &state,
                "11111111-1111-4111-8111-111111111111",
                "operator-sitting",
                None,
                sender,
            )
            .unwrap_err(),
            RECORDER_REASON_NO_ENCODER
        );

        // Stop with no active take.
        assert!(
            preview_enrollment_stop_sitting_for(&state)
                .unwrap_err()
                .contains("No setup recording is in progress")
        );

        // Stop after the thread vanished: the control survives until the
        // thread clears it, so a dead receiver is named, not ignored.
        let (sender, receiver) = mpsc::channel();
        *state.sitting_task.lock().unwrap() = Some(SittingTaskControl {
            sitting_id: "11111111-1111-4111-8111-111111111111".into(),
            sender,
        });
        drop(receiver);
        assert!(
            preview_enrollment_stop_sitting_for(&state)
                .unwrap_err()
                .contains("ended before Stop completed")
        );
        clear_sitting_task(&state, "11111111-1111-4111-8111-111111111111");
        assert!(!sitting_task_active(&state));

        // A live receiver: Stop lands exactly one signal.
        let (sender, receiver) = mpsc::channel();
        *state.sitting_task.lock().unwrap() = Some(SittingTaskControl {
            sitting_id: "33333333-3333-4333-8333-333333333333".into(),
            sender,
        });
        preview_enrollment_stop_sitting_for(&state).unwrap();
        assert!(receiver.try_recv().is_ok());
    }

    /// The relay document is the worker's canonical arithmetic verbatim:
    /// snake_case in, camelCase out, unknown fields refused, and the § I
    /// vocabulary preserved so the surface can only show what was measured.
    #[test]
    fn measured_choices_round_trip_without_invention() {
        let body = serde_json::json!({
            "schema": "profile-choices/1",
            "encoder_sha256": "ab".repeat(32),
            "evidence": {
                "sittings": [{"sitting_id": "s", "audio_sha256": "cd".repeat(32)}],
                "negative_sources": [{"sitting_id": "n", "audio_sha256": "ef".repeat(32)}],
            },
            "n_operator_scores": 12,
            "n_negative_scores": 22,
            "negative_scorable_seconds": 77.0,
            "choices": [{
                "target_frr": 0.05,
                "threshold": 0.24,
                "measured_frr": 0.041,
                "n_operator": 12,
                "false_admit_rate": 0.0,
                "n_other": 22,
            }],
        });
        let document: ChoicesDocument = serde_json::from_value(body).unwrap();
        assert_eq!(document.schema, "profile-choices/1");
        assert_eq!(document.choices.len(), 1);
        let rendered = serde_json::to_value(&document.choices[0]).unwrap();
        assert_eq!(rendered["targetFrr"], serde_json::json!(0.05));
        assert_eq!(rendered["falseAdmitRate"], serde_json::json!(0.0));
        assert_eq!(rendered["nOperator"], serde_json::json!(12));

        let unknown = serde_json::json!({
            "target_frr": 0.05, "threshold": 0.2, "measured_frr": 0.0,
            "n_operator": 5, "false_admit_rate": 0.0, "n_other": 20,
            "invented": true,
        });
        assert!(serde_json::from_value::<MeasuredOperatingPoint>(unknown).is_err());
    }

    /// Both profile-build commands refuse their boundaries before any worker
    /// exchange: an active take, a malformed digest, a target outside (0, 1).
    #[test]
    fn profile_build_commands_refuse_their_boundaries() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
        }
        assert!(
            preview_enrollment_build_profile_for(&state, 0.05, "not-a-digest")
                .unwrap_err()
                .contains("could not be identified")
        );
        assert!(
            preview_enrollment_build_profile_for(&state, 1.5, &"ab".repeat(32))
                .unwrap_err()
                .contains("not one of the measured choices")
        );
        let (sender, _receiver) = mpsc::channel();
        *state.sitting_task.lock().unwrap() = Some(SittingTaskControl {
            sitting_id: "11111111-1111-4111-8111-111111111111".into(),
            sender,
        });
        assert_eq!(
            preview_enrollment_operating_points_for(&state).state,
            "refused"
        );
        assert!(
            preview_enrollment_build_profile_for(&state, 0.05, &"ab".repeat(32))
                .unwrap_err()
                .contains("Finish the setup recording")
        );
    }

    /// The writer-lock interlock: while a sitting is active, every command
    /// that would queue behind the take's writer lock refuses in its own
    /// vocabulary instead of blocking with command_lock held.
    #[test]
    fn sitting_interlock_refuses_writer_lock_commands() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let (sender, _receiver) = mpsc::channel();
        *state.sitting_task.lock().unwrap() = Some(SittingTaskControl {
            sitting_id: "11111111-1111-4111-8111-111111111111".into(),
            sender,
        });
        assert!(
            preview_profile_preserve_legacy_for(&state)
                .unwrap_err()
                .contains("Finish the setup recording")
        );
        assert!(
            preview_profile_reset_for(&state, true)
                .unwrap_err()
                .contains("Finish the setup recording")
        );
        let deletion = preview_delete_meeting_audio_for("handle".into(), &state);
        assert_eq!(deletion.state, "capture-active");
        assert!(deletion.message.contains("Finish the setup recording"));
    }

    #[test]
    fn profile_lifecycle_attention_does_not_reduce_capture_admission() {
        let (_temporary, storage) = test_storage();
        durable_create_new(
            &storage.path().join("profile/voiceprint.json"),
            b"stored-profile-sentinel",
        )
        .unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let before = serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap();

        reconcile_preview_profile_lifecycle(&state).unwrap();

        let after = serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap();
        assert_eq!(after, before);
        let model = state.model.lock().unwrap();
        assert_eq!(model.reducer.startup(), StartupState::Ready);
        assert_eq!(model.reducer.capture(), CaptureState::Idle);
        assert!(model.retention_operational);
        assert_eq!(
            preview_profile_snapshot_for(&state).state,
            "migration-review-required"
        );
    }

    #[test]
    fn preview_preserves_legacy_profile_without_activating_or_changing_it() {
        let (_temporary, storage) = test_storage();
        let legacy = b"stored-profile-sentinel";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.preview_profile.lock().unwrap() =
            PreviewProfileSnapshot::lifecycle_unreadable("migration-review-required");
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let before_model = serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap();

        let snapshot = preview_profile_preserve_legacy_for(&state).unwrap();

        assert_eq!(snapshot, PreviewProfileSnapshot::baseline(true, false));
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );
        assert!(storage.path().join("profile/lifecycle").is_dir());
        assert_eq!(
            serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap(),
            before_model
        );
        let tree = storage_tree_bytes(storage.path());
        assert!(preview_profile_preserve_legacy_for(&state).is_err());
        assert_eq!(storage_tree_bytes(storage.path()), tree);
    }

    #[test]
    fn preview_profile_reset_requires_confirmation_and_leaves_meetings_untouched() {
        let (_temporary, storage) = test_storage();
        let legacy = b"stored-profile-sentinel";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        durable_create_new(
            &storage.path().join("meetings/meeting-storage-sentinel"),
            b"meeting-storage-sentinel",
        )
        .unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        *state.preview_profile.lock().unwrap() =
            PreviewProfileSnapshot::lifecycle_unreadable("migration-review-required");
        preview_profile_preserve_legacy_for(&state).unwrap();

        assert!(preview_profile_reset_for(&state, false).is_err());
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );

        assert_eq!(
            preview_profile_reset_for(&state, true).unwrap(),
            PreviewProfileSnapshot::baseline(false, false)
        );
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            b""
        );
        assert_eq!(
            fs::read(storage.path().join("meetings/meeting-storage-sentinel")).unwrap(),
            b"meeting-storage-sentinel"
        );
    }

    #[test]
    fn active_meeting_keeps_legacy_profile_and_migration_state_unchanged() {
        let (_temporary, storage) = test_storage();
        let legacy = b"stored-profile-sentinel";
        durable_create_new(&storage.path().join("profile/voiceprint.json"), legacy).unwrap();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.preview_profile.lock().unwrap() =
            PreviewProfileSnapshot::lifecycle_unreadable("migration-review-required");
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let coordination = state.meeting_storage_coordination().unwrap();
        let _active = coordination.acquire("active-meeting").unwrap();

        assert!(preview_profile_preserve_legacy_for(&state).is_err());
        assert_eq!(
            preview_profile_snapshot_for(&state).state,
            "migration-review-required"
        );
        assert_eq!(
            fs::read(storage.path().join("profile/voiceprint.json")).unwrap(),
            legacy
        );
        assert!(!storage.path().join("profile/lifecycle").exists());
    }

    #[test]
    fn profile_lifecycle_authority_loss_is_not_profile_attention() {
        let (_temporary, storage) = test_storage();
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let original_root = storage.path().to_path_buf();
        let displaced_root = original_root.with_file_name("app-data.displaced");
        fs::rename(&original_root, &displaced_root).unwrap();
        create_private_dir(&original_root).unwrap();
        for child in ["diagnostics", "profile", "meetings"] {
            create_private_dir(&original_root.join(child)).unwrap();
        }

        assert_eq!(
            reconcile_preview_profile_lifecycle(&state),
            Err("the app-data writer authority changed")
        );
        assert_eq!(
            preview_profile_snapshot_for(&state),
            PreviewProfileSnapshot::unavailable()
        );
        assert!(!original_root.join("profile/voiceprint.json").exists());
        assert!(!displaced_root.join("profile/voiceprint.json").exists());
    }

    #[test]
    fn active_meeting_lease_is_exclusive_and_released_on_drop() {
        let state = ApplicationState::default();
        let (_temporary, storage) = test_storage();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        let coordination = state.meeting_storage_coordination().unwrap();
        let lease = coordination.acquire("meeting-a").unwrap();
        assert!(
            state
                .meeting_storage_coordination()
                .unwrap()
                .acquire("meeting-a")
                .is_err()
        );
        let active = coordination
            .lock_sequence()
            .unwrap()
            .active_meeting_ids()
            .unwrap();
        assert_eq!(active.len(), 1);
        assert!(active.contains("meeting-a"));

        drop(lease);

        assert!(
            coordination
                .lock_sequence()
                .unwrap()
                .active_meeting_ids()
                .unwrap()
                .is_empty()
        );
        assert!(
            state
                .meeting_storage_coordination()
                .unwrap()
                .acquire("meeting-a")
                .is_ok()
        );
    }

    #[test]
    fn preview_sequence_barrier_blocks_reader_entry_without_sleeping() {
        let coordination = Arc::new(MeetingStorageCoordination::default());
        let held = coordination.lock_sequence().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let (entered_sender, entered_receiver) = mpsc::channel();
        let task_coordination = Arc::clone(&coordination);
        let task_barrier = Arc::clone(&barrier);
        let task = std::thread::spawn(move || {
            task_barrier.wait();
            with_meeting_storage_sequence(&task_coordination, |_| {
                entered_sender.send(()).unwrap();
            })
            .unwrap();
        });

        barrier.wait();
        assert!(entered_receiver.try_recv().is_err());
        drop(held);
        entered_receiver.recv().unwrap();
        task.join().unwrap();
    }

    #[test]
    fn poisoned_preview_sequence_fails_closed_before_reader_operation() {
        let coordination = Arc::new(MeetingStorageCoordination::default());
        let poisoned = Arc::clone(&coordination);
        assert!(
            std::thread::spawn(move || {
                let _sequence = poisoned.lock_sequence().unwrap();
                panic!("synthetic sequence poison");
            })
            .join()
            .is_err()
        );
        let called = std::cell::Cell::new(false);

        let response = with_meeting_storage_sequence(&coordination, |_| called.set(true));

        assert!(response.is_err());
        assert!(!called.get());
    }

    #[test]
    fn poisoned_preview_storage_returns_unavailable_without_mutation() {
        let state = Arc::new(ApplicationState::default());
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let before_model = serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap();
        let poisoned = Arc::clone(&state);
        assert!(
            std::thread::spawn(move || {
                let _storage = poisoned.storage.lock().unwrap();
                panic!("synthetic storage poison");
            })
            .join()
            .is_err()
        );

        let snapshot = library_snapshot_for(&state);
        let deletion = preview_delete_meeting_audio_for("synthetic-handle".into(), &state);

        assert_eq!(snapshot.state, "unavailable");
        assert!(snapshot.rows.is_empty());
        assert_eq!(snapshot.unavailable_count, 0);
        assert_eq!(deletion.state, "unavailable");
        assert!(deletion.audio_retention.is_none());
        assert!(state.preview_library.lock().unwrap().is_none());
        assert_eq!(
            serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap(),
            before_model
        );
    }

    #[test]
    fn poisoned_preview_invalidation_returns_unavailable_before_mutation() {
        let state = Arc::new(ApplicationState::default());
        let poisoned = Arc::clone(&state);
        assert!(
            std::thread::spawn(move || {
                let _library = poisoned.preview_library.lock().unwrap();
                panic!("synthetic Preview library poison");
            })
            .join()
            .is_err()
        );
        let mutated = std::cell::Cell::new(false);

        let response = with_preview_library_invalidated(&state, || {
            mutated.set(true);
            unavailable_preview_audio_deletion()
        })
        .unwrap_or_else(|_| unavailable_preview_audio_deletion());

        assert_eq!(response.state, "unavailable");
        assert!(response.audio_retention.is_none());
        assert!(!mutated.get());
    }

    #[test]
    fn preview_commands_share_sequence_before_reader_mutex_order() {
        let coordination = Arc::new(MeetingStorageCoordination::default());
        let reader = Arc::new(Mutex::new(()));
        let (first_entered_sender, first_entered_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (second_done_sender, second_done_receiver) = mpsc::channel();

        let first_coordination = Arc::clone(&coordination);
        let first_reader = Arc::clone(&reader);
        let first = std::thread::spawn(move || {
            with_meeting_storage_sequence(&first_coordination, |_| {
                let _reader = first_reader.lock().unwrap();
                first_entered_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
            })
            .unwrap();
        });
        first_entered_receiver.recv().unwrap();

        let second_coordination = Arc::clone(&coordination);
        let second_reader = Arc::clone(&reader);
        let second = std::thread::spawn(move || {
            with_meeting_storage_sequence(&second_coordination, |_| {
                let _reader = second_reader.lock().unwrap();
                second_done_sender.send(()).unwrap();
            })
            .unwrap();
        });
        assert!(second_done_receiver.try_recv().is_err());
        release_sender.send(()).unwrap();
        second_done_receiver.recv().unwrap();
        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    fn preview_audio_deletion_gate_refuses_before_consuming_handle() {
        let mut handle = Some("single-use-handle");
        for startup in [
            StartupState::ShellRendered,
            StartupState::Checking,
            StartupState::RuntimeMissing,
            StartupState::ServiceTimeout,
            StartupState::DiagnosticWritten,
            StartupState::Retrying,
            StartupState::ReinstallRequired,
        ] {
            let refused =
                with_preview_audio_deletion_gate(startup, CaptureState::Idle, || handle.take());
            assert_eq!(refused.unwrap_err().state, "not-ready");
            assert_eq!(handle, Some("single-use-handle"));
        }
        for capture in [
            CaptureState::Arming,
            CaptureState::Recording,
            CaptureState::Stopping,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::Summarizing,
            CaptureState::Ready,
            CaptureState::TranscriptionFailed,
            CaptureState::SummaryFailed,
            CaptureState::RecoveredInterrupted,
        ] {
            let refused =
                with_preview_audio_deletion_gate(StartupState::Ready, capture, || handle.take());
            assert_eq!(refused.unwrap_err().state, "capture-active");
            assert_eq!(handle, Some("single-use-handle"));
        }

        let authorized =
            with_preview_audio_deletion_gate(StartupState::Ready, CaptureState::Idle, || {
                handle.take()
            })
            .unwrap();
        assert_eq!(authorized, Some("single-use-handle"));
        assert_eq!(handle, None);

        let mut transcript_ready_handle = Some("transcript-ready-handle");
        assert_eq!(
            with_preview_audio_deletion_gate(
                StartupState::Ready,
                CaptureState::TranscriptReady,
                || transcript_ready_handle.take(),
            )
            .unwrap(),
            Some("transcript-ready-handle")
        );
    }

    /// The D-LOCK invariant: one ineligible meeting must never refuse app-wide
    /// recording readiness. A completed retention tick — whatever its
    /// per-meeting outcomes — never holds readiness; only a genuine failure of
    /// the retention machine itself does. Property-checked across the outcomes
    /// that used to (wrongly) trip the global flag.
    #[test]
    fn a_per_meeting_retention_outcome_never_holds_app_readiness() {
        let per_meeting_reports = [
            ScheduledRetentionReport {
                retention: vec![RetentionOutcome::Quarantined("stuck".into())],
                trash_purge: vec![],
            },
            ScheduledRetentionReport {
                retention: vec![RetentionOutcome::DeferredTranscription("awaiting".into())],
                trash_purge: vec![],
            },
            ScheduledRetentionReport {
                retention: vec![
                    RetentionOutcome::Quarantined("stuck".into()),
                    RetentionOutcome::AudioReleased("done".into()),
                ],
                trash_purge: vec![TrashPurgeOutcome::Quarantined("trash-stuck".into())],
            },
        ];
        for report in per_meeting_reports {
            assert!(
                !scheduled_retention_holds_app_readiness(&Ok(Some(report))),
                "a per-meeting quarantine or defer must not block all recording"
            );
        }
        assert!(!scheduled_retention_holds_app_readiness(&Ok(None)));

        // Only a genuine failure of the retention machine itself holds readiness.
        assert!(scheduled_retention_holds_app_readiness(&Err(
            ScheduledRetentionError::Coordination("lock unavailable")
        )));
        assert!(scheduled_retention_holds_app_readiness(&Err(
            ScheduledRetentionError::Retention
        )));
    }

    #[test]
    fn retention_failure_waits_for_command_before_changing_ready_state() {
        let state = Arc::new(ApplicationState::default());
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
            model.retention_operational = true;
        }
        let held_command = state.command_lock.lock().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let task_state = Arc::clone(&state);
        let task_barrier = Arc::clone(&barrier);
        let (done_sender, done_receiver) = mpsc::channel();
        let task = std::thread::spawn(move || {
            task_barrier.wait();
            mark_retention_unavailable(&task_state);
            done_sender.send(()).unwrap();
        });

        barrier.wait();
        assert!(done_receiver.try_recv().is_err());
        {
            let model = state.model.lock().unwrap();
            assert_eq!(model.reducer.startup(), StartupState::Ready);
            assert!(model.retention_operational);
        }

        drop(held_command);
        done_receiver.recv().unwrap();
        task.join().unwrap();
        let model = state.model.lock().unwrap();
        assert_eq!(model.reducer.startup(), StartupState::DiagnosticWritten);
        assert!(!model.retention_operational);
    }

    #[test]
    fn preview_reader_commands_preserve_app_snapshot_and_storage_bytes() {
        let state = ApplicationState::default();
        let before_snapshot = serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap();
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(
            &storage,
            &meeting_id,
            10,
            AudioState::Retained,
            "stable synthetic words",
        );
        let before_storage = storage_tree_bytes(storage.path());
        let coordination = MeetingStorageCoordination::default();

        with_meeting_storage_sequence(&coordination, |active| {
            let mut reader = library_reader::LibraryReader::rebuild(storage.clone(), active)
                .expect("synthetic Preview reader");
            let snapshot = reader.snapshot(active);
            let note = reader.open_note(&snapshot.rows[0].handle, active, None);
            let transcript_handle = note.transcript_handle.unwrap();
            let opened = reader
                .open_transcript_bound(
                    &transcript_handle,
                    active,
                    load_bound_preview_transcript_projection,
                )
                .unwrap()
                .unwrap();
            assert_eq!(opened.0[0].text, "stable synthetic words");
            assert_eq!(reader.search("", active).state, "invalid");
        })
        .unwrap();

        // Amended 2026-08-07, when the derived corpus index landed on this path.
        // The contract was "no byte in the storage root changes"; it is now
        // "no canonical byte changes, and the derived cache is the only thing
        // that moved". That is narrower where it has to be and stricter
        // everywhere else — a read path writing anything but the index still
        // fails here.
        let after_storage = storage_tree_bytes(storage.path());
        let canonical = |entries: &[(String, Vec<u8>)]| -> Vec<(String, Vec<u8>)> {
            entries
                .iter()
                .filter(|(path, _)| !path.starts_with("library/corpus.sqlite3"))
                .cloned()
                .collect()
        };
        assert_eq!(canonical(&after_storage), canonical(&before_storage));
        let moved: Vec<_> = after_storage
            .iter()
            .filter(|entry| !before_storage.contains(entry))
            .map(|(path, _)| path.as_str())
            .collect();
        assert_eq!(moved, vec!["library/corpus.sqlite3"]);
        assert_eq!(
            serde_json::to_value(state.model.lock().unwrap().snapshot()).unwrap(),
            before_snapshot
        );
    }

    #[test]
    fn preview_snapshot_excludes_active_meeting_and_keeps_prior_stable_row() {
        let (_temporary, storage) = test_storage();
        let stable_id = Uuid::new_v4().to_string();
        let active_id = Uuid::new_v4().to_string();
        write_transcript_fixture(
            &storage,
            &stable_id,
            10,
            AudioState::Retained,
            "prior stable words",
        );
        let active_directory = write_transcript_fixture(
            &storage,
            &active_id,
            20,
            AudioState::Retained,
            "partial active words",
        )
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
        fs::write(
            active_directory.join("attempt.json"),
            b"writer is replacing this receipt",
        )
        .unwrap();
        let coordination = MeetingStorageCoordination::default();
        let _active = coordination.acquire(&active_id).unwrap();

        with_meeting_storage_sequence(&coordination, |active| {
            let mut reader = library_reader::LibraryReader::rebuild(storage.clone(), active)
                .expect("stable meetings remain readable");
            let snapshot = reader.snapshot(active);
            assert_eq!(snapshot.rows.len(), 1);
            assert_eq!(snapshot.rows[0].meeting_id, stable_id);
        })
        .unwrap();
    }

    #[test]
    fn completed_meeting_enters_the_library_after_its_active_lease_ends() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(
            &storage,
            &meeting_id,
            10,
            AudioState::Retained,
            "completed meeting words",
        );
        let coordination = MeetingStorageCoordination::default();
        let active = coordination.acquire(&meeting_id).unwrap();

        with_meeting_storage_sequence(&coordination, |active_meeting_ids| {
            let mut reader =
                library_reader::LibraryReader::rebuild(storage.clone(), active_meeting_ids)
                    .expect("the active meeting is excluded without breaking the library");
            assert!(reader.snapshot(active_meeting_ids).rows.is_empty());
        })
        .unwrap();

        drop(active);

        with_meeting_storage_sequence(&coordination, |active_meeting_ids| {
            let mut reader =
                library_reader::LibraryReader::rebuild(storage.clone(), active_meeting_ids)
                    .expect("the completed meeting is readable after capture releases it");
            let snapshot = reader.snapshot(active_meeting_ids);
            assert_eq!(snapshot.rows.len(), 1);
            assert_eq!(snapshot.rows[0].meeting_id, meeting_id);
        })
        .unwrap();
    }

    #[test]
    fn app_data_writer_lock_is_exclusive_and_released_with_its_file() {
        let (_temporary, storage) = test_storage();
        let held = acquire_app_data_writer_lock(&storage).unwrap();

        assert!(acquire_app_data_writer_lock(&storage).is_err());

        drop(held);
        assert!(acquire_app_data_writer_lock(&storage).is_ok());
    }

    fn write_transcript_fixture(
        storage: &StorageRoot,
        meeting_id: &str,
        created_at_epoch_seconds: u64,
        audio_state: AudioState,
        text: &str,
    ) -> PathBuf {
        write_transcript_fixture_with_turns(
            storage,
            meeting_id,
            created_at_epoch_seconds,
            audio_state,
            json!([{
                "start": 0.0,
                "end": 1.0,
                "speaker": "Me",
                "text": text,
            }]),
        )
    }

    pub(crate) fn write_transcript_fixture_with_turns(
        storage: &StorageRoot,
        meeting_id: &str,
        created_at_epoch_seconds: u64,
        audio_state: AudioState,
        turns: Value,
    ) -> PathBuf {
        let directory = meeting_dir(storage, meeting_id).unwrap();
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        create_private_dir(&directory.join("transcript")).unwrap();
        create_private_dir(&directory.join("deletion")).unwrap();
        durable_create_new(&directory.join("ownership.json"), b"{}\n").unwrap();
        durable_create_new(&directory.join("capture/session.json"), b"{}\n").unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), b"synthetic microphone").unwrap();
        durable_create_new(&directory.join("capture/system.wav"), b"synthetic system").unwrap();
        let microphone_audio = artifact_ref(&directory, "capture/mic.wav").unwrap();
        let system_audio = artifact_ref(&directory, "capture/system.wav").unwrap();
        let deletion_receipt = if audio_state == AudioState::Released {
            fs::remove_file(directory.join("capture/mic.wav")).unwrap();
            fs::remove_file(directory.join("capture/system.wav")).unwrap();
            durable_create_new(&directory.join("deletion/audio-deletion.json"), b"{}\n").unwrap();
            Some(artifact_ref(&directory, "deletion/audio-deletion.json").unwrap())
        } else {
            None
        };
        let rule = AudioRetentionRule::DeleteAfter { seconds: 86_400 };
        let policy_sha256 = retention_policy_sha256(&rule);
        let attempt = CaptureAttemptReceipt {
            schema: "capture-attempt/1".into(),
            meeting_id: meeting_id.into(),
            attempt_id: Uuid::new_v4().to_string(),
            created_at_epoch_seconds,
            application_build_sha256: "a".repeat(64),
            participant_notice_version: PARTICIPANT_NOTICE_VERSION.into(),
            operator_attestation: valid_attestation(),
            retention_policy_sha256: policy_sha256.clone(),
        };
        durable_create_new(
            &directory.join("attempt.json"),
            &serde_json::to_vec_pretty(&attempt).unwrap(),
        )
        .unwrap();
        let transcript_bytes = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic-restart-fixture",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": turns,
        }))
        .unwrap();
        let transcript_digest = format!("{:x}", Sha256::digest(&transcript_bytes));
        let transcript_relative = format!("transcript/{transcript_digest}.json");
        let transcript_path = directory.join(&transcript_relative);
        durable_create_new(&transcript_path, &transcript_bytes).unwrap();
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: meeting_id.into(),
            lifecycle: MeetingLifecycle::TranscriptReady,
            retention: AudioRetention {
                rule,
                policy_sha256,
                next_deletion_at_epoch_seconds: Some(created_at_epoch_seconds + 86_400),
                state: audio_state,
                deletion_receipt,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: Some(microphone_audio),
                system_audio: Some(system_audio),
                current_transcript: Some(artifact_ref(&directory, &transcript_relative).unwrap()),
                current_note: None,
            },
            pending_storage_operation: None,
        };
        write_meeting(&directory, &meeting).unwrap();
        transcript_path
    }

    /// Roadmap intake W8-B's own gate, requirement 3: prove that the
    /// *registered* command path -- `preview_library_search_for` and
    /// `preview_library_open_search_result_for`, exactly what
    /// `preview_library_search`/`preview_library_open_search_result` delegate
    /// to -- inherits the W5-B lock exclusion rather than reimplementing it.
    /// `library_reader.rs`'s own
    /// `locking_a_meeting_removes_it_from_search_and_a_stale_hit_refuses_to_open`
    /// already proves the exclusion itself at the `LibraryReader` layer; this
    /// test proves the thin, flag-gated wrapper this packet adds does not
    /// bypass it. It reuses the same reader instance across the lock
    /// transition, unrebuilt, because `search_current` recomputes the locked
    /// set from disk on every call -- the same shape the library_reader.rs
    /// test relies on.
    #[test]
    fn the_registered_search_commands_inherit_the_w5_b_lock_exclusion() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(&storage, &meeting_id, 10, AudioState::Retained, "exact probe needle");
        let state = vocabulary_command_state(&storage);
        fs::write(storage.path().join(search_probe::FLAG_FILE), b"").unwrap();

        // Populate `state.preview_library`, the same way the real Home screen
        // does before ever calling a search command.
        let snapshot = library_snapshot_for(&state);
        assert_eq!(snapshot.state, "populated");

        let before = preview_library_search_for("exact probe needle", &state);
        assert_eq!(before.state, "results");
        assert_eq!(before.results.len(), 1);
        let stale_handle = before.results[0].handle.clone();

        let directory = meeting_dir(&storage, &meeting_id).unwrap();
        meeting_lock::write(&directory, true).unwrap();

        // Same query, same unrebuilt reader -- only the lock file on disk
        // changed. If the registered command reimplemented the exclusion
        // instead of delegating to `LibraryReader::search`, this is exactly
        // where that would show up as a leaked hit.
        let after = preview_library_search_for("exact probe needle", &state);
        assert_ne!(after.state, "results");
        assert!(after.results.is_empty());

        // And the handle sealed before the lock refuses rather than reopening
        // the meeting it named.
        let opened = preview_library_open_search_result_for(&stale_handle, &state);
        assert_eq!(opened.state, "stale");
        assert!(opened.meeting_id.is_none());
    }

    /// Requirement 1's flag mechanism, exercised through the registered
    /// command functions rather than through `search_probe` directly: absent
    /// the marker, both commands refuse with the exact quiet sentence the
    /// packet names, and neither one touches the library at all (no handle,
    /// no probe-log line -- covered separately in `search_probe`'s own
    /// tests).
    #[test]
    fn the_registered_search_commands_refuse_with_the_quiet_sentence_while_the_flag_is_absent() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(&storage, &meeting_id, 10, AudioState::Retained, "exact probe needle");
        let state = vocabulary_command_state(&storage);
        // Deliberately no flag file written.

        let search = preview_library_search_for("exact probe needle", &state);
        assert_eq!(search.state, "probe-disabled");
        assert_eq!(search.message, "Search across meetings is not enabled.");
        assert!(search.results.is_empty());
        // Refusing must not have populated the library or minted a handle --
        // it is the same as an unregistered command, not merely one that
        // answers "no results."
        assert!(state.preview_library.lock().unwrap().is_none());

        let opened = preview_library_open_search_result_for("anything", &state);
        assert_eq!(opened.state, "probe-disabled");
        assert_eq!(opened.message, "Search across meetings is not enabled.");
        assert!(opened.meeting_id.is_none());

        assert!(!storage.path().join(search_probe::LOG_FILE).exists());
    }

    /// Roadmap packet W10, requirement 1: the frontend's only two signals for
    /// the once-only first-run sheet, both riding the `library_snapshot`
    /// response it already fetches on every Home render -- `total` (already
    /// existed) and `firstRunSheetSeen` (new). Zero meetings and an
    /// undismissed sheet is exactly the "show it once" state.
    #[test]
    fn library_snapshot_reports_unseen_first_run_sheet_when_no_meetings_exist() {
        let (_temporary, storage) = test_storage();
        let state = vocabulary_command_state(&storage);
        // Deliberately no `first-run-seen.flag` written.

        let response = library_snapshot_response_for(library_reader::LibraryFilterArgs::default(), &state);
        assert_eq!(response.library.total, 0);
        assert!(!response.first_run_sheet_seen);
    }

    /// Requirement 1's other half: an operator who already has meetings never
    /// saw the sheet dismissed (no flag was ever written for them), but the
    /// `total` half of the same response is what the frontend uses to
    /// suppress it anyway -- an upgrading operator is not a stranger. This
    /// proves both facts land in the same response for that combination to be
    /// possible client-side.
    #[test]
    fn library_snapshot_reports_a_nonzero_total_for_an_operator_with_existing_meetings() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(&storage, &meeting_id, 3, AudioState::Retained, "an existing meeting");
        let state = vocabulary_command_state(&storage);

        let response = library_snapshot_response_for(library_reader::LibraryFilterArgs::default(), &state);
        assert_eq!(response.library.total, 1);
        // No flag was ever written for this operator -- the point is that
        // `total` alone is enough to suppress the sheet regardless.
        assert!(!response.first_run_sheet_seen);
    }

    /// Requirement 1, the dismissal path: `dismiss_first_run_sheet` writes the
    /// marker, and a *fresh* `library_snapshot` call -- not a cached field --
    /// reflects it. That freshness is what makes this "restart-shaped": a
    /// real relaunch is nothing more than a new call against the same
    /// storage root, which is exactly what this test performs.
    #[test]
    fn dismissing_the_first_run_sheet_persists_across_a_fresh_library_snapshot_call() {
        let (_temporary, storage) = test_storage();
        let state = vocabulary_command_state(&storage);

        let before = library_snapshot_response_for(library_reader::LibraryFilterArgs::default(), &state);
        assert!(!before.first_run_sheet_seen);

        dismiss_first_run_sheet_for(&state);

        let after = library_snapshot_response_for(library_reader::LibraryFilterArgs::default(), &state);
        assert!(after.first_run_sheet_seen, "dismissal must persist across a fresh snapshot call, not just in-memory");
    }

    #[test]
    fn start_requires_every_assertion_and_a_closed_retention_choice() {
        assert!(validate_start_request(1, &valid_attestation()).is_ok());
        assert!(validate_start_request(7, &valid_attestation()).is_ok());
        assert!(validate_start_request(30, &valid_attestation()).is_ok());
        assert!(validate_start_request(2, &valid_attestation()).is_err());
        for field in 0..3 {
            let mut attestation = valid_attestation();
            match field {
                0 => attestation.participants_consented = false,
                1 => attestation.headphones = false,
                _ => attestation.operator_alone = false,
            }
            assert!(validate_start_request(7, &attestation).is_err());
        }
    }

    /// The load-bearing accounting: what the receipt says about pauses and what
    /// the worker is told about elapsed time have to describe the same meeting.
    /// Nothing in the merge gate reaches the Python integrity floor that depends
    /// on this, so the arithmetic is pinned here.
    #[test]
    fn a_paused_recording_reports_recorded_time_and_the_gaps_reconcile() {
        let start = Instant::now();
        let mut ledger = PauseLedger::new(start);
        ledger.begin_pause(start + Duration::from_secs(10));
        ledger.end_pause(start + Duration::from_secs(40));
        ledger.begin_pause(start + Duration::from_secs(55));
        ledger.end_pause(start + Duration::from_secs(70));
        ledger.finish(start + Duration::from_secs(80));

        // 10 + 15 + 10 recorded, 30 + 15 paused, 80 on the wall clock.
        assert_eq!(ledger.recorded_elapsed(), Duration::from_secs(35));
        let receipt = ledger.receipt().unwrap().expect("two gaps were recorded");
        assert_eq!(
            receipt,
            json!({
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": 160_000, "resumed_at_samples": 640_000},
                    {"paused_at_samples": 880_000, "resumed_at_samples": 1_120_000}
                ]
            })
        );

        let recorded = elapsed_samples(ledger.recorded_elapsed()).unwrap();
        let paused: u64 = receipt["spans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|span| {
                span["resumed_at_samples"].as_u64().unwrap()
                    - span["paused_at_samples"].as_u64().unwrap()
            })
            .sum();
        assert_eq!(recorded + paused, 80 * 16_000);
        // Every span sits inside the wall clock the two numbers imply, which is
        // the bound the capture receipt is validated against.
        assert!(receipt["spans"].as_array().unwrap().iter().all(|span| {
            span["resumed_at_samples"].as_u64().unwrap() <= recorded + paused
        }));
    }

    #[test]
    fn an_unpaused_recording_records_no_pause_field_at_all() {
        let start = Instant::now();
        let mut ledger = PauseLedger::new(start);
        ledger.finish(start + Duration::from_secs(12));
        assert_eq!(ledger.recorded_elapsed(), Duration::from_secs(12));
        assert_eq!(ledger.receipt().unwrap(), None);
    }

    #[test]
    fn stopping_while_paused_still_records_the_gap_the_operator_held_open() {
        let start = Instant::now();
        let mut ledger = PauseLedger::new(start);
        ledger.begin_pause(start + Duration::from_secs(5));
        ledger.finish(start + Duration::from_secs(25));

        // The trailing pause is time the operator held the meeting open with
        // nothing being captured, so it is evidence even though no audio
        // follows it.
        assert_eq!(ledger.recorded_elapsed(), Duration::from_secs(5));
        assert_eq!(
            ledger.receipt().unwrap().unwrap()["spans"],
            json!([{"paused_at_samples": 80_000, "resumed_at_samples": 400_000}])
        );
    }

    #[test]
    fn a_recording_paused_past_the_receipt_bound_refuses_rather_than_truncating() {
        let start = Instant::now();
        let mut ledger = PauseLedger::new(start);
        for index in 0..(MAX_PAUSE_SPANS as u64 + 1) {
            ledger.begin_pause(start + Duration::from_secs(index * 2));
            ledger.end_pause(start + Duration::from_secs(index * 2 + 1));
        }
        assert!(ledger.receipt().is_err());
    }

    #[test]
    fn capture_events_are_closed_and_format_bound() {
        assert_eq!(
            parse_capture_event(br#"{"schema":"capture-event/1","event":"paused"}"#).unwrap(),
            CaptureEvent::Paused
        );
        assert_eq!(
            parse_capture_event(
                br#"{"schema":"capture-event/1","event":"recording","format":{"encoding":"pcm_s16le","sample_rate":16000,"channels":1}}"#
            )
            .unwrap(),
            CaptureEvent::Recording
        );
        assert_eq!(
            parse_capture_event(br#"{"schema":"capture-event/1","event":"suspended"}"#).unwrap(),
            CaptureEvent::Suspended
        );
        assert_eq!(
            parse_capture_event(br#"{"schema":"capture-event/1","event":"resumed"}"#).unwrap(),
            CaptureEvent::Resumed
        );
        assert!(
            parse_capture_event(br#"{"schema":"capture-event/1","event":"paused","extra":true}"#)
                .is_err()
        );
        assert!(parse_capture_event(
            br#"{"schema":"capture-event/1","event":"suspended","extra":true}"#
        )
        .is_err());
        assert!(
            parse_capture_event(
                br#"{"schema":"capture-event/1","event":"recording","format":{"encoding":"pcm_f32le","sample_rate":16000,"channels":1}}"#
            )
            .is_err()
        );
        assert_eq!(
            parse_capture_event(
                br#"{"schema":"capture-event/1","event":"finalized","legs":{"mic":{"samples":7}}}"#
            )
            .unwrap(),
            CaptureEvent::FinalizedMicOnly { mic_samples: 7 }
        );
        assert!(
            parse_capture_event(
                br#"{"schema":"capture-event/1","event":"finalized","legs":{"system":{"samples":7}}}"#
            )
            .is_err()
        );
        assert!(
            parse_capture_event(
                br#"{"schema":"capture-event/1","event":"finalized","legs":{"mic":{"samples":7},"system":{"samples":7},"extra":{"samples":7}}}"#
            )
            .is_err()
        );
    }

    /// Writes an executable /bin/sh stand-in for the capture helper's sitting
    /// mode. The preamble binds AUDIO/CONTROL/EVENT to the fd numbers the
    /// spawner passed and defines `emit` (one event line) and `await_control`
    /// (block for one control byte); `body` scripts the scenario.
    fn write_sitting_helper(directory: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("fake-sitting-helper.sh");
        let script = format!(
            "#!/bin/sh\n\
             while [ $# -gt 0 ]; do\n\
               case \"$1\" in\n\
                 --sitting-audio-fd) AUDIO=\"$2\"; shift 2 ;;\n\
                 --control-fd) CONTROL=\"$2\"; shift 2 ;;\n\
                 --event-fd) EVENT=\"$2\"; shift 2 ;;\n\
                 *) shift ;;\n\
               esac\n\
             done\n\
             emit() {{ printf '%s\\n' \"$1\" >&\"$EVENT\"; }}\n\
             await_control() {{ dd bs=1 count=1 <&\"$CONTROL\" >/dev/null 2>&1; }}\n\
             {body}\n"
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// A throwaway process group for the fake helper, so CaptureProcess
    /// cleanup signals never reach the test runner's own group.
    fn spawn_group_anchor() -> std::process::Child {
        let mut command = Command::new("/bin/sleep");
        command
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.spawn().unwrap()
    }

    /// What the caller does with the stop channel during a harness run:
    /// request Stop up front, request it only after a delay long enough for
    /// the helper's early events to land first, hold it silent for the whole
    /// take, or drop it unsent — the control-plane fault the driver must
    /// refuse to admit.
    #[derive(Clone, Copy)]
    enum StopScript {
        Requested,
        DelayedRequested,
        Never,
        Dropped,
    }

    struct SittingDriverHarness {
        _temporary: TempDir,
        _scripts: TempDir,
        state: ApplicationState,
        storage: StorageRoot,
        helper: PathBuf,
        anchor: std::process::Child,
    }

    impl SittingDriverHarness {
        fn new(body: &str) -> Self {
            let (_temporary, storage) = test_storage();
            let state = ApplicationState::default();
            ensure_app_data_writer_lock(&state, &storage).unwrap();
            let scripts = TempDir::new().unwrap();
            let helper = write_sitting_helper(scripts.path(), body);
            let anchor = spawn_group_anchor();
            Self {
                _temporary,
                _scripts: scripts,
                state,
                storage,
                helper,
                anchor,
            }
        }

        fn run(
            &self,
            sitting_id: &str,
            stop: StopScript,
        ) -> Result<SittingCaptureReceipt, SittingCaptureFailure> {
            use local_meeting_notes_session_core::sitting_evidence::SittingKind;
            let (stop_sender, stop_receiver) = mpsc::channel();
            match stop {
                StopScript::Requested => stop_sender.send(()).unwrap(),
                StopScript::DelayedRequested => {
                    let sender = stop_sender.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(1));
                        let _ = sender.send(());
                    });
                }
                StopScript::Never => {}
                StopScript::Dropped => drop(stop_sender),
            }
            let held = self.state.app_data_writer_lock.lock().unwrap();
            let authority = held.as_ref().unwrap().sitting_evidence_authority();
            run_sitting_capture(
                &authority,
                &self.helper,
                self.anchor.id() as i32,
                sitting_id,
                SittingKind::OperatorSitting,
                None,
                &stop_receiver,
            )
        }

        /// Mechanical abandonment proof: an abandoned sitting is a rehearsal,
        /// so the store refuses to finalize it ever after.
        fn assert_abandoned(&self, sitting_id: &str) {
            let held = self.state.app_data_writer_lock.lock().unwrap();
            let authority = held.as_ref().unwrap().sitting_evidence_authority();
            assert!(authority.finalize_capture(sitting_id, 9_999).is_err());
        }
    }

    impl Drop for SittingDriverHarness {
        fn drop(&mut self) {
            let _ = self.anchor.kill();
            let _ = self.anchor.wait();
        }
    }

    const SITTING_EMIT_PAUSED: &str = r#"emit '{"schema":"capture-event/1","event":"paused"}'"#;
    const SITTING_EMIT_RECORDING: &str = r#"emit '{"schema":"capture-event/1","event":"recording","format":{"encoding":"pcm_s16le","sample_rate":16000,"channels":1}}'"#;

    #[test]
    fn sitting_capture_streams_helper_bytes_into_the_evidence_store() {
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=100 >&\"$AUDIO\" 2>/dev/null\n\
             await_control\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":16000}}}}}}'\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b01";
        let receipt = harness.run(sitting_id, StopScript::Requested).unwrap();
        assert_eq!(
            receipt,
            SittingCaptureReceipt {
                mic_samples: 16_000
            }
        );
        // The store holds exactly the streamed bytes, and the capture row is
        // closed: further appends are refused.
        let entries = storage_tree_bytes(harness.storage.path());
        let raw: Vec<_> = entries
            .iter()
            .filter(|(name, _)| name.ends_with("audio.raw"))
            .collect();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].1, vec![0_u8; 32_000]);
        let held = harness.state.app_data_writer_lock.lock().unwrap();
        let authority = held.as_ref().unwrap().sitting_evidence_authority();
        assert!(authority.append_raw_audio(sitting_id, b"late").is_err());
    }

    #[test]
    fn sitting_capture_refuses_a_vanished_stop_channel_and_abandons() {
        // A dropped stop sender is a control-plane fault, not an operator
        // Stop: admitting the take would let a panicked caller turn a partial
        // recording into completed enrolment evidence. Mirrors the meeting
        // loop's capture_control_disconnected refusal. The helper here is the
        // fully cooperative happy-path script — only the control channel is
        // at fault.
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=100 >&\"$AUDIO\" 2>/dev/null\n\
             await_control\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":16000}}}}}}'\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b06";
        let failure = harness.run(sitting_id, StopScript::Dropped).unwrap_err();
        assert_eq!(failure.code, "sitting_control_disconnected");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_end_of_stream_without_finalized_receipt() {
        // Stop is requested and honored, but the helper exits zero without
        // ever finalizing. A clean exit plus end-of-stream must not admit
        // the sitting: the receipt is the only completion authority.
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=10 >&\"$AUDIO\" 2>/dev/null\n\
             await_control\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b02";
        let failure = harness.run(sitting_id, StopScript::Requested).unwrap_err();
        assert_eq!(failure.code, "sitting_finalize_receipt_missing");
        assert!(failure.detail.contains("not completion authority"));
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_a_self_finalizing_helper_without_stop() {
        // The helper finalizes with a receipt that matches every byte it
        // streamed and exits cleanly — but nobody ever requested Stop. A
        // helper deciding on its own that the take is over could truncate a
        // recording and hand back self-consistent evidence for the part it
        // kept, so admission additionally requires the parent's explicit
        // Stop. (Unreachable with the shipped helper, which finalizes only
        // on the X control byte — this pins the parent-side boundary the
        // driver documents, since helper identity attestation is deferred.)
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=100 >&\"$AUDIO\" 2>/dev/null\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":16000}}}}}}'\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b07";
        let failure = harness.run(sitting_id, StopScript::Never).unwrap_err();
        assert_eq!(failure.code, "sitting_finalize_before_stop");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_end_of_stream_without_stop() {
        // The helper streams and exits cleanly without finalizing, and Stop
        // was never requested. This pins the end-of-stream half of the Stop
        // requirement on its own: no receipt-ordering refusal fires first,
        // so the without-stop gate must.
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=10 >&\"$AUDIO\" 2>/dev/null\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b08";
        let failure = harness.run(sitting_id, StopScript::Never).unwrap_err();
        assert_eq!(failure.code, "sitting_finalize_without_stop");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_a_receipt_that_precedes_stop() {
        // The truncation attack the end-of-stream gate alone misses: the
        // helper finalizes early with a byte-exact receipt for the part it
        // kept, holds the stream open until the operator's eventual Stop,
        // honors it, and exits cleanly. By end of stream every state gate
        // is green — Stop was requested, the receipt matches the drained
        // bytes — so the receipt must be refused at the moment it is
        // observed, before Stop was ever sent.
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=100 >&\"$AUDIO\" 2>/dev/null\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":16000}}}}}}'\n\
             await_control\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b09";
        let failure = harness
            .run(sitting_id, StopScript::DelayedRequested)
            .unwrap_err();
        assert_eq!(failure.code, "sitting_finalize_before_stop");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_sample_count_mismatch_and_abandons() {
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=10 >&\"$AUDIO\" 2>/dev/null\n\
             await_control\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":9999}}}}}}'\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b03";
        let failure = harness.run(sitting_id, StopScript::Requested).unwrap_err();
        assert_eq!(failure.code, "sitting_sample_count_mismatch");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_surfaces_helper_fault_and_abandons() {
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"failed\",\"code\":\"sitting_stream_write_failed\",\"detail\":\"\"}}'\n\
             exit 1"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b04";
        let failure = harness.run(sitting_id, StopScript::Never).unwrap_err();
        assert_eq!(failure.code, "sitting_stream_write_failed");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn sitting_capture_refuses_meeting_shaped_finalized_receipt() {
        let body = format!(
            "{SITTING_EMIT_PAUSED}\n\
             await_control\n\
             {SITTING_EMIT_RECORDING}\n\
             dd if=/dev/zero bs=320 count=1 >&\"$AUDIO\" 2>/dev/null\n\
             emit '{{\"schema\":\"capture-event/1\",\"event\":\"finalized\",\"legs\":{{\"mic\":{{\"samples\":160}},\"system\":{{\"samples\":160}}}}}}'\n\
             exit 0"
        );
        let harness = SittingDriverHarness::new(&body);
        let sitting_id = "3f2c8f2a-64f0-4f05-9be6-0a3d1f6f8b05";
        let failure = harness.run(sitting_id, StopScript::Never).unwrap_err();
        assert_eq!(failure.code, "sitting_helper_protocol_violation");
        harness.assert_abandoned(sitting_id);
    }

    #[test]
    fn worker_digest_sets_are_exact_and_lowercase() {
        let valid = HashMap::from([("transcript".into(), "a".repeat(64))]);
        assert!(exact_digests(&valid, &["transcript"]).is_ok());
        let extra = HashMap::from([
            ("transcript".into(), "a".repeat(64)),
            ("note".into(), "b".repeat(64)),
        ]);
        assert!(exact_digests(&extra, &["transcript"]).is_err());
        let uppercase = HashMap::from([("transcript".into(), "A".repeat(64))]);
        assert!(exact_digests(&uppercase, &["transcript"]).is_err());
    }

    #[test]
    fn transcript_projection_projects_gated_turns_as_content_free_withheld_rows() {
        let document = br#"{
          "schema":"capture-transcript/1",
          "source":"fixture",
          "attribution":"channel",
          "bleed":null,
          "voiceprint":null,
          "capture_health":{},
          "turns":[
            {"start":0.0,"end":1.0,"speaker":"Me","text":"visible"},
            {"start":1.0,"end":2.0,"speaker":"Me","text":"withheld","gated":true,"gate_score":0.1,"gate_reason":"fixture"}
          ]
        }"#;
        let (turns, warnings) =
            parse_transcript_projection_with(document, &BTreeSet::new()).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text, "visible");
        assert!(!turns[0].withheld);
        // The withheld row is positional only: no words, no speaker authority.
        assert!(turns[1].withheld);
        assert!(turns[1].text.is_empty());
        assert!(turns[1].speaker.is_none());
        assert_eq!(turns[1].source_turn_index, 1);
        let payload = serde_json::to_string(&turns[1]).unwrap();
        assert!(!payload.contains("withheld\":false"));
        assert!(!payload.contains("gate_score"));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn speaker_correction_projects_over_matching_turns_without_changing_source_identity() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4();
        let transcript_path = write_transcript_fixture_with_turns(
            &storage,
            &meeting_id.to_string(),
            10,
            AudioState::Retained,
            json!([
                {"start": 0.0, "end": 1.0, "speaker": "Me", "text": "first"},
                {"start": 1.0, "end": 2.0, "speaker": "Them", "text": "second"},
                {"start": 2.0, "end": 3.0, "speaker": "Them", "text": "third"}
            ]),
        );
        let directory = transcript_path.parent().unwrap().parent().unwrap();
        let meeting = load_meeting(directory).unwrap();
        let reference = meeting.artifacts.current_transcript.unwrap();
        speaker_correction::append(
            directory,
            meeting_id,
            speaker_correction::SpeakerCorrectionOperation {
                operation_id: Uuid::new_v4(),
                source_transcript_sha256: reference.sha256.clone(),
                source_speaker: Some("Them".into()),
                replacement: "Alex".into(),
                applied_at_epoch_seconds: 11,
            },
        )
        .unwrap();

        let (turns, warnings) =
            load_transcript_projection(directory, &meeting_id.to_string(), &reference).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(turns[0].speaker.as_deref(), Some("Me"));
        assert!(!turns[0].speaker_corrected);
        assert_eq!(turns[1].speaker.as_deref(), Some("Alex"));
        assert_eq!(turns[2].speaker.as_deref(), Some("Alex"));
        assert_eq!(turns[1].source_speaker.as_deref(), Some("Them"));
        assert_eq!(turns[2].source_speaker.as_deref(), Some("Them"));
        assert!(turns[1].speaker_corrected && turns[2].speaker_corrected);
        let (candidate_turns, candidate_warnings) = load_transcript_projection_without_corrections(
            directory,
            &meeting_id.to_string(),
            &reference,
        )
        .unwrap();
        assert!(candidate_warnings.is_empty());
        assert_eq!(candidate_turns[1].speaker.as_deref(), Some("Them"));
        assert!(!candidate_turns[1].speaker_corrected);
        let source_bytes = fs::read(transcript_path).unwrap();
        assert!(
            String::from_utf8(source_bytes)
                .unwrap()
                .contains("\"speaker\": \"Them\"")
        );
    }

    #[test]
    fn speaker_correction_command_saves_reopens_and_can_restore_the_source_label() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4();
        let transcript_path = write_transcript_fixture_with_turns(
            &storage,
            &meeting_id.to_string(),
            10,
            AudioState::Retained,
            json!([
                {"start": 0.0, "end": 1.0, "speaker": "Me", "text": "first"},
                {"start": 1.0, "end": 2.0, "speaker": "Them", "text": "second"},
                {"start": 2.0, "end": 3.0, "speaker": "Them", "text": "third"}
            ]),
        );
        let directory = transcript_path.parent().unwrap().parent().unwrap();
        let reference = load_meeting(directory)
            .unwrap()
            .artifacts
            .current_transcript
            .unwrap();
        let state = ApplicationState::default();
        *state.storage.lock().unwrap() = Some(StorageContext {
            storage: storage.clone(),
            resource_root: PathBuf::new(),
            manifest_path: PathBuf::new(),
            diagnostics: PathBuf::new(),
        });
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        {
            let mut model = state.model.lock().unwrap();
            transition_startup(&mut model, StartupState::Checking).unwrap();
            transition_startup(&mut model, StartupState::Ready).unwrap();
        }

        let saved = correct_speaker_name_for(
            meeting_id,
            reference.sha256.clone(),
            Some("Them".into()),
            "Alex".into(),
            &state,
        )
        .unwrap();
        assert_eq!(saved.applied_turn_count, 2);
        let (corrected, _) =
            load_transcript_projection(directory, &meeting_id.to_string(), &reference).unwrap();
        assert_eq!(corrected[1].speaker.as_deref(), Some("Alex"));
        assert!(corrected[1].speaker_corrected);

        correct_speaker_name_for(
            meeting_id,
            reference.sha256.clone(),
            Some("Them".into()),
            "Them".into(),
            &state,
        )
        .unwrap();
        let (restored, _) =
            load_transcript_projection(directory, &meeting_id.to_string(), &reference).unwrap();
        assert_eq!(restored[1].speaker.as_deref(), Some("Them"));
        assert!(!restored[1].speaker_corrected);
        assert!(
            correct_speaker_name_for(
                meeting_id,
                "b".repeat(64),
                Some("Them".into()),
                "Taylor".into(),
                &state,
            )
            .is_err()
        );
    }

    fn vocabulary_command_state(storage: &StorageRoot) -> ApplicationState {
        let state = ApplicationState::default();
        *state.storage.lock().unwrap() = Some(StorageContext {
            storage: storage.clone(),
            resource_root: PathBuf::new(),
            manifest_path: PathBuf::new(),
            diagnostics: PathBuf::new(),
        });
        ensure_app_data_writer_lock(&state, storage).unwrap();
        let mut model = state.model.lock().unwrap();
        transition_startup(&mut model, StartupState::Checking).unwrap();
        transition_startup(&mut model, StartupState::Ready).unwrap();
        drop(model);
        state
    }

    #[test]
    fn local_vocabulary_commands_persist_exact_counts_without_rewriting_transcript_bytes() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4();
        let transcript_path = write_transcript_fixture_with_turns(
            &storage,
            &meeting_id.to_string(),
            10,
            AudioState::Retained,
            json!([
                {"start": 0.0, "end": 1.0, "speaker": "Me", "text": "Kibbel Report and Kibbel"},
                {"start": 1.0, "end": 2.0, "speaker": "Me", "text": "Kibbel private", "gated": true, "gate_score": 0.1, "gate_reason": "fixture"}
            ]),
        );
        let directory = transcript_path.parent().unwrap().parent().unwrap();
        let reference = load_meeting(directory)
            .unwrap()
            .artifacts
            .current_transcript
            .unwrap();
        let source_digest = sha256_file(&transcript_path).unwrap();
        assert_eq!(source_digest, reference.sha256);
        let state = vocabulary_command_state(&storage);

        let first = local_vocabulary_add_for_test(
            meeting_id,
            &reference.sha256,
            "Kibbel",
            "Kibble",
            &state,
        )
        .unwrap();
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.entries[0].applied_turn_count, 2);
        let first_id = first.entries[0].id;

        let overlapped = local_vocabulary_add_for_test(
            meeting_id,
            &reference.sha256,
            "Kibbel Report",
            "Kibble report",
            &state,
        )
        .unwrap();
        assert_eq!(overlapped.entries.len(), 2);
        assert_eq!(
            overlapped
                .entries
                .iter()
                .find(|entry| entry.id == first_id)
                .unwrap()
                .applied_turn_count,
            1
        );
        let second_id = overlapped.entries[1].id;
        assert_eq!(overlapped.entries[1].applied_turn_count, 1);

        // A fresh process reads the same private store and re-derives the
        // count from the immutable, digest-bound projection.
        drop(state);
        let restarted = vocabulary_command_state(&storage);
        let after_restart =
            local_vocabulary_list_for_test(meeting_id, &reference.sha256, &restarted).unwrap();
        assert_eq!(after_restart.entries.len(), 2);
        assert_eq!(after_restart.entries[1].applied_turn_count, 1);

        let edited = local_vocabulary_edit_for_test(
            meeting_id,
            &reference.sha256,
            second_id,
            "Report",
            "Brief",
            &restarted,
        )
        .unwrap();
        assert_eq!(edited.entries[0].applied_turn_count, 2);
        assert_eq!(edited.entries[1].applied_turn_count, 1);
        let disabled = local_vocabulary_set_enabled_for_test(
            meeting_id,
            &reference.sha256,
            first_id,
            false,
            &restarted,
        )
        .unwrap();
        assert_eq!(disabled.entries.len(), 2, "disabled entries stay visible");
        assert!(!disabled.entries[0].enabled);
        assert_eq!(disabled.entries[0].applied_turn_count, 0);
        let enabled = local_vocabulary_set_enabled_for_test(
            meeting_id,
            &reference.sha256,
            first_id,
            true,
            &restarted,
        )
        .unwrap();
        assert!(enabled.entries[0].enabled);
        assert_eq!(enabled.entries[0].applied_turn_count, 2);
        let deleted =
            local_vocabulary_delete_for_test(meeting_id, &reference.sha256, second_id, &restarted)
                .unwrap();
        assert_eq!(deleted.entries.len(), 1);
        assert_eq!(sha256_file(&transcript_path).unwrap(), source_digest);

        assert!(
            local_vocabulary_add_for_test(
                meeting_id,
                &"b".repeat(64),
                "Other",
                "Other preferred",
                &restarted,
            )
            .is_err()
        );

        fs::write(
            storage.path().join("local-vocabulary.json"),
            b"not a vocabulary document",
        )
        .unwrap();
        assert!(local_vocabulary_list_for_test(meeting_id, &reference.sha256, &restarted).is_err());
        assert_eq!(sha256_file(&transcript_path).unwrap(), source_digest);
    }

    fn local_vocabulary_list_for_test(
        meeting_id: Uuid,
        digest: &str,
        state: &ApplicationState,
    ) -> Result<LocalVocabularySheetResponse, CommandError> {
        with_current_local_vocabulary(meeting_id, digest, state, local_vocabulary_sheet_response)
    }

    fn local_vocabulary_add_for_test(
        meeting_id: Uuid,
        digest: &str,
        source: &str,
        replacement: &str,
        state: &ApplicationState,
    ) -> Result<LocalVocabularySheetResponse, CommandError> {
        with_current_local_vocabulary(meeting_id, digest, state, |vocabulary, turns| {
            vocabulary
                .add(source, replacement)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        })
    }

    fn local_vocabulary_edit_for_test(
        meeting_id: Uuid,
        digest: &str,
        id: Uuid,
        source: &str,
        replacement: &str,
        state: &ApplicationState,
    ) -> Result<LocalVocabularySheetResponse, CommandError> {
        with_current_local_vocabulary(meeting_id, digest, state, |vocabulary, turns| {
            vocabulary
                .edit(id, source, replacement)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        })
    }

    fn local_vocabulary_set_enabled_for_test(
        meeting_id: Uuid,
        digest: &str,
        id: Uuid,
        enabled: bool,
        state: &ApplicationState,
    ) -> Result<LocalVocabularySheetResponse, CommandError> {
        with_current_local_vocabulary(meeting_id, digest, state, |vocabulary, turns| {
            vocabulary
                .set_enabled(id, enabled)
                .map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        })
    }

    fn local_vocabulary_delete_for_test(
        meeting_id: Uuid,
        digest: &str,
        id: Uuid,
        state: &ApplicationState,
    ) -> Result<LocalVocabularySheetResponse, CommandError> {
        with_current_local_vocabulary(meeting_id, digest, state, |vocabulary, turns| {
            vocabulary.delete(id).map_err(local_vocabulary_error)?;
            local_vocabulary_sheet_response(vocabulary, turns)
        })
    }

    fn gated_document_with_voiceprint(voiceprint: &str) -> Vec<u8> {
        format!(
            r#"{{
          "schema":"capture-transcript/1",
          "source":"fixture",
          "attribution":"channel",
          "bleed":null,
          "voiceprint":{voiceprint},
          "capture_health":{{}},
          "turns":[
            {{"start":0.0,"end":1.0,"speaker":"Me","text":"visible"}},
            {{"start":1.0,"end":2.0,"speaker":"Me","text":"withheld","gated":true,"gate_score":0.1,"gate_reason":"fixture"}}
          ]
        }}"#
        )
        .into_bytes()
    }

    #[test]
    fn a_co_located_speaker_alert_reaches_the_transcript_screen_first() {
        // The alert's only route to a human ran through a note, and no note
        // generator is admitted — so before this it could fire and reach nobody
        // while the gate deleted a colleague from a meeting that cannot be
        // re-run.
        let document = gated_document_with_voiceprint(
            r#"{"applied":true,"persistent_other":true,"rejected_seconds":100.0,
                "coherent_share":0.51,"measured_frr":0.05,"n_sittings":3}"#,
        );
        let (_turns, warnings) =
            parse_transcript_projection_with(&document, &BTreeSet::new()).unwrap();
        assert!(
            warnings[0].contains("keeps returning as one voice"),
            "the alert must lead, not sit among boilerplate: {warnings:?}"
        );
        // 51 seconds, not 100. `rejected_seconds` is everything the gate
        // dropped; only `coherent_share` of it was the one recurring voice, and
        // quoting the total beside "someone next to you is being removed"
        // overstates that person's loss on the one number an operator uses to
        // decide whether to reconstruct their contribution.
        assert!(warnings[0].contains("51 seconds"), "{warnings:?}");
        assert!(!warnings[0].contains("100 seconds"), "{warnings:?}");
        assert!(warnings[0].contains("51% of everything"), "{warnings:?}");
        assert!(warnings[0].contains("cannot be re-run"), "{warnings:?}");
    }

    #[test]
    fn an_alert_without_a_share_states_no_extent_rather_than_the_gate_total() {
        // The producer always writes `coherent_share` beside the flag, so this
        // is the malformed-artifact case — and the answer is silence, not the
        // total, which would be the overstatement this fixes.
        let document = gated_document_with_voiceprint(
            r#"{"applied":true,"persistent_other":true,"rejected_seconds":100.0}"#,
        );
        let (_turns, warnings) =
            parse_transcript_projection_with(&document, &BTreeSet::new()).unwrap();
        assert!(
            warnings[0].contains("keeps returning as one voice"),
            "{warnings:?}"
        );
        assert!(!warnings[0].contains("seconds"), "{warnings:?}");
        assert!(!warnings[0].contains("100"), "{warnings:?}");
    }

    #[test]
    fn a_gate_that_ran_always_says_what_its_threshold_was_measured_on() {
        // Stated even when nothing was withheld: a clean transcript was still
        // decided by this threshold.
        let document = gated_document_with_voiceprint(
            r#"{"applied":true,"persistent_other":false,"rejected_seconds":0.0,
                "measured_frr":0.05,"n_sittings":3}"#,
        );
        let (_turns, warnings) =
            parse_transcript_projection_with(&document, &BTreeSet::new()).unwrap();
        let disclosure = warnings.last().expect("a disclosure");
        assert!(disclosure.contains("not on meeting audio"), "{warnings:?}");
        assert!(
            disclosure.contains("3 enrolment sitting(s)"),
            "{warnings:?}"
        );
        assert!(disclosure.contains("5%"), "{warnings:?}");
        // No alert, because the dropped speech was not one recurring voice.
        assert!(
            !warnings.iter().any(|line| line.contains("one voice")),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_gate_that_did_not_run_claims_nothing_about_a_threshold() {
        // `applied:false` is the skipped-on-bleed case. A disclosure here would
        // imply a check the app did not perform, which is the rule this screen
        // already holds.
        let document = gated_document_with_voiceprint(
            r#"{"applied":false,"why":"bleed above the attribution cut","persistent_other":true}"#,
        );
        let (_turns, warnings) =
            parse_transcript_projection_with(&document, &BTreeSet::new()).unwrap();
        assert!(
            !warnings.iter().any(|line| line.contains("threshold")),
            "{warnings:?}"
        );
        assert!(
            !warnings.iter().any(|line| line.contains("one voice")),
            "{warnings:?}"
        );
    }

    #[test]
    fn an_unknown_capture_side_voiceprint_field_still_opens_the_transcript() {
        // The capture writes the full provenance block. Pinning its shape here
        // would turn every future field into a transcript that will not open.
        let document = gated_document_with_voiceprint(
            r#"{"applied":true,"persistent_other":false,"encoder_fingerprint":"x",
                "versions":{"anything":"1"},"a_field_added_next_year":7}"#,
        );
        assert!(parse_transcript_projection_with(&document, &BTreeSet::new()).is_ok());
    }

    #[test]
    fn a_missing_measurement_drops_the_basis_rather_than_inventing_one() {
        let document = gated_document_with_voiceprint(r#"{"applied":true}"#);
        let (_turns, warnings) =
            parse_transcript_projection_with(&document, &BTreeSet::new()).unwrap();
        let disclosure = warnings.last().expect("a disclosure");
        assert!(disclosure.contains("not on meeting audio"), "{warnings:?}");
        assert!(!disclosure.contains("sitting(s)"), "{warnings:?}");
    }

    #[test]
    fn channel_gated_turn_not_labeled_me_is_refused() {
        let document = br#"{
          "schema":"capture-transcript/1",
          "source":"fixture",
          "attribution":"channel",
          "bleed":null,
          "voiceprint":null,
          "capture_health":{},
          "turns":[
            {"start":0.0,"end":1.0,"speaker":"Them","text":"withheld","gated":true}
          ]
        }"#;
        assert!(parse_transcript_projection_with(document, &BTreeSet::new()).is_err());
    }

    #[test]
    /// The digest travels with the projection it verified, and leaves with it.
    ///
    /// It is what the frozen restore shape requires back, so the recording
    /// screen can only ever name the transcript the operator was reading. If it
    /// outlived a dismissal it would name a meeting that is no longer open; if
    /// it were absent after a restart the screen would render withheld turns
    /// with the control silently missing, which is the state feature 6 has been
    /// in since the gate started withholding anything.
    #[test]
    fn the_bound_transcript_digest_arrives_and_leaves_with_its_projection() {
        let mut model = AppModel::default();
        assert!(model.snapshot().current_transcript_sha256.is_none());
        transition_startup(&mut model, StartupState::Checking).unwrap();

        apply_restored_transcript_projection(
            &mut model,
            RestoredTranscriptProjection {
                meeting_id: "meeting".into(),
                turns: vec![TranscriptTurn {
                    source_turn_index: 0,
                    source_speaker: Some("Me".into()),
                    speaker: Some("Me".into()),
                    speaker_corrected: false,
                    start: 0.0,
                    text: "words".into(),
                    withheld: false,
                }],
                current_transcript_sha256: "a".repeat(64),
                warnings: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(
            model.snapshot().current_transcript_sha256.as_deref(),
            Some("a".repeat(64).as_str())
        );

        model.clear_meeting_projection();
        assert!(model.snapshot().current_transcript_sha256.is_none());
        assert!(model.snapshot().turns.is_empty());
    }

    #[test]
    fn snapshot_marks_when_an_active_capture_phase_began() {
        let mut model = AppModel::default();
        transition_capture(&mut model, CaptureState::Arming).unwrap();
        assert!(
            model
                .snapshot()
                .capture_state_started_at_epoch_seconds
                .is_some()
        );
        assert!(
            model
                .snapshot()
                .transcription_last_worker_heartbeat_at_epoch_seconds
                .is_none()
        );

        transition_capture(&mut model, CaptureState::RecoveredInterrupted).unwrap();
        assert!(
            model
                .snapshot()
                .capture_state_started_at_epoch_seconds
                .is_none()
        );
    }

    #[test]
    fn transcription_heartbeat_marks_the_matching_active_meeting() {
        let state = ApplicationState::default();
        let meeting_id = Uuid::new_v4();
        {
            let mut model = state.model.lock().unwrap();
            model.meeting_id = Some(meeting_id.to_string());
            transition_capture(&mut model, CaptureState::Arming).unwrap();
            transition_capture(&mut model, CaptureState::Recording).unwrap();
            transition_capture(&mut model, CaptureState::Stopping).unwrap();
            transition_capture(&mut model, CaptureState::Captured).unwrap();
            transition_capture(&mut model, CaptureState::Transcribing).unwrap();
        }
        let heartbeat = WorkerProgress {
            schema: local_meeting_notes_session_core::protocol::WorkerEventSchema::V2,
            request_id: Uuid::new_v4(),
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Transcribing,
            meeting_id,
        };

        record_transcription_heartbeat(&state, &heartbeat).unwrap();
        assert!(
            state
                .model
                .lock()
                .unwrap()
                .snapshot()
                .transcription_last_worker_heartbeat_at_epoch_seconds
                .is_some()
        );
    }

    #[test]
    fn durable_queue_release_moves_stopping_capture_through_captured_to_idle() {
        let mut model = AppModel::default();
        transition_capture(&mut model, CaptureState::Arming).unwrap();
        transition_capture(&mut model, CaptureState::Recording).unwrap();
        transition_capture(&mut model, CaptureState::Stopping).unwrap();
        transition_capture(&mut model, CaptureState::Captured).unwrap();
        transition_capture(&mut model, CaptureState::Idle).unwrap();
        assert_eq!(model.reducer.capture(), CaptureState::Idle);
    }

    #[test]
    fn queue_recovery_selection_repairs_results_but_skips_terminal_items() {
        use local_meeting_notes_session_core::transcription_queue::{
            TranscriptionQueueItem, TranscriptionRequestSchema, TranscriptionResult,
            TranscriptionResultSchema, TranscriptionTerminal, TranscriptionTerminalKind,
            TranscriptionTerminalSchema,
        };
        let request = TranscriptionRequest {
            schema: TranscriptionRequestSchema::V1,
            request_id: Uuid::new_v4(),
            meeting_id: Uuid::new_v4().to_string(),
            capture_session_sha256: "a".repeat(64),
            microphone_audio_sha256: "b".repeat(64),
            system_audio_sha256: "c".repeat(64),
            model_identity: "model/v1".into(),
            worker_runtime_identity: "runtime/v1".into(),
            enqueued_at_epoch_seconds: 1,
        };
        let result = TranscriptionResult {
            schema: TranscriptionResultSchema::V1,
            request_id: request.request_id,
            meeting_id: request.meeting_id.clone(),
            transcript: ArtifactRef {
                relative_path: "transcript/test.json".into(),
                sha256: "d".repeat(64),
            },
            result_sha256: "e".repeat(64),
            produced_at_epoch_seconds: 2,
        };
        let repairable = TranscriptionQueueItem {
            request: request.clone(),
            claim: None,
            result: Some(result),
            terminal: None,
            commit: None,
        };
        assert!(queue_item_is_repairable(&repairable));
        assert!(!queue_item_is_eligible(&repairable));
        let quarantined = TranscriptionQueueItem {
            request,
            claim: None,
            result: Some(repairable.result.clone().unwrap()),
            terminal: Some(TranscriptionTerminal {
                schema: TranscriptionTerminalSchema::V1,
                request_id: repairable.request.request_id,
                kind: TranscriptionTerminalKind::Quarantined,
                at_epoch_seconds: 3,
            }),
            commit: None,
        };
        assert!(!queue_item_is_repairable(&quarantined));
        assert!(!queue_item_is_eligible(&quarantined));
    }

    fn view_current_transcript_resolves_with_restored_turn_visible() {
        use local_meeting_notes_session_core::operations::{TranscriptView, TranscriptViewSchema};
        use local_meeting_notes_session_core::storage::{create_private_dir, durable_create_new};

        let temporary = tempfile::TempDir::new().unwrap();
        let meeting_dir = temporary.path().join("meeting");
        create_private_dir(&meeting_dir).unwrap();
        create_private_dir(&meeting_dir.join("transcript")).unwrap();
        let base_bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "capture-transcript/1",
            "source": "fixture",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [
                {"start": 0.0, "end": 1.0, "speaker": "Me", "text": "visible"},
                {"start": 1.0, "end": 2.0, "speaker": "Me", "text": "restored words",
                 "gated": true, "gate_score": 0.1, "gate_reason": "fixture"},
                {"start": 2.0, "end": 3.0, "speaker": "Me", "text": "still hidden",
                 "gated": true, "gate_score": 0.2, "gate_reason": "fixture"}
            ]
        }))
        .unwrap();
        let base_digest = format!("{:x}", Sha256::digest(&base_bytes));
        durable_create_new(
            &meeting_dir.join(format!("transcript/{base_digest}.json")),
            &base_bytes,
        )
        .unwrap();
        let meeting_id = Uuid::new_v4();
        let view = TranscriptView {
            schema: TranscriptViewSchema::V1,
            meeting_id,
            base_transcript_sha256: base_digest.clone(),
            parent_transcript_sha256: base_digest,
            restored_source_turn_indices: vec![1],
        };
        let view_bytes = serde_json::to_vec_pretty(&view).unwrap();
        let view_relative = format!("transcript/{:x}.json", Sha256::digest(&view_bytes));
        durable_create_new(&meeting_dir.join(&view_relative), &view_bytes).unwrap();
        let reference = artifact_ref(&meeting_dir, &view_relative).unwrap();

        let (turns, warnings) = project_current_transcript(
            &meeting_dir,
            &meeting_id.to_string(),
            &reference,
            &view_bytes,
            true,
        )
        .unwrap();
        assert_eq!(turns.len(), 3);
        assert!(!turns[1].withheld);
        assert_eq!(turns[1].text, "restored words");
        assert_eq!(turns[1].speaker.as_deref(), Some("Me"));
        assert!(turns[2].withheld);
        assert!(turns[2].text.is_empty());
        // Only the still-withheld turn is counted in the warning.
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("withheld 1"));
    }

    #[test]
    fn fresh_process_projects_latest_valid_transcript() {
        let (_temporary, storage) = test_storage();
        let older = Uuid::new_v4().to_string();
        let newer = Uuid::new_v4().to_string();
        write_transcript_fixture(&storage, &older, 10, AudioState::Retained, "older");
        write_transcript_fixture(&storage, &newer, 20, AudioState::Retained, "newer");

        let projection = load_latest_transcript_projection(&storage, &[newer.clone(), older])
            .unwrap()
            .unwrap();
        assert_eq!(projection.meeting_id, newer);
        assert_eq!(projection.turns[0].text, "newer");
    }

    #[test]
    fn restored_projection_becomes_the_ready_startup_snapshot() {
        let mut model = AppModel::default();
        transition_startup(&mut model, StartupState::Checking).unwrap();
        apply_restored_transcript_projection(
            &mut model,
            RestoredTranscriptProjection {
                meeting_id: "meeting-fixture".into(),
                turns: vec![TranscriptTurn {
                    source_turn_index: 0,
                    source_speaker: Some("Me".into()),
                    speaker: Some("Me".into()),
                    speaker_corrected: false,
                    start: 0.0,
                    text: "visible".into(),
                    withheld: false,
                }],
                current_transcript_sha256: "b".repeat(64),
                warnings: Vec::new(),
            },
        )
        .unwrap();
        transition_startup(&mut model, StartupState::Ready).unwrap();

        let snapshot = model.snapshot();
        assert_eq!(snapshot.startup, StartupState::Ready);
        assert_eq!(snapshot.capture, CaptureState::TranscriptReady);
        assert_eq!(snapshot.meeting_id.as_deref(), Some("meeting-fixture"));
        assert_eq!(snapshot.turns[0].text, "visible");
    }

    #[test]
    fn keeping_the_current_transcript_reports_only_that() {
        let response = retry_decision_response(TranscriptRetryOutcome::CurrentKept).unwrap();

        assert_eq!(response.outcome, "current-kept");
        assert_eq!(response.message, "The current transcript was kept.");
    }

    /// The promoted copy is deliberately identical whether or not the meeting
    /// had a generated note, because this command is handed one settled outcome
    /// and no note fact. Both cases must read truthfully off the same string.
    #[test]
    fn promoting_a_retry_never_claims_a_note_was_cleared() {
        let response = retry_decision_response(TranscriptRetryOutcome::CandidatePromoted).unwrap();

        assert_eq!(response.outcome, "candidate-promoted");
        assert_eq!(
            response.message,
            "The retry transcript is now current. Generate a new note when you're ready."
        );
        assert!(!response.message.contains("cleared"));
        assert!(!response.message.contains("previous note"));
    }

    #[test]
    fn an_undecided_candidate_is_refused_rather_than_reported_as_settled() {
        assert_eq!(
            retry_decision_response(TranscriptRetryOutcome::CandidateAvailableForComparison)
                .unwrap_err()
                .message,
            "The retry candidate is still awaiting a decision."
        );
    }

    #[test]
    fn startup_retry_preserves_the_specific_attempt_failure() {
        let mut model = AppModel::default();
        transition_startup(&mut model, StartupState::Checking).unwrap();
        transition_startup(&mut model, StartupState::Ready).unwrap();
        transition_capture(&mut model, CaptureState::Arming).unwrap();
        transition_capture(&mut model, CaptureState::RecoveredInterrupted).unwrap();
        transition_startup(&mut model, StartupState::DiagnosticWritten).unwrap();
        model.error =
            Some("Microphone access was not granted. Nothing was marked complete.".into());

        prepare_startup_retry(&mut model).unwrap();

        assert_eq!(model.reducer.startup(), StartupState::Retrying);
        assert_eq!(
            model.error.as_deref(),
            Some("Microphone access was not granted. Nothing was marked complete.")
        );
    }

    #[test]
    fn startup_retry_clears_an_idle_installation_failure() {
        let mut model = AppModel::default();
        transition_startup(&mut model, StartupState::Checking).unwrap();
        transition_startup(&mut model, StartupState::DiagnosticWritten).unwrap();
        model.error = Some("stale installation failure".into());

        prepare_startup_retry(&mut model).unwrap();

        assert_eq!(model.reducer.startup(), StartupState::Retrying);
        assert!(model.error.is_none());
    }

    #[test]
    fn equal_attempt_times_choose_the_greatest_meeting_id() {
        let (_temporary, storage) = test_storage();
        let mut ids = [Uuid::new_v4().to_string(), Uuid::new_v4().to_string()];
        ids.sort();
        write_transcript_fixture(&storage, &ids[0], 10, AudioState::Retained, "lower");
        write_transcript_fixture(&storage, &ids[1], 10, AudioState::Retained, "higher");

        let projection =
            load_latest_transcript_projection(&storage, &[ids[1].clone(), ids[0].clone()])
                .unwrap()
                .unwrap();
        assert_eq!(projection.meeting_id, ids[1]);
        assert_eq!(projection.turns[0].text, "higher");
    }

    #[test]
    fn fresh_process_keeps_transcript_visible_after_audio_release() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(
            &storage,
            &meeting_id,
            10,
            AudioState::Released,
            "still here",
        );

        let projection =
            load_latest_transcript_projection(&storage, std::slice::from_ref(&meeting_id))
                .unwrap()
                .unwrap();
        assert_eq!(projection.turns[0].text, "still here");
        assert!(projection.warnings.iter().any(|warning| {
            warning.contains("audio was deleted") && warning.contains("transcript remains")
        }));
    }

    #[test]
    fn fresh_process_with_no_transcript_remains_idle() {
        let (_temporary, storage) = test_storage();
        assert!(
            load_latest_transcript_projection(&storage, &[])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn changed_transcript_is_not_projected() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        let transcript_path =
            write_transcript_fixture(&storage, &meeting_id, 10, AudioState::Retained, "before");
        fs::write(transcript_path, b"changed").unwrap();

        assert!(load_latest_transcript_projection(&storage, &[meeting_id]).is_err());
    }

    #[test]
    fn preview_bound_transcript_rejects_pointer_replacement_after_reader_validation() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        write_transcript_fixture(
            &storage,
            &meeting_id,
            10,
            AudioState::Retained,
            "stable synthetic words",
        );
        let directory = meeting_dir(&storage, &meeting_id).unwrap();
        let mut meeting = load_meeting(&directory).unwrap();
        let expected = meeting.artifacts.current_transcript.clone().unwrap();
        let (turns, _) =
            load_bound_preview_transcript_projection(&storage, &meeting_id, &expected).unwrap();
        assert_eq!(turns[0].text, "stable synthetic words");

        let replacement = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic-replacement-fixture",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [{
                "start": 0.0,
                "end": 1.0,
                "speaker": "Me",
                "text": "replacement synthetic words",
            }],
        }))
        .unwrap();
        let replacement_digest = format!("{:x}", Sha256::digest(&replacement));
        let replacement_relative = format!("transcript/{replacement_digest}.json");
        durable_create_new(&directory.join(&replacement_relative), &replacement).unwrap();
        meeting.artifacts.current_transcript =
            Some(artifact_ref(&directory, &replacement_relative).unwrap());
        write_meeting(&directory, &meeting).unwrap();

        assert!(
            load_bound_preview_transcript_projection(&storage, &meeting_id, &expected).is_err()
        );
    }

    #[test]
    fn preview_bound_transcript_hashes_the_exact_bytes_it_parses() {
        let (_temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4().to_string();
        let transcript_path = write_transcript_fixture(
            &storage,
            &meeting_id,
            10,
            AudioState::Retained,
            "stable synthetic words",
        );
        let directory = meeting_dir(&storage, &meeting_id).unwrap();
        let expected = load_meeting(&directory)
            .unwrap()
            .artifacts
            .current_transcript
            .unwrap();
        fs::write(transcript_path, b"changed synthetic bytes").unwrap();

        assert!(
            load_bound_preview_transcript_projection(&storage, &meeting_id, &expected).is_err()
        );
    }
}
