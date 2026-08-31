//! Storage-backed coordinator and worker-process bridge for the frozen
//! product operations behind `product_facade`.
//!
//! This supplies the real machinery the facade was designed around: restore
//! requests cross the supervised worker process boundary and the session-core
//! coordinator re-verifies every artifact from storage before publication.
//! Nothing here widens the packaged runtime's admission — an operation the
//! worker's frozen set excludes is refused by the worker itself and surfaces
//! through the facade's generic copy. Registering the facade commands (and
//! pairing each accepted synchronous operation with
//! `ProductOperationFacade::finish`, since `accept_restore` returns only after
//! the coordinator's terminal receipt) remains a separate slice gated on the
//! operator's admission decision.
//!
//! Note regeneration composes two seams (docs/note-runtime-decision.md,
//! slice 3): the sandboxed generate child proposes located candidate points
//! under the manifest-pinned model, and the worker's `note.create` assembles
//! and publishes the note deterministically from that payload. The
//! generation object crosses this module verbatim -- the worker's frozen
//! contract deep-validates it, the assembler re-derives every digest from
//! the retained transcript, and the coordinator re-inspects the published
//! pair before anything lands in the meeting record.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use local_meeting_notes_session_core::meeting::{
    AudioState, NoteRevisionRef, load_meeting, read_private_bytes, verify_record_artifacts,
};
use local_meeting_notes_session_core::meeting_coordination::MeetingStorageCoordination;
use local_meeting_notes_session_core::note_generation::{
    NoteArtifactError, NoteArtifactInspector, NoteGenerationCoordinator,
    NoteGenerationCoordinatorError, NoteGenerationWorker, NoteGenerationWorkerError,
    NoteWorkerResult,
};
use local_meeting_notes_session_core::note_projector_process::{
    GENERATE_MANIFEST_FILE, GenerateNoteRequest, NoteGenerationChildOutcome, ProcessNoteGenerator,
    admit_note_generator, parse_note_generation_result,
};
use local_meeting_notes_session_core::operations::{
    NoteCreateWorkerArgs, NoteCreateWorkerFailure, NoteCreateWorkerFailureCode,
    TranscriptRestoreWorkerArgs, TranscriptRetryUiArgs,
};
use local_meeting_notes_session_core::protocol::{
    CaptureProgressState, Operation, ProgressEvent, ProtocolError, WorkerCommand, WorkerProgress,
    WorkerResult,
};
use local_meeting_notes_session_core::retention::AppDataWriterLock;
use local_meeting_notes_session_core::runtime::RuntimeManifest;
use local_meeting_notes_session_core::storage::StorageRoot;
use local_meeting_notes_session_core::supervision::OwnedChild;
use local_meeting_notes_session_core::transcript_restoration::{
    StoredTranscriptArtifactInspector, TranscriptRestorationCoordinator,
    TranscriptRestorationCoordinatorError, TranscriptRestoreWorker, TranscriptRestoreWorkerError,
};
use local_meeting_notes_session_core::transcript_retry::{
    TranscriptRetryCandidate, TranscriptRetryOutcome, TranscriptRetryPendingCandidate,
    TranscriptRetrySourceBinding,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::product_facade::{
    CoordinatorError, MeetingOperationSource, ProductOperationCoordinator, TranscriptRetryDecision,
    TranscriptRetryOperation,
};
use crate::{StorageContext, WORKER_REQUEST_TIMEOUT};

/// One request/response exchange with the worker process. The seam exists so
/// bridge translation can be exercised without a live subprocess; the real
/// port owns the take-on-supervision-error semantics for the shared child.
pub(crate) trait WorkerPort: Send + Sync {
    fn request(
        &self,
        operation: Operation,
        arguments: Value,
        timeout: Duration,
    ) -> Result<WorkerResult, WorkerPortUnavailable>;

    /// Retry uses the worker's transcribing heartbeat. The default keeps
    /// deterministic fake ports small; the real process port overrides it so
    /// progress is parsed and admitted rather than rejected as an unknown
    /// frame by `request`.
    fn request_transcript_retry(
        &self,
        arguments: Value,
        timeout: Duration,
    ) -> Result<WorkerResult, WorkerPortUnavailable> {
        self.request(Operation::TranscriptRetry, arguments, timeout)
    }
}

/// The worker process could not serve the exchange at all. Carries the
/// supervisor's own description for diagnostics; never worker content.
#[derive(Debug)]
pub(crate) struct WorkerPortUnavailable(pub(crate) String);

pub(crate) struct ProcessWorkerPort {
    worker: Arc<Mutex<Option<OwnedChild>>>,
}

impl ProcessWorkerPort {
    pub(crate) fn new(worker: Arc<Mutex<Option<OwnedChild>>>) -> Self {
        Self { worker }
    }

    pub(crate) fn request_with_progress<F>(
        &self,
        operation: Operation,
        arguments: Value,
        timeout: Duration,
        on_progress: F,
    ) -> Result<WorkerResult, WorkerPortUnavailable>
    where
        F: FnMut(&WorkerProgress) -> Result<(), ProtocolError>,
    {
        let command = WorkerCommand::new(operation, arguments);
        let mut guard = self
            .worker
            .lock()
            .map_err(|_| WorkerPortUnavailable("worker process lock is poisoned".into()))?;
        let Some(worker) = guard.as_mut() else {
            return Err(WorkerPortUnavailable("worker is unavailable".into()));
        };
        match worker.request_until(&command, Instant::now() + timeout, on_progress) {
            Ok(result) => Ok(result),
            Err(error) => {
                guard.take();
                Err(WorkerPortUnavailable(error.to_string()))
            }
        }
    }
}

impl WorkerPort for ProcessWorkerPort {
    fn request(
        &self,
        operation: Operation,
        arguments: Value,
        timeout: Duration,
    ) -> Result<WorkerResult, WorkerPortUnavailable> {
        self.request_with_progress(operation, arguments, timeout, |_| {
            Err(ProtocolError::InvalidEvent)
        })
    }

    fn request_transcript_retry(
        &self,
        arguments: Value,
        timeout: Duration,
    ) -> Result<WorkerResult, WorkerPortUnavailable> {
        self.request_with_progress(
            Operation::TranscriptRetry,
            arguments,
            timeout,
            retry_progress_is_admitted,
        )
    }
}

/// Sends `transcript.restore` across the process boundary. The returned digest
/// map is a claim only; the coordinator re-reads and re-verifies the artifacts
/// it names before anything is published.
pub(crate) struct WorkerProcessRestoreBridge {
    port: Arc<dyn WorkerPort>,
}

impl WorkerProcessRestoreBridge {
    pub(crate) fn new(port: Arc<dyn WorkerPort>) -> Self {
        Self { port }
    }
}

impl TranscriptRestoreWorker for WorkerProcessRestoreBridge {
    fn restore(
        &self,
        arguments: &TranscriptRestoreWorkerArgs,
    ) -> Result<HashMap<String, String>, TranscriptRestoreWorkerError> {
        let arguments = serde_json::to_value(arguments)
            .map_err(|_| TranscriptRestoreWorkerError::Unavailable)?;
        let result = self
            .port
            .request(
                Operation::TranscriptRestore,
                arguments,
                WORKER_REQUEST_TIMEOUT,
            )
            .map_err(|_| TranscriptRestoreWorkerError::Unavailable)?;
        if result.ok {
            Ok(result.artifact_digests)
        } else {
            Err(TranscriptRestoreWorkerError::Refused)
        }
    }
}

/// Runs the two-step note generation behind the session-core coordinator's
/// `NoteGenerationWorker` seam: admit and run the sandboxed generate child,
/// then hand its validated points to the worker's `note.create` assembler.
///
/// Admission happens per call, from live storage state -- the same rule as
/// the projector cache's source: a model installed or removed since startup
/// must change the answer.
pub(crate) struct WorkerProcessNoteGenerationBridge {
    port: Arc<dyn WorkerPort>,
    storage: Arc<Mutex<Option<StorageContext>>>,
}

impl WorkerProcessNoteGenerationBridge {
    pub(crate) fn new(
        port: Arc<dyn WorkerPort>,
        storage: Arc<Mutex<Option<StorageContext>>>,
    ) -> Self {
        Self { port, storage }
    }

    fn admitted_generator(&self) -> Option<ProcessNoteGenerator> {
        let context = self.storage.lock().ok()?.clone()?;
        let manifest = RuntimeManifest::load_and_verify(&context.manifest_path).ok()?;
        let catalog = crate::verified_model_catalog(&context.manifest_path, &manifest).ok()??;
        admit_note_generator(
            &context.storage,
            &catalog,
            &context.resource_root.join(GENERATE_MANIFEST_FILE),
        )
    }
}

impl NoteGenerationWorker for WorkerProcessNoteGenerationBridge {
    fn create(
        &self,
        arguments: &NoteCreateWorkerArgs,
    ) -> Result<NoteWorkerResult, NoteGenerationWorkerError> {
        let generator = self
            .admitted_generator()
            .ok_or(NoteGenerationWorkerError::Unavailable)?;
        let request = GenerateNoteRequest {
            request_id: Uuid::new_v4(),
            meeting_id: arguments.meeting_id.to_string(),
            transcript_sha256: arguments.source_transcript_sha256.clone(),
            speaker_label_overrides: arguments.speaker_label_overrides.clone(),
            vocabulary_replacements: arguments.vocabulary_replacements.clone(),
            pre_meeting_context: arguments.pre_meeting_context.clone(),
        };
        let frame = generator
            .generate(&request)
            .map_err(|_| NoteGenerationWorkerError::Unavailable)?;
        let generation = match parse_note_generation_result(&frame, &request)
            .map_err(|_| NoteGenerationWorkerError::Unavailable)?
        {
            NoteGenerationChildOutcome::Generated(generation) => generation,
            NoteGenerationChildOutcome::TranscriptOnly { recoverable, .. } => {
                // The child's failure codes are content-free by design; the
                // durable receipt records the one product fact -- no note,
                // transcript stands -- plus whether a retry could differ.
                return Ok(NoteWorkerResult::Rejected(NoteCreateWorkerFailure {
                    code: NoteCreateWorkerFailureCode::NoteRejected,
                    recoverable,
                    artifact_digests: HashMap::new(),
                }));
            }
        };
        let mut create_arguments =
            serde_json::to_value(arguments).map_err(|_| NoteGenerationWorkerError::Unavailable)?;
        let Some(fields) = create_arguments.as_object_mut() else {
            return Err(NoteGenerationWorkerError::Unavailable);
        };
        fields.insert("generation".into(), generation);
        let result = self
            .port
            .request(
                Operation::NoteCreate,
                create_arguments,
                WORKER_REQUEST_TIMEOUT,
            )
            .map_err(|_| NoteGenerationWorkerError::Unavailable)?;
        if result.ok {
            Ok(NoteWorkerResult::Accepted(result.artifact_digests))
        } else {
            // The generate child already validated these points against the
            // same retained transcript, so an assembler refusal is a local
            // contract break, not a model outcome.
            Err(NoteGenerationWorkerError::Refused)
        }
    }
}

/// Fresh `note.inspect` across the process boundary: the published pair must
/// re-verify content-addressed and semantically passing before the meeting
/// record advances. The worker's refusal is deliberately reasonless; either
/// artifact fault maps to "changed", and only a dead port reads as missing.
pub(crate) struct WorkerProcessNoteInspectBridge {
    port: Arc<dyn WorkerPort>,
}

impl WorkerProcessNoteInspectBridge {
    pub(crate) fn new(port: Arc<dyn WorkerPort>) -> Self {
        Self { port }
    }
}

impl NoteArtifactInspector for WorkerProcessNoteInspectBridge {
    fn inspect(
        &self,
        _meeting_dir: &Path,
        meeting_id: Uuid,
        note: &NoteRevisionRef,
    ) -> Result<HashMap<String, String>, NoteArtifactError> {
        let arguments = serde_json::json!({
            "meeting_id": meeting_id.to_string(),
            "note_id": note.json.sha256,
            "transcript_id": note.source_transcript_sha256,
        });
        let result = self
            .port
            .request(Operation::NoteInspect, arguments, WORKER_REQUEST_TIMEOUT)
            .map_err(|_| NoteArtifactError::Missing)?;
        if result.ok {
            Ok(result.artifact_digests)
        } else {
            Err(NoteArtifactError::Changed)
        }
    }
}

/// The storage-backed `ProductOperationCoordinator`. Holds live handles to the
/// application's storage, writer-lock, and worker slots so every call reads
/// the current runtime state instead of a startup snapshot.
pub(crate) struct DesktopProductCoordinator {
    storage: Arc<Mutex<Option<StorageContext>>>,
    app_data_writer_lock: Arc<Mutex<Option<Arc<AppDataWriterLock>>>>,
    port: Arc<dyn WorkerPort>,
}

impl DesktopProductCoordinator {
    pub(crate) fn new(
        storage: Arc<Mutex<Option<StorageContext>>>,
        app_data_writer_lock: Arc<Mutex<Option<Arc<AppDataWriterLock>>>>,
        port: Arc<dyn WorkerPort>,
    ) -> Self {
        Self {
            storage,
            app_data_writer_lock,
            port,
        }
    }

    fn storage_root(&self) -> Result<StorageRoot, CoordinatorError> {
        self.storage
            .lock()
            .map_err(|_| CoordinatorError::Unavailable)?
            .as_ref()
            .map(|context| context.storage.clone())
            .ok_or(CoordinatorError::Unavailable)
    }

    fn coordination(&self) -> Result<Arc<MeetingStorageCoordination>, CoordinatorError> {
        self.app_data_writer_lock
            .lock()
            .map_err(|_| CoordinatorError::Unavailable)?
            .as_ref()
            .map(|writer| writer.coordination())
            .ok_or(CoordinatorError::Unavailable)
    }

    fn writer_lock(&self) -> Result<Arc<AppDataWriterLock>, CoordinatorError> {
        self.app_data_writer_lock
            .lock()
            .map_err(|_| CoordinatorError::Unavailable)?
            .as_ref()
            .cloned()
            .ok_or(CoordinatorError::Unavailable)
    }

    fn restoration_coordinator(
        &self,
    ) -> Result<TranscriptRestorationCoordinator, CoordinatorError> {
        TranscriptRestorationCoordinator::new(
            self.storage_root()?,
            self.coordination()?,
            Arc::new(WorkerProcessRestoreBridge::new(self.port.clone())),
            Arc::new(StoredTranscriptArtifactInspector),
        )
        .map_err(|_| CoordinatorError::Unavailable)
    }

    fn generation_coordinator(&self) -> Result<NoteGenerationCoordinator, CoordinatorError> {
        NoteGenerationCoordinator::new(
            self.storage_root()?,
            self.coordination()?,
            Arc::new(WorkerProcessNoteGenerationBridge::new(
                self.port.clone(),
                self.storage.clone(),
            )),
            Arc::new(WorkerProcessNoteInspectBridge::new(self.port.clone())),
        )
        .map_err(|_| CoordinatorError::Unavailable)
    }

    /// Reads the complete source identity while holding the meeting lease. It
    /// is deliberately separate from the worker result: the webview never
    /// names audio paths or digests, and the worker's echoed values are only
    /// compared to this locally reverified binding.
    fn retry_source_binding(
        &self,
        args: &TranscriptRetryUiArgs,
    ) -> Result<(StorageRoot, TranscriptRetrySourceBinding), CoordinatorError> {
        let storage = self.storage_root()?;
        let coordination = self.coordination()?;
        let _lease = coordination
            .acquire(&args.meeting_id.to_string())
            .map_err(|_| CoordinatorError::Unavailable)?;
        let meeting_dir = storage
            .resolve(&Path::new("meetings").join(args.meeting_id.to_string()))
            .map_err(|_| CoordinatorError::Refused)?;
        let meeting = load_meeting(&meeting_dir).map_err(|_| CoordinatorError::Refused)?;
        if meeting.meeting_id != args.meeting_id.to_string()
            || meeting.retention.state != AudioState::Retained
            || meeting.pending_storage_operation.is_some()
        {
            return Err(CoordinatorError::Refused);
        }
        verify_record_artifacts(&meeting_dir, &meeting).map_err(|_| CoordinatorError::Refused)?;
        let current = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(CoordinatorError::Refused)?;
        let capture = meeting
            .artifacts
            .capture_session
            .as_ref()
            .ok_or(CoordinatorError::Refused)?;
        let microphone = meeting
            .artifacts
            .microphone_audio
            .as_ref()
            .ok_or(CoordinatorError::Refused)?;
        let system = meeting
            .artifacts
            .system_audio
            .as_ref()
            .ok_or(CoordinatorError::Refused)?;
        if current.sha256 != args.source_transcript_sha256 {
            return Err(CoordinatorError::Refused);
        }
        Ok((
            storage,
            TranscriptRetrySourceBinding {
                source_transcript_sha256: current.sha256.clone(),
                capture_session_sha256: capture.sha256.clone(),
                microphone_audio_sha256: microphone.sha256.clone(),
                system_audio_sha256: system.sha256.clone(),
                // Filled only after the worker candidate bytes are re-read.
                candidate_transcript_sha256: String::new(),
            },
        ))
    }

    fn retry_operation(candidate: TranscriptRetryCandidate) -> TranscriptRetryOperation {
        TranscriptRetryOperation {
            operation_id: candidate.operation_id,
            meeting_id: Uuid::parse_str(&candidate.meeting_id).expect("authority validates UUID"),
            source_transcript_sha256: candidate.source_transcript_sha256,
            candidate_transcript: candidate.candidate_transcript,
        }
    }

    fn retry_operation_from_pending(
        candidate: TranscriptRetryPendingCandidate,
    ) -> Result<TranscriptRetryOperation, CoordinatorError> {
        if candidate.state
            != local_meeting_notes_session_core::transcript_retry::TranscriptRetryState::CandidateAvailableForComparison
        {
            return Err(CoordinatorError::Refused);
        }
        Ok(TranscriptRetryOperation {
            operation_id: candidate.operation_id,
            meeting_id: Uuid::parse_str(&candidate.meeting_id)
                .map_err(|_| CoordinatorError::Refused)?,
            source_transcript_sha256: candidate.source_transcript_sha256,
            candidate_transcript: local_meeting_notes_session_core::meeting::ArtifactRef {
                relative_path: format!("transcript/{}.json", candidate.candidate_transcript_sha256),
                sha256: candidate.candidate_transcript_sha256,
            },
        })
    }

    fn inspect_retry_under_lease(
        &self,
        meeting_id: Uuid,
        operation_id: Uuid,
    ) -> Result<TranscriptRetryOperation, CoordinatorError> {
        let coordination = self.coordination()?;
        let writer = self.writer_lock()?;
        let _lease = coordination
            .acquire(&meeting_id.to_string())
            .map_err(|_| CoordinatorError::Unavailable)?;
        let candidate = writer
            .transcript_retry_authority()
            .inspect_candidate(&meeting_id.to_string(), operation_id)
            .map_err(|_| CoordinatorError::Refused)?;
        Ok(Self::retry_operation(candidate))
    }

    fn discover_pending_retry(
        &self,
        args: &TranscriptRetryUiArgs,
    ) -> Result<Option<TranscriptRetryOperation>, CoordinatorError> {
        let writer = self.writer_lock()?;
        let pending = writer
            .transcript_retry_authority()
            .discover_pending_candidate(&args.meeting_id.to_string())
            .map_err(|_| CoordinatorError::Refused)?;
        match pending {
            Some(candidate)
                if candidate.source_transcript_sha256 == args.source_transcript_sha256 =>
            {
                Self::retry_operation_from_pending(candidate).map(Some)
            }
            Some(_) => Err(CoordinatorError::Refused),
            None => Ok(None),
        }
    }
}

impl ProductOperationCoordinator for DesktopProductCoordinator {
    /// A lease-free read the facade uses as its optimistic pre-check. The
    /// coordinator repeats every check authoritatively under the meeting
    /// lease, so this reflects the stored record without re-hashing artifacts.
    fn source_for(&self, meeting_id: Uuid) -> Result<MeetingOperationSource, CoordinatorError> {
        let storage = self.storage_root()?;
        let meeting_dir = storage
            .resolve(&Path::new("meetings").join(meeting_id.to_string()))
            .map_err(|_| CoordinatorError::Unavailable)?;
        let meeting = load_meeting(&meeting_dir).map_err(|_| CoordinatorError::Unavailable)?;
        let record_meeting_id =
            Uuid::parse_str(&meeting.meeting_id).map_err(|_| CoordinatorError::Refused)?;
        let current = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(CoordinatorError::Refused)?;
        Ok(MeetingOperationSource {
            meeting_id: record_meeting_id,
            current_transcript_sha256: current.sha256.clone(),
            lifecycle: meeting.lifecycle,
            has_current_note: meeting.artifacts.current_note.is_some(),
        })
    }

    fn accept_restore(
        &self,
        args: &local_meeting_notes_session_core::operations::RestoreWithheldTurnUiArgs,
    ) -> Result<Uuid, CoordinatorError> {
        self.restoration_coordinator()?
            .restore_withheld_turn(args)
            .map_err(|error| match error {
                TranscriptRestorationCoordinatorError::Worker(
                    TranscriptRestoreWorkerError::Unavailable,
                )
                | TranscriptRestorationCoordinatorError::StorageUnavailable => {
                    CoordinatorError::Unavailable
                }
                _ => CoordinatorError::Refused,
            })
    }

    fn accept_regeneration(
        &self,
        args: &local_meeting_notes_session_core::operations::RegenerateNoteUiArgs,
    ) -> Result<Uuid, CoordinatorError> {
        let storage = self.storage_root()?;
        self.generation_coordinator()?
            // The Tauri command derives this overlay from the local correction
            // sidecar. Re-derive it under the core coordinator's meeting lease
            // so a stale, malformed, or caller-supplied value cannot replace
            // the current note.
            .regenerate_note_with(args, |meeting_dir| {
                let current_overrides = crate::speaker_correction::current_label_overrides(
                    meeting_dir,
                    args.meeting_id,
                    &args.source_transcript_sha256,
                )
                .map_err(|_| {
                    NoteGenerationCoordinatorError::Ambiguous("speaker corrections are invalid")
                })?;
                if current_overrides != args.speaker_label_overrides {
                    return Err(NoteGenerationCoordinatorError::Ambiguous(
                        "speaker corrections changed",
                    ));
                }
                let current_vocabulary = crate::current_vocabulary_replacements(
                    &storage,
                    meeting_dir,
                    args.meeting_id,
                    &args.source_transcript_sha256,
                )
                .map_err(|_| {
                    NoteGenerationCoordinatorError::Ambiguous("local vocabulary is invalid")
                })?;
                if current_vocabulary != args.vocabulary_replacements {
                    return Err(NoteGenerationCoordinatorError::Ambiguous(
                        "local vocabulary changed",
                    ));
                }
                // Same re-attestation shape as the two overlays above, for the
                // operator's pre-meeting context sidecar: the Tauri command
                // derived `args.pre_meeting_context` from `meeting-context.json`
                // before this call, and a fresh read under the lease must still
                // agree with it before the worker sees it.
                let current_context = crate::meeting_context::read(meeting_dir);
                if current_context.unreadable {
                    return Err(NoteGenerationCoordinatorError::Ambiguous(
                        "meeting context could not be read",
                    ));
                }
                let current_context_value =
                    (!current_context.text.is_empty()).then_some(current_context.text);
                if current_context_value != args.pre_meeting_context {
                    return Err(NoteGenerationCoordinatorError::Ambiguous(
                        "meeting context changed",
                    ));
                }
                Ok(())
            })
            .map_err(|error| match error {
                NoteGenerationCoordinatorError::Worker(NoteGenerationWorkerError::Unavailable)
                | NoteGenerationCoordinatorError::StorageUnavailable => {
                    CoordinatorError::Unavailable
                }
                _ => CoordinatorError::Refused,
            })
    }

    fn start_transcript_retry(
        &self,
        args: &TranscriptRetryUiArgs,
    ) -> Result<TranscriptRetryOperation, CoordinatorError> {
        let (storage, expected) = self.retry_source_binding(args)?;
        if let Some(pending) = self.discover_pending_retry(args)? {
            return Ok(pending);
        }
        let worker_result = self
            .port
            .request_transcript_retry(
                serde_json::json!({
                    "meeting_id": args.meeting_id.to_string(),
                    "source_transcript_sha256": expected.source_transcript_sha256,
                    "capture_session_sha256": expected.capture_session_sha256,
                    "microphone_audio_sha256": expected.microphone_audio_sha256,
                    "system_audio_sha256": expected.system_audio_sha256,
                }),
                WORKER_REQUEST_TIMEOUT,
            )
            .map_err(|_| CoordinatorError::Unavailable)?;
        if !worker_result.ok {
            return Err(CoordinatorError::Refused);
        }
        let binding = retry_worker_binding(&worker_result.artifact_digests, &expected)?;
        let meeting_dir = storage
            .resolve(&Path::new("meetings").join(args.meeting_id.to_string()))
            .map_err(|_| CoordinatorError::Refused)?;
        let candidate_path = meeting_dir
            .join("transcript")
            .join(format!("{}.json", binding.candidate_transcript_sha256));
        let candidate_bytes = read_private_bytes(&candidate_path, 16 * 1024 * 1024)
            .map_err(|_| CoordinatorError::Refused)?;
        if format!("{:x}", Sha256::digest(&candidate_bytes)) != binding.candidate_transcript_sha256
        {
            return Err(CoordinatorError::Refused);
        }
        // The digest is now tied to the bytes in storage, rather than to the
        // worker's result map. The authority re-verifies all four sources and
        // writes the sole durable candidate receipt under its own lease.
        let operation_id = Uuid::new_v4();
        let writer = self.writer_lock()?;
        writer
            .transcript_retry_authority()
            .create_candidate(
                &args.meeting_id.to_string(),
                operation_id,
                &candidate_bytes,
                &binding,
            )
            .map_err(|_| CoordinatorError::Refused)?;
        self.inspect_retry_under_lease(args.meeting_id, operation_id)
    }

    fn pending_transcript_retry(
        &self,
        args: &TranscriptRetryUiArgs,
    ) -> Result<Option<TranscriptRetryOperation>, CoordinatorError> {
        let _ = self.retry_source_binding(args)?;
        self.discover_pending_retry(args)
    }

    fn decide_transcript_retry(
        &self,
        operation: &TranscriptRetryOperation,
        decision: TranscriptRetryDecision,
    ) -> Result<TranscriptRetryOutcome, CoordinatorError> {
        let _ = self.retry_source_binding(&TranscriptRetryUiArgs {
            meeting_id: operation.meeting_id,
            source_transcript_sha256: operation.source_transcript_sha256.clone(),
        })?;
        let writer = self.writer_lock()?;
        let authority = writer.transcript_retry_authority();
        let candidate = authority
            .inspect_candidate(&operation.meeting_id.to_string(), operation.operation_id)
            .map_err(|_| CoordinatorError::Refused)?;
        if candidate.source_transcript_sha256 != operation.source_transcript_sha256
            || candidate.candidate_transcript != operation.candidate_transcript
        {
            return Err(CoordinatorError::Refused);
        }
        match decision {
            TranscriptRetryDecision::KeepCurrent => {
                authority.keep_current(&operation.meeting_id.to_string(), operation.operation_id)
            }
            TranscriptRetryDecision::UseRetry => authority
                .promote_candidate(&operation.meeting_id.to_string(), operation.operation_id),
        }
        .map_err(|_| CoordinatorError::Refused)
    }
}

fn retry_worker_binding(
    digests: &HashMap<String, String>,
    expected: &TranscriptRetrySourceBinding,
) -> Result<TranscriptRetrySourceBinding, CoordinatorError> {
    const KEYS: [&str; 5] = [
        "candidate-transcript",
        "source-transcript",
        "capture-session",
        "capture-mic",
        "capture-system",
    ];
    if digests.len() != KEYS.len()
        || KEYS.iter().any(|key| !digests.contains_key(*key))
        || digests.values().any(|digest| !valid_sha256(digest))
        || digests["source-transcript"] != expected.source_transcript_sha256
        || digests["capture-session"] != expected.capture_session_sha256
        || digests["capture-mic"] != expected.microphone_audio_sha256
        || digests["capture-system"] != expected.system_audio_sha256
    {
        return Err(CoordinatorError::Refused);
    }
    Ok(TranscriptRetrySourceBinding {
        source_transcript_sha256: expected.source_transcript_sha256.clone(),
        capture_session_sha256: expected.capture_session_sha256.clone(),
        microphone_audio_sha256: expected.microphone_audio_sha256.clone(),
        system_audio_sha256: expected.system_audio_sha256.clone(),
        candidate_transcript_sha256: digests["candidate-transcript"].clone(),
    })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn retry_progress_is_admitted(progress: &WorkerProgress) -> Result<(), ProtocolError> {
    if progress.event == ProgressEvent::CaptureState
        && progress.state == CaptureProgressState::Transcribing
    {
        Ok(())
    } else {
        Err(ProtocolError::InvalidEvent)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use local_meeting_notes_session_core::meeting::AudioState;
    use local_meeting_notes_session_core::meeting::MeetingLifecycle;
    use local_meeting_notes_session_core::operations::{
        RegenerateNoteUiArgs, RestoreWithheldTurnUiArgs, SpeakerLabelOverride,
        TranscriptRetryUiArgs,
    };
    use local_meeting_notes_session_core::protocol::{ResultSchema, WorkerEventSchema};
    use local_meeting_notes_session_core::storage::durable_create_new;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::product_facade::{ProductOperationFacade, ProductOperationFacadeError};
    use crate::tests::{test_storage, write_transcript_fixture_with_turns};
    use crate::{ApplicationState, ensure_app_data_writer_lock};

    enum FakeOutcome {
        Accept(HashMap<String, String>),
        Refuse,
        Unavailable,
    }

    struct FakePort {
        requests: Mutex<Vec<(Operation, Value)>>,
        outcome: FakeOutcome,
    }

    impl FakePort {
        fn new(outcome: FakeOutcome) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                outcome,
            }
        }
    }

    impl WorkerPort for FakePort {
        fn request(
            &self,
            operation: Operation,
            arguments: Value,
            _timeout: Duration,
        ) -> Result<WorkerResult, WorkerPortUnavailable> {
            self.requests.lock().unwrap().push((operation, arguments));
            match &self.outcome {
                FakeOutcome::Accept(digests) => Ok(worker_result(true, digests.clone())),
                FakeOutcome::Refuse => Ok(worker_result(false, HashMap::new())),
                FakeOutcome::Unavailable => {
                    Err(WorkerPortUnavailable("worker is unavailable".into()))
                }
            }
        }
    }

    fn worker_result(ok: bool, artifact_digests: HashMap<String, String>) -> WorkerResult {
        WorkerResult {
            schema: ResultSchema::V2,
            request_id: Uuid::new_v4(),
            ok,
            code: None,
            recoverable: None,
            artifact_digests,
        }
    }

    fn restore_args(meeting_id: Uuid, sha256: &str, index: u32) -> TranscriptRestoreWorkerArgs {
        TranscriptRestoreWorkerArgs {
            meeting_id,
            source_transcript_sha256: sha256.into(),
            source_turn_index: index,
        }
    }

    /// The bridge's serialized arguments are the Python contract's exact key
    /// set; drift here would be refused by the worker at runtime only.
    #[test]
    fn restore_bridge_pins_the_wire_shape_and_passes_digests_through() {
        let digests: HashMap<String, String> = [
            ("base-transcript", "a"),
            ("parent-transcript", "b"),
            ("transcript", "c"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.repeat(64)))
        .collect();
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(digests.clone())));
        let bridge = WorkerProcessRestoreBridge::new(port.clone());

        let meeting_id = Uuid::new_v4();
        let sha256 = "d".repeat(64);
        let returned = bridge
            .restore(&restore_args(meeting_id, &sha256, 3))
            .unwrap();
        assert_eq!(returned, digests);

        let requests = port.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, Operation::TranscriptRestore);
        assert_eq!(
            requests[0].1,
            json!({
                "meeting_id": meeting_id.to_string(),
                "source_transcript_sha256": sha256,
                "source_turn_index": 3,
            })
        );
    }

    #[test]
    fn restore_bridge_maps_refusal_and_absence_to_typed_errors() {
        let refused = WorkerProcessRestoreBridge::new(Arc::new(FakePort::new(FakeOutcome::Refuse)));
        assert_eq!(
            refused.restore(&restore_args(Uuid::new_v4(), &"e".repeat(64), 0)),
            Err(TranscriptRestoreWorkerError::Refused)
        );

        let absent =
            WorkerProcessRestoreBridge::new(Arc::new(FakePort::new(FakeOutcome::Unavailable)));
        assert_eq!(
            absent.restore(&restore_args(Uuid::new_v4(), &"e".repeat(64), 0)),
            Err(TranscriptRestoreWorkerError::Unavailable)
        );
    }

    fn empty_runtime_coordinator() -> DesktopProductCoordinator {
        DesktopProductCoordinator::new(
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            Arc::new(FakePort::new(FakeOutcome::Refuse)),
        )
    }

    #[test]
    fn coordinator_reports_unavailable_before_the_runtime_exists() {
        let coordinator = empty_runtime_coordinator();
        assert_eq!(
            coordinator.source_for(Uuid::new_v4()),
            Err(CoordinatorError::Unavailable)
        );
        assert_eq!(
            coordinator.accept_restore(&RestoreWithheldTurnUiArgs {
                meeting_id: Uuid::new_v4(),
                source_transcript_sha256: "f".repeat(64),
                source_turn_index: 0,
            }),
            Err(CoordinatorError::Unavailable)
        );
        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: Uuid::new_v4(),
                source_transcript_sha256: "f".repeat(64),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }),
            Err(CoordinatorError::Unavailable)
        );
    }

    struct RuntimeFixture {
        // Owns the tempdir for the fixture's lifetime.
        _temporary: tempfile::TempDir,
        state: ApplicationState,
        meeting_id: Uuid,
        transcript_sha256: String,
    }

    fn runtime_fixture(turns: Value) -> RuntimeFixture {
        let (temporary, storage) = test_storage();
        let meeting_id = Uuid::new_v4();
        let transcript_path = write_transcript_fixture_with_turns(
            &storage,
            &meeting_id.to_string(),
            1_700_000_000,
            AudioState::Retained,
            turns,
        );
        let transcript_sha256 = format!(
            "{:x}",
            Sha256::digest(std::fs::read(&transcript_path).unwrap())
        );
        let state = ApplicationState::default();
        ensure_app_data_writer_lock(&state, &storage).unwrap();
        *state.storage.lock().unwrap() = Some(StorageContext {
            storage,
            resource_root: PathBuf::from("/unused"),
            manifest_path: PathBuf::from("/unused"),
            diagnostics: temporary.path().join("diagnostics"),
        });
        RuntimeFixture {
            _temporary: temporary,
            state,
            meeting_id,
            transcript_sha256,
        }
    }

    fn coordinator_for(
        fixture: &RuntimeFixture,
        port: Arc<dyn WorkerPort>,
    ) -> DesktopProductCoordinator {
        DesktopProductCoordinator::new(
            fixture.state.storage.clone(),
            fixture.state.app_data_writer_lock.clone(),
            port,
        )
    }

    fn gated_turns() -> Value {
        json!([
            { "start": 0.0, "end": 1.0, "speaker": "Me", "text": "kept" },
            { "start": 1.0, "end": 2.0, "speaker": "Me", "text": "", "gated": true },
        ])
    }

    fn retry_args(fixture: &RuntimeFixture) -> TranscriptRetryUiArgs {
        TranscriptRetryUiArgs {
            meeting_id: fixture.meeting_id,
            source_transcript_sha256: fixture.transcript_sha256.clone(),
        }
    }

    fn retry_digests(fixture: &RuntimeFixture, candidate_bytes: &[u8]) -> HashMap<String, String> {
        let storage = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .clone();
        let directory = storage
            .resolve(&Path::new("meetings").join(fixture.meeting_id.to_string()))
            .unwrap();
        let candidate_sha = format!("{:x}", Sha256::digest(candidate_bytes));
        durable_create_new(
            &directory
                .join("transcript")
                .join(format!("{candidate_sha}.json")),
            candidate_bytes,
        )
        .unwrap();
        let meeting = load_meeting(&directory).unwrap();
        [
            ("candidate-transcript", candidate_sha),
            ("source-transcript", fixture.transcript_sha256.clone()),
            (
                "capture-session",
                meeting.artifacts.capture_session.unwrap().sha256,
            ),
            (
                "capture-mic",
                meeting.artifacts.microphone_audio.unwrap().sha256,
            ),
            (
                "capture-system",
                meeting.artifacts.system_audio.unwrap().sha256,
            ),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
    }

    #[test]
    fn retry_progress_accepts_only_the_transcribing_heartbeat() {
        let progress = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id: Uuid::new_v4(),
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Transcribing,
            meeting_id: Uuid::new_v4(),
        };
        assert_eq!(retry_progress_is_admitted(&progress), Ok(()));
        let wrong_state = WorkerProgress {
            state: CaptureProgressState::Recording,
            ..progress
        };
        assert_eq!(
            retry_progress_is_admitted(&wrong_state),
            Err(ProtocolError::InvalidEvent)
        );
    }

    #[test]
    fn retry_worker_digest_map_is_closed_and_binds_all_five_artifacts() {
        let expected = TranscriptRetrySourceBinding {
            source_transcript_sha256: "a".repeat(64),
            capture_session_sha256: "b".repeat(64),
            microphone_audio_sha256: "c".repeat(64),
            system_audio_sha256: "d".repeat(64),
            candidate_transcript_sha256: String::new(),
        };
        let digests = HashMap::from([
            ("candidate-transcript".into(), "e".repeat(64)),
            (
                "source-transcript".into(),
                expected.source_transcript_sha256.clone(),
            ),
            (
                "capture-session".into(),
                expected.capture_session_sha256.clone(),
            ),
            (
                "capture-mic".into(),
                expected.microphone_audio_sha256.clone(),
            ),
            (
                "capture-system".into(),
                expected.system_audio_sha256.clone(),
            ),
        ]);
        assert_eq!(
            retry_worker_binding(&digests, &expected)
                .unwrap()
                .candidate_transcript_sha256,
            "e".repeat(64)
        );
        let mut malformed = digests.clone();
        malformed.insert("extra".into(), "f".repeat(64));
        assert_eq!(
            retry_worker_binding(&malformed, &expected),
            Err(CoordinatorError::Refused)
        );
        let mut stale = digests;
        stale.insert("capture-mic".into(), "f".repeat(64));
        assert_eq!(
            retry_worker_binding(&stale, &expected),
            Err(CoordinatorError::Refused)
        );
    }

    #[test]
    fn retry_start_keeps_the_current_pointer_and_keep_is_the_only_receipt_change() {
        let fixture = runtime_fixture(gated_turns());
        let candidate = serde_json::to_vec(&json!({
            "schema": "capture-transcript/1",
            "source": "retry",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [{ "start": 0.0, "end": 1.0, "speaker": "Me", "text": "retry" }],
        }))
        .unwrap();
        let digests = retry_digests(&fixture, &candidate);
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(digests)));
        let coordinator = coordinator_for(&fixture, port.clone());
        let storage = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .clone();
        let directory = storage
            .resolve(&Path::new("meetings").join(fixture.meeting_id.to_string()))
            .unwrap();
        let before = std::fs::read(directory.join("meeting.json")).unwrap();

        let operation = coordinator
            .start_transcript_retry(&retry_args(&fixture))
            .unwrap();
        assert_eq!(
            std::fs::read(directory.join("meeting.json")).unwrap(),
            before
        );
        assert_eq!(
            operation.source_transcript_sha256,
            fixture.transcript_sha256
        );
        let requests = port.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, Operation::TranscriptRetry);
        assert_eq!(
            requests[0].1["source_transcript_sha256"],
            fixture.transcript_sha256
        );
        drop(requests);

        assert_eq!(
            coordinator
                .decide_transcript_retry(&operation, TranscriptRetryDecision::KeepCurrent)
                .unwrap(),
            TranscriptRetryOutcome::CurrentKept
        );
        assert_eq!(
            std::fs::read(directory.join("meeting.json")).unwrap(),
            before
        );
        assert!(
            coordinator
                .pending_transcript_retry(&retry_args(&fixture))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn retry_promotion_changes_only_the_current_transcript_pointer() {
        let fixture = runtime_fixture(gated_turns());
        let candidate = serde_json::to_vec(&json!({
            "schema": "capture-transcript/1",
            "source": "retry",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [{ "start": 0.0, "end": 1.0, "speaker": "Me", "text": "retry" }],
        }))
        .unwrap();
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(retry_digests(
            &fixture, &candidate,
        ))));
        let coordinator = coordinator_for(&fixture, port);
        let operation = coordinator
            .start_transcript_retry(&retry_args(&fixture))
            .unwrap();
        assert_eq!(
            coordinator
                .decide_transcript_retry(&operation, TranscriptRetryDecision::UseRetry)
                .unwrap(),
            TranscriptRetryOutcome::CandidatePromoted
        );
        let storage = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .clone();
        let directory = storage
            .resolve(&Path::new("meetings").join(fixture.meeting_id.to_string()))
            .unwrap();
        let meeting = load_meeting(&directory).unwrap();
        assert_eq!(
            meeting.artifacts.current_transcript,
            Some(operation.candidate_transcript)
        );
        assert!(meeting.artifacts.current_note.is_none());
        assert_eq!(meeting.lifecycle, MeetingLifecycle::TranscriptReady);
    }

    #[test]
    fn source_for_reflects_the_stored_meeting_record() {
        let fixture = runtime_fixture(gated_turns());
        let coordinator = coordinator_for(&fixture, Arc::new(FakePort::new(FakeOutcome::Refuse)));
        let source = coordinator.source_for(fixture.meeting_id).unwrap();
        assert_eq!(source.meeting_id, fixture.meeting_id);
        assert_eq!(source.current_transcript_sha256, fixture.transcript_sha256);
        assert_eq!(source.lifecycle, MeetingLifecycle::TranscriptReady);
        assert!(!source.has_current_note);
    }

    /// The full desktop assembly — storage handles, meeting lease, record
    /// verification, withheld-index sourcing — must carry a restore request
    /// all the way to the worker seam, and a worker refusal must come back as
    /// a typed refusal rather than a panic or a fabricated acceptance.
    #[test]
    fn accept_restore_drives_the_real_coordinator_to_the_worker_seam() {
        let fixture = runtime_fixture(gated_turns());
        let port = Arc::new(FakePort::new(FakeOutcome::Refuse));
        let coordinator = coordinator_for(&fixture, port.clone());

        let result = coordinator.accept_restore(&RestoreWithheldTurnUiArgs {
            meeting_id: fixture.meeting_id,
            source_transcript_sha256: fixture.transcript_sha256.clone(),
            source_turn_index: 1,
        });
        assert_eq!(result, Err(CoordinatorError::Refused));

        let requests = port.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, Operation::TranscriptRestore);
        assert_eq!(
            requests[0].1,
            json!({
                "meeting_id": fixture.meeting_id.to_string(),
                "source_transcript_sha256": fixture.transcript_sha256,
                "source_turn_index": 1,
            })
        );
    }

    /// A turn that is not withheld must be refused before the worker is ever
    /// consulted — restoration of ordinary text is not a reachable request.
    #[test]
    fn accept_restore_refuses_a_non_withheld_turn_before_the_worker() {
        let fixture = runtime_fixture(gated_turns());
        let port = Arc::new(FakePort::new(FakeOutcome::Refuse));
        let coordinator = coordinator_for(&fixture, port.clone());

        let result = coordinator.accept_restore(&RestoreWithheldTurnUiArgs {
            meeting_id: fixture.meeting_id,
            source_transcript_sha256: fixture.transcript_sha256.clone(),
            source_turn_index: 0,
        });
        assert_eq!(result, Err(CoordinatorError::Refused));
        assert!(port.requests.lock().unwrap().is_empty());
    }

    /// The exact assembly main() manages: facade over the desktop coordinator.
    /// The pre-check passes against the stored record (so the failure is not
    /// SourceChanged) and the worker refusal surfaces as the generic copy.
    #[test]
    fn the_managed_facade_assembly_reaches_the_worker_seam() {
        let fixture = runtime_fixture(gated_turns());
        let port = Arc::new(FakePort::new(FakeOutcome::Refuse));
        let facade = ProductOperationFacade::new(Arc::new(coordinator_for(&fixture, port.clone())));

        let result = facade.restore_withheld_turn(RestoreWithheldTurnUiArgs {
            meeting_id: fixture.meeting_id,
            source_transcript_sha256: fixture.transcript_sha256.clone(),
            source_turn_index: 1,
        });
        assert_eq!(
            result,
            Err(ProductOperationFacadeError::OperationUnavailable)
        );
        assert_eq!(port.requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn regeneration_without_an_installed_note_model_is_unavailable_before_the_worker() {
        // Admission is the gate now, not a stub: the fixture runtime has no
        // verified note-model install, so the generate child is never
        // launched and the worker port is never touched.
        let fixture = runtime_fixture(gated_turns());
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(HashMap::new())));
        let coordinator = coordinator_for(&fixture, port.clone());
        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: fixture.meeting_id,
                source_transcript_sha256: fixture.transcript_sha256.clone(),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }),
            Err(CoordinatorError::Unavailable)
        );
        assert!(port.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn regeneration_refuses_an_overlay_that_the_current_sidecar_does_not_attest() {
        let fixture = runtime_fixture(gated_turns());
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(HashMap::new())));
        let coordinator = coordinator_for(&fixture, port.clone());

        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: fixture.meeting_id,
                source_transcript_sha256: fixture.transcript_sha256.clone(),
                speaker_label_overrides: vec![SpeakerLabelOverride {
                    source_speaker: Some("Them".into()),
                    replacement: "Alex".into(),
                }],
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }),
            Err(CoordinatorError::Refused)
        );
        assert!(port.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn regeneration_refuses_a_vocabulary_overlay_the_current_store_does_not_attest() {
        let fixture = runtime_fixture(gated_turns());
        let storage = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .clone();
        local_meeting_notes_session_core::local_vocabulary::LocalVocabularyStore::open(&storage)
            .unwrap()
            .add("kept", "Kibble")
            .unwrap();
        let derived = crate::vocabulary_replacements_for(
            fixture.meeting_id,
            &fixture.transcript_sha256,
            &fixture.state,
        )
        .unwrap();
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].turn, 0);

        let port = Arc::new(FakePort::new(FakeOutcome::Accept(HashMap::new())));
        let coordinator = coordinator_for(&fixture, port.clone());
        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: fixture.meeting_id,
                source_transcript_sha256: fixture.transcript_sha256.clone(),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }),
            Err(CoordinatorError::Refused)
        );
        assert!(port.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn regeneration_refuses_a_malformed_correction_sidecar_before_worker_work() {
        let fixture = runtime_fixture(gated_turns());
        let meeting_dir = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .resolve(&Path::new("meetings").join(fixture.meeting_id.to_string()))
            .unwrap();
        std::fs::write(
            meeting_dir.join("speaker-corrections.json"),
            b"{not corrections",
        )
        .unwrap();
        let port = Arc::new(FakePort::new(FakeOutcome::Accept(HashMap::new())));
        let coordinator = coordinator_for(&fixture, port.clone());

        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: fixture.meeting_id,
                source_transcript_sha256: fixture.transcript_sha256.clone(),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }),
            Err(CoordinatorError::Refused)
        );
        assert!(port.requests.lock().unwrap().is_empty());
    }

    /// The same re-attestation gate this packet adds, mirroring
    /// `regeneration_refuses_a_vocabulary_overlay_the_current_store_does_not_attest`:
    /// a caller-supplied context that disagrees with what is on disk right now
    /// must refuse before the worker is touched, exactly like a stale
    /// vocabulary or speaker overlay.
    #[test]
    fn regeneration_refuses_a_context_value_the_current_sidecar_does_not_attest() {
        let fixture = runtime_fixture(gated_turns());
        let meeting_dir = fixture
            .state
            .storage
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .storage
            .resolve(&Path::new("meetings").join(fixture.meeting_id.to_string()))
            .unwrap();
        crate::meeting_context::write(&meeting_dir, "the real, current framing").unwrap();

        let port = Arc::new(FakePort::new(FakeOutcome::Accept(HashMap::new())));
        let coordinator = coordinator_for(&fixture, port.clone());
        assert_eq!(
            coordinator.accept_regeneration(&RegenerateNoteUiArgs {
                meeting_id: fixture.meeting_id,
                source_transcript_sha256: fixture.transcript_sha256.clone(),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: Some("a stale value the caller still believes".into()),
            }),
            Err(CoordinatorError::Refused)
        );
        assert!(port.requests.lock().unwrap().is_empty());
    }
}
