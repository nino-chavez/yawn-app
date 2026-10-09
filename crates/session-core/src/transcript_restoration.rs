use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
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
    OperationContractError, ProductOperationKind, ProductOperationOutcome,
    RestoreWithheldTurnUiArgs, TranscriptRestorationRequest, TranscriptRestorationRequestSchema,
    TranscriptRestorationResult, TranscriptRestorationResultSchema, TranscriptRestoreWorkerArgs,
    TranscriptView, validate_restoration_join, validate_restore_worker_join,
};
use crate::storage::StorageRoot;

const MAX_TRANSCRIPT_REVISION_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TRANSCRIPT_TURNS: usize = 20_000;
const MAX_TRANSCRIPT_VIEW_DEPTH: usize = 20_000;
// A legitimate view is a few hundred bytes plus ~11 bytes per cumulative
// restored index, so even a fully restored 20,000-turn base stays well under
// 1 MiB per view. This cap bounds what any single chain walk may read and
// retain across ALL its view hops, so a hostile chain cannot amplify one
// meeting into unbounded reader I/O or memory; the base revision keeps its
// own separate per-file bound.
const MAX_TRANSCRIPT_VIEW_CHAIN_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorationDurablePhase {
    RequestStored,
    WorkerArtifactReturned,
    ResultStored,
    MeetingPublished,
    CommitStored,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRestoreWorkerError {
    #[error("transcript restoration worker is unavailable")]
    Unavailable,
    #[error("transcript restoration worker refused the request")]
    Refused,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptArtifactError {
    #[error("transcript artifact is missing")]
    Missing,
    #[error("transcript artifact is unsafe or changed")]
    Changed,
    #[error("transcript artifact contract is malformed")]
    Malformed,
    #[error("transcript view chain is cyclic")]
    Cyclic,
    #[error("transcript view chain is too deep")]
    TooDeep,
    #[error("transcript view restores a turn that was not withheld")]
    NonWithheld,
    #[error("transcript view is not a one-turn successor")]
    NotOneTurnSuccessor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedTranscriptRevision {
    pub digest: String,
    pub base_transcript_sha256: String,
    pub head_view: Option<TranscriptView>,
    pub restored_source_turn_indices: Vec<u32>,
    pub withheld_source_turn_indices: BTreeSet<u32>,
}

pub trait TranscriptRestoreWorker: Send + Sync {
    fn restore(
        &self,
        arguments: &TranscriptRestoreWorkerArgs,
    ) -> Result<HashMap<String, String>, TranscriptRestoreWorkerError>;
}

/// An independent artifact authority. Implementations must re-read immutable
/// bytes on every call; cached worker claims are not sufficient.
pub trait TranscriptArtifactInspector: Send + Sync {
    fn inspect_revision(
        &self,
        meeting_dir: &Path,
        meeting_id: Uuid,
        transcript_sha256: &str,
    ) -> Result<InspectedTranscriptRevision, TranscriptArtifactError>;
}

pub trait RestorationIdentitySource: Send + Sync {
    fn operation_id(&self) -> Uuid;
    fn now_epoch_seconds(&self) -> u64;
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("restoration failure was injected")]
pub struct RestorationInjectedFailure;

pub trait RestorationFailureInjection: Send + Sync {
    fn after_phase(&self, phase: RestorationDurablePhase)
    -> Result<(), RestorationInjectedFailure>;
}

#[derive(Debug, Default)]
pub struct SystemRestorationIdentity;

impl RestorationIdentitySource for SystemRestorationIdentity {
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
pub struct NoRestorationFailureInjection;

impl RestorationFailureInjection for NoRestorationFailureInjection {
    fn after_phase(
        &self,
        _phase: RestorationDurablePhase,
    ) -> Result<(), RestorationInjectedFailure> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct StoredTranscriptArtifactInspector;

impl TranscriptArtifactInspector for StoredTranscriptArtifactInspector {
    fn inspect_revision(
        &self,
        meeting_dir: &Path,
        meeting_id: Uuid,
        transcript_sha256: &str,
    ) -> Result<InspectedTranscriptRevision, TranscriptArtifactError> {
        inspect_stored_revision(meeting_dir, meeting_id, transcript_sha256)
    }
}

#[derive(Debug, Error)]
pub enum TranscriptRestorationCoordinatorError {
    #[error(transparent)]
    Coordination(#[from] MeetingCoordinationError),
    #[error(transparent)]
    Meeting(#[from] MeetingError),
    #[error(transparent)]
    OperationStore(#[from] OperationStoreError),
    #[error(transparent)]
    Contract(#[from] OperationContractError),
    #[error(transparent)]
    Worker(#[from] TranscriptRestoreWorkerError),
    #[error(transparent)]
    Artifact(#[from] TranscriptArtifactError),
    #[error("meeting restoration state is ambiguous: {0}")]
    Ambiguous(&'static str),
    #[error("injected crash after {0:?}")]
    InjectedCrash(RestorationDurablePhase),
    #[error("private storage path is unavailable")]
    StorageUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorationQuarantineReason {
    MultipleNonterminalOperations,
    SourceOrPriorNoteChanged,
    InvalidReceipt,
    InvalidArtifact,
    WorkerUnavailable,
    StorageUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorationRecoveryAction {
    RetriedRequest,
    AppliedValidatedResult,
    WroteMissingCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestorationRecoveryDisposition {
    Recovered {
        operation_id: Uuid,
        action: RestorationRecoveryAction,
    },
    Quarantined {
        meeting_id: Uuid,
        operation_id: Option<Uuid>,
        reason: RestorationQuarantineReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestorationRecoveryReport {
    pub operations: Vec<RestorationRecoveryDisposition>,
}

pub struct TranscriptRestorationCoordinator {
    storage: StorageRoot,
    operations: OperationStore,
    coordination: Arc<MeetingStorageCoordination>,
    worker: Arc<dyn TranscriptRestoreWorker>,
    artifacts: Arc<dyn TranscriptArtifactInspector>,
    identities: Arc<dyn RestorationIdentitySource>,
    failures: Arc<dyn RestorationFailureInjection>,
}

impl TranscriptRestorationCoordinator {
    pub fn new(
        storage: StorageRoot,
        coordination: Arc<MeetingStorageCoordination>,
        worker: Arc<dyn TranscriptRestoreWorker>,
        artifacts: Arc<dyn TranscriptArtifactInspector>,
    ) -> Result<Self, TranscriptRestorationCoordinatorError> {
        Self::with_runtime(
            storage,
            coordination,
            worker,
            artifacts,
            Arc::new(SystemRestorationIdentity),
            Arc::new(NoRestorationFailureInjection),
        )
    }

    pub fn with_runtime(
        storage: StorageRoot,
        coordination: Arc<MeetingStorageCoordination>,
        worker: Arc<dyn TranscriptRestoreWorker>,
        artifacts: Arc<dyn TranscriptArtifactInspector>,
        identities: Arc<dyn RestorationIdentitySource>,
        failures: Arc<dyn RestorationFailureInjection>,
    ) -> Result<Self, TranscriptRestorationCoordinatorError> {
        let operations = OperationStore::open(&storage)?;
        Ok(Self {
            storage,
            operations,
            coordination,
            worker,
            artifacts,
            identities,
            failures,
        })
    }

    pub fn restore_withheld_turn(
        &self,
        arguments: &RestoreWithheldTurnUiArgs,
    ) -> Result<Uuid, TranscriptRestorationCoordinatorError> {
        arguments.validate()?;
        let meeting_id = arguments.meeting_id.to_string();
        let _lease = self.coordination.acquire(&meeting_id)?;
        if !self
            .nonterminal_for_meeting(arguments.meeting_id)?
            .is_empty()
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "another nonterminal operation already exists",
            ));
        }

        let operation_id = self.identities.operation_id();
        let meeting_dir = self.meeting_dir(arguments.meeting_id)?;
        let meeting = self.load_and_verify_meeting(&meeting_dir)?;
        self.require_current_source(
            &meeting_dir,
            &meeting,
            arguments.meeting_id,
            &arguments.source_transcript_sha256,
            arguments.source_turn_index,
            None,
        )?;
        let request = TranscriptRestorationRequest {
            schema: TranscriptRestorationRequestSchema::V1,
            operation_id,
            meeting_id: arguments.meeting_id,
            requested_at_epoch_seconds: self.identities.now_epoch_seconds(),
            source_transcript_sha256: arguments.source_transcript_sha256.clone(),
            source_turn_index: arguments.source_turn_index,
            prior_note: meeting.artifacts.current_note.clone(),
        };
        request.validate()?;
        self.operations
            .write_request(&StoredOperationRequest::Restoration(request.clone()))?;
        self.checkpoint(RestorationDurablePhase::RequestStored)?;
        self.execute_request(&meeting_dir, &request)?;
        Ok(operation_id)
    }

    pub fn recover_incomplete(
        &self,
    ) -> Result<RestorationRecoveryReport, TranscriptRestorationCoordinatorError> {
        let receipts = self.operations.scan()?;
        let mut restoration_meetings = BTreeSet::new();
        for receipt in receipts.values() {
            if receipt.commit.is_none()
                && let StoredOperationRequest::Restoration(request) = &receipt.request
            {
                restoration_meetings.insert(request.meeting_id);
            }
        }

        let mut report = RestorationRecoveryReport::default();
        for meeting_id in restoration_meetings {
            let _lease = match self.coordination.acquire(&meeting_id.to_string()) {
                Ok(lease) => lease,
                Err(_) => {
                    report
                        .operations
                        .push(RestorationRecoveryDisposition::Quarantined {
                            meeting_id,
                            operation_id: None,
                            reason: RestorationQuarantineReason::StorageUnavailable,
                        });
                    continue;
                }
            };
            let nonterminal = self.nonterminal_for_meeting(meeting_id)?;
            if nonterminal.len() != 1 {
                report
                    .operations
                    .push(RestorationRecoveryDisposition::Quarantined {
                        meeting_id,
                        operation_id: None,
                        reason: RestorationQuarantineReason::MultipleNonterminalOperations,
                    });
                continue;
            }
            let (operation_id, receipt) = &nonterminal[0];
            let StoredOperationRequest::Restoration(request) = &receipt.request else {
                report
                    .operations
                    .push(RestorationRecoveryDisposition::Quarantined {
                        meeting_id,
                        operation_id: Some(*operation_id),
                        reason: RestorationQuarantineReason::MultipleNonterminalOperations,
                    });
                continue;
            };
            match self.resume_request(request, receipt.result.as_ref()) {
                Ok(action) => report
                    .operations
                    .push(RestorationRecoveryDisposition::Recovered {
                        operation_id: *operation_id,
                        action,
                    }),
                Err(error) => report
                    .operations
                    .push(RestorationRecoveryDisposition::Quarantined {
                        meeting_id,
                        operation_id: Some(*operation_id),
                        reason: quarantine_reason(&error),
                    }),
            }
        }
        Ok(report)
    }

    fn execute_request(
        &self,
        meeting_dir: &Path,
        request: &TranscriptRestorationRequest,
    ) -> Result<(), TranscriptRestorationCoordinatorError> {
        let meeting = self.load_and_verify_meeting(meeting_dir)?;
        let source = self.require_current_source(
            meeting_dir,
            &meeting,
            request.meeting_id,
            &request.source_transcript_sha256,
            request.source_turn_index,
            Some(request.prior_note.as_ref()),
        )?;
        let worker_arguments = TranscriptRestoreWorkerArgs {
            meeting_id: request.meeting_id,
            source_transcript_sha256: request.source_transcript_sha256.clone(),
            source_turn_index: request.source_turn_index,
        };
        worker_arguments.validate()?;
        let digests = self.worker.restore(&worker_arguments)?;
        self.checkpoint(RestorationDurablePhase::WorkerArtifactReturned)?;
        let result = self.inspect_worker_result(meeting_dir, request, &source, &digests)?;
        self.operations.write_result(
            request.operation_id,
            &StoredOperationResult::Restoration(result.clone()),
        )?;
        self.checkpoint(RestorationDurablePhase::ResultStored)?;
        self.apply_result(meeting_dir, request, &result)
    }

    fn resume_request(
        &self,
        request: &TranscriptRestorationRequest,
        stored_result: Option<&StoredOperationResult>,
    ) -> Result<RestorationRecoveryAction, TranscriptRestorationCoordinatorError> {
        let meeting_dir = self.meeting_dir(request.meeting_id)?;
        let meeting = self.load_and_verify_meeting(&meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            TranscriptRestorationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        let result = match stored_result {
            None => {
                let recovery = crate::operations::classify_restoration_recovery(
                    request,
                    None,
                    meeting.lifecycle,
                    &current.sha256,
                    meeting.artifacts.current_note.as_ref(),
                )?;
                if recovery != IncompleteOperationRecovery::RetryRequest {
                    return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                        "request-only recovery was not retryable",
                    ));
                }
                self.require_current_source(
                    &meeting_dir,
                    &meeting,
                    request.meeting_id,
                    &request.source_transcript_sha256,
                    request.source_turn_index,
                    Some(request.prior_note.as_ref()),
                )?;
                self.execute_request(&meeting_dir, request)?;
                return Ok(RestorationRecoveryAction::RetriedRequest);
            }
            Some(StoredOperationResult::Restoration(result)) => result,
            Some(StoredOperationResult::NoteGeneration(_)) => {
                return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                    "restoration request has another result schema",
                ));
            }
        };

        self.inspect_stored_result(&meeting_dir, request, result)?;
        let recovery = crate::operations::classify_restoration_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )?;
        match recovery {
            IncompleteOperationRecovery::ApplyValidatedResult => {
                self.apply_result(&meeting_dir, request, result)?;
                Ok(RestorationRecoveryAction::AppliedValidatedResult)
            }
            IncompleteOperationRecovery::WriteMissingCommit => {
                self.write_terminal_commit(&meeting_dir, request, result)?;
                Ok(RestorationRecoveryAction::WroteMissingCommit)
            }
            IncompleteOperationRecovery::RetryRequest => {
                Err(TranscriptRestorationCoordinatorError::Ambiguous(
                    "stored result unexpectedly classified as request-only",
                ))
            }
        }
    }

    fn inspect_worker_result(
        &self,
        meeting_dir: &Path,
        request: &TranscriptRestorationRequest,
        source: &InspectedTranscriptRevision,
        digests: &HashMap<String, String>,
    ) -> Result<TranscriptRestorationResult, TranscriptRestorationCoordinatorError> {
        crate::operations::validate_restore_worker_digests(
            digests,
            &request.source_transcript_sha256,
        )?;
        let successor = self.artifacts.inspect_revision(
            meeting_dir,
            request.meeting_id,
            &digests["transcript"],
        )?;
        if successor.digest != digests["transcript"] {
            return Err(TranscriptArtifactError::Malformed.into());
        }
        let view = successor
            .head_view
            .as_ref()
            .ok_or(TranscriptArtifactError::Malformed)?;
        let result = TranscriptRestorationResult {
            schema: TranscriptRestorationResultSchema::V1,
            operation_id: request.operation_id,
            meeting_id: request.meeting_id,
            source_transcript_sha256: request.source_transcript_sha256.clone(),
            view: ArtifactRef {
                relative_path: format!("transcript/{}.json", successor.digest),
                sha256: successor.digest.clone(),
            },
            base_transcript_sha256: successor.base_transcript_sha256.clone(),
            parent_transcript_sha256: view.parent_transcript_sha256.clone(),
            restored_source_turn_indices: successor.restored_source_turn_indices.clone(),
        };
        validate_restore_worker_join(digests, request, source.head_view.as_ref(), view, &result)?;
        Ok(result)
    }

    fn inspect_stored_result(
        &self,
        meeting_dir: &Path,
        request: &TranscriptRestorationRequest,
        result: &TranscriptRestorationResult,
    ) -> Result<(), TranscriptRestorationCoordinatorError> {
        let source = self.artifacts.inspect_revision(
            meeting_dir,
            request.meeting_id,
            &request.source_transcript_sha256,
        )?;
        let successor = self.artifacts.inspect_revision(
            meeting_dir,
            request.meeting_id,
            &result.view.sha256,
        )?;
        if source.digest != request.source_transcript_sha256
            || successor.digest != result.view.sha256
        {
            return Err(TranscriptArtifactError::Malformed.into());
        }
        let view = successor
            .head_view
            .as_ref()
            .ok_or(TranscriptArtifactError::Malformed)?;
        validate_restoration_join(request, source.head_view.as_ref(), view, result)?;
        if successor.base_transcript_sha256 != result.base_transcript_sha256
            || successor.restored_source_turn_indices != result.restored_source_turn_indices
            || !successor
                .withheld_source_turn_indices
                .contains(&request.source_turn_index)
        {
            return Err(TranscriptArtifactError::Malformed.into());
        }
        Ok(())
    }

    fn apply_result(
        &self,
        meeting_dir: &Path,
        request: &TranscriptRestorationRequest,
        result: &TranscriptRestorationResult,
    ) -> Result<(), TranscriptRestorationCoordinatorError> {
        let mut meeting = self.load_and_verify_meeting(meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            TranscriptRestorationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        if crate::operations::classify_restoration_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )? != IncompleteOperationRecovery::ApplyValidatedResult
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting no longer accepts the validated result",
            ));
        }
        self.inspect_stored_result(meeting_dir, request, result)?;
        meeting.artifacts.current_transcript = Some(result.view.clone());
        meeting.artifacts.current_note = None;
        meeting.lifecycle = MeetingLifecycle::TranscriptReady;
        write_meeting(meeting_dir, &meeting)?;
        self.checkpoint(RestorationDurablePhase::MeetingPublished)?;
        self.write_terminal_commit(meeting_dir, request, result)
    }

    fn write_terminal_commit(
        &self,
        meeting_dir: &Path,
        request: &TranscriptRestorationRequest,
        result: &TranscriptRestorationResult,
    ) -> Result<(), TranscriptRestorationCoordinatorError> {
        let meeting = self.load_and_verify_meeting(meeting_dir)?;
        require_no_pending_storage(&meeting)?;
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            TranscriptRestorationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        if crate::operations::classify_restoration_recovery(
            request,
            Some(result),
            meeting.lifecycle,
            &current.sha256,
            meeting.artifacts.current_note.as_ref(),
        )? != IncompleteOperationRecovery::WriteMissingCommit
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting does not match the exact successor state",
            ));
        }
        self.inspect_stored_result(meeting_dir, request, result)?;
        let meeting_bytes =
            read_private_bytes(&meeting_dir.join("meeting.json"), MAX_MEETING_RECORD_BYTES)?;
        if meeting_bytes != canonical_bytes(&meeting)? {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting bytes are not canonical",
            ));
        }
        let commit = MeetingOperationCommit {
            schema: MeetingOperationCommitSchema::V1,
            operation_id: request.operation_id,
            meeting_id: request.meeting_id,
            kind: ProductOperationKind::RestoreWithheldTurn,
            outcome: ProductOperationOutcome::TranscriptRestored,
            request_sha256: digest_canonical(request)?,
            result_sha256: digest_canonical(result)?,
            committed_meeting_sha256: digest_bytes(&meeting_bytes),
            committed_at_epoch_seconds: self.identities.now_epoch_seconds(),
            lifecycle: meeting.lifecycle,
            current_transcript_sha256: current.sha256.clone(),
            current_note: meeting.artifacts.current_note.clone(),
        };
        self.operations
            .write_commit(request.operation_id, &commit, &meeting, &meeting_bytes)?;
        self.checkpoint(RestorationDurablePhase::CommitStored)
    }

    fn require_current_source(
        &self,
        meeting_dir: &Path,
        meeting: &MeetingRecord,
        meeting_id: Uuid,
        source_transcript_sha256: &str,
        source_turn_index: u32,
        expected_prior_note: Option<Option<&NoteRevisionRef>>,
    ) -> Result<InspectedTranscriptRevision, TranscriptRestorationCoordinatorError> {
        require_no_pending_storage(meeting)?;
        if meeting.meeting_id != meeting_id.to_string() {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting identity changed",
            ));
        }
        let current = meeting.artifacts.current_transcript.as_ref().ok_or(
            TranscriptRestorationCoordinatorError::Ambiguous("meeting has no current transcript"),
        )?;
        if current.sha256 != source_transcript_sha256
            || current.relative_path != format!("transcript/{source_transcript_sha256}.json")
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting current transcript changed",
            ));
        }
        let eligible = match meeting.lifecycle {
            MeetingLifecycle::Ready => meeting.artifacts.current_note.is_some(),
            MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed => {
                meeting.artifacts.current_note.is_none()
            }
            _ => false,
        };
        if !eligible {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting lifecycle cannot accept restoration",
            ));
        }
        if let Some(expected) = expected_prior_note
            && meeting.artifacts.current_note.as_ref() != expected
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "meeting prior note changed",
            ));
        }
        let source =
            self.artifacts
                .inspect_revision(meeting_dir, meeting_id, source_transcript_sha256)?;
        if source.digest != source_transcript_sha256 {
            return Err(TranscriptArtifactError::Malformed.into());
        }
        if !source
            .withheld_source_turn_indices
            .contains(&source_turn_index)
        {
            return Err(TranscriptArtifactError::NonWithheld.into());
        }
        if source
            .restored_source_turn_indices
            .binary_search(&source_turn_index)
            .is_ok()
        {
            return Err(TranscriptRestorationCoordinatorError::Ambiguous(
                "requested turn is already restored",
            ));
        }
        Ok(source)
    }

    fn load_and_verify_meeting(
        &self,
        meeting_dir: &Path,
    ) -> Result<MeetingRecord, TranscriptRestorationCoordinatorError> {
        let meeting = load_meeting(meeting_dir)?;
        verify_record_artifacts(meeting_dir, &meeting)?;
        Ok(meeting)
    }

    fn nonterminal_for_meeting(
        &self,
        meeting_id: Uuid,
    ) -> Result<Vec<(Uuid, StoredOperationReceipt)>, TranscriptRestorationCoordinatorError> {
        let mut operations: Vec<_> = self
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
        operations.sort_by_key(|(operation_id, _)| *operation_id);
        Ok(operations)
    }

    fn meeting_dir(
        &self,
        meeting_id: Uuid,
    ) -> Result<PathBuf, TranscriptRestorationCoordinatorError> {
        self.storage
            .resolve(&Path::new("meetings").join(meeting_id.to_string()))
            .map_err(|_| TranscriptRestorationCoordinatorError::StorageUnavailable)
    }

    fn checkpoint(
        &self,
        phase: RestorationDurablePhase,
    ) -> Result<(), TranscriptRestorationCoordinatorError> {
        self.failures
            .after_phase(phase)
            .map_err(|_| TranscriptRestorationCoordinatorError::InjectedCrash(phase))
    }
}

fn require_no_pending_storage(
    meeting: &MeetingRecord,
) -> Result<(), TranscriptRestorationCoordinatorError> {
    if meeting.pending_storage_operation.is_some() {
        return Err(TranscriptRestorationCoordinatorError::Ambiguous(
            "meeting has a pending storage operation",
        ));
    }
    Ok(())
}

/// A reader-resolved current transcript: the audited chain inspection plus
/// the base revision's verified bytes, so a reader parses exactly the bytes
/// the chain walk validated.
#[derive(Debug, Clone)]
pub struct ResolvedStoredTranscript {
    pub inspection: InspectedTranscriptRevision,
    pub base_bytes: Vec<u8>,
}

pub fn resolve_stored_transcript(
    meeting_dir: &Path,
    meeting_id: Uuid,
    transcript_sha256: &str,
) -> Result<ResolvedStoredTranscript, TranscriptArtifactError> {
    let mut read = |digest: &str| read_revision(meeting_dir, digest);
    resolve_stored_transcript_with(&mut read, meeting_id, transcript_sha256)
}

/// Resolution that serves the head revision from bytes the caller already
/// read and verified against the meeting record, so the head file is not
/// read (or budget-charged) twice. The walker still re-verifies the digest
/// of the primed bytes like any other hop.
pub fn resolve_stored_transcript_primed(
    meeting_dir: &Path,
    meeting_id: Uuid,
    transcript_sha256: &str,
    head_bytes: Vec<u8>,
) -> Result<ResolvedStoredTranscript, TranscriptArtifactError> {
    let mut primed = Some(head_bytes);
    let head_digest = transcript_sha256.to_owned();
    let mut read = |digest: &str| {
        if digest == head_digest
            && let Some(bytes) = primed.take()
        {
            return Ok(bytes);
        }
        read_revision(meeting_dir, digest)
    };
    resolve_stored_transcript_with(&mut read, meeting_id, transcript_sha256)
}

/// The same resolution under a caller-supplied byte reader, so budgeted
/// readers (the library projection) account every hop against their own
/// limits. Each hop's content address is re-verified here regardless of what
/// the reader returned.
pub fn resolve_stored_transcript_with(
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, TranscriptArtifactError>,
    meeting_id: Uuid,
    transcript_sha256: &str,
) -> Result<ResolvedStoredTranscript, TranscriptArtifactError> {
    inspect_revision_chain(read, meeting_id, transcript_sha256).map(|(inspection, base_bytes)| {
        ResolvedStoredTranscript {
            inspection,
            base_bytes,
        }
    })
}

fn inspect_stored_revision(
    meeting_dir: &Path,
    meeting_id: Uuid,
    transcript_sha256: &str,
) -> Result<InspectedTranscriptRevision, TranscriptArtifactError> {
    let mut read = |digest: &str| read_revision(meeting_dir, digest);
    inspect_revision_chain(&mut read, meeting_id, transcript_sha256)
        .map(|(inspection, _)| inspection)
}

fn inspect_revision_chain(
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, TranscriptArtifactError>,
    meeting_id: Uuid,
    transcript_sha256: &str,
) -> Result<(InspectedTranscriptRevision, Vec<u8>), TranscriptArtifactError> {
    validate_digest(transcript_sha256)?;
    let mut current = transcript_sha256.to_owned();
    let mut seen = BTreeSet::new();
    let mut views = Vec::new();
    let mut view_bytes_total: u64 = 0;
    let (base_digest, withheld, base_bytes) = loop {
        if !seen.insert(current.clone()) {
            return Err(TranscriptArtifactError::Cyclic);
        }
        if views.len() >= MAX_TRANSCRIPT_VIEW_DEPTH {
            return Err(TranscriptArtifactError::TooDeep);
        }
        let bytes = read(&current)?;
        if digest_bytes(&bytes) != current {
            return Err(TranscriptArtifactError::Changed);
        }
        let document: Value =
            serde_json::from_slice(&bytes).map_err(|_| TranscriptArtifactError::Malformed)?;
        if document
            .as_object()
            .and_then(|object| object.get("schema"))
            .and_then(Value::as_str)
            == Some("transcript-view/1")
        {
            let view: TranscriptView =
                serde_json::from_value(document).map_err(|_| TranscriptArtifactError::Malformed)?;
            view.validate()
                .map_err(|_| TranscriptArtifactError::Malformed)?;
            if view.meeting_id != meeting_id
                || canonical_bytes(&view).map_err(|_| TranscriptArtifactError::Malformed)? != bytes
            {
                return Err(TranscriptArtifactError::Malformed);
            }
            view_bytes_total = view_bytes_total.saturating_add(bytes.len() as u64);
            if view_bytes_total > MAX_TRANSCRIPT_VIEW_CHAIN_BYTES {
                return Err(TranscriptArtifactError::TooDeep);
            }
            let parent = view.parent_transcript_sha256.clone();
            views.push((current, view));
            current = parent;
            continue;
        }
        let withheld = inspect_base_transcript(&document)?;
        break (current, withheld, bytes);
    };

    let mut previous_digest = base_digest.clone();
    let mut restored = Vec::new();
    for (digest, view) in views.iter().rev() {
        if view.base_transcript_sha256 != base_digest
            || view.parent_transcript_sha256 != previous_digest
        {
            return Err(TranscriptArtifactError::Malformed);
        }
        let previous: BTreeSet<_> = restored.iter().copied().collect();
        let next: BTreeSet<_> = view.restored_source_turn_indices.iter().copied().collect();
        if !previous.is_subset(&next)
            || next.len() != previous.len() + 1
            || !next.is_subset(&withheld)
        {
            return Err(if !next.is_subset(&withheld) {
                TranscriptArtifactError::NonWithheld
            } else {
                TranscriptArtifactError::NotOneTurnSuccessor
            });
        }
        restored = view.restored_source_turn_indices.clone();
        previous_digest = digest.clone();
    }

    Ok((
        InspectedTranscriptRevision {
            digest: transcript_sha256.to_owned(),
            base_transcript_sha256: base_digest,
            head_view: views.first().map(|(_, view)| view.clone()),
            restored_source_turn_indices: restored,
            withheld_source_turn_indices: withheld,
        },
        base_bytes,
    ))
}

fn read_revision(meeting_dir: &Path, digest: &str) -> Result<Vec<u8>, TranscriptArtifactError> {
    validate_digest(digest)?;
    let path = meeting_dir
        .join("transcript")
        .join(format!("{digest}.json"));
    let bytes = read_private_bytes(&path, MAX_TRANSCRIPT_REVISION_BYTES).map_err(|error| {
        if matches!(&error, MeetingError::Io(io) if io.kind() == std::io::ErrorKind::NotFound) {
            TranscriptArtifactError::Missing
        } else {
            TranscriptArtifactError::Changed
        }
    })?;
    if digest_bytes(&bytes) != digest {
        return Err(TranscriptArtifactError::Changed);
    }
    Ok(bytes)
}

fn inspect_base_transcript(document: &Value) -> Result<BTreeSet<u32>, TranscriptArtifactError> {
    let object = document
        .as_object()
        .ok_or(TranscriptArtifactError::Malformed)?;
    if object.get("schema").and_then(Value::as_str) != Some("capture-transcript/1")
        || object.get("source").and_then(Value::as_str).is_none()
        || !matches!(
            object.get("attribution").and_then(Value::as_str),
            Some("named" | "channel" | "none")
        )
        || !object.get("capture_health").is_some_and(Value::is_object)
    {
        return Err(TranscriptArtifactError::Malformed);
    }
    let turns = object
        .get("turns")
        .and_then(Value::as_array)
        .ok_or(TranscriptArtifactError::Malformed)?;
    if turns.len() > MAX_TRANSCRIPT_TURNS {
        return Err(TranscriptArtifactError::Malformed);
    }
    let mut withheld = BTreeSet::new();
    for (index, turn) in turns.iter().enumerate() {
        let turn = turn.as_object().ok_or(TranscriptArtifactError::Malformed)?;
        let start = turn
            .get("start")
            .and_then(Value::as_f64)
            .ok_or(TranscriptArtifactError::Malformed)?;
        let end = turn
            .get("end")
            .and_then(Value::as_f64)
            .ok_or(TranscriptArtifactError::Malformed)?;
        let text = turn
            .get("text")
            .and_then(Value::as_str)
            .ok_or(TranscriptArtifactError::Malformed)?;
        let gated = match turn.get("gated") {
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(TranscriptArtifactError::Malformed),
            None => false,
        };
        // An explicit JSON null counts as absent, matching both typed readers,
        // so the same base bytes get one verdict whether or not a view sits
        // above them.
        let gate_field_present = |name: &str| turn.get(name).is_some_and(|value| !value.is_null());
        if !start.is_finite()
            || !end.is_finite()
            || start < 0.0
            || end < start
            || text.len() > 100_000
            || ((gate_field_present("gate_score") || gate_field_present("gate_reason")) && !gated)
        {
            return Err(TranscriptArtifactError::Malformed);
        }
        if gated {
            withheld.insert(index as u32);
        }
    }
    Ok(withheld)
}

fn validate_digest(value: &str) -> Result<(), TranscriptArtifactError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TranscriptArtifactError::Malformed);
    }
    Ok(())
}

fn canonical_bytes<T: Serialize>(
    value: &T,
) -> Result<Vec<u8>, TranscriptRestorationCoordinatorError> {
    serde_json::to_vec_pretty(value).map_err(|_| {
        TranscriptRestorationCoordinatorError::Ambiguous("contract cannot be serialized")
    })
}

fn digest_canonical<T: Serialize>(
    value: &T,
) -> Result<String, TranscriptRestorationCoordinatorError> {
    Ok(digest_bytes(&canonical_bytes(value)?))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn quarantine_reason(error: &TranscriptRestorationCoordinatorError) -> RestorationQuarantineReason {
    match error {
        TranscriptRestorationCoordinatorError::Worker(_) => {
            RestorationQuarantineReason::WorkerUnavailable
        }
        TranscriptRestorationCoordinatorError::Artifact(_) => {
            RestorationQuarantineReason::InvalidArtifact
        }
        TranscriptRestorationCoordinatorError::Ambiguous(_) => {
            RestorationQuarantineReason::SourceOrPriorNoteChanged
        }
        TranscriptRestorationCoordinatorError::Coordination(_)
        | TranscriptRestorationCoordinatorError::StorageUnavailable => {
            RestorationQuarantineReason::StorageUnavailable
        }
        TranscriptRestorationCoordinatorError::Meeting(_)
        | TranscriptRestorationCoordinatorError::OperationStore(_)
        | TranscriptRestorationCoordinatorError::Contract(_)
        | TranscriptRestorationCoordinatorError::InjectedCrash(_) => {
            RestorationQuarantineReason::InvalidReceipt
        }
    }
}

#[cfg(test)]
mod tests;
