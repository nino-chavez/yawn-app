use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::local_vocabulary::{
    VocabularyRangeReplacement, vocabulary_range_replacements_are_valid,
};
use crate::meeting::{ArtifactRef, MeetingLifecycle, MeetingRecord, NoteRevisionRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptViewSchema {
    #[serde(rename = "transcript-view/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptView {
    pub schema: TranscriptViewSchema,
    pub meeting_id: Uuid,
    pub base_transcript_sha256: String,
    pub parent_transcript_sha256: String,
    pub restored_source_turn_indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreWithheldTurnUiArgs {
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    pub source_turn_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegenerateNoteUiArgs {
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    /// Exact speaker-label substitutions for this generation only. The
    /// retained transcript remains the source record and its digest remains
    /// the evidence identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speaker_label_overrides: Vec<SpeakerLabelOverride>,
    /// Prompt-only text substitutions tied to exact retained-source spans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vocabulary_replacements: Vec<VocabularyRangeReplacement>,
    /// The operator's own labeled framing for this meeting, re-derived by the
    /// caller from `meeting-context.json` immediately before this call.
    ///
    /// Never transcript-backed and never a claim: it may only widen or narrow
    /// which retained excerpts the generator emphasizes. Absent when the
    /// operator left it blank, which keeps a no-context request byte-identical
    /// to the request shape before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_meeting_context: Option<String>,
}

/// The browser may name only the transcript it is comparing. The desktop
/// process derives the retained capture and audio identities under its meeting
/// lease; accepting caller-supplied audio identities would weaken that bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptRetryUiArgs {
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
}

/// One exact source speaker group and the label the note generator may see.
///
/// The closed three-group vocabulary mirrors the capture transcript: the
/// operator channel, the remote channel, or an unattributed row. It cannot
/// introduce a new source identity or change any source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeakerLabelOverride {
    pub source_speaker: Option<String>,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRestoreWorkerArgs {
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    pub source_turn_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteCreateWorkerArgs {
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speaker_label_overrides: Vec<SpeakerLabelOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vocabulary_replacements: Vec<VocabularyRangeReplacement>,
    /// Forwarded into the sandboxed generate child's request; the worker's
    /// `note.create` assembler does not read it back, since generation has
    /// already run by the time this reaches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_meeting_context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductOperationKind {
    RestoreWithheldTurn,
    GenerateNote,
    TranscriptRetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiOperationState {
    Correcting,
    Summarizing,
    Transcribing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiOperationSchema {
    #[serde(rename = "ui-operation/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiOperationAccepted {
    pub schema: UiOperationSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub kind: ProductOperationKind,
    pub state: UiOperationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptRestorationRequestSchema {
    #[serde(rename = "transcript-restoration-request/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRestorationRequest {
    pub schema: TranscriptRestorationRequestSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub requested_at_epoch_seconds: u64,
    pub source_transcript_sha256: String,
    pub source_turn_index: u32,
    pub prior_note: Option<NoteRevisionRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptRestorationResultSchema {
    #[serde(rename = "transcript-restoration-result/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptRestorationResult {
    pub schema: TranscriptRestorationResultSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    pub view: ArtifactRef,
    pub base_transcript_sha256: String,
    pub parent_transcript_sha256: String,
    pub restored_source_turn_indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteGenerationRequestSchema {
    #[serde(rename = "note-generation-request/1")]
    V1,
    #[serde(rename = "note-generation-request/2")]
    V2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteGenerationRequest {
    pub schema: NoteGenerationRequestSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub requested_at_epoch_seconds: u64,
    pub source_transcript_sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speaker_label_overrides: Vec<SpeakerLabelOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vocabulary_replacements: Vec<VocabularyRangeReplacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_note: Option<NoteRevisionRef>,
    /// Carried verbatim from `RegenerateNoteUiArgs` so a crash-recovered
    /// replay reissues the same generation the operator asked for. See that
    /// field's doc comment for what it may and may not do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_meeting_context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteGenerationStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteFailureCode {
    NoteRejected,
    ModelUnavailable,
    OperationInterrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteCreateWorkerFailureCode {
    NoteRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteCreateWorkerFailure {
    pub code: NoteCreateWorkerFailureCode,
    pub recoverable: bool,
    pub artifact_digests: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteGenerationResultSchema {
    #[serde(rename = "note-generation-result/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteGenerationResult {
    pub schema: NoteGenerationResultSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub source_transcript_sha256: String,
    pub status: NoteGenerationStatus,
    pub note: Option<NoteRevisionRef>,
    pub failure_code: Option<NoteFailureCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductOperationOutcome {
    TranscriptRestored,
    NoteAccepted,
    NoteRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IncompleteOperationRecovery {
    RetryRequest,
    ApplyValidatedResult,
    WriteMissingCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeetingOperationCommitSchema {
    #[serde(rename = "meeting-operation-commit/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeetingOperationCommit {
    pub schema: MeetingOperationCommitSchema,
    pub operation_id: Uuid,
    pub meeting_id: Uuid,
    pub kind: ProductOperationKind,
    pub outcome: ProductOperationOutcome,
    pub request_sha256: String,
    pub result_sha256: String,
    pub committed_meeting_sha256: String,
    pub committed_at_epoch_seconds: u64,
    pub lifecycle: MeetingLifecycle,
    pub current_transcript_sha256: String,
    pub current_note: Option<NoteRevisionRef>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationContractError {
    #[error("operation contract is malformed: {0}")]
    Malformed(&'static str),
}

impl TranscriptView {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.base_transcript_sha256)?;
        validate_digest(&self.parent_transcript_sha256)?;
        if self.restored_source_turn_indices.is_empty()
            || !strictly_increasing(&self.restored_source_turn_indices)
        {
            return Err(OperationContractError::Malformed(
                "restored turn indices must be nonempty, unique, and sorted",
            ));
        }
        Ok(())
    }
}

impl RestoreWithheldTurnUiArgs {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)
    }
}

impl RegenerateNoteUiArgs {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)?;
        validate_speaker_label_overrides(&self.speaker_label_overrides)?;
        validate_vocabulary_replacements(&self.vocabulary_replacements)?;
        validate_pre_meeting_context(&self.pre_meeting_context)
    }
}

impl TranscriptRetryUiArgs {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)
    }
}

impl TranscriptRestoreWorkerArgs {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)
    }
}

impl NoteCreateWorkerArgs {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)?;
        validate_speaker_label_overrides(&self.speaker_label_overrides)?;
        validate_vocabulary_replacements(&self.vocabulary_replacements)?;
        validate_pre_meeting_context(&self.pre_meeting_context)
    }
}

impl UiOperationAccepted {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        if !matches!(
            (self.kind, self.state),
            (
                ProductOperationKind::RestoreWithheldTurn,
                UiOperationState::Correcting
            ) | (
                ProductOperationKind::GenerateNote,
                UiOperationState::Summarizing
            ) | (
                ProductOperationKind::TranscriptRetry,
                UiOperationState::Transcribing
            )
        ) {
            return Err(OperationContractError::Malformed(
                "UI operation kind and state disagree",
            ));
        }
        Ok(())
    }
}

impl TranscriptRestorationRequest {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        if self.requested_at_epoch_seconds == 0 {
            return Err(OperationContractError::Malformed(
                "request time must be positive",
            ));
        }
        validate_digest(&self.source_transcript_sha256)?;
        if let Some(note) = &self.prior_note {
            validate_note(note)?;
            if note.source_transcript_sha256 != self.source_transcript_sha256 {
                return Err(OperationContractError::Malformed(
                    "prior note binds another transcript",
                ));
            }
        }
        Ok(())
    }
}

impl TranscriptRestorationResult {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)?;
        validate_digest(&self.base_transcript_sha256)?;
        validate_digest(&self.parent_transcript_sha256)?;
        self.view
            .validate_digest_named("transcript", "json")
            .map_err(|_| OperationContractError::Malformed("transcript view is invalid"))?;
        if self.parent_transcript_sha256 != self.source_transcript_sha256 {
            return Err(OperationContractError::Malformed(
                "restoration result parent differs from request source",
            ));
        }
        if self.restored_source_turn_indices.is_empty()
            || !strictly_increasing(&self.restored_source_turn_indices)
        {
            return Err(OperationContractError::Malformed(
                "restored turn indices must be nonempty, unique, and sorted",
            ));
        }
        Ok(())
    }
}

impl NoteGenerationRequest {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        if self.requested_at_epoch_seconds == 0 {
            return Err(OperationContractError::Malformed(
                "request time must be positive",
            ));
        }
        validate_digest(&self.source_transcript_sha256)?;
        validate_speaker_label_overrides(&self.speaker_label_overrides)?;
        validate_vocabulary_replacements(&self.vocabulary_replacements)?;
        validate_pre_meeting_context(&self.pre_meeting_context)?;
        match (self.schema, &self.prior_note) {
            (NoteGenerationRequestSchema::V1, Some(_)) => Err(OperationContractError::Malformed(
                "request/1 cannot replace a prior note",
            )),
            (_, Some(note)) => {
                validate_note(note)?;
                if note.source_transcript_sha256 != self.source_transcript_sha256 {
                    return Err(OperationContractError::Malformed(
                        "prior note binds another transcript",
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl NoteGenerationResult {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        validate_digest(&self.source_transcript_sha256)?;
        match (self.status, &self.note, self.failure_code) {
            (NoteGenerationStatus::Accepted, Some(note), None) => {
                validate_note(note)?;
                if note.source_transcript_sha256 != self.source_transcript_sha256 {
                    return Err(OperationContractError::Malformed(
                        "accepted note binds another transcript",
                    ));
                }
            }
            (NoteGenerationStatus::Rejected, None, Some(_)) => {}
            _ => {
                return Err(OperationContractError::Malformed(
                    "note result status, note, and failure code disagree",
                ));
            }
        }
        Ok(())
    }
}

impl NoteCreateWorkerFailure {
    pub fn validate(&self) -> Result<NoteFailureCode, OperationContractError> {
        if !self.recoverable || !self.artifact_digests.is_empty() {
            return Err(OperationContractError::Malformed(
                "rejected note worker result must be recoverable and artifact-free",
            ));
        }
        match self.code {
            NoteCreateWorkerFailureCode::NoteRejected => Ok(NoteFailureCode::NoteRejected),
        }
    }
}

impl MeetingOperationCommit {
    pub fn validate(&self) -> Result<(), OperationContractError> {
        for digest in [
            &self.request_sha256,
            &self.result_sha256,
            &self.committed_meeting_sha256,
            &self.current_transcript_sha256,
        ] {
            validate_digest(digest)?;
        }
        if self.committed_at_epoch_seconds == 0 {
            return Err(OperationContractError::Malformed(
                "commit time must be positive",
            ));
        }
        if let Some(note) = &self.current_note {
            validate_note(note)?;
            if note.source_transcript_sha256 != self.current_transcript_sha256 {
                return Err(OperationContractError::Malformed(
                    "committed note binds another transcript",
                ));
            }
        }
        let coherent = matches!(
            (
                self.kind,
                self.outcome,
                self.lifecycle,
                self.current_note.is_some()
            ),
            (
                ProductOperationKind::RestoreWithheldTurn,
                ProductOperationOutcome::TranscriptRestored,
                MeetingLifecycle::TranscriptReady,
                false
            ) | (
                ProductOperationKind::GenerateNote,
                ProductOperationOutcome::NoteAccepted,
                MeetingLifecycle::Ready,
                true
            ) | (
                ProductOperationKind::GenerateNote,
                ProductOperationOutcome::NoteRejected,
                MeetingLifecycle::SummaryFailed,
                false
            ) | (
                ProductOperationKind::GenerateNote,
                ProductOperationOutcome::NoteRejected,
                MeetingLifecycle::Ready,
                true
            )
        );
        if !coherent {
            return Err(OperationContractError::Malformed(
                "commit outcome, lifecycle, and note pointer disagree",
            ));
        }
        Ok(())
    }
}

pub fn validate_restore_worker_digests(
    digests: &HashMap<String, String>,
    source_transcript_sha256: &str,
) -> Result<(), OperationContractError> {
    validate_exact_digests(
        digests,
        &["base-transcript", "parent-transcript", "transcript"],
    )?;
    if digests["parent-transcript"] != source_transcript_sha256 {
        return Err(OperationContractError::Malformed(
            "worker restoration parent differs from request source",
        ));
    }
    Ok(())
}

pub fn validate_note_worker_digests(
    digests: &HashMap<String, String>,
    source_transcript_sha256: &str,
) -> Result<(), OperationContractError> {
    validate_exact_digests(digests, &["note", "note-markdown", "transcript"])?;
    if digests["transcript"] != source_transcript_sha256 {
        return Err(OperationContractError::Malformed(
            "worker note result differs from request source",
        ));
    }
    Ok(())
}

pub fn validate_restore_worker_join(
    digests: &HashMap<String, String>,
    request: &TranscriptRestorationRequest,
    parent_view: Option<&TranscriptView>,
    view: &TranscriptView,
    result: &TranscriptRestorationResult,
) -> Result<(), OperationContractError> {
    validate_restore_worker_digests(digests, &request.source_transcript_sha256)?;
    validate_restoration_join(request, parent_view, view, result)?;
    if digests["base-transcript"] != view.base_transcript_sha256
        || digests["parent-transcript"] != view.parent_transcript_sha256
        || digests["transcript"] != result.view.sha256
    {
        return Err(OperationContractError::Malformed(
            "worker restoration digests disagree with authoritative artifacts",
        ));
    }
    Ok(())
}

pub fn validate_note_worker_join(
    digests: &HashMap<String, String>,
    request: &NoteGenerationRequest,
    result: &NoteGenerationResult,
) -> Result<(), OperationContractError> {
    validate_note_worker_digests(digests, &request.source_transcript_sha256)?;
    request.validate()?;
    result.validate()?;
    let Some(note) = &result.note else {
        return Err(OperationContractError::Malformed(
            "accepted note worker digests lack a note result",
        ));
    };
    if result.status != NoteGenerationStatus::Accepted
        || request.operation_id != result.operation_id
        || request.meeting_id != result.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
        || digests["note"] != note.json.sha256
        || digests["note-markdown"] != note.markdown.sha256
        || digests["transcript"] != note.source_transcript_sha256
    {
        return Err(OperationContractError::Malformed(
            "worker note digests disagree with authoritative artifacts",
        ));
    }
    Ok(())
}

pub fn validate_note_worker_failure_join(
    failure: &NoteCreateWorkerFailure,
    request: &NoteGenerationRequest,
    result: &NoteGenerationResult,
) -> Result<(), OperationContractError> {
    request.validate()?;
    result.validate()?;
    let mapped_failure = failure.validate()?;
    if result.status != NoteGenerationStatus::Rejected
        || result.note.is_some()
        || result.failure_code != Some(mapped_failure)
        || request.operation_id != result.operation_id
        || request.meeting_id != result.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
    {
        return Err(OperationContractError::Malformed(
            "worker note rejection disagrees with persisted result",
        ));
    }
    Ok(())
}

pub fn validate_restoration_join(
    request: &TranscriptRestorationRequest,
    parent_view: Option<&TranscriptView>,
    view: &TranscriptView,
    result: &TranscriptRestorationResult,
) -> Result<(), OperationContractError> {
    request.validate()?;
    view.validate()?;
    result.validate()?;
    if request.operation_id != result.operation_id
        || request.meeting_id != view.meeting_id
        || request.meeting_id != result.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
        || request.source_transcript_sha256 != view.parent_transcript_sha256
        || view.base_transcript_sha256 != result.base_transcript_sha256
        || view.parent_transcript_sha256 != result.parent_transcript_sha256
        || view.restored_source_turn_indices != result.restored_source_turn_indices
        || result.view.sha256 != digest_pretty(view)?
    {
        return Err(OperationContractError::Malformed(
            "restoration request, view, and result disagree",
        ));
    }

    let mut expected_indices = match parent_view {
        Some(parent) => {
            parent.validate()?;
            if parent.meeting_id != request.meeting_id
                || parent.base_transcript_sha256 != view.base_transcript_sha256
                || digest_pretty(parent)? != request.source_transcript_sha256
                || parent
                    .restored_source_turn_indices
                    .contains(&request.source_turn_index)
            {
                return Err(OperationContractError::Malformed(
                    "parent transcript view cannot accept this restoration",
                ));
            }
            parent.restored_source_turn_indices.clone()
        }
        None => {
            if view.base_transcript_sha256 != request.source_transcript_sha256 {
                return Err(OperationContractError::Malformed(
                    "first restoration does not start from the base transcript",
                ));
            }
            Vec::new()
        }
    };
    expected_indices.push(request.source_turn_index);
    expected_indices.sort_unstable();
    if expected_indices != view.restored_source_turn_indices {
        return Err(OperationContractError::Malformed(
            "successor transcript view must add exactly the requested turn",
        ));
    }
    Ok(())
}

pub fn validate_restoration_receipts(
    request: &TranscriptRestorationRequest,
    result: &TranscriptRestorationResult,
    commit: &MeetingOperationCommit,
    meeting: &MeetingRecord,
    meeting_bytes: &[u8],
) -> Result<(), OperationContractError> {
    request.validate()?;
    result.validate()?;
    commit.validate()?;
    if request.operation_id != result.operation_id
        || request.operation_id != commit.operation_id
        || request.meeting_id != result.meeting_id
        || request.meeting_id != commit.meeting_id
        || digest_pretty(request)? != commit.request_sha256
        || digest_pretty(result)? != commit.result_sha256
        || commit.kind != ProductOperationKind::RestoreWithheldTurn
        || commit.outcome != ProductOperationOutcome::TranscriptRestored
        || commit.current_transcript_sha256 != result.view.sha256
    {
        return Err(OperationContractError::Malformed(
            "restoration receipts do not form one operation",
        ));
    }
    validate_commit_meeting(commit, meeting, meeting_bytes)
}

pub fn validate_note_receipts(
    request: &NoteGenerationRequest,
    result: &NoteGenerationResult,
    commit: &MeetingOperationCommit,
    meeting: &MeetingRecord,
    meeting_bytes: &[u8],
) -> Result<(), OperationContractError> {
    request.validate()?;
    result.validate()?;
    commit.validate()?;
    let expected_outcome = match result.status {
        NoteGenerationStatus::Accepted => ProductOperationOutcome::NoteAccepted,
        NoteGenerationStatus::Rejected => ProductOperationOutcome::NoteRejected,
    };
    let expected_current_note = match result.status {
        NoteGenerationStatus::Accepted => result.note.as_ref(),
        NoteGenerationStatus::Rejected => request.prior_note.as_ref(),
    };
    if request.operation_id != result.operation_id
        || request.operation_id != commit.operation_id
        || request.meeting_id != result.meeting_id
        || request.meeting_id != commit.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
        || request.source_transcript_sha256 != commit.current_transcript_sha256
        || digest_pretty(request)? != commit.request_sha256
        || digest_pretty(result)? != commit.result_sha256
        || commit.kind != ProductOperationKind::GenerateNote
        || commit.outcome != expected_outcome
        || commit.current_note.as_ref() != expected_current_note
    {
        return Err(OperationContractError::Malformed(
            "note receipts do not form one operation",
        ));
    }
    validate_commit_meeting(commit, meeting, meeting_bytes)
}

pub fn validate_commit_meeting(
    commit: &MeetingOperationCommit,
    meeting: &MeetingRecord,
    meeting_bytes: &[u8],
) -> Result<(), OperationContractError> {
    commit.validate()?;
    meeting
        .validate(&commit.meeting_id.to_string())
        .map_err(|_| OperationContractError::Malformed("committed meeting is invalid"))?;
    let parsed: MeetingRecord = serde_json::from_slice(meeting_bytes)
        .map_err(|_| OperationContractError::Malformed("committed meeting bytes are invalid"))?;
    let canonical = serde_json::to_vec_pretty(meeting)
        .map_err(|_| OperationContractError::Malformed("committed meeting cannot be serialized"))?;
    let current_transcript =
        meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(OperationContractError::Malformed(
                "committed meeting lacks a transcript",
            ))?;
    if parsed != *meeting
        || meeting_bytes != canonical
        || digest_bytes(meeting_bytes) != commit.committed_meeting_sha256
        || meeting.meeting_id != commit.meeting_id.to_string()
        || meeting.lifecycle != commit.lifecycle
        || current_transcript.sha256 != commit.current_transcript_sha256
        || meeting.artifacts.current_note.as_ref() != commit.current_note.as_ref()
        || meeting.pending_storage_operation.is_some()
    {
        return Err(OperationContractError::Malformed(
            "commit receipt disagrees with exact meeting bytes",
        ));
    }
    Ok(())
}

pub fn classify_restoration_recovery(
    request: &TranscriptRestorationRequest,
    result: Option<&TranscriptRestorationResult>,
    lifecycle: MeetingLifecycle,
    current_transcript_sha256: &str,
    current_note: Option<&NoteRevisionRef>,
) -> Result<IncompleteOperationRecovery, OperationContractError> {
    request.validate()?;
    validate_digest(current_transcript_sha256)?;
    let source_state_matches = current_transcript_sha256 == request.source_transcript_sha256
        && match (&request.prior_note, current_note, lifecycle) {
            (Some(prior), Some(current), MeetingLifecycle::Ready) => prior == current,
            (None, None, MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed) => {
                true
            }
            _ => false,
        };
    let Some(result) = result else {
        return if source_state_matches {
            Ok(IncompleteOperationRecovery::RetryRequest)
        } else {
            Err(OperationContractError::Malformed(
                "restoration request no longer matches meeting state",
            ))
        };
    };
    result.validate()?;
    if request.operation_id != result.operation_id
        || request.meeting_id != result.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
    {
        return Err(OperationContractError::Malformed(
            "restoration request and result disagree",
        ));
    }
    if source_state_matches {
        return Ok(IncompleteOperationRecovery::ApplyValidatedResult);
    }
    if current_transcript_sha256 == result.view.sha256
        && lifecycle == MeetingLifecycle::TranscriptReady
        && current_note.is_none()
    {
        return Ok(IncompleteOperationRecovery::WriteMissingCommit);
    }
    Err(OperationContractError::Malformed(
        "restoration recovery state is ambiguous",
    ))
}

pub fn classify_note_recovery(
    request: &NoteGenerationRequest,
    result: Option<&NoteGenerationResult>,
    lifecycle: MeetingLifecycle,
    current_transcript_sha256: &str,
    current_note: Option<&NoteRevisionRef>,
) -> Result<IncompleteOperationRecovery, OperationContractError> {
    request.validate()?;
    validate_digest(current_transcript_sha256)?;
    let source_state_matches = current_transcript_sha256 == request.source_transcript_sha256
        && match (&request.prior_note, current_note, lifecycle) {
            (Some(prior), Some(current), MeetingLifecycle::Ready) => prior == current,
            (None, None, MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed) => {
                true
            }
            _ => false,
        };
    let Some(result) = result else {
        return if source_state_matches {
            Ok(IncompleteOperationRecovery::RetryRequest)
        } else {
            Err(OperationContractError::Malformed(
                "note request no longer matches meeting state",
            ))
        };
    };
    result.validate()?;
    if request.operation_id != result.operation_id
        || request.meeting_id != result.meeting_id
        || request.source_transcript_sha256 != result.source_transcript_sha256
    {
        return Err(OperationContractError::Malformed(
            "note request and result disagree",
        ));
    }
    let result_is_applied = match result.status {
        NoteGenerationStatus::Accepted => {
            lifecycle == MeetingLifecycle::Ready && current_note == result.note.as_ref()
        }
        NoteGenerationStatus::Rejected => match &request.prior_note {
            Some(prior) => lifecycle == MeetingLifecycle::Ready && current_note == Some(prior),
            None => lifecycle == MeetingLifecycle::SummaryFailed && current_note.is_none(),
        },
    };
    if current_transcript_sha256 == request.source_transcript_sha256 && result_is_applied {
        return Ok(IncompleteOperationRecovery::WriteMissingCommit);
    }
    if source_state_matches {
        return Ok(IncompleteOperationRecovery::ApplyValidatedResult);
    }
    Err(OperationContractError::Malformed(
        "note recovery state is ambiguous",
    ))
}

fn validate_exact_digests(
    values: &HashMap<String, String>,
    expected: &[&str],
) -> Result<(), OperationContractError> {
    if values.len() != expected.len() || expected.iter().any(|key| !values.contains_key(*key)) {
        return Err(OperationContractError::Malformed(
            "worker digest result has the wrong keys",
        ));
    }
    for value in values.values() {
        validate_digest(value)?;
    }
    Ok(())
}

fn validate_note(note: &NoteRevisionRef) -> Result<(), OperationContractError> {
    note.validate()
        .map_err(|_| OperationContractError::Malformed("note revision is invalid"))
}

fn validate_digest(value: &str) -> Result<(), OperationContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(OperationContractError::Malformed(
            "digest must be lowercase SHA-256",
        ));
    }
    Ok(())
}

fn validate_speaker_label_overrides(
    overrides: &[SpeakerLabelOverride],
) -> Result<(), OperationContractError> {
    if overrides.len() > 3 {
        return Err(OperationContractError::Malformed(
            "speaker label overlay has too many source groups",
        ));
    }
    let mut previous = None;
    for override_ in overrides {
        let rank = match override_.source_speaker.as_deref() {
            None => 0,
            Some("Me") => 1,
            Some("Them") => 2,
            _ => {
                return Err(OperationContractError::Malformed(
                    "speaker label overlay names an unknown source group",
                ));
            }
        };
        if previous.is_some_and(|prior| prior >= rank) {
            return Err(OperationContractError::Malformed(
                "speaker label overlay source groups are not unique and ordered",
            ));
        }
        previous = Some(rank);
        if override_.replacement.is_empty()
            || override_.replacement.len() > 80
            || override_.replacement.trim() != override_.replacement
            || override_.replacement.chars().any(char::is_control)
        {
            return Err(OperationContractError::Malformed(
                "speaker label overlay replacement is invalid",
            ));
        }
    }
    Ok(())
}

fn validate_vocabulary_replacements(
    replacements: &[VocabularyRangeReplacement],
) -> Result<(), OperationContractError> {
    if vocabulary_range_replacements_are_valid(replacements) {
        Ok(())
    } else {
        Err(OperationContractError::Malformed(
            "vocabulary replacement overlay is invalid",
        ))
    }
}

/// Mirrors the sidecar's own 16 KiB ceiling
/// (`apps/desktop/src-tauri/src/meeting_context.rs::MAX_TEXT_BYTES`). The two
/// bounds are not shared code -- this crate does not depend on the desktop
/// crate -- so this is defense in depth against a caller that skipped the
/// sidecar's own limit, not the primary enforcement point.
const MAX_PRE_MEETING_CONTEXT_BYTES: usize = 16 * 1024;

fn validate_pre_meeting_context(value: &Option<String>) -> Result<(), OperationContractError> {
    let Some(text) = value else {
        return Ok(());
    };
    if text.is_empty()
        || text.len() > MAX_PRE_MEETING_CONTEXT_BYTES
        || text.chars().any(|character| {
            character.is_control() && character != '\n' && character != '\t'
        })
    {
        return Err(OperationContractError::Malformed(
            "pre-meeting context is invalid",
        ));
    }
    Ok(())
}

fn digest_pretty<T: Serialize>(value: &T) -> Result<String, OperationContractError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| OperationContractError::Malformed("contract cannot be serialized"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn strictly_increasing(values: &[u32]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    use super::*;

    fn fixtures() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/product-operations-v1.json"
        ))
        .expect("shared operation fixture must be JSON")
    }

    fn at(root: &Value, path: &[&str]) -> Value {
        let mut value = root;
        for segment in path {
            value = &value[*segment];
        }
        value.clone()
    }

    fn parse<T: DeserializeOwned>(root: &Value, path: &[&str]) -> T {
        serde_json::from_value(at(root, path)).expect("fixture must match closed schema")
    }

    fn assert_unknown_field_refused<T: DeserializeOwned>(root: &Value, path: &[&str]) {
        let mut value = at(root, path);
        value
            .as_object_mut()
            .expect("contract fixture must be an object")
            .insert("unexpected".to_owned(), Value::Bool(true));
        assert!(serde_json::from_value::<T>(value).is_err());
    }

    fn committed_meeting(root: &Value, case: &str) -> (MeetingRecord, Vec<u8>) {
        let meeting: MeetingRecord = parse(root, &["committed_meetings", case]);
        let bytes = serde_json::to_vec_pretty(&meeting).unwrap();
        (meeting, bytes)
    }

    #[test]
    fn shared_fixture_freezes_ui_worker_and_receipt_contracts() {
        let fixture = fixtures();
        assert_eq!(fixture["schema"], "product-operation-fixtures/1");

        let restore_ui: RestoreWithheldTurnUiArgs =
            parse(&fixture, &["restoration", "ui_arguments"]);
        let restore_response: UiOperationAccepted =
            parse(&fixture, &["restoration", "ui_response"]);
        let restore_worker: TranscriptRestoreWorkerArgs =
            parse(&fixture, &["restoration", "worker_arguments"]);
        let restore_worker_digests: HashMap<String, String> =
            parse(&fixture, &["restoration", "worker_artifact_digests"]);
        let restore_request: TranscriptRestorationRequest =
            parse(&fixture, &["restoration", "request"]);
        let restore_view: TranscriptView = parse(&fixture, &["restoration", "view"]);
        let restore_result: TranscriptRestorationResult =
            parse(&fixture, &["restoration", "result"]);
        let restore_commit: MeetingOperationCommit = parse(&fixture, &["restoration", "commit"]);

        restore_ui.validate().unwrap();
        restore_response.validate().unwrap();
        restore_worker.validate().unwrap();
        validate_restore_worker_join(
            &restore_worker_digests,
            &restore_request,
            None,
            &restore_view,
            &restore_result,
        )
        .unwrap();
        let (restore_meeting, restore_meeting_bytes) = committed_meeting(&fixture, "restoration");
        validate_restoration_receipts(
            &restore_request,
            &restore_result,
            &restore_commit,
            &restore_meeting,
            &restore_meeting_bytes,
        )
        .unwrap();
        assert_eq!(restore_ui.meeting_id, restore_worker.meeting_id);
        assert_eq!(
            restore_ui.source_transcript_sha256,
            restore_worker.source_transcript_sha256
        );
        assert_eq!(
            restore_ui.source_turn_index,
            restore_worker.source_turn_index
        );
        assert_eq!(restore_response.operation_id, restore_request.operation_id);

        for case in ["accepted_note", "rejected_note"] {
            let note_ui: RegenerateNoteUiArgs = parse(&fixture, &[case, "ui_arguments"]);
            let note_response: UiOperationAccepted = parse(&fixture, &[case, "ui_response"]);
            let note_worker: NoteCreateWorkerArgs = parse(&fixture, &[case, "worker_arguments"]);
            let note_request: NoteGenerationRequest = parse(&fixture, &[case, "request"]);
            let note_result: NoteGenerationResult = parse(&fixture, &[case, "result"]);
            let note_commit: MeetingOperationCommit = parse(&fixture, &[case, "commit"]);
            let (meeting, meeting_bytes) = committed_meeting(&fixture, case);
            note_ui.validate().unwrap();
            note_response.validate().unwrap();
            note_worker.validate().unwrap();
            validate_note_receipts(
                &note_request,
                &note_result,
                &note_commit,
                &meeting,
                &meeting_bytes,
            )
            .unwrap();
            assert_eq!(note_ui.meeting_id, note_worker.meeting_id);
            assert_eq!(
                note_ui.source_transcript_sha256,
                note_worker.source_transcript_sha256
            );
            assert_eq!(note_response.operation_id, note_request.operation_id);
        }
        let note_worker_digests: HashMap<String, String> =
            parse(&fixture, &["accepted_note", "worker_artifact_digests"]);
        let note_request: NoteGenerationRequest = parse(&fixture, &["accepted_note", "request"]);
        let note_result: NoteGenerationResult = parse(&fixture, &["accepted_note", "result"]);
        validate_note_worker_join(&note_worker_digests, &note_request, &note_result).unwrap();

        let worker_failure: NoteCreateWorkerFailure =
            parse(&fixture, &["rejected_note", "worker_error"]);
        let rejected_request: NoteGenerationRequest =
            parse(&fixture, &["rejected_note", "request"]);
        let rejected_result: NoteGenerationResult = parse(&fixture, &["rejected_note", "result"]);
        validate_note_worker_failure_join(&worker_failure, &rejected_request, &rejected_result)
            .unwrap();

        let mut changed_rejection = rejected_result;
        changed_rejection.failure_code = Some(NoteFailureCode::ModelUnavailable);
        assert!(
            validate_note_worker_failure_join(
                &worker_failure,
                &rejected_request,
                &changed_rejection
            )
            .is_err()
        );

        let recovery_request: TranscriptRestorationRequest =
            parse(&fixture, &["already_applied_restoration", "request"]);
        let recovery_result: TranscriptRestorationResult =
            parse(&fixture, &["already_applied_restoration", "result"]);
        assert_eq!(recovery_request.operation_id, recovery_result.operation_id);
        assert_eq!(
            fixture["already_applied_restoration"]["current_transcript_sha256"],
            recovery_result.view.sha256
        );
        let recovery_action = classify_restoration_recovery(
            &recovery_request,
            Some(&recovery_result),
            parse(
                &fixture,
                &["already_applied_restoration", "meeting_lifecycle"],
            ),
            fixture["already_applied_restoration"]["current_transcript_sha256"]
                .as_str()
                .unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            recovery_action,
            parse(
                &fixture,
                &["already_applied_restoration", "expected_recovery_action"]
            )
        );
    }

    #[test]
    fn every_versioned_object_and_ui_argument_refuses_unknown_fields() {
        let fixture = fixtures();
        assert_unknown_field_refused::<RestoreWithheldTurnUiArgs>(
            &fixture,
            &["restoration", "ui_arguments"],
        );
        assert_unknown_field_refused::<UiOperationAccepted>(
            &fixture,
            &["restoration", "ui_response"],
        );
        assert_unknown_field_refused::<TranscriptRestoreWorkerArgs>(
            &fixture,
            &["restoration", "worker_arguments"],
        );
        assert_unknown_field_refused::<TranscriptRestorationRequest>(
            &fixture,
            &["restoration", "request"],
        );
        assert_unknown_field_refused::<TranscriptView>(&fixture, &["restoration", "view"]);
        assert_unknown_field_refused::<TranscriptRestorationResult>(
            &fixture,
            &["restoration", "result"],
        );
        assert_unknown_field_refused::<MeetingOperationCommit>(
            &fixture,
            &["restoration", "commit"],
        );
        assert_unknown_field_refused::<RegenerateNoteUiArgs>(
            &fixture,
            &["accepted_note", "ui_arguments"],
        );
        assert_unknown_field_refused::<NoteCreateWorkerArgs>(
            &fixture,
            &["accepted_note", "worker_arguments"],
        );
        assert_unknown_field_refused::<NoteGenerationRequest>(
            &fixture,
            &["accepted_note", "request"],
        );
        assert_unknown_field_refused::<NoteGenerationResult>(
            &fixture,
            &["accepted_note", "result"],
        );
        assert_unknown_field_refused::<NoteCreateWorkerFailure>(
            &fixture,
            &["rejected_note", "worker_error"],
        );
    }

    #[test]
    fn semantic_join_checks_refuse_drift_and_no_op_restoration() {
        let fixture = fixtures();
        let request: TranscriptRestorationRequest = parse(&fixture, &["restoration", "request"]);
        let view: TranscriptView = parse(&fixture, &["restoration", "view"]);
        let result: TranscriptRestorationResult = parse(&fixture, &["restoration", "result"]);
        let commit: MeetingOperationCommit = parse(&fixture, &["restoration", "commit"]);

        let mut unsorted = view.clone();
        unsorted.restored_source_turn_indices = vec![7, 3];
        assert!(unsorted.validate().is_err());

        let mut changed_result = result.clone();
        changed_result.meeting_id = Uuid::new_v4();
        assert!(validate_restoration_join(&request, None, &view, &changed_result).is_err());

        let mut parent = view.clone();
        parent.parent_transcript_sha256 = parent.base_transcript_sha256.clone();
        let parent_digest = digest_pretty(&parent).unwrap();
        let mut duplicate_request = request.clone();
        duplicate_request.source_transcript_sha256 = parent_digest.clone();
        let mut duplicate_view = parent.clone();
        duplicate_view.parent_transcript_sha256 = parent_digest;
        let mut duplicate_result = result.clone();
        duplicate_result.source_transcript_sha256 =
            duplicate_request.source_transcript_sha256.clone();
        duplicate_result.parent_transcript_sha256 =
            duplicate_request.source_transcript_sha256.clone();
        duplicate_result.view.sha256 = digest_pretty(&duplicate_view).unwrap();
        duplicate_result.view.relative_path =
            format!("transcript/{}.json", duplicate_result.view.sha256);
        assert!(
            validate_restoration_join(
                &duplicate_request,
                Some(&parent),
                &duplicate_view,
                &duplicate_result
            )
            .is_err()
        );

        let mut changed_commit = commit;
        changed_commit.result_sha256 = "b".repeat(64);
        let (meeting, meeting_bytes) = committed_meeting(&fixture, "restoration");
        assert!(
            validate_restoration_receipts(
                &request,
                &result,
                &changed_commit,
                &meeting,
                &meeting_bytes
            )
            .is_err()
        );

        let commit: MeetingOperationCommit = parse(&fixture, &["restoration", "commit"]);
        let mut changed_meeting_bytes = meeting_bytes;
        changed_meeting_bytes.push(b'\n');
        assert!(validate_commit_meeting(&commit, &meeting, &changed_meeting_bytes).is_err());
    }

    #[test]
    fn worker_digest_maps_are_closed_and_source_bound() {
        let fixture = fixtures();
        let restore: HashMap<String, String> =
            parse(&fixture, &["restoration", "worker_artifact_digests"]);
        let restore_request: TranscriptRestorationRequest =
            parse(&fixture, &["restoration", "request"]);
        let restore_view: TranscriptView = parse(&fixture, &["restoration", "view"]);
        let restore_result: TranscriptRestorationResult =
            parse(&fixture, &["restoration", "result"]);
        for key in ["base-transcript", "parent-transcript", "transcript"] {
            let mut changed = restore.clone();
            changed.insert(key.to_owned(), "b".repeat(64));
            assert!(
                validate_restore_worker_join(
                    &changed,
                    &restore_request,
                    None,
                    &restore_view,
                    &restore_result
                )
                .is_err()
            );
        }
        let mut extra_restore = restore;
        extra_restore.insert("extra".to_owned(), "a".repeat(64));
        assert!(
            validate_restore_worker_join(
                &extra_restore,
                &restore_request,
                None,
                &restore_view,
                &restore_result
            )
            .is_err()
        );

        let note: HashMap<String, String> =
            parse(&fixture, &["accepted_note", "worker_artifact_digests"]);
        let note_request: NoteGenerationRequest = parse(&fixture, &["accepted_note", "request"]);
        let note_result: NoteGenerationResult = parse(&fixture, &["accepted_note", "result"]);
        for key in ["note", "note-markdown", "transcript"] {
            let mut changed = note.clone();
            changed.insert(key.to_owned(), "b".repeat(64));
            assert!(validate_note_worker_join(&changed, &note_request, &note_result).is_err());
        }
    }

    #[test]
    fn recovery_classification_accepts_only_exact_crash_states() {
        let fixture = fixtures();
        let restore_request: TranscriptRestorationRequest =
            parse(&fixture, &["restoration", "request"]);
        let restore_result: TranscriptRestorationResult =
            parse(&fixture, &["restoration", "result"]);
        assert_eq!(
            classify_restoration_recovery(
                &restore_request,
                None,
                MeetingLifecycle::Ready,
                &restore_request.source_transcript_sha256,
                restore_request.prior_note.as_ref(),
            )
            .unwrap(),
            IncompleteOperationRecovery::RetryRequest
        );
        assert_eq!(
            classify_restoration_recovery(
                &restore_request,
                Some(&restore_result),
                MeetingLifecycle::Ready,
                &restore_request.source_transcript_sha256,
                restore_request.prior_note.as_ref(),
            )
            .unwrap(),
            IncompleteOperationRecovery::ApplyValidatedResult
        );
        assert!(
            classify_restoration_recovery(
                &restore_request,
                Some(&restore_result),
                MeetingLifecycle::Ready,
                &restore_result.view.sha256,
                restore_request.prior_note.as_ref(),
            )
            .is_err()
        );

        for case in ["accepted_note", "rejected_note"] {
            let request: NoteGenerationRequest = parse(&fixture, &[case, "request"]);
            let result: NoteGenerationResult = parse(&fixture, &[case, "result"]);
            let commit: MeetingOperationCommit = parse(&fixture, &[case, "commit"]);
            assert_eq!(
                classify_note_recovery(
                    &request,
                    None,
                    MeetingLifecycle::TranscriptReady,
                    &request.source_transcript_sha256,
                    None,
                )
                .unwrap(),
                IncompleteOperationRecovery::RetryRequest
            );
            assert_eq!(
                classify_note_recovery(
                    &request,
                    Some(&result),
                    MeetingLifecycle::TranscriptReady,
                    &request.source_transcript_sha256,
                    None,
                )
                .unwrap(),
                IncompleteOperationRecovery::ApplyValidatedResult
            );
            assert_eq!(
                classify_note_recovery(
                    &request,
                    Some(&result),
                    commit.lifecycle,
                    &commit.current_transcript_sha256,
                    commit.current_note.as_ref(),
                )
                .unwrap(),
                IncompleteOperationRecovery::WriteMissingCommit
            );
        }
    }

    #[test]
    fn note_overlays_are_bounded_and_absent_from_legacy_wire_shapes() {
        let mut arguments = NoteCreateWorkerArgs {
            meeting_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: None,
        };
        arguments.validate().unwrap();
        assert!(
            !serde_json::to_string(&arguments)
                .unwrap()
                .contains("speaker_label_overrides")
        );
        assert!(
            !serde_json::to_string(&arguments)
                .unwrap()
                .contains("pre_meeting_context")
        );
        assert!(
            !serde_json::to_string(&arguments)
                .unwrap()
                .contains("vocabulary_replacements")
        );

        arguments.speaker_label_overrides = vec![SpeakerLabelOverride {
            source_speaker: Some("Them".into()),
            replacement: "Alex".into(),
        }];
        arguments.validate().unwrap();
        assert!(
            serde_json::to_string(&arguments)
                .unwrap()
                .contains("speaker_label_overrides")
        );

        arguments.speaker_label_overrides[0].source_speaker = Some("Other".into());
        assert!(arguments.validate().is_err());
        arguments.speaker_label_overrides.clear();
        arguments.vocabulary_replacements = vec![VocabularyRangeReplacement {
            turn: 0,
            char_start: 1,
            char_end: 4,
            source_sha256: "b".repeat(64),
            replacement: "Kibble".into(),
        }];
        arguments.validate().unwrap();
        assert!(
            serde_json::to_string(&arguments)
                .unwrap()
                .contains("vocabulary_replacements")
        );
        arguments.vocabulary_replacements[0].char_end = 1;
        assert!(arguments.validate().is_err());
    }

    /// Decision 6's gate: a request with no pre-meeting context must serialize
    /// to the exact bytes it did before this field existed, for every wire
    /// shape it crosses -- the UI args, the durable request, and the worker
    /// args. Absent, not null, and not an empty string: the field must not
    /// appear on the wire at all when the operator left it blank.
    #[test]
    fn absent_context_is_byte_identical_to_the_pre_context_wire_shape() {
        let no_context = NoteCreateWorkerArgs {
            meeting_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: None,
        };
        no_context.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&no_context).unwrap(),
            serde_json::json!({
                "meeting_id": no_context.meeting_id,
                "source_transcript_sha256": no_context.source_transcript_sha256,
            }),
            "no-context worker args must carry no pre_meeting_context key"
        );

        let ui_args = RegenerateNoteUiArgs {
            meeting_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: None,
        };
        ui_args.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&ui_args).unwrap(),
            serde_json::json!({
                "meetingId": ui_args.meeting_id,
                "sourceTranscriptSha256": ui_args.source_transcript_sha256,
            }),
            "no-context UI args must carry no preMeetingContext key"
        );

        let request = NoteGenerationRequest {
            schema: NoteGenerationRequestSchema::V1,
            operation_id: Uuid::new_v4(),
            meeting_id: Uuid::new_v4(),
            requested_at_epoch_seconds: 1,
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            prior_note: None,
            pre_meeting_context: None,
        };
        request.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({
                "schema": "note-generation-request/1",
                "operation_id": request.operation_id,
                "meeting_id": request.meeting_id,
                "requested_at_epoch_seconds": 1,
                "source_transcript_sha256": request.source_transcript_sha256,
            }),
            "no-context durable request must carry no pre_meeting_context key"
        );
    }

    /// A present context must round-trip and be readable back off the wire,
    /// on both the UI args and the worker args this packet adds it to.
    #[test]
    fn present_context_serializes_and_round_trips_on_every_wire_shape_it_crosses() {
        let mut worker_args = NoteCreateWorkerArgs {
            meeting_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: Some("Deciding the Q3 roadmap.".into()),
        };
        worker_args.validate().unwrap();
        let wire = serde_json::to_value(&worker_args).unwrap();
        assert_eq!(wire["pre_meeting_context"], "Deciding the Q3 roadmap.");
        let round_tripped: NoteCreateWorkerArgs = serde_json::from_value(wire).unwrap();
        assert_eq!(round_tripped, worker_args);

        // Empty string is not a legal present value -- an operator who typed
        // nothing is represented as absent, not as `Some("")`.
        worker_args.pre_meeting_context = Some(String::new());
        assert!(worker_args.validate().is_err());

        // The ceiling matches the desktop sidecar's own 16 KiB cap.
        worker_args.pre_meeting_context = Some("x".repeat(16 * 1024));
        worker_args.validate().unwrap();
        worker_args.pre_meeting_context = Some("x".repeat(16 * 1024 + 1));
        assert!(worker_args.validate().is_err());

        // Multi-line context is legitimate; other control characters are not.
        worker_args.pre_meeting_context = Some("Line one.\nLine two.".into());
        worker_args.validate().unwrap();
        worker_args.pre_meeting_context = Some("Has a null: \u{0}".into());
        assert!(worker_args.validate().is_err());
    }

    #[test]
    fn transcript_retry_ui_arguments_are_exact_and_reject_unknown_fields() {
        let arguments = TranscriptRetryUiArgs {
            meeting_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
        };
        arguments.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&arguments).unwrap(),
            serde_json::json!({
                "meetingId": arguments.meeting_id,
                "sourceTranscriptSha256": arguments.source_transcript_sha256,
            })
        );
        let mut malformed = serde_json::to_value(arguments).unwrap();
        malformed
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), Value::Bool(true));
        assert!(serde_json::from_value::<TranscriptRetryUiArgs>(malformed).is_err());
    }
}
