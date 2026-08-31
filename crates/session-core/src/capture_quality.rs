//! Reader-safe projection of persisted microphone quality evidence.
//!
//! Capture quality is guidance, not the capture-integrity verdict. This module
//! verifies the retained receipt and microphone binding, then returns only a
//! closed set of observations with product-authored explanations. Raw metrics,
//! device details, paths, and receipt text never cross this boundary.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::meeting::{
    read_private_bytes, resolve_artifact, verify_artifact_ref, MeetingError, MeetingRecord,
    MAX_RECEIPT_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureQualityState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureQualityObservationKind {
    Silence,
    Clipping,
    LowInput,
    BackgroundNoise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureQualityObservationStatus {
    Observed,
    NotObserved,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureQualityObservation {
    pub kind: CaptureQualityObservationKind,
    pub status: CaptureQualityObservationStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureQualityProjection {
    pub state: CaptureQualityState,
    pub observations: Vec<CaptureQualityObservation>,
    pub message: String,
}

/// Reader-safe projection of whether a recording-device identity was retained.
///
/// This is intentionally separate from recording-quality guidance. `Identified`
/// means only that Yawn verified a microphone identity was recorded in the
/// capture receipt. It does not say that the recorded microphone was the input
/// the person intended to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordingDeviceState {
    Identified,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordingDeviceAction {
    CheckAudioInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDeviceProjection {
    pub state: RecordingDeviceState,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<RecordingDeviceAction>,
}

impl RecordingDeviceProjection {
    fn unknown(message: impl Into<String>) -> Self {
        Self {
            state: RecordingDeviceState::Unknown,
            message: message.into(),
            next_action: Some(RecordingDeviceAction::CheckAudioInput),
        }
    }
}

/// Reader-safe projection of the spans where the operator held a recording open
/// with nothing being captured.
///
/// A pause is a capture-integrity event, so this is deliberately not part of the
/// quality guidance above: quality describes the signal that was recorded, while
/// this describes the time that was not. `NotPaused` covers both a capture that
/// was never paused and every receipt written before pause existed — in each
/// case the retained audio is continuous, which is the fact a reader needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapturePauseState {
    NotPaused,
    Paused,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePauseProjection {
    pub state: CapturePauseState,
    /// How many times the operator paused. Zero whenever the state is not
    /// `Paused`, so a reader never has to interpret a count against a state.
    pub count: u64,
    /// Total time nothing was captured, in whole seconds.
    pub total_paused_seconds: u64,
    pub message: String,
}

impl CapturePauseProjection {
    fn not_paused() -> Self {
        Self {
            state: CapturePauseState::NotPaused,
            count: 0,
            total_paused_seconds: 0,
            message: "Recording was not paused during this meeting.".into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            state: CapturePauseState::Unavailable,
            count: 0,
            total_paused_seconds: 0,
            message: message.into(),
        }
    }

    /// For a caller that could not reach the meeting record at all. It says the
    /// check did not happen, which is the honest reading — never "no pauses".
    pub fn unchecked() -> Self {
        Self::unavailable("Yawn could not check whether this recording was paused.")
    }
}

impl CaptureQualityProjection {
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            state: CaptureQualityState::Unavailable,
            observations: Vec::new(),
            message: message.into(),
        }
    }
}

/// Returns persisted quality guidance only when its closed shape and
/// microphone digest still match the retained meeting artifacts.
pub fn project_capture_quality(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<CaptureQualityProjection, MeetingError> {
    let Some(session) = meeting.artifacts.capture_session.as_ref() else {
        return Ok(CaptureQualityProjection::unavailable(
            "Recording-quality evidence is unavailable for this meeting.",
        ));
    };
    let Some(microphone) = meeting.artifacts.microphone_audio.as_ref() else {
        return Ok(CaptureQualityProjection::unavailable(
            "Recording-quality evidence cannot be verified without retained microphone audio.",
        ));
    };

    verify_artifact_ref(meeting_dir, session)?;
    verify_artifact_ref(meeting_dir, microphone)?;
    let bytes = read_private_bytes(
        &resolve_artifact(meeting_dir, &session.relative_path)?,
        MAX_RECEIPT_BYTES,
    )?;
    let receipt: Value = serde_json::from_slice(&bytes)?;

    let Some(receipt_object) = receipt.as_object() else {
        return Ok(malformed_projection());
    };
    if receipt_object.get("schema").and_then(Value::as_str) != Some("capture-session/2") {
        return Ok(malformed_projection());
    }
    let Some(quality) = receipt_object.get("quality") else {
        return Ok(CaptureQualityProjection::unavailable(
            "This meeting predates persisted recording-quality evidence.",
        ));
    };
    let Some(quality_object) = quality.as_object() else {
        return Ok(malformed_projection());
    };
    if quality_object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "schema" | "source" | "metrics" | "observations"
        )
    }) || quality_object.get("schema").and_then(Value::as_str) != Some("capture-quality/1")
    {
        return Ok(malformed_projection());
    }

    let Some(source) = quality_object.get("source").and_then(Value::as_object) else {
        return Ok(malformed_projection());
    };
    if source
        .keys()
        .any(|key| !matches!(key.as_str(), "leg" | "artifact" | "samples" | "sha256"))
        || source.get("leg").and_then(Value::as_str) != Some("mic")
        || source.get("artifact").and_then(Value::as_str) != Some("mic.wav")
        || source.get("sha256").and_then(Value::as_str) != Some(&microphone.sha256)
    {
        return Ok(malformed_projection());
    }

    let Some(observations) = quality_object
        .get("observations")
        .and_then(Value::as_object)
    else {
        return Ok(malformed_projection());
    };
    if observations.len() != 4 {
        return Ok(malformed_projection());
    }

    let definitions = [
        (
            "silence",
            CaptureQualityObservationKind::Silence,
            "The microphone recording may be mostly silent.",
            "The microphone recording contains measurable signal.",
            "Yawn could not assess whether the microphone recording is silent.",
        ),
        (
            "clipping",
            CaptureQualityObservationKind::Clipping,
            "The microphone signal may be clipped.",
            "No material microphone clipping was observed.",
            "Yawn could not assess microphone clipping.",
        ),
        (
            "low_input",
            CaptureQualityObservationKind::LowInput,
            "The microphone level may be too low.",
            "The microphone level cleared the low-input floor.",
            "Yawn could not assess the microphone input level.",
        ),
        (
            "background_noise",
            CaptureQualityObservationKind::BackgroundNoise,
            "Steady background sound may reduce transcription quality.",
            "No material steady background sound was observed.",
            "Yawn could not assess steady background sound.",
        ),
    ];

    let mut projected = Vec::with_capacity(definitions.len());
    for (name, kind, observed, not_observed, unknown) in definitions {
        let Some(value) = observations.get(name).and_then(Value::as_object) else {
            return Ok(malformed_projection());
        };
        if value
            .keys()
            .any(|key| !matches!(key.as_str(), "status" | "detail" | "metrics"))
        {
            return Ok(malformed_projection());
        }
        let (status, message) = match value.get("status").and_then(Value::as_str) {
            Some("observed") => (CaptureQualityObservationStatus::Observed, observed),
            Some("not_observed") => (CaptureQualityObservationStatus::NotObserved, not_observed),
            Some("unknown") => (CaptureQualityObservationStatus::Unknown, unknown),
            _ => return Ok(malformed_projection()),
        };
        projected.push(CaptureQualityObservation {
            kind,
            status,
            message: message.to_owned(),
        });
    }

    Ok(CaptureQualityProjection {
        state: CaptureQualityState::Available,
        observations: projected,
        message: "Recording-quality evidence describes the retained microphone signal; it is guidance, not a capture-integrity verdict.".into(),
    })
}

/// Returns the closed recording-device context only after the retained capture
/// receipt and microphone audio still verify. No device metadata crosses this
/// boundary.
pub fn project_recording_device(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<RecordingDeviceProjection, MeetingError> {
    let Some(session) = meeting.artifacts.capture_session.as_ref() else {
        return Ok(RecordingDeviceProjection::unknown(
            "Yawn could not verify that a microphone identity was recorded for this meeting.",
        ));
    };
    let Some(microphone) = meeting.artifacts.microphone_audio.as_ref() else {
        return Ok(RecordingDeviceProjection::unknown(
            "Yawn could not verify that a microphone identity was recorded for this meeting.",
        ));
    };

    // This is the same retained receipt/audio binding checked before quality
    // guidance is projected. Changed bytes remain an integrity error, rather
    // than becoming a potentially misleading device result.
    verify_artifact_ref(meeting_dir, session)?;
    verify_artifact_ref(meeting_dir, microphone)?;
    let bytes = read_private_bytes(
        &resolve_artifact(meeting_dir, &session.relative_path)?,
        MAX_RECEIPT_BYTES,
    )?;
    let receipt: Value = serde_json::from_slice(&bytes)?;

    let Some(receipt_object) = receipt.as_object() else {
        return Ok(unknown_device_projection());
    };
    if receipt_object.get("schema").and_then(Value::as_str) != Some("capture-session/2") {
        return Ok(unknown_device_projection());
    }
    let Some(device) = receipt_object.get("microphone") else {
        return Ok(RecordingDeviceProjection::unknown(
            "This meeting predates persisted recording-device evidence.",
        ));
    };
    let Some(device) = device.as_object() else {
        return Ok(unknown_device_projection());
    };
    if device
        .keys()
        .any(|key| !matches!(key.as_str(), "schema" | "index" | "name" | "hostapi"))
        || device.get("schema").and_then(Value::as_str) != Some("capture-microphone/1")
        || device.get("index").and_then(Value::as_u64).is_none()
        || device
            .get("hostapi")
            .is_some_and(|hostapi| hostapi.as_u64().is_none())
        || device
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(|name| name.trim().is_empty())
    {
        return Ok(unknown_device_projection());
    }

    Ok(RecordingDeviceProjection {
        state: RecordingDeviceState::Identified,
        message: "Yawn verified that a microphone identity was recorded for this meeting. This does not confirm it was the audio input you intended to use.".into(),
        next_action: None,
    })
}

/// Sample rate every retained capture leg is normalized to. Pause spans are
/// measured in the same unit as `capture_elapsed_samples` so the two can be
/// reconciled without a second time base.
const CAPTURE_RATE: u64 = 16_000;

/// The largest number of pause spans one capture receipt may carry. This is the
/// reader's half of the bound the capture writer enforces; a longer list is a
/// receipt this reader will not summarize.
const MAX_PAUSE_SPANS: usize = 64;

/// Returns how often and how long a recording was held open with nothing being
/// captured.
///
/// The pause record binds to the capture receipt rather than to the retained
/// audio, so this stays readable after audio retention deletes the WAV pair.
/// Only the receipt's own bytes are verified before anything is projected.
pub fn project_capture_pauses(
    meeting_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<CapturePauseProjection, MeetingError> {
    let Some(session) = meeting.artifacts.capture_session.as_ref() else {
        return Ok(CapturePauseProjection::unavailable(
            "Yawn could not check whether this recording was paused.",
        ));
    };
    verify_artifact_ref(meeting_dir, session)?;
    let bytes = read_private_bytes(
        &resolve_artifact(meeting_dir, &session.relative_path)?,
        MAX_RECEIPT_BYTES,
    )?;
    let receipt: Value = serde_json::from_slice(&bytes)?;

    let Some(receipt_object) = receipt.as_object() else {
        return Ok(malformed_pause_projection());
    };
    if receipt_object.get("schema").and_then(Value::as_str) != Some("capture-session/2") {
        return Ok(malformed_pause_projection());
    }
    // A receipt written before pause existed describes a capture that could not
    // have been paused. It reads as an uninterrupted recording, which is true,
    // rather than as missing evidence.
    let Some(pauses) = receipt_object.get("pauses").filter(|value| !value.is_null()) else {
        return Ok(CapturePauseProjection::not_paused());
    };
    let Some(pauses) = pauses.as_object() else {
        return Ok(malformed_pause_projection());
    };
    if pauses
        .keys()
        .any(|key| !matches!(key.as_str(), "schema" | "spans"))
        || pauses.get("schema").and_then(Value::as_str) != Some("capture-pauses/1")
    {
        return Ok(malformed_pause_projection());
    }
    let Some(spans) = pauses.get("spans").and_then(Value::as_array) else {
        return Ok(malformed_pause_projection());
    };
    if spans.len() > MAX_PAUSE_SPANS {
        return Ok(malformed_pause_projection());
    }

    // Both offsets are wall-clock positions from the moment recording began.
    // Spans are therefore ordered and non-overlapping; a receipt that says
    // otherwise is not one this reader will summarize.
    let mut total_samples: u64 = 0;
    let mut previous_end: Option<u64> = None;
    for span in spans {
        let Some(span) = span.as_object() else {
            return Ok(malformed_pause_projection());
        };
        if span
            .keys()
            .any(|key| !matches!(key.as_str(), "paused_at_samples" | "resumed_at_samples"))
        {
            return Ok(malformed_pause_projection());
        }
        let (Some(start), Some(end)) = (
            span.get("paused_at_samples").and_then(Value::as_u64),
            span.get("resumed_at_samples").and_then(Value::as_u64),
        ) else {
            return Ok(malformed_pause_projection());
        };
        if end <= start || previous_end.is_some_and(|previous| start < previous) {
            return Ok(malformed_pause_projection());
        }
        previous_end = Some(end);
        let Some(running) = total_samples.checked_add(end - start) else {
            return Ok(malformed_pause_projection());
        };
        total_samples = running;
    }

    if spans.is_empty() {
        return Ok(CapturePauseProjection::not_paused());
    }
    let count = spans.len() as u64;
    let seconds = total_samples / CAPTURE_RATE;
    Ok(CapturePauseProjection {
        state: CapturePauseState::Paused,
        count,
        total_paused_seconds: seconds,
        message: format!(
            "Recording was paused {} {}, for {} in total. Nothing was captured during {}.",
            count,
            if count == 1 { "time" } else { "times" },
            clock_label(seconds),
            if count == 1 { "that gap" } else { "those gaps" },
        ),
    })
}

/// Renders a whole-second duration as the same m:ss shape the recording surface
/// already uses, so a gap reads in the units the operator watched it in.
fn clock_label(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn malformed_pause_projection() -> CapturePauseProjection {
    CapturePauseProjection::unavailable(
        "Yawn could not verify whether this recording was paused.",
    )
}

fn malformed_projection() -> CaptureQualityProjection {
    CaptureQualityProjection::unavailable(
        "Recording-quality evidence is present but could not be verified.",
    )
}

fn unknown_device_projection() -> RecordingDeviceProjection {
    RecordingDeviceProjection::unknown(
        "Yawn could not verify that a microphone identity was recorded for this meeting.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use crate::meeting::{
        artifact_ref, retention_policy_sha256, AudioRetention, AudioRetentionRule, AudioState,
        MeetingArtifacts, MeetingLifecycle, MeetingRecord, MeetingSchema,
    };

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn private_file(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn fixture() -> (TempDir, MeetingRecord) {
        let temporary = TempDir::new().unwrap();
        let meeting_dir = temporary.path().join("meeting-a");
        private_directory(&meeting_dir.join("capture"));
        private_file(&meeting_dir.join("attempt.json"), b"attempt");
        private_file(&meeting_dir.join("ownership.json"), b"ownership");
        private_file(&meeting_dir.join("capture/mic.wav"), b"microphone");
        private_file(&meeting_dir.join("capture/system.wav"), b"system");

        let microphone = artifact_ref(&meeting_dir, "capture/mic.wav").unwrap();
        let system = artifact_ref(&meeting_dir, "capture/system.wav").unwrap();
        let receipt = serde_json::json!({
            "schema": "capture-session/2",
            "status": "complete",
            "started_at": "2000-01-01T00:00:00+0000",
            "finalized_at": "2000-01-01T00:00:01+0000",
            "health": {"schema": "capture-health/1", "usable": true},
            "quality": {
                "schema": "capture-quality/1",
                "source": {"leg": "mic", "artifact": "mic.wav", "samples": 1600, "sha256": microphone.sha256},
                "metrics": {"duration_s": 0.1},
                "observations": {
                    "silence": {"status": "not_observed", "detail": "ignored"},
                    "clipping": {"status": "observed", "detail": "ignored"},
                    "low_input": {"status": "unknown", "detail": "ignored"},
                    "background_noise": {"status": "not_observed", "detail": "ignored"}
                }
            },
            "microphone": {"schema": "capture-microphone/1", "index": 4, "name": "Private device name"},
            "reconciliation": {"legs": {}},
            "artifacts": []
        });
        private_file(
            &meeting_dir.join("capture/session.json"),
            &serde_json::to_vec(&receipt).unwrap(),
        );
        let session = artifact_ref(&meeting_dir, "capture/session.json").unwrap();
        let meeting = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: "meeting-a".into(),
            lifecycle: MeetingLifecycle::Captured,
            pending_storage_operation: None,
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&meeting_dir, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&meeting_dir, "ownership.json").unwrap()),
                capture_session: Some(session),
                microphone_audio: Some(microphone),
                system_audio: Some(system),
                current_transcript: None,
                current_note: None,
            },
            retention: AudioRetention {
                rule: AudioRetentionRule::UntilManualDeletion,
                policy_sha256: retention_policy_sha256(&AudioRetentionRule::UntilManualDeletion),
                next_deletion_at_epoch_seconds: None,
                state: AudioState::Retained,
                deletion_receipt: None,
            },
        };
        (temporary, meeting)
    }

    #[test]
    fn projects_only_closed_reader_safe_observations() {
        let (temporary, meeting) = fixture();
        let projection =
            project_capture_quality(&temporary.path().join("meeting-a"), &meeting).unwrap();
        assert_eq!(projection.state, CaptureQualityState::Available);
        assert_eq!(projection.observations.len(), 4);
        assert_eq!(
            projection.observations[1].status,
            CaptureQualityObservationStatus::Observed
        );
        let encoded = serde_json::to_string(&projection).unwrap();
        assert!(!encoded.contains("Private device name"));
        assert!(!encoded.contains("duration_s"));
        assert!(!encoded.contains("ignored"));
        assert!(!encoded.contains("capture/mic.wav"));
    }

    #[test]
    fn legacy_or_mismatched_evidence_is_explicitly_unavailable() {
        let (temporary, mut meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        let session_path = meeting_dir.join("capture/session.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&session_path).unwrap()).unwrap();
        receipt.as_object_mut().unwrap().remove("quality");
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        let legacy = project_capture_quality(&meeting_dir, &meeting).unwrap();
        assert_eq!(legacy.state, CaptureQualityState::Unavailable);
        assert!(legacy.message.contains("predates"));

        receipt["quality"] = serde_json::json!({
            "schema": "capture-quality/1",
            "source": {"leg": "mic", "artifact": "mic.wav", "sha256": "0".repeat(64)},
            "metrics": null,
            "observations": {}
        });
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        let mismatched = project_capture_quality(&meeting_dir, &meeting).unwrap();
        assert_eq!(mismatched.state, CaptureQualityState::Unavailable);
        assert!(mismatched.message.contains("could not be verified"));
    }

    #[test]
    fn changed_session_or_microphone_bytes_fail_before_projection() {
        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        private_file(&meeting_dir.join("capture/mic.wav"), b"changed");
        assert!(project_capture_quality(&meeting_dir, &meeting).is_err());

        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        private_file(&meeting_dir.join("capture/session.json"), b"changed");
        assert!(project_capture_quality(&meeting_dir, &meeting).is_err());
    }

    #[test]
    fn projects_only_closed_recording_device_context() {
        let (temporary, meeting) = fixture();
        let projection =
            project_recording_device(&temporary.path().join("meeting-a"), &meeting).unwrap();

        assert_eq!(projection.state, RecordingDeviceState::Identified);
        assert_eq!(projection.next_action, None);
        assert!(projection.message.contains("Yawn verified"));
        assert!(projection.message.contains("intended to use"));
        let encoded = serde_json::to_value(&projection).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "state": "identified",
                "message": "Yawn verified that a microphone identity was recorded for this meeting. This does not confirm it was the audio input you intended to use."
            })
        );
        let serialized = encoded.to_string();
        for private_value in [
            "Private device name",
            "capture/mic.wav",
            "sha256",
            "index",
            "hostapi",
            "duration_s",
            "ignored",
        ] {
            assert!(!serialized.contains(private_value));
        }
    }

    #[test]
    fn legacy_unknown_or_extra_device_evidence_projects_unknown_with_closed_action() {
        let (temporary, mut meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        let session_path = meeting_dir.join("capture/session.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&session_path).unwrap()).unwrap();
        receipt.as_object_mut().unwrap().remove("microphone");
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());

        let legacy = project_recording_device(&meeting_dir, &meeting).unwrap();
        assert_eq!(legacy.state, RecordingDeviceState::Unknown);
        assert_eq!(
            legacy.next_action,
            Some(RecordingDeviceAction::CheckAudioInput)
        );
        assert!(legacy.message.contains("predates"));
        assert_eq!(
            serde_json::to_value(&legacy).unwrap()["nextAction"],
            "check-audio-input"
        );

        receipt["microphone"] = serde_json::json!({
            "schema": "capture-microphone/1",
            "index": 4,
            "name": "Private device name",
            "hostapi": "Private host API"
        });
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        let unrecognized = project_recording_device(&meeting_dir, &meeting).unwrap();
        assert_eq!(unrecognized.state, RecordingDeviceState::Unknown);
        let serialized = serde_json::to_string(&unrecognized).unwrap();
        assert!(!serialized.contains("Private device name"));
        assert!(!serialized.contains("Private host API"));

        receipt["microphone"] = serde_json::json!({
            "schema": "capture-microphone/1",
            "index": 4,
            "name": "Private device name",
            "hostapi": 2
        });
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        let canonical = project_recording_device(&meeting_dir, &meeting).unwrap();
        assert_eq!(canonical.state, RecordingDeviceState::Identified);
        assert!(!serde_json::to_string(&canonical)
            .unwrap()
            .contains("hostapi"));

        receipt["microphone"]["unrecognized"] = serde_json::json!(true);
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(&meeting_dir, "capture/session.json").unwrap());
        let extra = project_recording_device(&meeting_dir, &meeting).unwrap();
        assert_eq!(extra.state, RecordingDeviceState::Unknown);
        assert!(!serde_json::to_string(&extra)
            .unwrap()
            .contains("unrecognized"));
    }

    /// Rewrites the fixture receipt and returns the meeting bound to the new
    /// bytes, so each pause case verifies against a receipt it actually wrote.
    fn with_receipt(
        meeting_dir: &Path,
        meeting: &mut MeetingRecord,
        edit: impl FnOnce(&mut Value),
    ) {
        let session_path = meeting_dir.join("capture/session.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&session_path).unwrap()).unwrap();
        edit(&mut receipt);
        private_file(&session_path, &serde_json::to_vec(&receipt).unwrap());
        meeting.artifacts.capture_session =
            Some(artifact_ref(meeting_dir, "capture/session.json").unwrap());
    }

    #[test]
    fn a_receipt_without_the_pause_field_reads_as_an_uninterrupted_recording() {
        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");

        // The fixture receipt predates pause and carries no `pauses` key at all.
        let legacy = project_capture_pauses(&meeting_dir, &meeting).unwrap();
        assert_eq!(legacy.state, CapturePauseState::NotPaused);
        assert_eq!(legacy.count, 0);
        assert_eq!(legacy.total_paused_seconds, 0);
        assert_eq!(
            legacy.message,
            "Recording was not paused during this meeting."
        );
        assert_eq!(
            serde_json::to_value(&legacy).unwrap(),
            serde_json::json!({
                "state": "not-paused",
                "count": 0,
                "totalPausedSeconds": 0,
                "message": "Recording was not paused during this meeting."
            })
        );

        // An explicitly empty span list is the same fact, stated by a writer
        // that knows about pause. It must not read differently.
        let mut empty = meeting.clone();
        with_receipt(&meeting_dir, &mut empty, |receipt| {
            receipt["pauses"] =
                serde_json::json!({"schema": "capture-pauses/1", "spans": []});
        });
        assert_eq!(
            project_capture_pauses(&meeting_dir, &empty).unwrap(),
            legacy
        );
    }

    #[test]
    fn recorded_pauses_project_a_count_and_a_total() {
        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");

        let mut once = meeting.clone();
        with_receipt(&meeting_dir, &mut once, |receipt| {
            receipt["pauses"] = serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 16_000, "resumed_at_samples": 16_000 * 46}]
            });
        });
        let single = project_capture_pauses(&meeting_dir, &once).unwrap();
        assert_eq!(single.state, CapturePauseState::Paused);
        assert_eq!(single.count, 1);
        assert_eq!(single.total_paused_seconds, 45);
        assert_eq!(
            single.message,
            "Recording was paused 1 time, for 0:45 in total. Nothing was captured during that gap."
        );

        let mut twice = meeting.clone();
        with_receipt(&meeting_dir, &mut twice, |receipt| {
            receipt["pauses"] = serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": 16_000, "resumed_at_samples": 16_000 * 46},
                    {"paused_at_samples": 16_000 * 60, "resumed_at_samples": 16_000 * 105}
                ]
            });
        });
        let pair = project_capture_pauses(&meeting_dir, &twice).unwrap();
        assert_eq!(pair.count, 2);
        assert_eq!(pair.total_paused_seconds, 90);
        assert_eq!(
            pair.message,
            "Recording was paused 2 times, for 1:30 in total. Nothing was captured during those gaps."
        );
    }

    #[test]
    fn an_unreadable_pause_record_is_never_summarized_as_no_pauses() {
        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");

        let malformed = [
            serde_json::json!({"schema": "capture-pauses/2", "spans": []}),
            serde_json::json!({"schema": "capture-pauses/1"}),
            serde_json::json!({"schema": "capture-pauses/1", "spans": [], "extra": true}),
            serde_json::json!({"schema": "capture-pauses/1", "spans": {}}),
            // A zero-length gap is not a pause that happened.
            serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 0, "resumed_at_samples": 0}]
            }),
            // Offsets must advance: two gaps cannot sit at the same place in
            // the retained audio.
            serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": 320_000, "resumed_at_samples": 336_000},
                    {"paused_at_samples": 16_000, "resumed_at_samples": 32_000}
                ]
            }),
            serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 0, "resumed_at_samples": -4}]
            }),
            serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 0, "resumed_at_samples": 16_000, "why": "no"}]
            }),
            serde_json::json!([]),
        ];
        for shape in malformed {
            let mut broken = meeting.clone();
            with_receipt(&meeting_dir, &mut broken, |receipt| {
                receipt["pauses"] = shape.clone();
            });
            let projection = project_capture_pauses(&meeting_dir, &broken).unwrap();
            assert_eq!(
                projection.state,
                CapturePauseState::Unavailable,
                "{shape} must not be summarized"
            );
            assert_eq!(projection.count, 0);
            assert!(projection.message.contains("could not verify"));
        }

        let mut too_many = meeting.clone();
        with_receipt(&meeting_dir, &mut too_many, |receipt| {
            let spans = (0..=MAX_PAUSE_SPANS)
                .map(|index| {
                    serde_json::json!({
                        "paused_at_samples": index * 32_000,
                        "resumed_at_samples": index * 32_000 + 16_000
                    })
                })
                .collect::<Vec<_>>();
            receipt["pauses"] =
                serde_json::json!({"schema": "capture-pauses/1", "spans": spans});
        });
        assert_eq!(
            project_capture_pauses(&meeting_dir, &too_many)
                .unwrap()
                .state,
            CapturePauseState::Unavailable
        );
    }

    #[test]
    fn pause_evidence_survives_deleted_audio_but_not_changed_receipt_bytes() {
        // Retention deletes the WAV pair; the capture-integrity record of what
        // was never recorded still has to read.
        let (temporary, mut meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        with_receipt(&meeting_dir, &mut meeting, |receipt| {
            receipt["pauses"] = serde_json::json!({
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 16_000, "resumed_at_samples": 48_000}]
            });
        });
        meeting.artifacts.microphone_audio = None;
        meeting.artifacts.system_audio = None;
        let projection = project_capture_pauses(&meeting_dir, &meeting).unwrap();
        assert_eq!(projection.state, CapturePauseState::Paused);
        assert_eq!(projection.total_paused_seconds, 2);

        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        private_file(&meeting_dir.join("capture/session.json"), b"changed");
        assert!(project_capture_pauses(&meeting_dir, &meeting).is_err());

        let mut missing = meeting.clone();
        missing.artifacts.capture_session = None;
        let unavailable = project_capture_pauses(&meeting_dir, &missing).unwrap();
        assert_eq!(unavailable.state, CapturePauseState::Unavailable);
        assert!(unavailable.message.contains("could not check"));
    }

    #[test]
    fn changed_session_or_microphone_bytes_fail_before_device_projection() {
        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        private_file(&meeting_dir.join("capture/mic.wav"), b"changed");
        assert!(project_recording_device(&meeting_dir, &meeting).is_err());

        let (temporary, meeting) = fixture();
        let meeting_dir = temporary.path().join("meeting-a");
        private_file(&meeting_dir.join("capture/session.json"), b"changed");
        assert!(project_recording_device(&meeting_dir, &meeting).is_err());
    }
}
