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

/// The second progress event: how much of the meeting's audio the Apple speech
/// path has covered. It is deliberately not a `WorkerProgress`: that frame is a
/// closed state heartbeat whose callbacks may fail a request, and this one is
/// best-effort measurement that must never fail one.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum TranscriptionProgressEvent {
    #[serde(rename = "transcription.progress")]
    TranscriptionProgress,
}

/// A `transcription.progress` frame as it arrives.
///
/// The frame's identity (`schema`, `event`, `request_id`, `meeting_id`) is
/// strict, and an unknown key is malformed like anywhere else on this stream.
/// The two amounts are kept as raw JSON on purpose: a value the app cannot use
/// (negative, fractional, not a number, or missing) makes the frame ignorable,
/// not malformed, so a bad measurement can never fail a transcript. Whether
/// the values are usable is decided by `RequestTracker::transcription_progress`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerTranscriptionProgress {
    pub schema: WorkerEventSchema,
    pub request_id: Uuid,
    pub event: TranscriptionProgressEvent,
    pub meeting_id: Uuid,
    #[serde(default)]
    pub audio_seconds_done: Value,
    #[serde(default)]
    pub audio_seconds_total: Value,
}

/// A transcription measurement that passed admission: it belongs to the pending
/// `transcript.create` or `transcript.retry` for `meeting_id`, and
/// `0 <= audio_seconds_done <= audio_seconds_total` with `audio_seconds_total > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptionProgress {
    pub meeting_id: Uuid,
    pub audio_seconds_done: u32,
    pub audio_seconds_total: u32,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WorkerOutput {
    Progress(WorkerProgress),
    TranscriptionProgress(WorkerTranscriptionProgress),
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
    /// The last admitted `(done, total)`, so a measurement that goes backwards
    /// or changes its total within one request is ignored, not shown.
    measured: Option<(u32, u32)>,
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
            measured: None,
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

    /// Admits a `transcription.progress` frame, or ignores it.
    ///
    /// This never returns an error, on purpose. A measurement cannot change a
    /// transcript, so a frame the app cannot use is dropped and the request
    /// carries on; an error here would kill the worker (the dispatcher treats a
    /// tracker error as a protocol fault). A frame is admitted only for the
    /// pending `transcript.create` or `transcript.retry`, for the request's own
    /// meeting, with `total > 0`, `0 <= done <= total`, a total that stays the
    /// same, and a done that never goes down. Everything else, including a
    /// frame that arrives after the request's result, is ignored.
    pub fn transcription_progress(
        &mut self,
        progress: &WorkerTranscriptionProgress,
    ) -> Option<TranscriptionProgress> {
        let pending = self.pending.as_mut()?;
        if pending.request_id != progress.request_id
            || pending.meeting_id != Some(progress.meeting_id)
            || !matches!(
                pending.operation,
                Operation::TranscriptCreate | Operation::TranscriptRetry
            )
        {
            return None;
        }
        let done = u32::try_from(progress.audio_seconds_done.as_u64()?).ok()?;
        let total = u32::try_from(progress.audio_seconds_total.as_u64()?).ok()?;
        if total == 0 || done > total {
            return None;
        }
        if let Some((last_done, last_total)) = pending.measured
            && (total != last_total || done < last_done)
        {
            return None;
        }
        pending.measured = Some((done, total));
        Some(TranscriptionProgress {
            meeting_id: progress.meeting_id,
            audio_seconds_done: done,
            audio_seconds_total: total,
        })
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
            WorkerOutput::TranscriptionProgress(_) | WorkerOutput::Result(_) => {
                panic!("expected a progress frame")
            }
        }
    }

    fn transcript_command(operation: Operation) -> (WorkerCommand, Uuid, Uuid) {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let command = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation,
            arguments: serde_json::json!({"meeting_id": meeting_id}),
        };
        (command, request_id, meeting_id)
    }

    fn progress_frame(
        request_id: Uuid,
        meeting_id: Uuid,
        done: Value,
        total: Value,
    ) -> WorkerTranscriptionProgress {
        WorkerTranscriptionProgress {
            schema: WorkerEventSchema::V2,
            request_id,
            event: TranscriptionProgressEvent::TranscriptionProgress,
            meeting_id,
            audio_seconds_done: done,
            audio_seconds_total: total,
        }
    }

    fn ok_result(request_id: Uuid) -> WorkerResult {
        WorkerResult {
            schema: ResultSchema::V2,
            request_id,
            ok: true,
            code: None,
            recoverable: None,
            artifact_digests: HashMap::new(),
        }
    }

    #[test]
    fn the_contracts_literal_progress_line_parses() {
        let line = br#"{"audio_seconds_done":1234,"audio_seconds_total":7920,"event":"transcription.progress","meeting_id":"6f1f4c1e-8d5d-4f43-9d54-3b1f2a9c7e10","request_id":"0d6b8b8e-3a58-4f6c-8a7f-9c1d2e3f4a5b","schema":"worker-event/2"}"#;
        let WorkerOutput::TranscriptionProgress(frame) = parse_output(line).unwrap() else {
            panic!("the contract's line is a transcription.progress frame");
        };
        assert_eq!(frame.audio_seconds_done, serde_json::json!(1234));
        assert_eq!(frame.audio_seconds_total, serde_json::json!(7920));
        assert_eq!(
            frame.meeting_id.to_string(),
            "6f1f4c1e-8d5d-4f43-9d54-3b1f2a9c7e10"
        );
    }

    #[test]
    fn a_malformed_progress_frame_still_fails_closed() {
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let malformed = [
            // An unknown key.
            format!(
                "{{\"audio_seconds_done\":1,\"audio_seconds_total\":2,\"event\":\"transcription.progress\",\"meeting_id\":\"{meeting_id}\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/2\",\"extra\":true}}"
            ),
            // No meeting identity.
            format!(
                "{{\"audio_seconds_done\":1,\"audio_seconds_total\":2,\"event\":\"transcription.progress\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/2\"}}"
            ),
            // The wrong schema.
            format!(
                "{{\"audio_seconds_done\":1,\"audio_seconds_total\":2,\"event\":\"transcription.progress\",\"meeting_id\":\"{meeting_id}\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/1\"}}"
            ),
            // An event name nothing admits.
            format!(
                "{{\"audio_seconds_done\":1,\"audio_seconds_total\":2,\"event\":\"transcription.pr0gress\",\"meeting_id\":\"{meeting_id}\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/2\"}}"
            ),
        ];
        for frame in malformed {
            assert_eq!(
                parse_output(frame.as_bytes()).unwrap_err(),
                ProtocolError::Malformed,
                "{frame}"
            );
        }
    }

    #[test]
    fn a_progress_value_the_app_cannot_use_parses_and_is_then_ignored() {
        let (command, request_id, meeting_id) = transcript_command(Operation::TranscriptCreate);
        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        for (done, total) in [
            ("-1", "10"),
            ("1.5", "10"),
            ("\"1\"", "10"),
            ("null", "10"),
            ("1", "0"),
            ("11", "10"),
            ("99999999999", "99999999999"),
        ] {
            let line = format!(
                "{{\"audio_seconds_done\":{done},\"audio_seconds_total\":{total},\"event\":\"transcription.progress\",\"meeting_id\":\"{meeting_id}\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/2\"}}"
            );
            let WorkerOutput::TranscriptionProgress(frame) =
                parse_output(line.as_bytes()).unwrap_or_else(|_| panic!("{line}"))
            else {
                panic!("{line}");
            };
            assert_eq!(tracker.transcription_progress(&frame), None, "{line}");
        }
        // A frame with no amounts at all is a bad value, not a bad frame.
        let line = format!(
            "{{\"event\":\"transcription.progress\",\"meeting_id\":\"{meeting_id}\",\"request_id\":\"{request_id}\",\"schema\":\"worker-event/2\"}}"
        );
        let WorkerOutput::TranscriptionProgress(frame) = parse_output(line.as_bytes()).unwrap()
        else {
            panic!("{line}");
        };
        assert_eq!(tracker.transcription_progress(&frame), None);
        // None of that disturbed the request.
        tracker.terminal(&ok_result(request_id)).unwrap();
    }

    #[test]
    fn progress_is_admitted_for_a_transcript_create_and_a_transcript_retry() {
        for operation in [Operation::TranscriptCreate, Operation::TranscriptRetry] {
            let (command, request_id, meeting_id) = transcript_command(operation);
            let mut tracker = RequestTracker::default();
            tracker.register(&command).unwrap();
            let admitted = tracker
                .transcription_progress(&progress_frame(
                    request_id,
                    meeting_id,
                    serde_json::json!(0),
                    serde_json::json!(7920),
                ))
                .unwrap();
            assert_eq!(admitted.meeting_id, meeting_id);
            assert_eq!(admitted.audio_seconds_done, 0);
            assert_eq!(admitted.audio_seconds_total, 7920);
            assert!(
                tracker
                    .transcription_progress(&progress_frame(
                        request_id,
                        meeting_id,
                        serde_json::json!(7920),
                        serde_json::json!(7920),
                    ))
                    .is_some()
            );
            tracker.terminal(&ok_result(request_id)).unwrap();
        }
    }

    #[test]
    fn progress_is_ignored_unless_it_belongs_to_the_pending_transcript_request() {
        let usable = |request_id, meeting_id| {
            progress_frame(
                request_id,
                meeting_id,
                serde_json::json!(5),
                serde_json::json!(10),
            )
        };
        // No request is pending.
        let mut tracker = RequestTracker::default();
        assert_eq!(
            tracker.transcription_progress(&usable(Uuid::new_v4(), Uuid::new_v4())),
            None
        );

        // A request that is not a transcription.
        let request_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let capture = WorkerCommand {
            schema: "worker-command/2",
            request_id,
            operation: Operation::CaptureStart,
            arguments: serde_json::json!({"meeting_id": meeting_id, "profile_id": "fixture"}),
        };
        let mut tracker = RequestTracker::default();
        tracker.register(&capture).unwrap();
        assert_eq!(
            tracker.transcription_progress(&usable(request_id, meeting_id)),
            None
        );

        // Another request's frame, another meeting's frame, and a late frame.
        let (command, request_id, meeting_id) = transcript_command(Operation::TranscriptCreate);
        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        assert_eq!(
            tracker.transcription_progress(&usable(Uuid::new_v4(), meeting_id)),
            None
        );
        assert_eq!(
            tracker.transcription_progress(&usable(request_id, Uuid::new_v4())),
            None
        );
        tracker.terminal(&ok_result(request_id)).unwrap();
        assert_eq!(
            tracker.transcription_progress(&usable(request_id, meeting_id)),
            None
        );
    }

    #[test]
    fn progress_that_goes_backwards_or_changes_its_total_is_ignored() {
        let (command, request_id, meeting_id) = transcript_command(Operation::TranscriptCreate);
        let mut tracker = RequestTracker::default();
        tracker.register(&command).unwrap();
        let send = |tracker: &mut RequestTracker, done: u64, total: u64| {
            tracker.transcription_progress(&progress_frame(
                request_id,
                meeting_id,
                serde_json::json!(done),
                serde_json::json!(total),
            ))
        };
        assert!(send(&mut tracker, 10, 100).is_some());
        assert!(
            send(&mut tracker, 10, 100).is_some(),
            "a repeat is not a regression"
        );
        assert!(send(&mut tracker, 40, 100).is_some());
        assert_eq!(send(&mut tracker, 39, 100), None);
        assert_eq!(send(&mut tracker, 50, 120), None);
        // The ignored frames did not move the floor.
        assert!(send(&mut tracker, 41, 100).is_some());
    }
}
