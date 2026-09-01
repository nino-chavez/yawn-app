//! Packet W7-C: the intent-to-recording path, timed.
//!
//! Design intake D2 (`docs/roadmap.md`) proposes a stated seconds budget for
//! Yawn's start latency, but the budget was never stated because it was
//! never measured. This module writes a local receipt of one start
//! attempt's checkpoints, once that attempt reaches `CaptureState::
//! Recording`, so a later operator decision can read real numbers instead
//! of estimating one.
//!
//! **This is receipt-only evidence, not telemetry.** Nothing here renders in
//! the UI, nothing is aggregated across meetings, and nothing leaves this
//! Mac. The file sits beside the meeting's other private sidecars --
//! `<meeting_dir>/capture-timing.json` -- and is meant to be read the same
//! way `operator_note.rs`'s sidecar is read: by grepping the meeting
//! directory, not by a rendered surface.
//!
//! **Only timestamps and durations are recorded.** No query text, no
//! meeting title, no transcript content, nothing derived from what was said
//! or typed -- five integers and two of their differences, plus the build
//! that produced them.
//!
//! **A start that never reaches Recording writes nothing.** [`write`] is
//! called from exactly one place: past the `CaptureState::Recording`
//! transition in `run_capture_task`. A cancelled sheet, a refused
//! attestation, or a failed arm never reaches that call. Absence in a
//! meeting directory means this attempt was never measured, not that its
//! start was instant -- [`read`] must never be treated as returning zero
//! when it returns `None`.
//!
//! ## Clock model
//!
//! `t0` (the operator's start intent), `t1` (the start sheet rendered and
//! interactive), and `t2` (consent confirmed) are measured in the frontend
//! with one `Date.now()` anchor taken at `t0`, plus `performance.now()`
//! deltas for the two later marks. `performance.now()` is monotonic within
//! a page lifetime and immune to a wall-clock step (NTP, DST, an operator
//! adjusting the Mac's clock); anchoring the delta back to the single
//! `Date.now()` read converts it to an epoch-millis timestamp comparable
//! with this process's own clock, without re-reading the wall clock a
//! second or third time. `t3` (`start_meeting` returns, Arming accepted)
//! and `t4` (`CaptureState::Recording` reached) are stamped in this process
//! with [`now_epoch_millis`] at the two matching Rust-side events. All five
//! share one epoch (Unix milliseconds), so either named span is plain
//! subtraction across the JS/Rust boundary.
//!
//! Two spans are kept, and kept distinct because they answer different
//! questions: `operator_span_ms` (`t0` -> `t2`) is how long the person took
//! to read the sheet and confirm -- interesting, but not this product's
//! budget. `app_span_ms` (`t2` -> `t4`) is the app's own start latency --
//! the number design intake D2's seconds budget will govern.
//!
//! ## Validation
//!
//! A receipt is refused, not written, if the five checkpoints do not run
//! forward in order (`t0 <= t1 <= t2 <= t3 <= t4`). The two clocks involved
//! (the frontend's anchored estimate, this process's wall clock) should
//! never disagree enough to break that ordering during one start attempt on
//! one machine; when they do, something else is wrong -- a corrupted IPC
//! payload, a stalled tab, clock skew mid-attempt -- and a plausible-looking
//! but wrong number is worse than none. See `build_receipt`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use local_meeting_notes_session_core::meeting::read_private_bytes;
use local_meeting_notes_session_core::storage::durable_replace;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "capture-timing.json";

/// The receipt is a handful of integers -- nowhere near `operator_note`'s
/// runaway-writer ceiling is possible -- but a bound still means a
/// truncated or corrupted file is refused outright rather than partially
/// parsed.
const MAX_FILE_BYTES: u64 = 8 * 1024;

/// The three frontend checkpoints of one start attempt, epoch-millis,
/// computed via the clock model in the module doc. Carried into the Tauri
/// `start_meeting` command and, from there, into the spawned capture-attempt
/// thread, so the receipt can be written once -- and only once --
/// `CaptureState::Recording` is reached.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JourneyTiming {
    /// t0: the operator's start intent -- the click (or hotkey) that opened
    /// the start sheet.
    pub t0_epoch_ms: u64,
    /// t1: the start sheet rendered and interactive.
    pub t1_epoch_ms: u64,
    /// t2: consent confirmed -- the sheet's confirm action fired
    /// `start_meeting`.
    pub t2_epoch_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum TimingSchema {
    #[serde(rename = "capture-timing/1")]
    V1,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CaptureTimingReceipt {
    schema: TimingSchema,
    pub t0_epoch_ms: u64,
    pub t1_epoch_ms: u64,
    pub t2_epoch_ms: u64,
    /// t3: `start_meeting` returns to the frontend (Arming accepted).
    pub t3_epoch_ms: u64,
    /// t4: `CaptureState::Recording` is first reached.
    pub t4_epoch_ms: u64,
    /// t0 -> t2. Operator time, not the app's budget -- see the module doc.
    pub operator_span_ms: u64,
    /// t2 -> t4. The app's own start latency -- the number design intake D2
    /// governs.
    pub app_span_ms: u64,
    pub app_version: String,
}

/// Wall-clock epoch milliseconds -- this module's equivalent of the
/// codebase's existing `now_epoch_seconds` helpers, at the precision the
/// frontend's anchored marks need to be compared against.
pub fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn build_receipt(
    journey: JourneyTiming,
    t3_epoch_ms: u64,
    t4_epoch_ms: u64,
    app_version: &str,
) -> Result<CaptureTimingReceipt, String> {
    let JourneyTiming {
        t0_epoch_ms,
        t1_epoch_ms,
        t2_epoch_ms,
    } = journey;
    // Clock-model sanity, checked at the only place a receipt can be
    // created. A nonsensical ordering is refused rather than clamped or
    // reordered -- see the module doc's Validation section.
    if !(t0_epoch_ms <= t1_epoch_ms
        && t1_epoch_ms <= t2_epoch_ms
        && t2_epoch_ms <= t3_epoch_ms
        && t3_epoch_ms <= t4_epoch_ms)
    {
        return Err("capture-timing receipt rejected: checkpoints are not in order".into());
    }
    Ok(CaptureTimingReceipt {
        schema: TimingSchema::V1,
        t0_epoch_ms,
        t1_epoch_ms,
        t2_epoch_ms,
        t3_epoch_ms,
        t4_epoch_ms,
        operator_span_ms: t2_epoch_ms - t0_epoch_ms,
        app_span_ms: t4_epoch_ms - t2_epoch_ms,
        app_version: app_version.to_string(),
    })
}

/// Writes the receipt once, into `meeting_dir`. Called from exactly one
/// place: past the `CaptureState::Recording` transition in
/// `run_capture_task`. A cancelled sheet, a refused attestation, or a failed
/// arm never reaches that call, so a meeting directory without this file
/// was never measured -- see the module doc. A write failure (or a refused,
/// nonsensical receipt) must never abort or delay the recording it would
/// have described; callers treat this as best-effort and discard the error.
pub fn write(
    meeting_dir: &Path,
    journey: JourneyTiming,
    t3_epoch_ms: u64,
    t4_epoch_ms: u64,
    app_version: &str,
) -> Result<(), String> {
    let receipt = build_receipt(journey, t3_epoch_ms, t4_epoch_ms, app_version)?;
    let bytes = serde_json::to_vec(&receipt)
        .map_err(|_| "capture-timing receipt could not be encoded".to_string())?;
    durable_replace(&meeting_dir.join(FILE_NAME), &bytes)
        .map_err(|_| "capture-timing receipt could not be saved".to_string())
}

/// Reads the receipt, if one exists. `None` covers both a meeting directory
/// with no file (the attempt never reached Recording, or predates this
/// module) and one whose file could not be parsed -- both are read the same
/// way a missing measurement should be: as unmeasured, never as zero.
///
/// No command in this app calls this today -- there is deliberately no
/// rendered surface for these numbers (see the module doc); the operator
/// reads the file directly. Kept `pub` and exercised by this module's own
/// tests so the read side of the round trip stays proven.
#[allow(dead_code)]
pub fn read(meeting_dir: &Path) -> Option<CaptureTimingReceipt> {
    let path = meeting_dir.join(FILE_NAME);
    if !path.exists() {
        return None;
    }
    read_private_bytes(&path, MAX_FILE_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CaptureTimingReceipt>(&bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use local_meeting_notes_session_core::storage::create_private_dir;
    use tempfile::TempDir;

    fn meeting() -> TempDir {
        let temporary = TempDir::new().unwrap();
        create_private_dir(&temporary.path().join("meeting")).unwrap();
        temporary
    }

    fn journey() -> JourneyTiming {
        JourneyTiming {
            t0_epoch_ms: 1_000,
            t1_epoch_ms: 1_050,
            t2_epoch_ms: 1_800,
        }
    }

    #[test]
    fn a_meeting_with_no_receipt_reads_as_unmeasured() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        assert!(read(&directory).is_none());
    }

    #[test]
    fn a_receipt_round_trips_with_its_written_values() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, journey(), 1_820, 2_100, "0.5.9").unwrap();
        let found = read(&directory).expect("receipt should be readable after write");
        assert_eq!(found.t0_epoch_ms, 1_000);
        assert_eq!(found.t1_epoch_ms, 1_050);
        assert_eq!(found.t2_epoch_ms, 1_800);
        assert_eq!(found.t3_epoch_ms, 1_820);
        assert_eq!(found.t4_epoch_ms, 2_100);
        assert_eq!(found.app_version, "0.5.9");
        // One file, same as the operator-note sidecar's replace-in-place --
        // no accumulation across writes.
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn spans_are_the_named_pairs_not_adjacent_checkpoints() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, journey(), 1_820, 2_100, "0.5.9").unwrap();
        let found = read(&directory).unwrap();
        // Operator span is t0 -> t2 (click to confirm), not t0 -> t1.
        assert_eq!(found.operator_span_ms, 800);
        // App span is t2 -> t4 (confirm to Recording), not t3 -> t4.
        assert_eq!(found.app_span_ms, 300);
    }

    #[test]
    fn a_meeting_that_never_reaches_recording_has_no_receipt() {
        // Nothing here calls `write` -- the same as a cancelled sheet, a
        // refused attestation, or a failed arm, none of which reach the one
        // call site past the `CaptureState::Recording` transition. Absence
        // is the rule, not a fallback: this must never be read as a
        // zero-length start.
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        assert!(
            read(&directory).is_none(),
            "a failed or abandoned start must write nothing"
        );
    }

    #[test]
    fn a_nonsensical_ordering_is_refused_not_normalized() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        let backwards = JourneyTiming {
            t0_epoch_ms: 5_000,
            t1_epoch_ms: 5_050,
            // t2 before t1: the operator "confirmed" before the sheet
            // rendered, which cannot happen in the real journey -- refuse
            // rather than reorder or clamp.
            t2_epoch_ms: 4_000,
        };
        assert!(write(&directory, backwards, 4_100, 4_500, "0.5.9").is_err());
        assert!(
            read(&directory).is_none(),
            "a refused receipt must not partially land on disk"
        );
    }

    #[test]
    fn app_span_going_backwards_is_also_refused() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        // t4 before t3: Recording claimed to have been reached before the
        // command that arms it returned.
        assert!(write(&directory, journey(), 2_000, 1_900, "0.5.9").is_err());
        assert!(read(&directory).is_none());
    }
}
