//! Private, unregistered note-generation persistence coordinator.
//!
//! The Python worker remains the artifact authority: it publishes and then
//! independently re-inspects a `note/2` pair.  This module owns only the
//! durable request/result/meeting/commit sequence around that seam.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::meeting::{
    ArtifactRef, MAX_MEETING_RECORD_BYTES, MeetingError, MeetingLifecycle, MeetingRecord,
    NoteRevisionRef, load_meeting, read_private_bytes, verify_record_artifacts, write_meeting,
};
use crate::meeting_coordination::{MeetingCoordinationError, MeetingStorageCoordination};
use crate::operation_store::{
    OperationStore, OperationStoreError, StoredOperationReceipt, StoredOperationRequest,
    StoredOperationResult,
};
use crate::operations::{
    IncompleteOperationRecovery, MeetingOperationCommit, MeetingOperationCommitSchema,
    NoteCreateWorkerArgs, NoteCreateWorkerFailure, NoteGenerationRequest,
    NoteGenerationRequestSchema, NoteGenerationResult, NoteGenerationResultSchema,
    NoteGenerationStatus, OperationContractError, ProductOperationKind, ProductOperationOutcome,
    RegenerateNoteUiArgs, classify_note_recovery, validate_note_worker_digests,
    validate_note_worker_failure_join, validate_note_worker_join,
};
use crate::storage::StorageRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteGenerationDurablePhase {
    RequestStored,
    WorkerArtifactReturned,
    ResultStored,
    MeetingPublished,
    CommitStored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteWorkerResult {
    Accepted(HashMap<String, String>),
    Rejected(NoteCreateWorkerFailure),
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum NoteGenerationWorkerError {
    #[error("note generation worker is unavailable")]
    Unavailable,
    #[error("note generation worker refused the request")]
    Refused,
}

/// The implementation calls Python's `note.create` seam. It is intentionally
/// injected: no model or generator is admitted by this coordinator.
pub trait NoteGenerationWorker: Send + Sync {
    fn create(
        &self,
        arguments: &NoteCreateWorkerArgs,
    ) -> Result<NoteWorkerResult, NoteGenerationWorkerError>;
}

/// The implementation calls Python's `note.inspect` seam after publication.
/// A worker's returned digest map is never trusted without this fresh read.
pub trait NoteArtifactInspector: Send + Sync {
    fn inspect(
        &self,
        meeting_dir: &Path,
        meeting_id: Uuid,
        note: &NoteRevisionRef,
    ) -> Result<HashMap<String, String>, NoteArtifactError>;
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum NoteArtifactError {
    #[error("note artifact is missing")]
    Missing,
    #[error("note artifact is unsafe or changed")]
    Changed,
    #[error("note artifact contract is malformed")]
    Malformed,
}

pub trait NoteGenerationIdentitySource: Send + Sync {
    fn operation_id(&self) -> Uuid;
    fn now_epoch_seconds(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteGenerationInjectedFailure;

pub trait NoteGenerationFailureInjection: Send + Sync {
    fn after_phase(
        &self,
        phase: NoteGenerationDurablePhase,
    ) -> Result<(), NoteGenerationInjectedFailure>;
}

#[derive(Debug, Default)]
pub struct SystemNoteGenerationIdentity;
impl NoteGenerationIdentitySource for SystemNoteGenerationIdentity {
    fn operation_id(&self) -> Uuid {
        Uuid::new_v4()
    }
    fn now_epoch_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .max(1)
    }
}

#[derive(Debug, Default)]
pub struct NoNoteGenerationFailureInjection;
impl NoteGenerationFailureInjection for NoNoteGenerationFailureInjection {
    fn after_phase(
        &self,
        _: NoteGenerationDurablePhase,
    ) -> Result<(), NoteGenerationInjectedFailure> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum NoteGenerationCoordinatorError {
    #[error(transparent)]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    OperationStore(#[from] OperationStoreError),
    #[error(transparent)]
    Contract(#[from] OperationContractError),
    #[error(transparent)]
    Worker(#[from] NoteGenerationWorkerError),
    #[error(transparent)]
    Artifact(#[from] NoteArtifactError),
    #[error("note generation state is ambiguous: {0}")]
    Ambiguous(&'static str),
    #[error("injected crash after {0:?}")]
    InjectedCrash(NoteGenerationDurablePhase),
    #[error("private storage path is unavailable")]
    StorageUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteGenerationQuarantineReason {
    MultipleNonterminalOperations,
    SourceChanged,
    InvalidReceipt,
    InvalidArtifact,
    WorkerUnavailable,
    StorageUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteGenerationRecoveryAction {
    RetriedRequest,
    AppliedValidatedResult,
    WroteMissingCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteGenerationRecoveryDisposition {
    Recovered {
        operation_id: Uuid,
        action: NoteGenerationRecoveryAction,
    },
    Quarantined {
        meeting_id: Uuid,
        operation_id: Option<Uuid>,
        reason: NoteGenerationQuarantineReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoteGenerationRecoveryReport {
    pub operations: Vec<NoteGenerationRecoveryDisposition>,
}

pub struct NoteGenerationCoordinator {
    storage: StorageRoot,
    operations: OperationStore,
    coordination: Arc<MeetingStorageCoordination>,
    worker: Arc<dyn NoteGenerationWorker>,
    artifacts: Arc<dyn NoteArtifactInspector>,
    identities: Arc<dyn NoteGenerationIdentitySource>,
    failures: Arc<dyn NoteGenerationFailureInjection>,
}

impl NoteGenerationCoordinator {
    pub fn new(
        storage: StorageRoot,
        coordination: Arc<MeetingStorageCoordination>,
        worker: Arc<dyn NoteGenerationWorker>,
        artifacts: Arc<dyn NoteArtifactInspector>,
    ) -> Result<Self, NoteGenerationCoordinatorError> {
        Self::with_runtime(
            storage,
            coordination,
            worker,
            artifacts,
            Arc::new(SystemNoteGenerationIdentity),
            Arc::new(NoNoteGenerationFailureInjection),
        )
    }

    pub fn with_runtime(
        storage: StorageRoot,
        coordination: Arc<MeetingStorageCoordination>,
        worker: Arc<dyn NoteGenerationWorker>,
        artifacts: Arc<dyn NoteArtifactInspector>,
        identities: Arc<dyn NoteGenerationIdentitySource>,
        failures: Arc<dyn NoteGenerationFailureInjection>,
    ) -> Result<Self, NoteGenerationCoordinatorError> {
        Ok(Self {
            operations: OperationStore::open(&storage)?,
            storage,
            coordination,
            worker,
            artifacts,
            identities,
            failures,
        })
    }

    pub fn regenerate_note(
        &self,
        arguments: &RegenerateNoteUiArgs,
    ) -> Result<Uuid, NoteGenerationCoordinatorError> {
        self.regenerate_note_with(arguments, |_| Ok(()))
    }

    /// Runs the caller's source-side admission under the meeting lease and
    /// before a replacement request exists. Desktop uses this for correction
    /// overlays that are local to its storage layer; session-core keeps no
    /// correction sidecar authority of its own.
    pub fn regenerate_note_with(
        &self,
        arguments: &RegenerateNoteUiArgs,
        source_admission: impl FnOnce(&Path) -> Result<(), NoteGenerationCoordinatorError>,
    ) -> Result<Uuid, NoteGenerationCoordinatorError> {
        arguments.validate()?;
        let _lease = self
            .coordination
            .acquire(&arguments.meeting_id.to_string())?;
        if !self
            .nonterminal_for_meeting(arguments.meeting_id)?
            .is_empty()
        {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "another nonterminal operation already exists",
            ));
        }
        let meeting_dir = self.meeting_dir(arguments.meeting_id)?;
        let meeting = self.load_and_verify_meeting(&meeting_dir)?;
        self.require_source(
            &meeting,
            arguments.meeting_id,
            &arguments.source_transcript_sha256,
        )?;
        source_admission(&meeting_dir)?;
        let request = NoteGenerationRequest {
            schema: if meeting.artifacts.current_note.is_some() {
                NoteGenerationRequestSchema::V2
            } else {
                NoteGenerationRequestSchema::V1
            },
            operation_id: self.identities.operation_id(),
            meeting_id: arguments.meeting_id,
            requested_at_epoch_seconds: self.identities.now_epoch_seconds(),
            source_transcript_sha256: arguments.source_transcript_sha256.clone(),
            speaker_label_overrides: arguments.speaker_label_overrides.clone(),
            vocabulary_replacements: arguments.vocabulary_replacements.clone(),
            prior_note: meeting.artifacts.current_note.clone(),
            pre_meeting_context: arguments.pre_meeting_context.clone(),
        };
        request.validate()?;
        self.operations
            .write_request(&StoredOperationRequest::NoteGeneration(request.clone()))?;
        self.checkpoint(NoteGenerationDurablePhase::RequestStored)?;
        self.execute_request(&meeting_dir, &request)?;
        Ok(request.operation_id)
    }

    pub fn recover_incomplete(
        &self,
    ) -> Result<NoteGenerationRecoveryReport, NoteGenerationCoordinatorError> {
        let receipts = self.operations.scan()?;
        let mut meetings = BTreeSet::new();
        for receipt in receipts.values() {
            if receipt.commit.is_none()
                && let StoredOperationRequest::NoteGeneration(request) = &receipt.request
            {
                meetings.insert(request.meeting_id);
            }
        }
        let mut report = NoteGenerationRecoveryReport::default();
        for meeting_id in meetings {
            let _lease = match self.coordination.acquire(&meeting_id.to_string()) {
                Ok(lease) => lease,
                Err(_) => {
                    report
                        .operations
                        .push(NoteGenerationRecoveryDisposition::Quarantined {
                            meeting_id,
                            operation_id: None,
                            reason: NoteGenerationQuarantineReason::StorageUnavailable,
                        });
                    continue;
                }
            };
            let nonterminal = self.nonterminal_for_meeting(meeting_id)?;
            if nonterminal.len() != 1 {
                report
                    .operations
                    .push(NoteGenerationRecoveryDisposition::Quarantined {
                        meeting_id,
                        operation_id: None,
                        reason: NoteGenerationQuarantineReason::MultipleNonterminalOperations,
                    });
                continue;
            }
            let (operation_id, receipt) = &nonterminal[0];
            let StoredOperationRequest::NoteGeneration(request) = &receipt.request else {
                report
                    .operations
                    .push(NoteGenerationRecoveryDisposition::Quarantined {
                        meeting_id,
                        operation_id: Some(*operation_id),
                        reason: NoteGenerationQuarantineReason::MultipleNonterminalOperations,
                    });
                continue;
            };
            match self.resume_request(request, receipt.result.as_ref()) {
                Ok(action) => {
                    report
                        .operations
                        .push(NoteGenerationRecoveryDisposition::Recovered {
                            operation_id: *operation_id,
                            action,
                        })
                }
                Err(error) => {
                    report
                        .operations
                        .push(NoteGenerationRecoveryDisposition::Quarantined {
                            meeting_id,
                            operation_id: Some(*operation_id),
                            reason: quarantine_reason(&error),
                        })
                }
            }
        }
        Ok(report)
    }

    fn execute_request(
        &self,
        meeting_dir: &Path,
        request: &NoteGenerationRequest,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        let meeting = self.load_and_verify_meeting(meeting_dir)?;
        self.require_source(
            &meeting,
            request.meeting_id,
            &request.source_transcript_sha256,
        )?;
        let arguments = NoteCreateWorkerArgs {
            meeting_id: request.meeting_id,
            source_transcript_sha256: request.source_transcript_sha256.clone(),
            speaker_label_overrides: request.speaker_label_overrides.clone(),
            vocabulary_replacements: request.vocabulary_replacements.clone(),
            pre_meeting_context: request.pre_meeting_context.clone(),
        };
        let result = match self.worker.create(&arguments)? {
            NoteWorkerResult::Accepted(worker_digests) => {
                self.checkpoint(NoteGenerationDurablePhase::WorkerArtifactReturned)?;
                self.inspect_accepted(meeting_dir, request, &worker_digests)?
            }
            NoteWorkerResult::Rejected(failure) => {
                self.checkpoint(NoteGenerationDurablePhase::WorkerArtifactReturned)?;
                let result = rejected_result(request, &failure)?;
                validate_note_worker_failure_join(&failure, request, &result)?;
                result
            }
        };
        self.operations.write_result(
            request.operation_id,
            &StoredOperationResult::NoteGeneration(result.clone()),
        )?;
        self.checkpoint(NoteGenerationDurablePhase::ResultStored)?;
        self.apply_result(meeting_dir, request, &result)
    }

    fn resume_request(
        &self,
        request: &NoteGenerationRequest,
        stored_result: Option<&StoredOperationResult>,
    ) -> Result<NoteGenerationRecoveryAction, NoteGenerationCoordinatorError> {
        let meeting_dir = self.meeting_dir(request.meeting_id)?;
        let meeting = self.load_and_verify_meeting(&meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            NoteGenerationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        let result = match stored_result {
            None => {
                if classify_note_recovery(
                    request,
                    None,
                    meeting.lifecycle,
                    &current.sha256,
                    meeting.artifacts.current_note.as_ref(),
                )? != IncompleteOperationRecovery::RetryRequest
                {
                    return Err(NoteGenerationCoordinatorError::Ambiguous(
                        "request-only recovery was not retryable",
                    ));
                }
                self.execute_request(&meeting_dir, request)?;
                return Ok(NoteGenerationRecoveryAction::RetriedRequest);
            }
            Some(StoredOperationResult::NoteGeneration(result)) => result,
            Some(StoredOperationResult::Restoration(_)) => {
                return Err(NoteGenerationCoordinatorError::Ambiguous(
                    "note request has another result schema",
                ));
            }
        };
        self.inspect_stored_result(&meeting_dir, request, result)?;
        match classify_note_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )? {
            IncompleteOperationRecovery::ApplyValidatedResult => {
                self.apply_result(&meeting_dir, request, result)?;
                Ok(NoteGenerationRecoveryAction::AppliedValidatedResult)
            }
            IncompleteOperationRecovery::WriteMissingCommit => {
                self.write_terminal_commit(&meeting_dir, request, result)?;
                Ok(NoteGenerationRecoveryAction::WroteMissingCommit)
            }
            IncompleteOperationRecovery::RetryRequest => {
                Err(NoteGenerationCoordinatorError::Ambiguous(
                    "stored result unexpectedly classified as request-only",
                ))
            }
        }
    }

    fn inspect_accepted(
        &self,
        meeting_dir: &Path,
        request: &NoteGenerationRequest,
        worker_digests: &HashMap<String, String>,
    ) -> Result<NoteGenerationResult, NoteGenerationCoordinatorError> {
        validate_note_worker_digests(worker_digests, &request.source_transcript_sha256)?;
        let note = NoteRevisionRef {
            json: ArtifactRef {
                relative_path: format!("notes/{}.json", worker_digests["note"]),
                sha256: worker_digests["note"].clone(),
            },
            markdown: ArtifactRef {
                relative_path: format!("notes/{}.md", worker_digests["note-markdown"]),
                sha256: worker_digests["note-markdown"].clone(),
            },
            source_transcript_sha256: request.source_transcript_sha256.clone(),
        };
        let result = NoteGenerationResult {
            schema: NoteGenerationResultSchema::V1,
            operation_id: request.operation_id,
            meeting_id: request.meeting_id,
            source_transcript_sha256: request.source_transcript_sha256.clone(),
            status: NoteGenerationStatus::Accepted,
            note: Some(note.clone()),
            failure_code: None,
        };
        // Fresh `note.inspect`: the pair must still be exact and semantically passing.
        let inspected = self
            .artifacts
            .inspect(meeting_dir, request.meeting_id, &note)?;
        validate_note_worker_join(&inspected, request, &result)?;
        if &inspected != worker_digests {
            return Err(NoteGenerationCoordinatorError::Artifact(
                NoteArtifactError::Changed,
            ));
        }
        Ok(result)
    }

    fn inspect_stored_result(
        &self,
        meeting_dir: &Path,
        request: &NoteGenerationRequest,
        result: &NoteGenerationResult,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        result.validate()?;
        if result.status == NoteGenerationStatus::Accepted {
            let note = result.note.as_ref().ok_or(NoteArtifactError::Malformed)?;
            let inspected = self
                .artifacts
                .inspect(meeting_dir, request.meeting_id, note)?;
            validate_note_worker_join(&inspected, request, result)?;
        }
        Ok(())
    }

    fn apply_result(
        &self,
        meeting_dir: &Path,
        request: &NoteGenerationRequest,
        result: &NoteGenerationResult,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        let mut meeting = self.load_and_verify_meeting(meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            NoteGenerationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        let recovery = classify_note_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )?;
        if recovery == IncompleteOperationRecovery::WriteMissingCommit {
            return self.write_terminal_commit(meeting_dir, request, result);
        }
        if recovery != IncompleteOperationRecovery::ApplyValidatedResult {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting no longer accepts the validated result",
            ));
        }
        self.inspect_stored_result(meeting_dir, request, result)?;
        match result.status {
            NoteGenerationStatus::Accepted => {
                meeting.lifecycle = MeetingLifecycle::Ready;
                meeting.artifacts.current_note = result.note.clone();
            }
            NoteGenerationStatus::Rejected => {
                if request.prior_note.is_some() {
                    meeting.lifecycle = MeetingLifecycle::Ready;
                    meeting.artifacts.current_note = request.prior_note.clone();
                } else {
                    meeting.lifecycle = MeetingLifecycle::SummaryFailed;
                    meeting.artifacts.current_note = None;
                }
            }
        }
        write_meeting(meeting_dir, &meeting)?;
        self.checkpoint(NoteGenerationDurablePhase::MeetingPublished)?;
        self.write_terminal_commit(meeting_dir, request, result)
    }

    fn write_terminal_commit(
        &self,
        meeting_dir: &Path,
        request: &NoteGenerationRequest,
        result: &NoteGenerationResult,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        let meeting = self.load_and_verify_meeting(meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            NoteGenerationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        if classify_note_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )? != IncompleteOperationRecovery::WriteMissingCommit
        {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting does not match the exact successor state",
            ));
        }
        self.inspect_stored_result(meeting_dir, request, result)?;
        let bytes =
            read_private_bytes(&meeting_dir.join("meeting.json"), MAX_MEETING_RECORD_BYTES)?;
        if bytes
            != serde_json::to_vec_pretty(&meeting).map_err(|_| {
                NoteGenerationCoordinatorError::Ambiguous("meeting cannot be serialized")
            })?
        {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting bytes are not canonical",
            ));
        }
        let outcome = match result.status {
            NoteGenerationStatus::Accepted => ProductOperationOutcome::NoteAccepted,
            NoteGenerationStatus::Rejected => ProductOperationOutcome::NoteRejected,
        };
        let commit = MeetingOperationCommit {
            schema: MeetingOperationCommitSchema::V1,
            operation_id: request.operation_id,
            meeting_id: request.meeting_id,
            kind: ProductOperationKind::GenerateNote,
            outcome,
            request_sha256: digest_pretty(request)?,
            result_sha256: digest_pretty(result)?,
            committed_meeting_sha256: digest_bytes(&bytes),
            committed_at_epoch_seconds: self.identities.now_epoch_seconds(),
            lifecycle: meeting.lifecycle,
            current_transcript_sha256: current.sha256.clone(),
            current_note: meeting.artifacts.current_note.clone(),
        };
        self.operations
            .write_commit(request.operation_id, &commit, &meeting, &bytes)?;
        self.checkpoint(NoteGenerationDurablePhase::CommitStored)
    }

    fn require_source(
        &self,
        meeting: &MeetingRecord,
        meeting_id: Uuid,
        source: &str,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        require_no_pending_storage(meeting)?;
        if meeting.meeting_id != meeting_id.to_string() {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting identity changed",
            ));
        }
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            NoteGenerationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        if current.sha256 != source || current.relative_path != format!("transcript/{source}.json")
        {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting current transcript changed",
            ));
        }
        let can_generate = matches!(
            (&meeting.artifacts.current_note, meeting.lifecycle),
            (
                None,
                MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed
            ) | (Some(_), MeetingLifecycle::Ready)
        );
        if !can_generate {
            return Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting lifecycle cannot accept note generation",
            ));
        }
        Ok(())
    }

    fn load_and_verify_meeting(
        &self,
        meeting_dir: &Path,
    ) -> Result<MeetingRecord, NoteGenerationCoordinatorError> {
        let meeting = load_meeting(meeting_dir)?;
        verify_record_artifacts(meeting_dir, &meeting)?;
        Ok(meeting)
    }
    fn meeting_dir(&self, meeting_id: Uuid) -> Result<PathBuf, NoteGenerationCoordinatorError> {
        self.storage
            .resolve(&Path::new("meetings").join(meeting_id.to_string()))
            .map_err(|_| NoteGenerationCoordinatorError::StorageUnavailable)
    }
    fn checkpoint(
        &self,
        phase: NoteGenerationDurablePhase,
    ) -> Result<(), NoteGenerationCoordinatorError> {
        self.failures
            .after_phase(phase)
            .map_err(|_| NoteGenerationCoordinatorError::InjectedCrash(phase))
    }
    fn nonterminal_for_meeting(
        &self,
        meeting_id: Uuid,
    ) -> Result<Vec<(Uuid, StoredOperationReceipt)>, NoteGenerationCoordinatorError> {
        let mut records: Vec<_> = self
            .operations
            .scan()?
            .into_iter()
            .filter(|(_, receipt)| {
                receipt.commit.is_none()
                    && match &receipt.request {
                        StoredOperationRequest::Restoration(request) => {
                            request.meeting_id == meeting_id
                        }
                        StoredOperationRequest::NoteGeneration(request) => {
                            request.meeting_id == meeting_id
                        }
                    }
            })
            .collect();
        records.sort_by_key(|(id, _)| *id);
        Ok(records)
    }
}

fn rejected_result(
    request: &NoteGenerationRequest,
    failure: &NoteCreateWorkerFailure,
) -> Result<NoteGenerationResult, OperationContractError> {
    Ok(NoteGenerationResult {
        schema: NoteGenerationResultSchema::V1,
        operation_id: request.operation_id,
        meeting_id: request.meeting_id,
        source_transcript_sha256: request.source_transcript_sha256.clone(),
        status: NoteGenerationStatus::Rejected,
        note: None,
        failure_code: Some(failure.validate()?),
    })
}
fn require_no_pending_storage(
    meeting: &MeetingRecord,
) -> Result<(), NoteGenerationCoordinatorError> {
    if meeting.pending_storage_operation.is_some() {
        Err(NoteGenerationCoordinatorError::Ambiguous(
            "meeting has a pending storage operation",
        ))
    } else {
        Ok(())
    }
}
fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest_pretty<T: serde::Serialize>(value: &T) -> Result<String, NoteGenerationCoordinatorError> {
    Ok(digest_bytes(&serde_json::to_vec_pretty(value).map_err(
        |_| NoteGenerationCoordinatorError::Ambiguous("contract cannot be serialized"),
    )?))
}
fn quarantine_reason(error: &NoteGenerationCoordinatorError) -> NoteGenerationQuarantineReason {
    match error {
        NoteGenerationCoordinatorError::Worker(_) => {
            NoteGenerationQuarantineReason::WorkerUnavailable
        }
        NoteGenerationCoordinatorError::Artifact(_) => {
            NoteGenerationQuarantineReason::InvalidArtifact
        }
        NoteGenerationCoordinatorError::OperationStore(_)
        | NoteGenerationCoordinatorError::Contract(_) => {
            NoteGenerationQuarantineReason::InvalidReceipt
        }
        NoteGenerationCoordinatorError::StorageUnavailable
        | NoteGenerationCoordinatorError::Coordination(_) => {
            NoteGenerationQuarantineReason::StorageUnavailable
        }
        _ => NoteGenerationQuarantineReason::SourceChanged,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::fs;
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::*;
    use crate::meeting::{
        AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts, MeetingSchema,
        PendingStorageOperation, artifact_ref, retention_policy_sha256,
    };
    use crate::storage::{create_private_dir, durable_create_new};

    const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";
    const OPERATION_ID: &str = "22222222-2222-4222-8222-222222222222";

    struct Worker {
        meeting_dir: PathBuf,
        rejected: bool,
        calls: Mutex<u32>,
    }

    impl Worker {
        fn calls(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    impl NoteGenerationWorker for Worker {
        fn create(
            &self,
            arguments: &NoteCreateWorkerArgs,
        ) -> Result<NoteWorkerResult, NoteGenerationWorkerError> {
            *self.calls.lock().unwrap() += 1;
            if self.rejected {
                return Ok(NoteWorkerResult::Rejected(NoteCreateWorkerFailure {
                    code: crate::operations::NoteCreateWorkerFailureCode::NoteRejected,
                    recoverable: true,
                    artifact_digests: HashMap::new(),
                }));
            }
            let markdown = b"# fixture note\n";
            let markdown_sha = digest_bytes(markdown);
            let document = format!(
                "{{\"schema\":\"note/2\",\"meeting\":\"{}\",\"transcript\":\"{}\"}}\n",
                arguments.meeting_id, arguments.source_transcript_sha256
            )
            .into_bytes();
            let note_sha = digest_bytes(&document);
            for (path, bytes) in [
                (
                    self.meeting_dir
                        .join("notes")
                        .join(format!("{markdown_sha}.md")),
                    markdown.as_slice(),
                ),
                (
                    self.meeting_dir
                        .join("notes")
                        .join(format!("{note_sha}.json")),
                    document.as_slice(),
                ),
            ] {
                match fs::read(&path) {
                    Ok(existing) if existing == bytes => {}
                    Ok(_) => return Err(NoteGenerationWorkerError::Refused),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        durable_create_new(&path, bytes)
                            .map_err(|_| NoteGenerationWorkerError::Unavailable)?
                    }
                    Err(_) => return Err(NoteGenerationWorkerError::Unavailable),
                }
            }
            Ok(NoteWorkerResult::Accepted(HashMap::from([
                ("note".into(), note_sha),
                ("note-markdown".into(), markdown_sha),
                (
                    "transcript".into(),
                    arguments.source_transcript_sha256.clone(),
                ),
            ])))
        }
    }

    struct Inspector;
    impl NoteArtifactInspector for Inspector {
        fn inspect(
            &self,
            meeting_dir: &Path,
            _: Uuid,
            note: &NoteRevisionRef,
        ) -> Result<HashMap<String, String>, NoteArtifactError> {
            let json = fs::read(meeting_dir.join(&note.json.relative_path))
                .map_err(|_| NoteArtifactError::Missing)?;
            let markdown = fs::read(meeting_dir.join(&note.markdown.relative_path))
                .map_err(|_| NoteArtifactError::Missing)?;
            if digest_bytes(&json) != note.json.sha256
                || digest_bytes(&markdown) != note.markdown.sha256
            {
                return Err(NoteArtifactError::Changed);
            }
            Ok(HashMap::from([
                ("note".into(), note.json.sha256.clone()),
                ("note-markdown".into(), note.markdown.sha256.clone()),
                ("transcript".into(), note.source_transcript_sha256.clone()),
            ]))
        }
    }

    struct Identity {
        id: Uuid,
        times: Mutex<VecDeque<u64>>,
    }
    impl NoteGenerationIdentitySource for Identity {
        fn operation_id(&self) -> Uuid {
            self.id
        }
        fn now_epoch_seconds(&self) -> u64 {
            self.times.lock().unwrap().pop_front().unwrap_or(99)
        }
    }

    struct CrashOnce {
        phase: NoteGenerationDurablePhase,
        fired: Mutex<bool>,
    }
    impl NoteGenerationFailureInjection for CrashOnce {
        fn after_phase(
            &self,
            phase: NoteGenerationDurablePhase,
        ) -> Result<(), NoteGenerationInjectedFailure> {
            let mut fired = self.fired.lock().unwrap();
            if !*fired && phase == self.phase {
                *fired = true;
                return Err(NoteGenerationInjectedFailure);
            }
            Ok(())
        }
    }

    struct Fixture {
        _temp: TempDir,
        storage: StorageRoot,
        meeting_dir: PathBuf,
        meeting_id: Uuid,
        transcript: String,
        worker: Arc<Worker>,
        coordination: Arc<MeetingStorageCoordination>,
    }

    impl Fixture {
        fn new(rejected: bool) -> Self {
            let temp = TempDir::new().unwrap();
            let repo = temp.path().join("repo");
            create_private_dir(&repo).unwrap();
            let storage = StorageRoot::create(&temp.path().join("data"), &repo).unwrap();
            let meeting_id = Uuid::parse_str(MEETING_ID).unwrap();
            let meeting_dir = storage
                .resolve(&Path::new("meetings").join(MEETING_ID))
                .unwrap();
            for directory in [
                &meeting_dir,
                &meeting_dir.join("capture"),
                &meeting_dir.join("transcript"),
                &meeting_dir.join("notes"),
            ] {
                create_private_dir(directory).unwrap();
            }
            for (path, bytes) in [
                ("attempt.json", b"attempt".as_slice()),
                ("ownership.json", b"owner".as_slice()),
                ("capture/session.json", b"session".as_slice()),
                ("capture/mic.wav", b"mic".as_slice()),
                ("capture/system.wav", b"system".as_slice()),
            ] {
                durable_create_new(&meeting_dir.join(path), bytes).unwrap();
            }
            let transcript_bytes = b"{\"schema\":\"capture-transcript/1\"}\n";
            let transcript = digest_bytes(transcript_bytes);
            durable_create_new(
                &meeting_dir
                    .join("transcript")
                    .join(format!("{transcript}.json")),
                transcript_bytes,
            )
            .unwrap();
            let rule = AudioRetentionRule::UntilManualDeletion;
            let meeting = MeetingRecord {
                schema: MeetingSchema::V2,
                meeting_id: MEETING_ID.into(),
                lifecycle: MeetingLifecycle::TranscriptReady,
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
                    capture_session: Some(
                        artifact_ref(&meeting_dir, "capture/session.json").unwrap(),
                    ),
                    microphone_audio: Some(artifact_ref(&meeting_dir, "capture/mic.wav").unwrap()),
                    system_audio: Some(artifact_ref(&meeting_dir, "capture/system.wav").unwrap()),
                    current_transcript: Some(
                        artifact_ref(&meeting_dir, &format!("transcript/{transcript}.json"))
                            .unwrap(),
                    ),
                    current_note: None,
                },
                pending_storage_operation: None,
            };
            write_meeting(&meeting_dir, &meeting).unwrap();
            let worker = Arc::new(Worker {
                meeting_dir: meeting_dir.clone(),
                rejected,
                calls: Mutex::new(0),
            });
            Self {
                _temp: temp,
                storage,
                meeting_dir,
                meeting_id,
                transcript,
                worker,
                coordination: Arc::new(MeetingStorageCoordination::default()),
            }
        }

        fn coordinator(
            &self,
            failures: Arc<dyn NoteGenerationFailureInjection>,
        ) -> NoteGenerationCoordinator {
            NoteGenerationCoordinator::with_runtime(
                self.storage.clone(),
                self.coordination.clone(),
                self.worker.clone(),
                Arc::new(Inspector),
                Arc::new(Identity {
                    id: Uuid::parse_str(OPERATION_ID).unwrap(),
                    times: Mutex::new(VecDeque::from([1, 2, 3, 4])),
                }),
                failures,
            )
            .unwrap()
        }
        fn arguments(&self) -> RegenerateNoteUiArgs {
            RegenerateNoteUiArgs {
                meeting_id: self.meeting_id,
                source_transcript_sha256: self.transcript.clone(),
                speaker_label_overrides: Vec::new(),
                vocabulary_replacements: Vec::new(),
                pre_meeting_context: None,
            }
        }

        fn install_prior_note(&self) -> NoteRevisionRef {
            let markdown = b"# prior note\n";
            let document = format!(
                "{{\"schema\":\"note/2\",\"meeting\":\"{}\",\"transcript\":\"{}\",\"prior\":true}}\n",
                self.meeting_id, self.transcript
            )
            .into_bytes();
            let markdown_sha = digest_bytes(markdown);
            let note_sha = digest_bytes(&document);
            durable_create_new(
                &self
                    .meeting_dir
                    .join("notes")
                    .join(format!("{markdown_sha}.md")),
                markdown,
            )
            .unwrap();
            durable_create_new(
                &self
                    .meeting_dir
                    .join("notes")
                    .join(format!("{note_sha}.json")),
                &document,
            )
            .unwrap();
            let note = NoteRevisionRef {
                json: ArtifactRef {
                    relative_path: format!("notes/{note_sha}.json"),
                    sha256: note_sha,
                },
                markdown: ArtifactRef {
                    relative_path: format!("notes/{markdown_sha}.md"),
                    sha256: markdown_sha,
                },
                source_transcript_sha256: self.transcript.clone(),
            };
            let mut meeting = load_meeting(&self.meeting_dir).unwrap();
            meeting.lifecycle = MeetingLifecycle::Ready;
            meeting.artifacts.current_note = Some(note.clone());
            write_meeting(&self.meeting_dir, &meeting).unwrap();
            note
        }
    }

    #[test]
    fn every_durable_phase_recovers_with_a_fresh_coordinator() {
        for phase in [
            NoteGenerationDurablePhase::RequestStored,
            NoteGenerationDurablePhase::WorkerArtifactReturned,
            NoteGenerationDurablePhase::ResultStored,
            NoteGenerationDurablePhase::MeetingPublished,
            NoteGenerationDurablePhase::CommitStored,
        ] {
            let fixture = Fixture::new(false);
            let crash = fixture.coordinator(Arc::new(CrashOnce {
                phase,
                fired: Mutex::new(false),
            }));
            assert!(
                matches!(crash.regenerate_note(&fixture.arguments()), Err(NoteGenerationCoordinatorError::InjectedCrash(found)) if found == phase)
            );
            let fresh = fixture.coordinator(Arc::new(NoNoteGenerationFailureInjection));
            let report = fresh.recover_incomplete().unwrap();
            if phase != NoteGenerationDurablePhase::CommitStored {
                assert_eq!(
                    report.operations,
                    vec![NoteGenerationRecoveryDisposition::Recovered {
                        operation_id: Uuid::parse_str(OPERATION_ID).unwrap(),
                        action: match phase {
                            NoteGenerationDurablePhase::RequestStored
                            | NoteGenerationDurablePhase::WorkerArtifactReturned =>
                                NoteGenerationRecoveryAction::RetriedRequest,
                            NoteGenerationDurablePhase::ResultStored =>
                                NoteGenerationRecoveryAction::AppliedValidatedResult,
                            NoteGenerationDurablePhase::MeetingPublished =>
                                NoteGenerationRecoveryAction::WroteMissingCommit,
                            NoteGenerationDurablePhase::CommitStored => unreachable!(),
                        }
                    }]
                );
            }
            let meeting = load_meeting(&fixture.meeting_dir).unwrap();
            assert_eq!(meeting.lifecycle, MeetingLifecycle::Ready);
            assert!(meeting.artifacts.current_note.is_some());
            assert_eq!(
                OperationStore::open(&fixture.storage)
                    .unwrap()
                    .scan()
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn rejected_note_publishes_no_note_and_commits_bounded_failure() {
        let fixture = Fixture::new(true);
        fixture
            .coordinator(Arc::new(NoNoteGenerationFailureInjection))
            .regenerate_note(&fixture.arguments())
            .unwrap();
        let meeting = load_meeting(&fixture.meeting_dir).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::SummaryFailed);
        assert!(meeting.artifacts.current_note.is_none());
        assert!(
            fs::read_dir(fixture.meeting_dir.join("notes"))
                .unwrap()
                .next()
                .is_none()
        );
        let receipt = OperationStore::open(&fixture.storage)
            .unwrap()
            .scan()
            .unwrap()
            .remove(&Uuid::parse_str(OPERATION_ID).unwrap())
            .unwrap();
        assert!(receipt.commit.is_some());
        assert!(matches!(
            receipt.result,
            Some(StoredOperationResult::NoteGeneration(
                NoteGenerationResult {
                    status: NoteGenerationStatus::Rejected,
                    failure_code: Some(crate::operations::NoteFailureCode::NoteRejected),
                    ..
                }
            ))
        ));
    }

    #[test]
    fn ready_meeting_can_replace_its_note_and_preserves_the_prior_on_rejection() {
        let accepted = Fixture::new(false);
        let prior = accepted.install_prior_note();
        accepted
            .coordinator(Arc::new(NoNoteGenerationFailureInjection))
            .regenerate_note(&accepted.arguments())
            .unwrap();
        let meeting = load_meeting(&accepted.meeting_dir).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::Ready);
        assert_ne!(meeting.artifacts.current_note.as_ref(), Some(&prior));

        let rejected = Fixture::new(true);
        let prior = rejected.install_prior_note();
        rejected
            .coordinator(Arc::new(NoNoteGenerationFailureInjection))
            .regenerate_note(&rejected.arguments())
            .unwrap();
        let meeting = load_meeting(&rejected.meeting_dir).unwrap();
        assert_eq!(meeting.lifecycle, MeetingLifecycle::Ready);
        assert_eq!(meeting.artifacts.current_note, Some(prior));
    }

    #[test]
    fn pending_storage_refuses_before_worker_or_receipt() {
        let fixture = Fixture::new(false);
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        meeting.pending_storage_operation = Some(PendingStorageOperation::AudioDeletionV1);
        meeting.retention.state = AudioState::Deleting;
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();
        assert!(matches!(
            fixture
                .coordinator(Arc::new(NoNoteGenerationFailureInjection))
                .regenerate_note(&fixture.arguments()),
            Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting has a pending storage operation"
            ))
        ));
        assert_eq!(fixture.worker.calls(), 0);
        assert!(
            OperationStore::open(&fixture.storage)
                .unwrap()
                .scan()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn stale_source_refuses_before_worker_or_receipt() {
        let fixture = Fixture::new(false);
        let arguments = RegenerateNoteUiArgs {
            meeting_id: fixture.meeting_id,
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: None,
        };
        assert!(matches!(
            fixture
                .coordinator(Arc::new(NoNoteGenerationFailureInjection))
                .regenerate_note(&arguments),
            Err(NoteGenerationCoordinatorError::Ambiguous(
                "meeting current transcript changed"
            ))
        ));
        assert_eq!(fixture.worker.calls(), 0);
        assert!(
            OperationStore::open(&fixture.storage)
                .unwrap()
                .scan()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn source_admission_refuses_before_worker_or_replacement_receipt() {
        let fixture = Fixture::new(false);
        let worker = Arc::new(Worker {
            meeting_dir: fixture.meeting_dir.clone(),
            rejected: false,
            calls: Mutex::new(0),
        });
        let coordinator = NoteGenerationCoordinator::with_runtime(
            fixture.storage.clone(),
            fixture.coordination.clone(),
            worker.clone(),
            Arc::new(Inspector),
            Arc::new(Identity {
                id: Uuid::parse_str(OPERATION_ID).unwrap(),
                times: Mutex::new(VecDeque::from([1])),
            }),
            Arc::new(NoNoteGenerationFailureInjection),
        )
        .unwrap();

        assert!(matches!(
            coordinator.regenerate_note_with(&fixture.arguments(), |_| {
                Err(NoteGenerationCoordinatorError::Ambiguous("source admission refused"))
            }),
            Err(NoteGenerationCoordinatorError::Ambiguous("source admission refused"))
        ));
        assert_eq!(worker.calls(), 0);
        assert!(OperationStore::open(&fixture.storage)
            .unwrap()
            .scan()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn note_collision_never_overwrites_or_publishes() {
        let fixture = Fixture::new(false);
        let markdown = b"# fixture note\n";
        let path = fixture
            .meeting_dir
            .join("notes")
            .join(format!("{}.md", digest_bytes(markdown)));
        durable_create_new(&path, b"different bytes").unwrap();
        assert!(matches!(
            fixture
                .coordinator(Arc::new(NoNoteGenerationFailureInjection))
                .regenerate_note(&fixture.arguments()),
            Err(NoteGenerationCoordinatorError::Worker(
                NoteGenerationWorkerError::Refused
            ))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"different bytes");
        assert_eq!(
            load_meeting(&fixture.meeting_dir).unwrap().lifecycle,
            MeetingLifecycle::TranscriptReady
        );
        let receipt = OperationStore::open(&fixture.storage)
            .unwrap()
            .scan()
            .unwrap()
            .remove(&Uuid::parse_str(OPERATION_ID).unwrap())
            .unwrap();
        assert!(receipt.result.is_none());
    }

    #[test]
    fn pending_storage_quarantines_missing_commit_recovery_untouched() {
        let fixture = Fixture::new(false);
        let crashed = fixture.coordinator(Arc::new(CrashOnce {
            phase: NoteGenerationDurablePhase::MeetingPublished,
            fired: Mutex::new(false),
        }));
        assert!(matches!(
            crashed.regenerate_note(&fixture.arguments()),
            Err(NoteGenerationCoordinatorError::InjectedCrash(
                NoteGenerationDurablePhase::MeetingPublished
            ))
        ));
        let mut meeting = load_meeting(&fixture.meeting_dir).unwrap();
        meeting.pending_storage_operation = Some(PendingStorageOperation::AudioDeletionV1);
        meeting.retention.state = AudioState::Deleting;
        write_meeting(&fixture.meeting_dir, &meeting).unwrap();
        let before = fs::read(fixture.meeting_dir.join("meeting.json")).unwrap();
        let report = fixture
            .coordinator(Arc::new(NoNoteGenerationFailureInjection))
            .recover_incomplete()
            .unwrap();
        assert!(matches!(
            report.operations.as_slice(),
            [NoteGenerationRecoveryDisposition::Quarantined {
                reason: NoteGenerationQuarantineReason::SourceChanged,
                ..
            }]
        ));
        assert_eq!(
            fs::read(fixture.meeting_dir.join("meeting.json")).unwrap(),
            before
        );
    }

    #[test]
    fn multiple_nonterminal_operations_are_quarantined_without_worker_work() {
        let fixture = Fixture::new(false);
        let store = OperationStore::open(&fixture.storage).unwrap();
        for id in [
            Uuid::parse_str(OPERATION_ID).unwrap(),
            Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ] {
            store
                .write_request(&StoredOperationRequest::NoteGeneration(
                    NoteGenerationRequest {
                        schema: NoteGenerationRequestSchema::V1,
                        operation_id: id,
                        meeting_id: fixture.meeting_id,
                        requested_at_epoch_seconds: 1,
                        source_transcript_sha256: fixture.transcript.clone(),
                        speaker_label_overrides: Vec::new(),
                        vocabulary_replacements: Vec::new(),
                        prior_note: None,
                        pre_meeting_context: None,
                    },
                ))
                .unwrap();
        }
        let report = fixture
            .coordinator(Arc::new(NoNoteGenerationFailureInjection))
            .recover_incomplete()
            .unwrap();
        assert_eq!(report.operations.len(), 1);
        assert!(matches!(
            report.operations[0],
            NoteGenerationRecoveryDisposition::Quarantined {
                reason: NoteGenerationQuarantineReason::MultipleNonterminalOperations,
                ..
            }
        ));
        assert_eq!(fixture.worker.calls(), 0);
    }
}
