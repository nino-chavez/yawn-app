use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 1;
pub const MAX_QUEUED_OUTPUTS: usize = 32;
pub const MAX_PROGRESS_EVENTS_PER_SECOND: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    #[serde(rename = "profile.inspect")]
    ProfileInspect,
    #[serde(rename = "profile.choices")]
    ProfileChoices,
    #[serde(rename = "profile.build")]
    ProfileBuild,
    #[serde(rename = "profile.adopt")]
    ProfileAdopt,
    #[serde(rename = "profile.discard")]
    ProfileDiscard,
    #[serde(rename = "capture.start")]
    CaptureStart,
    #[serde(rename = "capture.finalize")]
    CaptureFinalize,
    #[serde(rename = "capture.inspect")]
    CaptureInspect,
    #[serde(rename = "sitting.derive")]
    SittingDerive,
    #[serde(rename = "transcript.create")]
    TranscriptCreate,
    #[serde(rename = "transcript.retry")]
    TranscriptRetry,
    #[serde(rename = "transcript.restore")]
    TranscriptRestore,
    #[serde(rename = "note.create")]
    NoteCreate,
    #[serde(rename = "note.inspect")]
    NoteInspect,
    #[serde(rename = "speaker.suggest")]
    SpeakerSuggest,
    /// Window text to vector. Registered in the boundary lane only, because no
    /// lane packages the embedding model yet — this variant exists so a
    /// boundary worker's ready event still parses, not because the app calls it.
    #[serde(rename = "corpus.embed")]
    CorpusEmbed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerReady {
    pub schema: WorkerEventSchema,
    pub event: ReadyEvent,
    pub protocol: u16,
    pub admission: WorkerAdmission,
    pub build: String,
    pub runtime: RuntimeStatus,
    pub tap: TapStatus,
    pub models: Vec<ResourceStatus>,
    pub operations: HashSet<Operation>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum WorkerEventSchema {
    #[serde(rename = "worker-event/2")]
    V2,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkerAdmission {
    BoundaryTest,
    InternalAlpha,
    Product,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ReadyEvent {
    #[serde(rename = "worker.ready")]
    WorkerReady,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeStatus {
    pub kind: RuntimeKind,
    pub digest: String,
}

#[derive(Debug, Deserialize)]
pub enum RuntimeKind {
    #[serde(rename = "bundled")]
    Bundled,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TapStatus {
    pub build: String,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceStatus {
    pub id: String,
    pub digest: String,
    pub available: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkerCommand {
    pub schema: &'static str,
    pub request_id: Uuid,
    pub operation: Operation,
    pub arguments: Value,
}

impl WorkerCommand {
    pub fn new(operation: Operation, arguments: Value) -> Self {
        Self {
            schema: "worker-command/2",
            request_id: Uuid::new_v4(),
            operation,
            arguments,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerResult {
    pub schema: ResultSchema,
    pub request_id: Uuid,
    pub ok: bool,
    pub code: Option<ResultCode>,
    pub recoverable: Option<bool>,
    #[serde(default)]
    pub artifact_digests: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ResultSchema {
    #[serde(rename = "worker-result/2")]
    V2,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultCode {
    TapReadyTimeout,
    RuntimeMissing,
    InvalidProfile,
    InvalidCapture,
    TranscriptionFailed,
    NoteRejected,
    ProtocolFailure,
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("protocol frame exceeds its byte limit")]
    FrameTooLarge,
    #[error("protocol frame is malformed or outside the closed schema")]
    Malformed,
    #[error("worker protocol version is unsupported")]
    UnsupportedProtocol,
    #[error("worker operation set differs from the application registry")]
    OperationMismatch,
    #[error("request limit reached")]
    RequestLimit,
    #[error("request identifier is already pending")]
    DuplicateRequest,
    #[error("result refers to an unknown request")]
    UnknownRequest,
    #[error("request already produced a terminal result")]
    DuplicateTerminal,
    #[error("progress event is invalid for the pending request")]
    InvalidEvent,
    #[error("progress event was emitted more than once")]
    DuplicateProgress,
    #[error("terminal result has an incoherent shape or event sequence")]
    InvalidResult,
    #[error("worker output queue exceeded its bound")]
    OutputQueueOverflow,
    #[error("worker progress event rate exceeded its bound")]
    EventRateExceeded,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ProgressEvent {
    #[serde(rename = "capture.state")]
    CaptureState,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureProgressState {
    Recording,
    Transcribing,
    // `note.create` heartbeats reuse the capture.state event and this state
    // enum, exactly as `transcript.create` does with `Transcribing` — the
    // worker owns one shared heartbeat wire shape, not one per operation.
    // The wire value mirrors the product brief's own status word for this
    // step (docs/product-brief.md: "recording, preparing, finishing,
    // transcript ready, or needs attention").
    Finishing,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerProgress {
    pub schema: WorkerEventSchema,
    pub request_id: Uuid,
    pub event: ProgressEvent,
    pub state: CaptureProgressState,
    pub meeting_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WorkerOutput {
    Progress(WorkerProgress),
    Result(WorkerResult),
}

pub fn parse_ready(
    frame: &[u8],
    expected: &HashSet<Operation>,
) -> Result<WorkerReady, ProtocolError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    let ready: WorkerReady = serde_json::from_slice(frame).map_err(|_| ProtocolError::Malformed)?;
    if ready.protocol != 2 {
        return Err(ProtocolError::UnsupportedProtocol);
    }
    if &ready.operations != expected {
        return Err(ProtocolError::OperationMismatch);
    }
    Ok(ready)
}

pub fn parse_output(frame: &[u8]) -> Result<WorkerOutput, ProtocolError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    serde_json::from_slice(frame).map_err(|_| ProtocolError::Malformed)
}

struct PendingRequest {
    request_id: Uuid,
    operation: Operation,
    meeting_id: Option<Uuid>,
    saw_recording: bool,
}

#[derive(Default)]
pub struct RequestTracker {
    pending: Option<PendingRequest>,
    last_terminal: Option<Uuid>,
}

impl RequestTracker {
    pub fn register(&mut self, command: &WorkerCommand) -> Result<(), ProtocolError> {
        if self.pending.is_some() {
            return Err(ProtocolError::RequestLimit);
        }
        if self.last_terminal == Some(command.request_id) {
            return Err(ProtocolError::DuplicateRequest);
        }
        let meeting_id = match command.operation {
            Operation::CaptureStart
            | Operation::TranscriptCreate
            | Operation::TranscriptRetry
            | Operation::NoteCreate
            | Operation::SpeakerSuggest => {
                let value = command
                    .arguments
                    .get("meeting_id")
                    .and_then(Value::as_str)
                    .ok_or(ProtocolError::Malformed)?;
                Some(Uuid::parse_str(value).map_err(|_| ProtocolError::Malformed)?)
            }
            _ => None,
        };
        self.pending = Some(PendingRequest {
            request_id: command.request_id,
            operation: command.operation,
            meeting_id,
            saw_recording: false,
        });
        Ok(())
    }

    pub fn progress(&mut self, progress: &WorkerProgress) -> Result<(), ProtocolError> {
        if self.last_terminal == Some(progress.request_id) {
            return Err(ProtocolError::DuplicateTerminal);
        }
        let pending = self.pending.as_mut().ok_or(ProtocolError::UnknownRequest)?;
        if pending.request_id != progress.request_id {
            return Err(ProtocolError::UnknownRequest);
        }
        if pending.meeting_id != Some(progress.meeting_id)
            || progress.event != ProgressEvent::CaptureState
        {
            return Err(ProtocolError::InvalidEvent);
        }
        match pending.operation {
            Operation::CaptureStart if progress.state == CaptureProgressState::Recording => {
                if pending.saw_recording {
                    return Err(ProtocolError::DuplicateProgress);
                }
                pending.saw_recording = true;
                Ok(())
            }
            Operation::TranscriptCreate | Operation::TranscriptRetry
                if progress.state == CaptureProgressState::Transcribing =>
            {
                // A transcription heartbeat confirms the worker still owns the
                // request. It is intentionally repeatable and carries no
                // fabricated completion estimate.
                Ok(())
            }
            Operation::NoteCreate if progress.state == CaptureProgressState::Finishing => {
                // A note-generation heartbeat confirms the worker still owns
                // the request, exactly like the transcription heartbeat above.
                // It is intentionally repeatable and carries no fabricated
                // completion estimate.
                Ok(())
            }
            _ => Err(ProtocolError::InvalidEvent),
        }
    }

    pub fn terminal(&mut self, result: &WorkerResult) -> Result<(), ProtocolError> {
        if self.last_terminal == Some(result.request_id) {
            return Err(ProtocolError::DuplicateTerminal);
        }
        let pending = self.pending.as_ref().ok_or(ProtocolError::UnknownRequest)?;
        if pending.request_id != result.request_id {
            return Err(ProtocolError::UnknownRequest);
        }
        let shape_is_valid = if result.ok {
            result.code.is_none() && result.recoverable.is_none()
        } else {
            result.code.is_some()
                && result.recoverable.is_some()
                && result.artifact_digests.is_empty()
        };
        if !shape_is_valid
            || (result.ok && pending.operation == Operation::CaptureStart && !pending.saw_recording)
        {
            return Err(ProtocolError::InvalidResult);
        }
        self.pending = None;
        self.last_terminal = Some(result.request_id);
        Ok(())
    }

    pub fn cancel(&mut self, request_id: Uuid) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            self.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ready_field_fails_closed() {
        let frame = br#"{"schema":"worker-event/2","event":"worker.ready","protocol":2,"admission":"boundary-test","build":"x","runtime":{"kind":"bundled","digest":"x"},"tap":{"build":"x","available":true},"models":[],"operations":[],"surprise":true}"#;
        assert_eq!(
            parse_ready(frame, &HashSet::new()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    #[test]
    fn duplicate_terminal_result_is_refused() {
        let request_id = Uuid::new_v4();
        let result = WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        };
        let mut tracker = RequestTracker::default();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::CaptureInspect,
            arguments: serde_json::json!({"meeting_id": "fixture"}),
        };
        tracker.register(&command).unwrap();
        tracker.terminal(&result).unwrap();
        assert_eq!(
            tracker.terminal(&result),
            Err(ProtocolError::DuplicateTerminal)
        );
    }

    #[test]
    fn result_with_unknown_field_fails_closed() {
        let request_id = Uuid::new_v4();
        let frame = format!(
            "{{\"schema\":\"worker-result/2\",\"request_id\":\"{request_id}\",\"ok\":true,\"code\":null,\"recoverable\":null,\"artifact_digests\":{{}},\"extra\":true}}"
        );
        assert_eq!(
            parse_output(frame.as_bytes()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    #[test]
    fn capture_progress_must_match_request_and_precede_success() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::CaptureStart,
            arguments: serde_json::json!({
                "meeting_id": meeting_id,
                "profile_id": "fixture"
            }),
        };
        let result = WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        };
        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        assert_eq!(tracker.terminal(&result), Err(ProtocolError::InvalidResult));

        let progress = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Recording,
            meeting_id,
        };
        tracker.progress(&progress).unwrap();
        assert_eq!(
            tracker.progress(&progress),
            Err(ProtocolError::DuplicateProgress)
        );
        tracker.terminal(&result).unwrap();
    }

    #[test]
    fn transcription_heartbeats_are_repeatable_for_the_active_request() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::TranscriptCreate,
            arguments: serde_json::json!({"meeting_id": meeting_id}),
        };
        let result = WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        };
        let heartbeat = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Transcribing,
            meeting_id,
        };

        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.terminal(&result).unwrap();
    }

    #[test]
    fn retry_transcription_heartbeats_are_repeatable_for_the_active_request() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::TranscriptRetry,
            arguments: serde_json::json!({"meeting_id": meeting_id}),
        };
        let result = WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        };
        let heartbeat = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Transcribing,
            meeting_id,
        };

        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.terminal(&result).unwrap();
    }

    #[test]
    fn note_generation_heartbeats_are_repeatable_for_the_active_request() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::NoteCreate,
            arguments: serde_json::json!({
                "meeting_id": meeting_id,
                "source_transcript_sha256": "a".repeat(64)
            }),
        };
        let result = WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        };
        let heartbeat = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Finishing,
            meeting_id,
        };

        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.progress(&heartbeat).unwrap();
        tracker.terminal(&result).unwrap();
    }

    #[test]
    fn note_generation_state_is_invalid_for_a_transcript_create_request() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::TranscriptCreate,
            arguments: serde_json::json!({"meeting_id": meeting_id}),
        };
        let mismatched = WorkerProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: ProgressEvent::CaptureState,
            state: CaptureProgressState::Finishing,
            meeting_id,
        };

        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        assert_eq!(
            tracker.progress(&mismatched),
            Err(ProtocolError::InvalidEvent)
        );
    }

    #[test]
    fn progress_with_unknown_field_fails_closed() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let frame = format!(
            "{{\"schema\":\"worker-event/2\",\"request_id\":\"{request_id}\",\"event\":\"capture.state\",\"state\":\"recording\",\"meeting_id\":\"{meeting_id}\",\"extra\":true}}"
        );
        assert_eq!(
            parse_output(frame.as_bytes()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    #[test]
    fn note_generation_finishing_state_parses_from_the_wire_shape() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let frame = format!(
            "{{\"schema\":\"worker-event/2\",\"request_id\":\"{request_id}\",\"event\":\"capture.state\",\"state\":\"finishing\",\"meeting_id\":\"{meeting_id}\"}}"
        );
        let output = parse_output(frame.as_bytes()).unwrap();
        match output {
            WorkerOutput::Progress(progress) => {
                assert_eq!(progress.state, CaptureProgressState::Finishing);
            }
            WorkerOutput::Result(_) => panic!("expected a progress frame"),
        }
    }
}
