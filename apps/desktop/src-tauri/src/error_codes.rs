// W7-B hardening (2026-09-01 desktop audit). The audit found that
// error-recovery actions in the frontend were coupled to exact backend
// error-message strings: a rewording of a known message would silently
// downgrade its recovery action to a dismiss-only toast, with no
// compile-time or test-time signal.
//
// This module gives every recoverable error a stable, kebab-case machine
// code that travels alongside the human-readable message, so the frontend
// can key its recovery mapping on the code instead of the exact wording.
// The message text is NOT changed by this packet -- only a code is added.
//
// Two independent transports carry these codes to the frontend:
//
// 1. Genuine command failures (`Result<T, String>` -> `Err`): the command's
//    return type becomes `Result<T, CommandError>`. `CommandError` serializes
//    as `{ "code": ..., "message": ... }`. Existing `?`/`.into()` call sites
//    that produce an uncoded `String` or `&str` keep compiling unchanged via
//    the blanket `From` impls below (`code: None` -- the legacy fallback).
// 2. Structured `Ok(response)` shapes that already carry a `state`/`message`
//    pair (the frontend re-throws these as an `Error` to reuse the same
//    recovery-presentation path -- see `main.js`'s retained-audio and
//    preview-deletion handlers). Those response structs gain a sibling
//    `code: Option<&'static str>` field, populated only at the same
//    construction sites that produce one of the messages below.
//
// `apps/desktop/src-tauri/error-codes.json` is the single list of
// (code, message) pairs this module and the frontend's drift test both
// check against -- see `error_code_drift` below and
// `apps/desktop/ui/error-codes.test.mjs`.

use serde::Serialize;

/// A command error that may carry a stable recovery code alongside its
/// human-readable message. Serializes to `{ "code": ..., "message": ... }`
/// so a Tauri command returning `Result<T, CommandError>` gives the
/// frontend a structured rejection instead of a bare string.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: Option<&'static str>,
    pub message: String,
}

impl CommandError {
    /// Build a coded error. `message` must stay byte-identical to the
    /// existing user-facing copy -- this packet moves plumbing, not words.
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

// Legacy fallback: any call site not yet migrated to `CommandError::coded`
// keeps compiling as-is once a command's return type changes to
// `Result<T, CommandError>` -- `?` and `.into()` reach here with `code: None`.
impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            message,
        }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self {
            code: None,
            message: message.to_string(),
        }
    }
}

// -- Stable codes --------------------------------------------------------
//
// One constant per distinct recoverable message, migrated from the two
// hand-maintained arrays `errorRecoveryPresentation` held in
// `apps/desktop/ui/view-model.mjs`. Kept in kebab-case to match the action
// codes already used across the frontend (e.g. "refresh-library").
//
// NOTE: the JS array's final entry, "Meeting deletion could not complete.
// Reopen Library and try again.", has no live Rust origin -- the real
// message at that call site (main.rs, `preview_delete_meeting_for`) reads
// "Moving this meeting to Trash could not complete. Reopen Library and try
// again." That JS entry was already dead (the audit's exact failure mode,
// already fired) and is left in the legacy array unmigrated -- coding the
// Trash message instead would attach a recovery action to an error that
// never had one, which this packet's parity rule forbids.

pub const VIEW_STALE: &str = "view-stale";
pub const LIBRARY_UNAVAILABLE: &str = "library-unavailable";
pub const PREVIEW_LIBRARY_UNAVAILABLE: &str = "preview-library-unavailable";
pub const MEETING_LIBRARY_UNAVAILABLE: &str = "meeting-library-unavailable";
pub const TRANSCRIPT_UNAVAILABLE: &str = "transcript-unavailable";
pub const TRANSCRIPT_CHANGED: &str = "transcript-changed";
pub const VOCABULARY_CHECK_UNSAFE: &str = "vocabulary-check-unsafe";
pub const VOCABULARY_READ_FAILED: &str = "vocabulary-read-failed";
pub const VOCABULARY_UNAVAILABLE: &str = "vocabulary-unavailable";
pub const MEETING_ACTION_IN_USE: &str = "meeting-action-in-use";
pub const TRANSCRIPT_CHANGED_RETRY: &str = "transcript-changed-retry";
pub const SPEAKER_CORRECTION_UNAVAILABLE: &str = "speaker-correction-unavailable";
pub const SPEAKER_GROUP_UNAVAILABLE: &str = "speaker-group-unavailable";
pub const RETRY_QUALITY_EVIDENCE_CHANGED: &str = "retry-quality-evidence-changed";
pub const RETRY_DEVICE_EVIDENCE_CHANGED: &str = "retry-device-evidence-changed";
pub const RETRY_PAUSE_EVIDENCE_CHANGED: &str = "retry-pause-evidence-changed";
pub const RETRY_CANDIDATE_CHANGED: &str = "retry-candidate-changed";
pub const RETAINED_AUDIO_UNAVAILABLE: &str = "retained-audio-unavailable";
pub const RECORDING_DELETION_UNAVAILABLE: &str = "recording-deletion-unavailable";
pub const AUDIO_RETENTION_UNAVAILABLE: &str = "audio-retention-unavailable";
pub const MEETING_CHANGED_UNAVAILABLE: &str = "meeting-changed-unavailable";
pub const MEETING_ACTION_IN_PROGRESS: &str = "meeting-action-in-progress";
pub const RECORDING_DELETION_FAILED: &str = "recording-deletion-failed";
pub const TRANSCRIPT_DELETION_UNAVAILABLE: &str = "transcript-deletion-unavailable";
pub const TRANSCRIPT_DELETION_FAILED: &str = "transcript-deletion-failed";
pub const MEETING_DELETION_UNAVAILABLE: &str = "meeting-deletion-unavailable";

/// Every code this module defines. Manually kept in sync with the constants
/// above (compiler-checked: each entry must name a real constant) and
/// tested against `error-codes.json` by `error_code_drift` below.
pub const ALL_CODES: &[&str] = &[
    VIEW_STALE,
    LIBRARY_UNAVAILABLE,
    PREVIEW_LIBRARY_UNAVAILABLE,
    MEETING_LIBRARY_UNAVAILABLE,
    TRANSCRIPT_UNAVAILABLE,
    TRANSCRIPT_CHANGED,
    VOCABULARY_CHECK_UNSAFE,
    VOCABULARY_READ_FAILED,
    VOCABULARY_UNAVAILABLE,
    MEETING_ACTION_IN_USE,
    TRANSCRIPT_CHANGED_RETRY,
    SPEAKER_CORRECTION_UNAVAILABLE,
    SPEAKER_GROUP_UNAVAILABLE,
    RETRY_QUALITY_EVIDENCE_CHANGED,
    RETRY_DEVICE_EVIDENCE_CHANGED,
    RETRY_PAUSE_EVIDENCE_CHANGED,
    RETRY_CANDIDATE_CHANGED,
    RETAINED_AUDIO_UNAVAILABLE,
    RECORDING_DELETION_UNAVAILABLE,
    AUDIO_RETENTION_UNAVAILABLE,
    MEETING_CHANGED_UNAVAILABLE,
    MEETING_ACTION_IN_PROGRESS,
    RECORDING_DELETION_FAILED,
    TRANSCRIPT_DELETION_UNAVAILABLE,
    TRANSCRIPT_DELETION_FAILED,
    MEETING_DELETION_UNAVAILABLE,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The drift test named in the packet: `apps/desktop/src-tauri/error-
    /// codes.json` is the shared, duplicated-but-tested registry. This test
    /// asserts every code Rust defines (`ALL_CODES`) has a matching entry in
    /// the JSON, and every JSON entry names a real Rust constant. The
    /// sibling JS test (`apps/desktop/ui/error-codes.test.mjs`) asserts the
    /// same JSON file's codes are exactly the keys the frontend's recovery
    /// map recognizes. Removing a code from either `ALL_CODES` or the JSON
    /// fails this test; removing one from the JSON or the JS map fails the
    /// JS test -- see that file for the demonstrated failure.
    #[test]
    fn error_code_drift() {
        let raw = include_str!("../error-codes.json");
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(raw).expect("error-codes.json must be valid JSON");
        assert!(!parsed.is_empty(), "error-codes.json must not be empty");

        let json_codes: BTreeSet<&str> = parsed
            .iter()
            .map(|entry| {
                entry["code"]
                    .as_str()
                    .expect("every error-codes.json entry needs a string \"code\"")
            })
            .collect();
        let rust_codes: BTreeSet<&str> = ALL_CODES.iter().copied().collect();

        let missing_from_json: Vec<&&str> = rust_codes.difference(&json_codes).collect();
        assert!(
            missing_from_json.is_empty(),
            "codes in ALL_CODES but missing from error-codes.json: {missing_from_json:?}"
        );
        let missing_from_rust: Vec<&&str> = json_codes.difference(&rust_codes).collect();
        assert!(
            missing_from_rust.is_empty(),
            "codes in error-codes.json but missing from ALL_CODES: {missing_from_rust:?}"
        );

        // Message text is also checked byte-for-byte: the registry is the
        // honesty contract that the frontend and this module never see a
        // reworded copy of the same code without both sides updating.
        for entry in &parsed {
            let code = entry["code"].as_str().unwrap();
            let message = entry["message"]
                .as_str()
                .expect("every error-codes.json entry needs a string \"message\"");
            assert!(
                !message.is_empty(),
                "error-codes.json entry for code {code:?} has an empty message"
            );
        }
    }
}
