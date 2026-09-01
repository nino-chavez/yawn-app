//! Roadmap intake W8-B: the one-week local usage probe for the dormant
//! cross-meeting exact-search commands (`preview_library_search`,
//! `preview_library_open_search_result` in `main.rs`). The exact-search
//! decision memo (recorded in `docs/roadmap.md`'s Wave 5 status) held the
//! semantic/exact search question open pending the now-landed lock exclusion
//! (roadmap intake I5 / W5-B), then recommended exactly this: let the
//! operator observe a week of their own usage of the already-built, already-
//! hardened commands before deciding whether to ship them for real.
//!
//! This module is the entire probe mechanism. It is deliberately small:
//!
//! - **A marker file turns the probe on.** `<storage_root>/search-probe.flag`
//!   is a hand-created, hand-deleted file with no content the app reads.
//!   Absence is exactly today's behavior, byte for byte -- both commands
//!   refuse and the frontend renders no new UI at all.
//! - **An append-only log is the probe's only output.**
//!   `<storage_root>/search-probe-log.jsonl` gets one line per event: `{
//!   "ts_epoch_ms": ..., "event": "invoked" }` when a transcript search runs,
//!   and `{ "ts_epoch_ms": ..., "event": "opened" }` when a hit is opened.
//!   Nothing else ever goes in this file -- no query text, no meeting id, no
//!   hit count. The operator reads it by hand after the week; nothing in the
//!   product surfaces it.
//!
//! # Why the flag is read fresh on every call, not cached
//!
//! The whole point of a hand-toggled marker is that the operator can turn the
//! probe on or off mid-session without restarting Yawn. Caching the flag at
//! startup (or behind any interior-mutability memo) would make "delete the
//! file" silently do nothing until the next launch -- exactly the kind of
//! quiet mismatch between what the operator did and what the app does that
//! this whole codebase treats as a bug. A `Path::is_file` stat is cheap next
//! to the storage-backed library rebuild every one of these call sites already
//! does, so reading fresh costs nothing measurable and buys an honest,
//! immediate toggle.
//!
//! # Why the log write is best-effort
//!
//! The log is a diagnostic the probe keeps for the operator, not a capability
//! the product depends on. A write failure here (a full disk, a permissions
//! problem) must never surface as a search or open failure, and must never be
//! retried into a corrupt line -- it is simply dropped.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use local_meeting_notes_session_core::storage::StorageRoot;

/// Hand-created, hand-deleted by the operator. Its presence is the only
/// switch; the app never writes it.
pub(crate) const FLAG_FILE: &str = "search-probe.flag";

/// Append-only. The app only ever appends a line; it never reads, rewrites,
/// or truncates this file.
pub(crate) const LOG_FILE: &str = "search-probe-log.jsonl";

/// The refusal a gated command hands back while the flag is absent. Quiet and
/// honest: it names what is off, not why, and does not describe the probe.
pub(crate) const REFUSAL_MESSAGE: &str = "Search across meetings is not enabled.";

/// Whether the operator's local probe marker exists right now.
///
/// Read fresh on every call -- see the module doc for why this must not be
/// cached.
pub(crate) fn enabled(storage: &StorageRoot) -> bool {
    storage.path().join(FLAG_FILE).is_file()
}

/// The two events this probe ever records. Deliberately just these two: there
/// is no "result count" or "query length" variant, because the log's whole
/// value is that it cannot grow into a query-content leak by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeEvent {
    Invoked,
    Opened,
}

impl ProbeEvent {
    fn as_str(self) -> &'static str {
        match self {
            ProbeEvent::Invoked => "invoked",
            ProbeEvent::Opened => "opened",
        }
    }
}

/// Appends one `{ts_epoch_ms, event}` line to the probe log. Best-effort: see
/// the module doc for why a write failure here is swallowed rather than
/// propagated.
pub(crate) fn record(storage: &StorageRoot, event: ProbeEvent) {
    let ts_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    // Hand-built rather than routed through serde_json: the schema is two
    // fixed fields, and hand-building here keeps the "never a third field"
    // guarantee visible at the call site instead of behind a struct someone
    // could quietly extend later.
    let line = format!(
        "{{\"ts_epoch_ms\":{ts_epoch_ms},\"event\":\"{}\"}}\n",
        event.as_str()
    );
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(storage.path().join(LOG_FILE))
    else {
        return;
    };
    let _ = file.write_all(line.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::fs;

    use local_meeting_notes_session_core::storage::{StorageRoot, create_private_dir};
    use tempfile::TempDir;

    use super::*;

    fn test_storage() -> (TempDir, StorageRoot) {
        let temporary = TempDir::new().unwrap();
        let repository = temporary.path().join("repository");
        create_private_dir(&repository).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &repository).unwrap();
        (temporary, storage)
    }

    #[test]
    fn absent_marker_reads_disabled_and_present_marker_reads_enabled() {
        let (_temporary, storage) = test_storage();
        assert!(!enabled(&storage));

        fs::write(storage.path().join(FLAG_FILE), b"").unwrap();
        assert!(enabled(&storage));

        fs::remove_file(storage.path().join(FLAG_FILE)).unwrap();
        assert!(!enabled(&storage), "deleting the marker must disable the probe on the very next read");
    }

    #[test]
    fn record_appends_one_line_per_event_with_only_the_two_named_fields() {
        let (_temporary, storage) = test_storage();
        record(&storage, ProbeEvent::Invoked);
        record(&storage, ProbeEvent::Opened);

        let contents = fs::read_to_string(storage.path().join(LOG_FILE)).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        for (line, expected_event) in lines.iter().zip(["invoked", "opened"]) {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            let object = parsed.as_object().expect("one JSON object per line");
            assert_eq!(object.len(), 2, "line must carry only ts_epoch_ms and event: {line}");
            assert!(object.get("ts_epoch_ms").is_some_and(|value| value.is_u64()));
            assert_eq!(object.get("event").and_then(|value| value.as_str()), Some(expected_event));
        }
    }

    #[test]
    fn record_never_writes_query_text_meeting_ids_or_counts() {
        let (_temporary, storage) = test_storage();
        record(&storage, ProbeEvent::Invoked);
        let contents = fs::read_to_string(storage.path().join(LOG_FILE)).unwrap();
        for forbidden in ["query", "meeting_id", "meetingId", "count", "hits", "total"] {
            assert!(
                !contents.contains(forbidden),
                "log line must never mention {forbidden}: {contents}"
            );
        }
    }

    #[test]
    fn a_missing_storage_directory_does_not_panic_the_caller() {
        // Best-effort: an unwritable log location must not be observable by
        // the caller. This deletes the storage root out from under the
        // recorder to exercise the failure path deterministically.
        let (_temporary, storage) = test_storage();
        fs::remove_dir_all(storage.path()).unwrap();
        record(&storage, ProbeEvent::Invoked);
    }
}
