//! Per-meeting lock: a local-access barrier over one meeting's note,
//! transcript, personal notes, context, retained audio, and export.
//!
//! Roadmap intake I5, governing constraint verbatim: "State the honest claim —
//! a local-access deterrent — unless encryption at rest actually ships."
//! Encryption at rest is not in this packet, so nothing here — in a doc
//! comment, an error string, or a rendered sentence — may call a locked
//! meeting encrypted, secure, or protected. It is a barrier in front of the
//! app's own read paths. The files on disk are exactly what they were.
//!
//! **The deterrent's honest ceiling.** This sidecar is a plain JSON file in the
//! meeting's own directory. Anyone with a Finder window and this Mac's login
//! can delete it, and the meeting opens. That is the whole shape of the claim:
//! it stops a passer-by holding an unlocked Mac, and it does not stop anyone
//! who reaches the filesystem. Bear's per-note lock and Standard Notes'
//! action-scoped re-auth are the design sources, and Standard Notes' own
//! "deterrent, not cryptography" framing is the one being copied here.
//!
//! **This mirrors `operator_note.rs` and `meeting_context.rs` for storage.**
//! Lock state is a fixed path in the meeting directory, replaced atomically,
//! rather than a field threaded through `meeting.json` — the frozen
//! `meeting/2` contract stays untouched, so a locked meeting stays readable by
//! a build that predates this module. **A build that predates this module also
//! ignores the lock entirely.** That is not a defect this packet can fix; it
//! is the same fact as the paragraph above, restated for old binaries.
//!
//! Where this deliberately differs from those two: an unreadable sidecar here
//! is **not** protected from replacement. Both of those modules refuse to
//! overwrite a file they could not read, because the file holds operator
//! content that a blind write would destroy. This file holds one boolean and
//! no operator content, so there is nothing to lose — and refusing the write
//! would strand a meeting whose lock file corrupted, permanently, with no
//! in-app way out. An unreadable file instead reads as *locked* (below), which
//! fails closed on the read side where it matters.

use std::collections::HashMap;
use std::path::Path;

use local_meeting_notes_session_core::meeting::read_private_bytes;
use local_meeting_notes_session_core::storage::durable_replace;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The read bound. One boolean and its schema tag never approach this; the
/// margin exists so a hand-edited or partly-written file is rejected by the
/// parser rather than by the reader refusing to look at it.
const MAX_FILE_BYTES: u64 = 4 * 1024;

const FILE_NAME: &str = "meeting-lock.json";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum LockSchema {
    #[serde(rename = "meeting-lock/1")]
    V1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLock {
    schema: LockSchema,
    locked: bool,
}

/// What every surface and every gate is told about one meeting's lock.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeetingLock {
    /// True when this meeting is locked, **including** when a lock file exists
    /// that this build could not read.
    ///
    /// Failing closed is the only safe direction: if an unreadable file read
    /// as unlocked, corrupting one JSON byte would be a bypass, and the gate
    /// would be defeated by the least deliberate act available. The recovery
    /// path is not "read it as open" — it is `write`, which still runs the
    /// confirmation check before it clears the flag.
    pub(crate) locked: bool,
    /// True when a lock file exists but could not be read or parsed.
    ///
    /// Reported separately from `locked` because the two lead a reader to
    /// different sentences: "this is locked" and "this is locked and Yawn
    /// cannot read why". It changes no gate decision — `locked` is already
    /// true — and exists so the shell can say the second thing honestly.
    pub(crate) unreadable: bool,
}

impl MeetingLock {
    /// Nothing to report: no meeting resolved, so neither a lock nor a failure
    /// to read one. Mirrors `OperatorNote::none()` and
    /// `MeetingContext::none()`, and reads identically to a real meeting that
    /// was never locked.
    pub(crate) fn none() -> Self {
        Self {
            locked: false,
            unreadable: false,
        }
    }
}

pub(crate) fn read(meeting_dir: &Path) -> MeetingLock {
    let path = meeting_dir.join(FILE_NAME);
    if !path.exists() {
        return MeetingLock::none();
    }
    // A symlink here is not lock state. `read_private_bytes` enforces the rest
    // of the safe-file predicate; this module adds no exception to it.
    match read_private_bytes(&path, MAX_FILE_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredLock>(&bytes).ok())
    {
        Some(stored) => MeetingLock {
            locked: stored.locked,
            unreadable: false,
        },
        None => MeetingLock {
            locked: true,
            unreadable: true,
        },
    }
}

/// Writes the lock flag, replacing whatever was there.
///
/// This function does **not** authenticate. Locking needs no confirmation, and
/// unlocking's confirmation is run by its command before it calls here — see
/// `main.rs`'s `unlock_meeting`. Keeping the check out of the writer is
/// deliberate: the confirmation blocks on a human for as long as they take,
/// and this call runs under the app's storage sequence.
pub(crate) fn write(meeting_dir: &Path, locked: bool) -> Result<(), String> {
    let bytes = serde_json::to_vec(&StoredLock {
        schema: LockSchema::V1,
        locked,
    })
    .map_err(|_| "That lock could not be saved.".to_string())?;
    // Atomic replace: a crash mid-write leaves the previous state, never a
    // half-written file. A half-written file would read as locked anyway.
    durable_replace(&meeting_dir.join(FILE_NAME), &bytes)
        .map_err(|_| "That lock could not be saved.".to_string())
}

/// The three things a locked meeting refuses without a fresh confirmation.
///
/// A closed enum rather than a string so a token minted to open a meeting for
/// reading can never be spent on an export, and neither can be spent on
/// playback. Standard Notes' action-scoped re-auth is the source: the scope of
/// one confirmation is one action, not a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LockedAction {
    /// Render this meeting's note, transcript, personal notes, and context.
    Open,
    /// Write this meeting's plain files and archive into its own directory.
    Export,
    /// Play one retained recording through the fixed native player.
    Playback,
}

impl LockedAction {
    /// The wire name the shell asks for, and the only names it may ask for.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "export" => Some(Self::Export),
            "playback" => Some(Self::Playback),
            _ => None,
        }
    }

    /// What the operator is told they are confirming. macOS renders this
    /// under the Touch ID prompt, so it names the act and the meeting's
    /// locality, never the meeting's title or any of its content.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Open => "open a locked meeting on this Mac",
            Self::Export => "export a locked meeting on this Mac",
            Self::Playback => "play a locked meeting's saved audio on this Mac",
        }
    }
}

/// **The one gate decision, made in exactly one place.**
///
/// Every refusal in this packet is this function returning `false`: the note
/// response, the export command, the playback authorization, and the
/// transcript-file open all call it and none of them re-derives the rule. The
/// shell hides controls for a locked meeting, but hiding is presentation —
/// this is the gate, and a webview that invents a handle and calls the command
/// directly meets it just the same.
///
/// `authorized_meeting_id` is whatever `LockedActionAuthority::consume` just
/// returned for this exact action: `Some(id)` when a fresh, unspent, correctly
/// scoped confirmation token was presented, `None` otherwise. Comparing it to
/// the meeting actually resolved from the handle is what stops a token minted
/// for one meeting from opening another.
pub(crate) fn permits(
    lock: MeetingLock,
    authorized_meeting_id: Option<&str>,
    meeting_id: &str,
) -> bool {
    if !lock.locked {
        // Decision 3: an unlocked meeting takes no friction on the default
        // path. Export and playback behave exactly as they did before this
        // module existed.
        return true;
    }
    authorized_meeting_id == Some(meeting_id)
}

/// One outstanding confirmation, minted for one action on one meeting and
/// spent by exactly one command.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Outstanding {
    token: String,
    meeting_id: String,
}

/// The app's single-use confirmation tokens: **at most one per action**.
///
/// Modelled on the retained-audio playback handle — an opaque identifier the
/// webview holds, whose meaning lives entirely on the native side, valid for
/// one immediate request. It differs in one way that matters: there is no
/// expiry and no timer to manage. A token dies when it is spent, when another
/// is minted for the same action, when the meeting is locked, or when the app
/// closes. Nothing here reads a clock, so nothing here has one to get wrong.
///
/// **Why one slot per action rather than one slot total.** The three actions
/// interleave in ordinary use: a reader confirms to open a locked meeting,
/// then confirms again to play its audio, and then the view refreshes itself.
/// A single slot would let the playback confirmation silently invalidate the
/// reading one, and the refresh would land the reader back at the lock screen
/// having done nothing wrong. Three slots keep each confirmation answering
/// only for the thing it was given for — which is the action-scoping rule,
/// not a relaxation of it. None of them is a session grant: each is spent by
/// one call, and none authorizes anything but its own action on its own
/// meeting.
#[derive(Debug, Default)]
pub(crate) struct LockedActionAuthority {
    outstanding: HashMap<LockedAction, Outstanding>,
}

impl LockedActionAuthority {
    /// Mints a token for one action on one meeting, discarding whatever token
    /// that same action was holding.
    pub(crate) fn mint(&mut self, meeting_id: &str, action: LockedAction) -> String {
        let token = Uuid::new_v4().to_string();
        self.outstanding.insert(
            action,
            Outstanding {
                token: token.clone(),
                meeting_id: meeting_id.to_owned(),
            },
        );
        token
    }

    /// Spends `token` for `action`, returning the meeting it authorizes.
    ///
    /// A mismatch — no token presented, an unknown token, a token minted for a
    /// different action, one already spent — returns `None` and **spends
    /// nothing**. A failed export must not burn the confirmation the operator
    /// gave for playback. A match consumes: the same token presented twice
    /// returns `None` the second time.
    pub(crate) fn consume(&mut self, token: Option<&str>, action: LockedAction) -> Option<String> {
        let token = token?;
        if self.outstanding.get(&action)?.token != token {
            return None;
        }
        self.outstanding
            .remove(&action)
            .map(|spent| spent.meeting_id)
    }

    /// Drops every outstanding confirmation. Called when a meeting is locked,
    /// beside the library invalidation that drops its stale capabilities.
    pub(crate) fn revoke_all(&mut self) {
        self.outstanding.clear();
    }
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

    #[test]
    fn none_matches_what_a_meeting_that_was_never_locked_reads() {
        // Every response falls back to `none()` when a meeting cannot be
        // resolved. It must read identically to a resolved unlocked meeting,
        // or a reader could tell "no meeting" and "not locked" apart from
        // this field alone.
        let temporary = meeting();
        assert_eq!(MeetingLock::none(), read(&temporary.path().join("meeting")));
    }

    #[test]
    fn a_meeting_with_no_lock_file_reads_unlocked_and_not_unreadable() {
        let temporary = meeting();
        assert_eq!(
            read(&temporary.path().join("meeting")),
            MeetingLock {
                locked: false,
                unreadable: false,
            }
        );
    }

    #[test]
    fn lock_round_trips_and_replaces_in_place() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, true).unwrap();
        assert!(read(&directory).locked);
        write(&directory, false).unwrap();
        assert!(!read(&directory).locked);
        // Replacement, not accumulation: one file, and no previous state left
        // beside it under another name for a later read to disagree with.
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn an_unreadable_lock_file_reads_locked_rather_than_open() {
        // The whole point of failing closed. If this returned unlocked, one
        // corrupt byte would be the bypass.
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(directory.join(FILE_NAME), b"{ not a lock").unwrap();
        assert_eq!(
            read(&directory),
            MeetingLock {
                locked: true,
                unreadable: true,
            }
        );
    }

    #[test]
    fn a_lock_carrying_an_unknown_field_reads_locked_rather_than_partly_read() {
        // `deny_unknown_fields`: lock state written by a later build carrying
        // a field this one does not understand is unreadable, and unreadable
        // is locked. Failing closed keeps this build from ignoring whatever
        // that field constrained.
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"meeting-lock/1","locked":false,"until":0}"#,
        )
        .unwrap();
        assert!(read(&directory).locked);
        assert!(read(&directory).unreadable);
    }

    #[test]
    fn a_lock_carrying_an_unknown_schema_reads_locked() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"meeting-lock/2","locked":false}"#,
        )
        .unwrap();
        assert!(read(&directory).locked);
    }

    #[test]
    fn an_unreadable_lock_file_can_still_be_replaced() {
        // The deliberate divergence from `operator_note` and
        // `meeting_context`. Those refuse to overwrite what they could not
        // read because the bytes are the operator's own words. This file
        // holds one boolean, so refusing would only strand the meeting.
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(directory.join(FILE_NAME), b"{ not a lock").unwrap();
        assert!(write(&directory, false).is_ok());
        assert_eq!(read(&directory), MeetingLock::none());
    }

    #[test]
    fn the_sidecar_survives_the_directory_rename_that_trash_and_restore_use() {
        // `meeting_trash` moves a whole meeting directory with `fs::rename`
        // in both directions (`trash_meeting`, `restore_meeting`), so lock
        // state rides along with every other file rather than needing its own
        // migration. This proves the property that carries the round trip; it
        // is not a test of the trash module, whose own suite owns the rename.
        let temporary = meeting();
        let live = temporary.path().join("meeting");
        write(&live, true).unwrap();
        let trashed = temporary.path().join("trashed");
        std::fs::rename(&live, &trashed).unwrap();
        assert!(read(&trashed).locked, "a trashed meeting stays locked");
        std::fs::rename(&trashed, &live).unwrap();
        assert!(read(&live).locked, "a restored meeting comes back locked");
    }

    #[test]
    fn deleting_a_meetings_audio_leaves_its_lock_exactly_as_it_was() {
        // Retention and manual audio deletion remove artifact files from the
        // meeting directory and never touch this sidecar. The privacy promise
        // outranks the lock: audio release proceeds on a locked meeting, and
        // the meeting stays locked afterwards.
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, true).unwrap();
        let audio = directory.join("microphone.wav");
        std::fs::write(&audio, b"audio bytes").unwrap();
        std::fs::remove_file(&audio).unwrap();
        assert!(read(&directory).locked);
    }

    #[test]
    fn an_unlocked_meeting_permits_every_action_with_no_token() {
        // Decision 3 stated as a test: the default path takes no friction.
        for action in [
            LockedAction::Open,
            LockedAction::Export,
            LockedAction::Playback,
        ] {
            let _ = action;
            assert!(permits(MeetingLock::none(), None, "meeting-a"));
        }
    }

    #[test]
    fn a_locked_meeting_refuses_without_an_authorization() {
        let locked = MeetingLock {
            locked: true,
            unreadable: false,
        };
        assert!(!permits(locked, None, "meeting-a"));
    }

    #[test]
    fn a_locked_meeting_refuses_an_authorization_minted_for_another_meeting() {
        let locked = MeetingLock {
            locked: true,
            unreadable: false,
        };
        assert!(!permits(locked, Some("meeting-b"), "meeting-a"));
        assert!(permits(locked, Some("meeting-a"), "meeting-a"));
    }

    #[test]
    fn an_unreadable_lock_gates_exactly_like_a_readable_one() {
        let unreadable = MeetingLock {
            locked: true,
            unreadable: true,
        };
        assert!(!permits(unreadable, None, "meeting-a"));
        assert!(permits(unreadable, Some("meeting-a"), "meeting-a"));
    }

    #[test]
    fn a_minted_token_authorizes_its_own_meeting_and_action_once() {
        let mut authority = LockedActionAuthority::default();
        let token = authority.mint("meeting-a", LockedAction::Export);
        assert_eq!(
            authority.consume(Some(&token), LockedAction::Export),
            Some("meeting-a".to_owned())
        );
        // Spent exactly once: the reused token is worth nothing.
        assert_eq!(authority.consume(Some(&token), LockedAction::Export), None);
    }

    #[test]
    fn a_token_minted_for_one_action_cannot_be_spent_on_another() {
        // The action-scoping rule. Confirming to open a meeting for reading
        // must not also authorize exporting it, which is the act that puts
        // its contents somewhere else.
        let mut authority = LockedActionAuthority::default();
        let token = authority.mint("meeting-a", LockedAction::Open);
        assert_eq!(authority.consume(Some(&token), LockedAction::Export), None);
        assert_eq!(
            authority.consume(Some(&token), LockedAction::Playback),
            None
        );
        // And the mismatch spent nothing, so the confirmation the operator
        // actually gave is still good for what they gave it for.
        assert_eq!(
            authority.consume(Some(&token), LockedAction::Open),
            Some("meeting-a".to_owned())
        );
    }

    #[test]
    fn no_token_and_an_unknown_token_both_authorize_nothing() {
        let mut authority = LockedActionAuthority::default();
        assert_eq!(authority.consume(None, LockedAction::Open), None);
        assert_eq!(
            authority.consume(Some("not-a-token"), LockedAction::Open),
            None
        );
        let token = authority.mint("meeting-a", LockedAction::Open);
        assert_eq!(
            authority.consume(Some(&Uuid::new_v4().to_string()), LockedAction::Open),
            None
        );
        // Guessing wrong did not spend the real one.
        assert_eq!(
            authority.consume(Some(&token), LockedAction::Open),
            Some("meeting-a".to_owned())
        );
    }

    #[test]
    fn minting_for_one_action_discards_only_that_actions_token() {
        // Confirming to play audio must not quietly invalidate the
        // confirmation that is holding the meeting open to read.
        let mut authority = LockedActionAuthority::default();
        let reading = authority.mint("meeting-a", LockedAction::Open);
        let playback = authority.mint("meeting-a", LockedAction::Playback);
        assert_eq!(
            authority.consume(Some(&playback), LockedAction::Playback),
            Some("meeting-a".to_owned())
        );
        assert_eq!(
            authority.consume(Some(&reading), LockedAction::Open),
            Some("meeting-a".to_owned())
        );
    }

    #[test]
    fn minting_twice_for_the_same_action_leaves_only_the_newer_token() {
        // Re-issuing is how a read that succeeded hands the next read its
        // authority. The one it replaces must be worthless, or the reader
        // accumulates spendable confirmations.
        let mut authority = LockedActionAuthority::default();
        let first = authority.mint("meeting-a", LockedAction::Open);
        let second = authority.mint("meeting-a", LockedAction::Open);
        assert_eq!(authority.consume(Some(&first), LockedAction::Open), None);
        assert_eq!(
            authority.consume(Some(&second), LockedAction::Open),
            Some("meeting-a".to_owned())
        );
    }

    #[test]
    fn re_issuing_for_a_different_meeting_replaces_the_previous_meetings_token() {
        // Opening meeting B must not leave a live reading confirmation for
        // meeting A behind it.
        let mut authority = LockedActionAuthority::default();
        let first = authority.mint("meeting-a", LockedAction::Open);
        let second = authority.mint("meeting-b", LockedAction::Open);
        assert_eq!(authority.consume(Some(&first), LockedAction::Open), None);
        assert_eq!(
            authority.consume(Some(&second), LockedAction::Open),
            Some("meeting-b".to_owned())
        );
    }

    #[test]
    fn locking_a_meeting_revokes_every_outstanding_confirmation() {
        let mut authority = LockedActionAuthority::default();
        let token = authority.mint("meeting-a", LockedAction::Export);
        authority.revoke_all();
        assert_eq!(authority.consume(Some(&token), LockedAction::Export), None);
    }

    #[test]
    fn only_the_three_named_actions_parse() {
        assert_eq!(LockedAction::parse("open"), Some(LockedAction::Open));
        assert_eq!(LockedAction::parse("export"), Some(LockedAction::Export));
        assert_eq!(
            LockedAction::parse("playback"),
            Some(LockedAction::Playback)
        );
        for unknown in ["", "Open", "delete", "unlock", "all"] {
            assert_eq!(LockedAction::parse(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn no_confirmation_reason_names_a_meeting_or_claims_encryption() {
        // The prompt macOS renders is product copy. It must not leak a
        // meeting's title and must not claim what this packet does not do.
        for action in [
            LockedAction::Open,
            LockedAction::Export,
            LockedAction::Playback,
        ] {
            let reason = action.reason();
            assert!(reason.contains("on this Mac"), "{reason}");
            for forbidden in ["encrypt", "secure", "protected"] {
                assert!(!reason.contains(forbidden), "{reason}");
            }
        }
    }
}
