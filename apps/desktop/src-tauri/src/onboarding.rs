//! Roadmap packet W10: the once-only first-run sheet (product brief,
//! "A first run must teach without counterfeiting," amended 2026-09-01). The
//! sheet explains the three moments -- before, during, after -- in the app's
//! own words the first time the operator lands on a ready Home with zero
//! meetings. It must never show again after dismissal, and must never show
//! for an operator who already has meetings (an upgrading operator is not a
//! stranger to the app).
//!
//! This module is the entire persistence mechanism, deliberately small and
//! shaped after `search_probe.rs`'s marker-file idiom:
//!
//! - **A marker file is the only state.** `<storage_root>/first-run-seen.flag`
//!   absent means the sheet has never been dismissed; present means it has.
//!   Unlike `search_probe`'s flag, the operator never hand-creates this one --
//!   the app writes it the moment "Got it" is clicked -- but the read side is
//!   the same shape: a cheap, uncached existence check.
//! - **The frontend decides *whether* to show it.** This module only answers
//!   "has it been seen" and "mark it seen." Whether zero meetings exist is a
//!   library-snapshot fact the Rust side already computes elsewhere
//!   (`library_reader::LibrarySnapshot::total`); duplicating that check here
//!   would be a second source of truth for the same fact.
//!
//! # Why the flag is read fresh, not cached
//!
//! Same reasoning as `search_probe::enabled`: caching this behind any
//! interior-mutability memo would let a stale in-memory `false` re-show the
//! sheet after a dismissal write that already landed on disk, or (worse)
//! survive across the length of one process's lifetime by construction. A
//! `Path::is_file` stat is cheap next to the storage-backed library rebuild
//! that already happens on every `library_snapshot` call.
//!
//! # Why marking seen is idempotent and best-effort
//!
//! `mark_seen` is called exactly once per operator (the "Got it" click), but
//! nothing prevents a retry or a double-invoke from the frontend, so the
//! write must not error on an already-present file. And, matching
//! `search_probe::record`, a failure to write here (full disk, permissions)
//! must never surface as an error toast over what is, at most, a one-time
//! inconvenience: the sheet reappearing on a later cold boot. It is silently
//! best-effort for the same reason the search-probe log write is.

use std::fs;

use local_meeting_notes_session_core::storage::StorageRoot;

/// Written once, on dismissal. Presence means the sheet has already run its
/// course for this operator; absence means it has not.
pub(crate) const FLAG_FILE: &str = "first-run-seen.flag";

/// Whether the operator has already dismissed the first-run sheet.
///
/// Read fresh on every call -- see the module doc for why this must not be
/// cached.
pub(crate) fn seen(storage: &StorageRoot) -> bool {
    storage.path().join(FLAG_FILE).is_file()
}

/// Records that the sheet has been dismissed. Idempotent: writing the marker
/// a second time is not an error. Best-effort by design -- see the module
/// doc for why a write failure here must stay silent.
pub(crate) fn mark_seen(storage: &StorageRoot) {
    let _ = fs::write(storage.path().join(FLAG_FILE), b"");
}

#[cfg(test)]
mod tests {
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
    fn unseen_by_default() {
        let (_temporary, storage) = test_storage();
        assert!(!seen(&storage));
    }

    #[test]
    fn mark_seen_persists_across_a_fresh_read() {
        // "Across a fresh read" stands in for "across a restart": nothing in
        // this module holds process-lifetime state, so a new `seen()` call
        // against the same storage root is exactly what a relaunch observes.
        let (_temporary, storage) = test_storage();
        mark_seen(&storage);
        assert!(seen(&storage), "the marker must persist across a fresh read of the same storage root");
    }

    #[test]
    fn marking_seen_twice_does_not_error_or_change_the_outcome() {
        let (_temporary, storage) = test_storage();
        mark_seen(&storage);
        mark_seen(&storage);
        assert!(seen(&storage));
    }

    #[test]
    fn a_missing_storage_directory_does_not_panic_the_caller() {
        // Best-effort: an unwritable location must not be observable by the
        // caller. This deletes the storage root out from under the writer to
        // exercise the failure path deterministically.
        let (_temporary, storage) = test_storage();
        fs::remove_dir_all(storage.path()).unwrap();
        mark_seen(&storage);
    }
}
