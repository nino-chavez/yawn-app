//! Private, read-only DTO adapter for an already-built meeting-library projection.
//!
//! This is deliberately not a Tauri command, state container, or storage builder.
//! Its only authority is to retain opaque projection handles long enough to reopen
//! the exact current claim or locator through `LibraryProjection`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use local_meeting_notes_session_core::corpus_index::CorpusIndex;
use local_meeting_notes_session_core::library_read::FolderFilter;
use local_meeting_notes_session_core::library_read::{
    ClaimEvidenceState, LibraryFilter, LibraryHit, LibraryProjection, LibraryReadError, LibraryRow,
    OpenedClaimLocator, OpenedLibraryHit, ReadLimits,
};
use local_meeting_notes_session_core::meeting::{
    ArtifactRef, AudioRetentionRule, AudioState, MeetingLifecycle, load_meeting, open_private_file,
    resolve_artifact, verify_record_artifacts, verify_record_static_artifacts,
};
use local_meeting_notes_session_core::meeting_title;
use local_meeting_notes_session_core::note_projection::{ClaimType, NoteProjector, UnavailableProjector};
use local_meeting_notes_session_core::retention::meeting_dir;
use local_meeting_notes_session_core::storage::StorageRoot;
use local_meeting_notes_session_core::transcript_deletion::transcript_deletion_completed;
use crate::meeting_lock::MeetingLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const STALE_MESSAGE: &str = "That view is no longer current. Reopen it and try again.";
const UNAVAILABLE_MESSAGE: &str = "The local library is unavailable. Reopen the app and try again.";
/// Roadmap intake I5. Said the same way everywhere a locked meeting refuses,
/// and deliberately not a security claim: the meeting is behind a local
/// barrier that a confirmation lifts, and the sentence says exactly that.
pub(crate) const LOCKED_MESSAGE: &str =
    "This meeting is locked on this Mac. Confirm it's you to continue.";

/// Brings the derived corpus index up to date beside the projection that just
/// validated the files, and **never lets its failure reach the operator**.
///
/// The index is a cache. If it cannot be opened or written — a widened mode, a
/// schema from a newer build, a full disk — the library must still work exactly
/// as it did before the index existed, so every error is dropped here. The
/// return value is discarded deliberately; a failed sync is not a degraded
/// library, and no diagnostic is emitted because the only content that
/// distinguishes these failures is private.
///
/// `sync_if_changed_excluding` makes this affordable on a path the app walks
/// whenever the library is opened or invalidated: an unchanged corpus costs one
/// hash over the projection's row identities and one `SELECT`, and writes
/// nothing.
///
/// Roadmap intake I5's honest-ceiling follow-up: a locked meeting's derived
/// content must not sit in this index outside the meeting's own directory, so
/// `locked_meeting_ids` below is threaded straight into the exclusion the index
/// already has a hook for. `lock_meeting` invalidates the library reader
/// (`with_preview_library_invalidated` in `main.rs`), and this function runs
/// again the next time anything rebuilds it — that is the whole hook: no
/// separate "remove from corpus" call exists or is needed, because
/// `sync_if_changed_excluding`'s digest already changes the moment a meeting
/// crosses into or out of the exclusion set, forcing the real write that drops
/// or restores its rows. Unlocking needs no push either: the next rebuild
/// simply stops excluding it and the meeting is re-admitted.
fn sync_corpus_index(storage: &StorageRoot, projection: &LibraryProjection) {
    if let Ok(mut index) = CorpusIndex::open(storage) {
        let locked_meeting_ids = locked_meeting_ids(storage, projection);
        let _ = index.sync_if_changed_excluding(projection, &locked_meeting_ids);
    }
}

/// Every meeting in `projection` whose lock reads locked right now, by the
/// same fail-closed rule every other locked-meeting gate in this file uses: an
/// unreadable lock sidecar is `meeting_lock::read`'s own `locked: true`, so it
/// reaches this set exactly like a readable one. A meeting whose directory
/// cannot be resolved at all is left out rather than excluded — the row came
/// from a projection that already resolved it once, so this is not the fail
/// path the honest-ceiling paragraph is about, and treating an unresolvable
/// directory as "locked" here would be inventing a lock state no file states.
fn locked_meeting_ids(storage: &StorageRoot, projection: &LibraryProjection) -> HashSet<String> {
    projection
        .rows()
        .iter()
        .filter(|row| {
            meeting_dir(storage, &row.meeting_id)
                .map(|directory| crate::meeting_lock::read(&directory).locked)
                .unwrap_or(false)
        })
        .map(|row| row.meeting_id.clone())
        .collect()
}

/// Owns only opaque handles into one immutable `LibraryProjection` snapshot.
pub(crate) struct LibraryReader {
    storage: StorageRoot,
    projection: LibraryProjection,
    excluded_meeting_ids: HashSet<String>,
    handles: HashMap<String, LibraryHit>,
    operator_note_handles: HashMap<String, LibraryHit>,
    audio_deletion_handles: HashMap<String, LibraryHit>,
    audio_playback_handles: HashMap<String, RetainedAudioPlaybackHandle>,
    transcript_deletion_handles: HashMap<String, LibraryHit>,
    meeting_deletion_handles: HashMap<String, LibraryHit>,
}

/// The filter as the shell states it, before it becomes a `LibraryFilter`.
///
/// Every field is optional and absent means "no constraint", so a surface that
/// has chosen nothing sends nothing and sees the whole library. `unfiled` is
/// separate from `folderId` because "any folder" and "no folder" are different
/// questions that one nullable identifier cannot ask.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LibraryFilterArgs {
    pub(crate) folder_id: Option<String>,
    pub(crate) unfiled: Option<bool>,
    pub(crate) start_epoch_seconds: Option<u64>,
    pub(crate) end_epoch_seconds: Option<u64>,
    pub(crate) title: Option<String>,
}

impl LibraryFilterArgs {
    pub(crate) fn to_filter(&self) -> LibraryFilter {
        LibraryFilter {
            // `unfiled` wins when both arrive. They contradict each other, and
            // resolving a contradiction by taking the narrower reading is
            // safer than showing more than was asked for.
            folder: match (self.unfiled, self.folder_id.as_deref()) {
                (Some(true), _) => FolderFilter::Unfiled,
                (_, Some(id)) => FolderFilter::Named(id.to_owned()),
                _ => FolderFilter::Any,
            },
            start_epoch_seconds: self.start_epoch_seconds,
            end_epoch_seconds: self.end_epoch_seconds,
            // An empty or whitespace-only box has not asked a question.
            title: self
                .title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySnapshot {
    pub(crate) state: &'static str,
    pub(crate) rows: Vec<LibrarySnapshotRow>,
    pub(crate) unavailable_count: usize,
    /// The organization revision these rows were read at, or null when the
    /// record is unreadable.
    ///
    /// Every mutation carries it back as `expected_revision` and refuses on
    /// mismatch, so a surface that cannot state which revision it is editing
    /// cannot edit. Null therefore disables renaming rather than defaulting to
    /// zero — writing over a record you could not read is the one outcome the
    /// conflict check exists to prevent.
    pub(crate) metadata_revision: Option<u64>,
    /// Every folder the operator has, whether or not any meeting is in it.
    pub(crate) folders: Vec<LibraryFolder>,
    /// Meetings this snapshot could show, before the filter.
    pub(crate) total: usize,
    /// Meetings it is showing. Equal to `total` when nothing is filtered.
    ///
    /// Two numbers rather than one, because "a shell that never lies" is a
    /// shipped property and a list quietly showing 3 of 40 breaks it. The
    /// surface renders "showing N of M" from these and offers a way back.
    pub(crate) shown: usize,
    pub(crate) filter_active: bool,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryFolder {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySnapshotRow {
    pub(crate) handle: String,
    pub(crate) meeting_id: String,
    /// Null when the meeting has neither an operator title nor a transcript to
    /// take one from. The shell falls back to `created_at_epoch_seconds`, which
    /// it already renders in the operator's own locale — a UTC copy of the same
    /// instant sent from here would be a duplicate, not a label.
    pub(crate) label: Option<String>,
    /// Which source produced `label`: `operator`, `derived`, or `date` when
    /// there is none.
    ///
    /// The shell needs it because one of the three is something the operator
    /// wrote and the others are not, and a list that renders them identically
    /// tells them they named a meeting when they did not.
    pub(crate) label_source: &'static str,
    /// Which folder this meeting is in, or null for unfiled. The identifier
    /// rather than the name, because two folders may share a name.
    pub(crate) folder_id: Option<String>,
    pub(crate) created_at_epoch_seconds: u64,
    pub(crate) transcript_available: bool,
    /// Design intake D1: the first sentence of the note's own overview, so a
    /// row previews the meeting's outcome and not only its title.
    ///
    /// `None` whenever this row has no current admitted note, or when the
    /// same digest-verified projection its other fields already trust
    /// cannot produce one -- never derived from the transcript, the
    /// operator's own note, or pre-meeting context. Those are real content
    /// about the meeting, but they are not the note's claimed outcome, and
    /// this field exists to promise exactly that. The governing constraint
    /// is "real generated content only; never a placeholder line", so an
    /// absent field renders no preview line at all, not a filler sentence.
    ///
    /// Also `None` for every locked row, whatever the meeting's note says —
    /// see `locked` below.
    pub(crate) note_preview: Option<String>,
    /// Roadmap intake I5: whether this meeting is behind the local lock.
    ///
    /// When true, `note_preview` above is suppressed at source rather than
    /// hidden by the shell (Bear's obscured-previews move): the preview is
    /// generated content about what was said, and a locked meeting's list row
    /// must not carry it. `transcript_available` stays truthful — it is one
    /// bit of metadata about a row already showing its title and date, and a
    /// field asserting "no transcript" about a meeting that has one would be
    /// the same class of lie this product forbids for withheld turns. The
    /// shell's locked branch renders neither the preview nor that meta line.
    pub(crate) locked: bool,
}

/// Operator title, then the meeting's own opening line, then nothing.
///
/// Before this existed every row read `Untitled meeting` — all of them, at
/// once, because the operator title is read from a record that
/// `library_metadata` has no writer for, so the fallback was never a fallback.
fn label_for(row: &LibraryRow) -> (Option<String>, &'static str) {
    meeting_title::label(row.title(), row.derived_title())
}

/// Design intake D1's row preview, cut from the same digest-verified claim
/// projection `open_note` already trusts.
///
/// `note_claims` returns the meeting's claims in ordinal order, and the
/// worker admits every overview sentence (`ClaimType::Summary`) before any
/// decision, follow-up, or open question (`worker/note_validator.py`'s
/// `admit_note_claims`), so the first `Summary` this finds is the note's
/// opening overview sentence. A meeting with no current admitted note
/// returns no claims at all, and any verification failure along the way
/// (a stale snapshot, an unopenable hit) returns `None` rather than a
/// partial or stale-feeling preview — the same "field absent, not a
/// placeholder" rule the row itself follows.
fn note_preview_for(projection: &LibraryProjection, meeting_id: &str) -> Option<String> {
    let hits = projection.note_claims(meeting_id).ok()?;
    for hit in hits {
        match projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Claim {
                claim_type: ClaimType::Summary,
                claim,
                ..
            }) => return Some(first_sentence_preview(&claim)),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

/// A library row promises a preview short enough to read as a caption, not
/// long enough to double as a second title.
const NOTE_PREVIEW_MAX_CHARS: usize = 140;

/// The claim's first sentence, then capped and cut at a word boundary.
///
/// Every admitted overview claim is already one short, plain sentence by
/// construction (the worker's synthesis prompt asks for "2 to 4 plain
/// sentences", one per claim), so this mostly returns the claim text
/// unchanged. The sentence split exists as a guard against an unusually long
/// or multi-sentence claim reaching the row, not as the primary shaping step.
fn first_sentence_preview(claim_text: &str) -> String {
    let trimmed = claim_text.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut end = chars.len();
    for (index, ch) in chars.iter().enumerate() {
        if matches!(ch, '.' | '!' | '?') {
            let at_boundary = chars
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace());
            if at_boundary {
                end = index + 1;
                break;
            }
        }
    }
    let sentence: String = chars[..end].iter().collect();
    truncate_at_word_boundary(sentence.trim(), NOTE_PREVIEW_MAX_CHARS)
}

/// Cuts `text` to at most `max_chars`, backing up to the previous word
/// boundary rather than splitting a word, then marks the cut with an
/// ellipsis. Text already within the cap is returned unchanged -- a complete
/// sentence never gains a trailing ellipsis it did not earn.
fn truncate_at_word_boundary(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_owned();
    }
    let mut cut = max_chars;
    while cut > 0 && !chars[cut - 1].is_whitespace() {
        cut -= 1;
    }
    if cut == 0 {
        // No whitespace at all inside the cap (one very long word): a hard
        // cut beats an empty preview.
        cut = max_chars;
    }
    let mut truncated: String = chars[..cut].iter().collect();
    let trimmed_len = truncated.trim_end().len();
    truncated.truncate(trimmed_len);
    truncated.push('…');
    truncated
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySearchResponse {
    pub(crate) state: &'static str,
    pub(crate) results: Vec<LibrarySearchResult>,
    /// Matches found before the page was cut. Separate from `results.len()`
    /// deliberately: the result is what a person can open, and this is the
    /// diagnostic that tells them whether narrowing is worth doing. It must
    /// never be rendered in the result position.
    pub(crate) total_matches: usize,
    pub(crate) unavailable_count: usize,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySearchResult {
    pub(crate) handle: String,
    pub(crate) kind: &'static str,
    pub(crate) meeting_id: String,
    pub(crate) text: Option<String>,
    pub(crate) source_turn_index: Option<u32>,
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
    pub(crate) claim_ordinal: Option<u64>,
    pub(crate) transcript_available: bool,
}

/// A verified search handle can only reopen its current projection target. It
/// deliberately returns a turn identity, never a filename or a general path.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySearchOpenResponse {
    pub(crate) state: &'static str,
    pub(crate) transcript_handle: Option<String>,
    pub(crate) meeting_id: Option<String>,
    pub(crate) source_turn_index: Option<u32>,
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryNoteResponse {
    pub(crate) state: &'static str,
    pub(crate) transcript_handle: Option<String>,
    /// A single-use authority to replace the operator's own note for this
    /// meeting. It is distinct from transcript and deletion handles: none of
    /// those acts authorizes changing the reader's private note.
    pub(crate) operator_note_handle: Option<String>,
    pub(crate) audio_deletion_handle: Option<String>,
    /// Opaque, source-specific native playback authorities. They carry no
    /// file identity and are valid for one immediate play request only.
    pub(crate) microphone_playback_handle: Option<String>,
    pub(crate) system_playback_handle: Option<String>,
    /// Separate from the raw-recording and whole-meeting handles. It exists
    /// only while this projection still proves an admitted source transcript.
    pub(crate) transcript_deletion_handle: Option<String>,
    /// Separate from `audio_deletion_handle`, and issued for every readable
    /// meeting rather than only for one holding retained audio: a meeting whose
    /// audio has already been released can still be removed in full.
    pub(crate) meeting_deletion_handle: Option<String>,
    pub(crate) meeting_id: String,
    /// The retained current transcript's digest when this meeting is eligible
    /// to create or replace a generated note. It is the exact source pin the
    /// `regenerate_note` command requires, so the browser can ask for
    /// generation without ever holding an artifact path or content.
    pub(crate) regeneration_source_sha256: Option<String>,
    pub(crate) claims: Vec<LibraryClaim>,
    pub(crate) audio_retention: LibraryAudioRetention,
    /// Whether this recording was held open with nothing being captured.
    ///
    /// It sits on every read of a meeting rather than only on a retry
    /// comparison, because a gap in the audio changes how the transcript
    /// should be read whether or not the operator ever retries it.
    pub(crate) capture_pauses:
        local_meeting_notes_session_core::capture_quality::CapturePauseProjection,
    /// The operator's own note (§ D), carried here so it stays reachable after
    /// the meeting is dismissed.
    ///
    /// Without it the note is readable exactly until the operator navigates
    /// away once, and then never again — saved, and lost as far as anyone using
    /// the app can tell. It carries its own `unreadable` flag rather than
    /// collapsing to text, because "could not be read" and "nothing written"
    /// lead a reader to opposite conclusions.
    pub(crate) operator_note: crate::operator_note::OperatorNote,
    /// The operator's pre-meeting context (roadmap intake I3's sidecar),
    /// carried here so it stays readable after the meeting the same way
    /// `operator_note` does.
    ///
    /// Read-only on this surface: nothing here is editable, and this packet
    /// adds no write path for it beyond the one `save_meeting_context`
    /// already has for the meeting currently being captured.
    pub(crate) meeting_context: crate::meeting_context::MeetingContext,
    /// Roadmap intake I4 / design D4's reverse half: which of `claims` cites
    /// each transcript turn, keyed by the turn's `sourceTurnIndex` -- the
    /// same identity the transcript reader already uses for a turn, and the
    /// same identity `claims[].locators` already carries. A turn with no
    /// citing claim is simply absent; the shell reads absence as "uncited,"
    /// never as "not yet checked."
    ///
    /// This is not a second link: it is `claims` folded the other direction,
    /// derived in `turns_cited` from the exact locators opened for this
    /// response. It therefore shares `claims`'s staleness lifecycle exactly --
    /// when the projection is not current, this response is `stale` before
    /// either field is populated, and both come back empty.
    pub(crate) turns_cited: Vec<TurnCitation>,
    /// Roadmap intake I5: this meeting's lock, so the shell can offer to lock,
    /// unlock, or say why it is showing nothing.
    ///
    /// A locked meeting opened without a fresh confirmation comes back as
    /// `state: "locked"` with every content field empty and **every handle
    /// `None`** — no transcript, no operator note, no playback, no deletion of
    /// audio or transcript. That is the gate: the webview is not asked to hide
    /// controls it holds authority for, it is never given the authority. The
    /// one exception is `meeting_deletion_handle`, which a locked meeting
    /// still issues — see the locked branch in `open_note_current`.
    pub(crate) lock: crate::meeting_lock::MeetingLock,
    /// A fresh single-use confirmation for reading this same locked meeting
    /// again, set by `library_open_note` after a read that passed the gate.
    ///
    /// Always `None` here: minting is the command's business, not the
    /// reader's, and every constructor in this file leaves it empty. See
    /// `library_open_note` in `main.rs` for why a successful read re-issues.
    pub(crate) lock_token: Option<String>,
    /// Whether this Mac can run the device-owner check at all, set by
    /// `library_open_note` from the app's confirmation seam.
    ///
    /// It is a fact about the machine, not about this meeting, and it rides
    /// the note response only because the meeting surface is where the lock
    /// affordance lives. The shell uses it to refuse to *offer* locking on a
    /// Mac with no Touch ID and no password set — locking there would make a
    /// meeting this app could never reopen. It gates no access: a meeting that
    /// is already locked stays locked whatever this says.
    ///
    /// Defaults to `false` in every content-free constructor here, so a
    /// response that never reached the command layer cannot invite a lock.
    pub(crate) can_confirm_operator: bool,
    pub(crate) message: String,
}

/// One transcript turn cited by at least one claim, and every claim ordinal
/// that cites it (ascending, deduplicated -- a claim with two locators on the
/// same turn appears once).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnCitation {
    pub(crate) turn: u32,
    pub(crate) claim_ordinals: Vec<u64>,
}

/// Content-free, freshly checked retention facts for one meeting detail view.
/// The browser receives no artifact path, digest, audio bytes, or deletion
/// authority. This remains a read-only Preview projection.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryAudioRetention {
    pub(crate) state: &'static str,
    pub(crate) policy: &'static str,
    pub(crate) deadline_epoch_seconds: Option<u64>,
    pub(crate) retained_bytes: Option<u64>,
    pub(crate) message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryAudioDeletionAccess {
    pub(crate) state: &'static str,
    pub(crate) meeting_id: Option<String>,
    pub(crate) message: String,
    /// W7-B (2026-09-01 desktop audit): a stable machine code for `message`,
    /// present only when `message` is one of the handful the frontend
    /// attaches a recovery action to. `None` for every other message, same as
    /// today. See `crate::error_codes`.
    pub(crate) code: Option<&'static str>,
}

/// The only two retained-audio sources a player may ever request. This is a
/// closed native enum rather than a string so a future player cannot turn a
/// source choice into arbitrary storage authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedAudioSource {
    Microphone,
    System,
}

#[derive(Debug, Clone)]
struct RetainedAudioPlaybackHandle {
    hit: LibraryHit,
    source: RetainedAudioSource,
    /// The exact artifact the reviewed detail view verified. Authorization
    /// rejects a record that was changed to point elsewhere before playback.
    artifact: ArtifactRef,
}

/// A consumed, native-only grant for one verified retained-audio file.
///
/// This intentionally has no serialization implementation and exposes neither
/// a path, a digest, nor a meeting identity. A fixed native player can read the
/// already-open private file without widening the webview's authority.
#[derive(Debug)]
pub(crate) struct LibraryAudioPlaybackGrant {
    source: RetainedAudioSource,
    file: File,
}

impl LibraryAudioPlaybackGrant {
    pub(crate) fn source(&self) -> RetainedAudioSource {
        self.source
    }

    pub(crate) fn file(&self) -> &File {
        &self.file
    }
}

/// Native-only failure information for playback authorization. It carries no
/// artifact identity because callers must never reflect one to the webview.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryAudioPlaybackAccess {
    pub(crate) state: &'static str,
    pub(crate) message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryTranscriptDeletionAccess {
    pub(crate) state: &'static str,
    pub(crate) meeting_id: Option<String>,
    pub(crate) message: String,
    /// See `LibraryAudioDeletionAccess::code`.
    pub(crate) code: Option<&'static str>,
}

/// Authorization to remove a whole meeting, which is a strictly larger act than
/// releasing its audio.
///
/// It is a separate type over a separate handle map on purpose. Reusing the
/// audio-deletion handle would mean a handle the operator obtained to free disk
/// space could destroy the retained evidence instead, and the transcript is the
/// one artifact this product promises to keep.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryMeetingDeletionAccess {
    pub(crate) state: &'static str,
    pub(crate) meeting_id: Option<String>,
    pub(crate) message: String,
    /// See `LibraryAudioDeletionAccess::code`.
    pub(crate) code: Option<&'static str>,
}

/// Authorization to restore one trash entry, re-read from disk rather than
/// from a retained handle — see `authorize_trash_restore`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryTrashRestoreAccess {
    pub(crate) state: &'static str,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryClaim {
    pub(crate) handle: String,
    pub(crate) ordinal: u64,
    pub(crate) claim_type: &'static str,
    pub(crate) claim: String,
    pub(crate) evidence_state: &'static str,
    pub(crate) locator_count: usize,
    /// Design intake D5's hover-preview depth: every one of this claim's
    /// locators, already resolved to the same digest-quoted transcript text
    /// `preview_library_open_evidence` proves one locator at a time -- gathered
    /// once here instead.
    ///
    /// This is not a new authority. `renderTranscript` already sends the
    /// webview every retained turn's full text over `library_open_transcript`;
    /// `spans` is a narrower slice of the same already-disclosed turns, quoted
    /// once so a hover does not have to spend a single-use claim handle (see
    /// `open_evidence_current`'s `self.handles.clear()`, which invalidates
    /// every other claim's handle the moment one is opened -- exactly the cost
    /// a 350ms hover must not pay). A claim whose locator count exceeds
    /// `spans.len()` had at least one locator this reader could not currently
    /// re-slice (see `claim_locator_spans`); the shell renders those claims
    /// with whatever spans did resolve rather than withholding the claim
    /// entirely.
    pub(crate) spans: Vec<LibraryClaimSpan>,
}

/// One locator's already-quoted transcript text, batched onto `LibraryClaim`
/// rather than fetched per hover. `sourceTurnIndex` is the same identity
/// `turnsCited` and the transcript reader already key on, so the browser needs
/// no second turn-lookup scheme to show a speaker or timestamp beside it.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryClaimSpan {
    pub(crate) source_turn_index: u32,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) text: String,
}

/// One located claim as `meeting_export` needs it: the claim text plus every
/// locator's turn number and its digest-verified quoted excerpt. Distinct from
/// `LibraryClaim` (the webview DTO), which reports only a `locator_count` —
/// export needs the actual turn numbers and quoted text, never a path or a
/// handle, so this stays private to the reader/export seam.
#[derive(Debug, Clone)]
pub(crate) struct ExportClaim {
    pub(crate) ordinal: u64,
    pub(crate) claim_type: &'static str,
    pub(crate) text: String,
    pub(crate) locators: Vec<ExportClaimLocator>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExportClaimLocator {
    pub(crate) source_turn_index: u32,
    pub(crate) text: String,
}

/// Failure information for `open_export_bound`. Carries no meeting identity —
/// same rule as the other bounded-access failures — because the export
/// pathway must not turn a stale handle into a way to probe which meetings
/// exist.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryExportAccess {
    pub(crate) state: &'static str,
    pub(crate) message: String,
    /// See `LibraryAudioDeletionAccess::code`.
    pub(crate) code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryEvidenceResponse {
    pub(crate) state: &'static str,
    pub(crate) transcript_handle: Option<String>,
    pub(crate) meeting_id: Option<String>,
    pub(crate) source_turn_index: Option<u32>,
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
    pub(crate) text: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug)]
pub(crate) struct LibraryTranscriptAccess {
    pub(crate) state: &'static str,
    pub(crate) meeting_id: Option<String>,
    pub(crate) transcript_artifact: Option<ArtifactRef>,
    pub(crate) message: String,
    /// See `LibraryAudioDeletionAccess::code`.
    pub(crate) code: Option<&'static str>,
}

/// Authorization for one edit of the operator's note in a retained meeting.
///
/// The browser never receives a path or a reusable meeting identifier that can
/// be written. It spends a fresh handle issued while that exact meeting detail
/// view was open; the callback receives the resolved storage root only after
/// the projection and active-meeting checks succeed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LibraryOperatorNoteAccess {
    pub(crate) state: &'static str,
    pub(crate) message: String,
    /// See `LibraryAudioDeletionAccess::code`.
    pub(crate) code: Option<&'static str>,
}

impl LibraryReader {
    /// Builds from an active-ID snapshot taken by the caller while it holds the
    /// meeting-storage sequence. Reader methods never acquire that sequence;
    /// commands keep one global sequence-before-reader-mutex lock order.
    pub(crate) fn rebuild(
        storage: StorageRoot,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Self, ()> {
        Self::rebuild_with_projector(
            storage,
            excluded_meeting_ids,
            std::sync::Arc::new(UnavailableProjector),
        )
    }

    /// The same rebuild through an admitted `note.project` transport.  The
    /// projector is injected by the caller because admission is cached off
    /// this hot path; `rebuild` above keeps the projector-less shape for
    /// callers (and tests) that never project claims.
    pub(crate) fn rebuild_with_projector(
        storage: StorageRoot,
        excluded_meeting_ids: &HashSet<String>,
        projector: std::sync::Arc<dyn NoteProjector>,
    ) -> Result<Self, ()> {
        let projection = LibraryProjection::rebuild_with_projector_excluding(
            &storage,
            ReadLimits::default(),
            projector,
            excluded_meeting_ids,
        )
        .map_err(|_| ())?;
        sync_corpus_index(&storage, &projection);
        Ok(Self::new_excluding(
            storage,
            projection,
            excluded_meeting_ids.clone(),
        ))
    }

    fn new_excluding(
        storage: StorageRoot,
        projection: LibraryProjection,
        excluded_meeting_ids: HashSet<String>,
    ) -> Self {
        Self {
            storage,
            projection,
            excluded_meeting_ids,
            handles: HashMap::new(),
            operator_note_handles: HashMap::new(),
            audio_deletion_handles: HashMap::new(),
            audio_playback_handles: HashMap::new(),
            transcript_deletion_handles: HashMap::new(),
            meeting_deletion_handles: HashMap::new(),
        }
    }

    pub(crate) fn new(storage: StorageRoot, projection: LibraryProjection) -> Self {
        Self::new_excluding(storage, projection, HashSet::new())
    }

    fn revalidate(&mut self, active_meeting_ids: &HashSet<String>) -> bool {
        if active_meeting_ids != &self.excluded_meeting_ids
            || self
                .projection
                .validate_snapshot_excluding(&self.storage, &active_meeting_ids)
                .is_err()
        {
            self.clear_handles();
            return false;
        }
        true
    }

    pub(crate) fn snapshot(&mut self, active_meeting_ids: &HashSet<String>) -> LibrarySnapshot {
        self.snapshot_filtered(active_meeting_ids, &LibraryFilter::default())
    }

    pub(crate) fn snapshot_filtered(
        &mut self,
        active_meeting_ids: &HashSet<String>,
        filter: &LibraryFilter,
    ) -> LibrarySnapshot {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_snapshot();
        }
        self.snapshot_current(filter)
    }

    fn folders(&self) -> Vec<LibraryFolder> {
        self.projection
            .folders()
            .into_iter()
            .map(|(id, name)| LibraryFolder {
                id: id.to_owned(),
                name: name.to_owned(),
            })
            .collect()
    }

    /// Identity-only projection rows for the standing §K retention overview.
    /// Deliberately mints no handle — `snapshot` clears the handle table on
    /// every call, so an overview read must not invalidate the generation
    /// the operator is already navigating. Rows join to the rendered library
    /// list by meeting id instead.
    pub(crate) fn retention_identities(
        &mut self,
        active_meeting_ids: &HashSet<String>,
    ) -> Option<Vec<(String, u64)>> {
        if !self.revalidate(active_meeting_ids) {
            return None;
        }
        Some(
            self.projection
                .rows()
                .iter()
                .map(|row| (row.meeting_id.clone(), row.created_at_epoch_seconds))
                .collect(),
        )
    }

    fn snapshot_current(&mut self, filter: &LibraryFilter) -> LibrarySnapshot {
        self.clear_handles();
        let unavailable_count = self.projection.quarantined_meetings();
        let total = self.projection.rows().len();
        let filtered = self.projection.filtered_rows(filter);
        let shown = filtered.len();
        if filtered.is_empty() {
            return LibrarySnapshot {
                state: if unavailable_count == 0 {
                    "empty"
                } else {
                    "incomplete"
                },
                rows: Vec::new(),
                unavailable_count,
                metadata_revision: self.projection.metadata_revision(),
                folders: self.folders(),
                total,
                shown,
                filter_active: !filter.is_empty(),
                // "No meetings match" and "you have no meetings" lead a reader
                // to opposite conclusions, so they are different sentences.
                message: if !filter.is_empty() && total > 0 {
                    format!("No meeting matches this filter. {total} are retained.")
                } else if unavailable_count == 0 {
                    "No retained meetings are available.".into()
                } else {
                    format!("{unavailable_count} retained meeting(s) could not be read.")
                },
            };
        }
        let source_rows: Vec<_> = filtered
            .into_iter()
            .map(|row| {
                let (label, label_source) = label_for(row);
                (
                    row.meeting_id.clone(),
                    label,
                    label_source,
                    row.folder_id().map(str::to_owned),
                    row.created_at_epoch_seconds,
                    row.transcript_sha256.is_some(),
                )
            })
            .collect();
        // Roadmap intake I5. Read before the previews below, because a locked
        // row skips its preview entirely rather than computing and discarding
        // one -- `note_claims` clears the projection's sealed-hit store on
        // every call (the D1 hazard documented below), so an opened-and-thrown-
        // away preview is not free.
        let locks: Vec<bool> = source_rows
            .iter()
            .map(|(meeting_id, ..)| self.meeting_lock(meeting_id).locked)
            .collect();
        // Design intake D1: every row's note preview is read before any
        // snapshot handle is minted below. `note_claims` clears the
        // projection's own sealed-hit store on every call -- documented on
        // `open_note_current` as "opening a note establishes the next
        // evidence-response boundary" -- so interleaving it with
        // `meeting_handle` calls per row would clear the *previous* rows'
        // just-minted handles the moment a later row's preview was read.
        // Reading every preview first, then minting every handle afterward
        // with no further clears to come, leaves that store in exactly the
        // state it was in before this field existed.
        let note_previews: Vec<Option<String>> = source_rows
            .iter()
            .zip(&locks)
            .map(|((meeting_id, ..), locked)| {
                (!locked)
                    .then(|| note_preview_for(&self.projection, meeting_id))
                    .flatten()
            })
            .collect();
        let mut rows = Vec::new();
        for (
            (
                (
                    meeting_id,
                    label,
                    label_source,
                    folder_id,
                    created_at_epoch_seconds,
                    transcript_available,
                ),
                note_preview,
            ),
            locked,
        ) in source_rows.into_iter().zip(note_previews).zip(locks)
        {
            let Ok(hit) = self.projection.meeting_handle(&meeting_id) else {
                return Self::unavailable_snapshot();
            };
            rows.push(LibrarySnapshotRow {
                handle: self.retain_handle(hit),
                meeting_id,
                label,
                label_source,
                folder_id,
                created_at_epoch_seconds,
                transcript_available,
                note_preview,
                locked,
            });
        }
        LibrarySnapshot {
            state: if unavailable_count == 0 {
                "populated"
            } else {
                "populated-incomplete"
            },
            rows,
            unavailable_count,
            metadata_revision: self.projection.metadata_revision(),
            folders: self.folders(),
            total,
            shown,
            filter_active: !filter.is_empty(),
            message: if !filter.is_empty() {
                // Said in words as well as in numbers. A count in a corner is
                // easy to miss; a list that is showing a fraction of itself
                // must say so where the reader is already looking.
                format!("Showing {shown} of {total} retained meetings.")
            } else if unavailable_count == 0 {
                "Retained meetings are available.".into()
            } else {
                format!("Retained meetings are available. {unavailable_count} could not be read.")
            },
        }
    }

    pub(crate) fn search(
        &mut self,
        query: &str,
        active_meeting_ids: &HashSet<String>,
    ) -> LibrarySearchResponse {
        self.search_filtered(query, active_meeting_ids, &LibraryFilter::default())
    }

    pub(crate) fn search_filtered(
        &mut self,
        query: &str,
        active_meeting_ids: &HashSet<String>,
        filter: &LibraryFilter,
    ) -> LibrarySearchResponse {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_search();
        }
        self.search_current(query, filter)
    }

    fn search_current(&mut self, query: &str, filter: &LibraryFilter) -> LibrarySearchResponse {
        // A handle is valid only for the response that returned it. Keeping old
        // handles would retain an unbounded amount of private snapshot state.
        self.clear_handles();
        let unavailable_count = self.projection.quarantined_meetings();
        // Roadmap intake I5's search-path hardening: this exact-search backend
        // has no reachable caller today (`preview_library_search` is
        // unregistered in `main.rs`), but a locked meeting's transcript,
        // claims, and title must be structurally impossible to reach through
        // it whenever it does gain one -- the same fail-closed exclusion the
        // corpus index already applies, computed the same way.
        let locked_meeting_ids = locked_meeting_ids(&self.storage, &self.projection);
        match self
            .projection
            .search_filtered_excluding(query, filter, &locked_meeting_ids)
        {
            Ok(found) if found.hits.is_empty() => LibrarySearchResponse {
                state: if unavailable_count == 0 {
                    "no-results"
                } else {
                    "incomplete"
                },
                results: Vec::new(),
                total_matches: 0,
                unavailable_count,
                message: if unavailable_count == 0 {
                    "No retained text matched that search.".into()
                } else {
                    format!(
                        "No match was found among readable meetings. {unavailable_count} could not be searched."
                    )
                },
            },
            Ok(found) => {
                let mut results = Vec::new();
                for hit in found.hits {
                    match self.projection.open_snapshot(&hit) {
                        Ok(OpenedLibraryHit::Claim {
                            meeting_id,
                            claim,
                            claim_ordinal,
                            ..
                        }) => results.push(LibrarySearchResult {
                            handle: self.retain_handle(hit),
                            kind: "claim",
                            meeting_id,
                            text: Some(claim),
                            source_turn_index: None,
                            start: None,
                            end: None,
                            claim_ordinal: Some(claim_ordinal),
                            transcript_available: true,
                        }),
                        Ok(OpenedLibraryHit::Transcript {
                            meeting_id,
                            text,
                            source_turn_index,
                            original_scalar_start,
                            original_scalar_end,
                            ..
                        }) => results.push(LibrarySearchResult {
                            handle: self.retain_handle(hit),
                            kind: "transcript",
                            meeting_id,
                            text: Some(text),
                            source_turn_index: Some(source_turn_index),
                            start: Some(original_scalar_start),
                            end: Some(original_scalar_end),
                            claim_ordinal: None,
                            transcript_available: true,
                        }),
                        Ok(OpenedLibraryHit::Withheld {
                            meeting_id,
                            source_turn_index,
                        }) => results.push(LibrarySearchResult {
                            handle: self.retain_handle(hit),
                            kind: "withheld",
                            meeting_id,
                            text: None,
                            source_turn_index: Some(source_turn_index),
                            start: None,
                            end: None,
                            claim_ordinal: None,
                            transcript_available: false,
                        }),
                        Ok(OpenedLibraryHit::Meeting {
                            meeting_id,
                            title,
                            folder,
                            ..
                        }) => {
                            let transcript_available = self.meeting_has_transcript(&meeting_id);
                            results.push(LibrarySearchResult {
                                handle: self.retain_handle(hit),
                                kind: "meeting",
                                meeting_id,
                                text: title.or(folder),
                                source_turn_index: None,
                                start: None,
                                end: None,
                                claim_ordinal: None,
                                transcript_available,
                            });
                        }
                        Err(_) => return Self::stale_search(),
                    }
                }
                LibrarySearchResponse {
                    state: if unavailable_count == 0 {
                        "results"
                    } else {
                        "results-incomplete"
                    },
                    results,
                    total_matches: found.total,
                    unavailable_count,
                    message: if unavailable_count == 0 {
                        "Exact results from the current library snapshot.".into()
                    } else {
                        format!(
                            "Exact results from readable meetings. {unavailable_count} could not be searched."
                        )
                    },
                }
            }
            Err(LibraryReadError::InvalidRequest) => Self::invalid_search(),
            // `bounded` is gone with the refusal that produced it. `search_filtered`
            // was the only path here that returned `CapacityExceeded`, and it now
            // truncates instead — so an arm for it would be dead copy the next
            // reader treats as reachable.
            Err(_) => Self::stale_search(),
        }
    }

    /// Reopens a result through the projection's normal stale-artifact checks.
    /// Search terms never become filesystem or transcript-enumeration authority.
    pub(crate) fn open_search_result(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
    ) -> LibrarySearchOpenResponse {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_search_open();
        }
        self.open_search_result_current(handle)
    }

    fn open_search_result_current(&mut self, handle: &str) -> LibrarySearchOpenResponse {
        let Some(hit) = self.handles.get(handle).cloned() else {
            self.clear_handles();
            return Self::stale_search_open();
        };
        self.clear_handles();
        match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Transcript {
                meeting_id,
                source_turn_index,
                original_scalar_start,
                original_scalar_end,
                ..
            }) => {
                // Roadmap intake I5: the hit was minted against a snapshot
                // that already excluded every locked meeting, but the lock is
                // re-checked here anyway -- this is the only place that can
                // catch a meeting that was unlocked when `search_current`
                // sealed this handle and became locked before it was opened.
                // A stale-hit refusal is exactly the right shape for that
                // race: nothing here distinguishes it from any other kind of
                // staleness to the caller.
                if self.meeting_lock(&meeting_id).locked {
                    return Self::stale_search_open();
                }
                self.retain_search_open(
                    hit,
                    "transcript",
                    Some(meeting_id),
                    Some(source_turn_index),
                    Some(original_scalar_start),
                    Some(original_scalar_end),
                    "Opening the exact retained transcript turn that matched.",
                )
            }
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => {
                if self.meeting_lock(&meeting_id).locked {
                    return Self::stale_search_open();
                }
                if self.meeting_has_transcript(&meeting_id) {
                    self.retain_search_open(
                        hit,
                        "meeting",
                        Some(meeting_id),
                        None,
                        None,
                        None,
                        "Opening this retained meeting's canonical transcript.",
                    )
                } else {
                    LibrarySearchOpenResponse {
                        state: "metadata-only",
                        transcript_handle: None,
                        meeting_id: Some(meeting_id),
                        source_turn_index: None,
                        start: None,
                        end: None,
                        message: "No transcript was created for this retained meeting.".into(),
                    }
                }
            }
            Ok(OpenedLibraryHit::Withheld {
                meeting_id,
                source_turn_index,
            }) => {
                if self.meeting_lock(&meeting_id).locked {
                    return Self::stale_search_open();
                }
                self.retain_search_open(
                    hit,
                    "withheld",
                    Some(meeting_id),
                    Some(source_turn_index),
                    None,
                    None,
                    "A voice check withheld this matching turn. It is not shown as transcript text.",
                )
            }
            // Preview does not expose note reading yet, so a retained claim is
            // not a transcript/title search destination.
            Ok(OpenedLibraryHit::Claim { .. }) | Err(_) => Self::stale_search_open(),
        }
    }

    /// Opens one meeting's detail view.
    ///
    /// `unlocked_meeting_id` is roadmap intake I5's gate input: the meeting a
    /// freshly spent `LockedAction::Open` confirmation authorizes, or `None`
    /// when the caller presented no valid token. Token custody stays in
    /// `main.rs` — this reader is told only which meeting was authorized, and
    /// compares it against the one the handle actually resolves to.
    pub(crate) fn open_note(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        unlocked_meeting_id: Option<&str>,
    ) -> LibraryNoteResponse {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_note("");
        }
        self.open_note_current(handle, active_meeting_ids, unlocked_meeting_id)
    }

    fn open_note_current(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        unlocked_meeting_id: Option<&str>,
    ) -> LibraryNoteResponse {
        // Opening a note establishes the next evidence-response boundary.
        let Some(hit) = self.handles.get(handle).cloned() else {
            self.clear_handles();
            return Self::stale_note("");
        };
        self.clear_handles();
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Self::stale_note(""),
        };
        // Roadmap intake I5's read gate, before anything about this meeting is
        // read or any capability is minted. A locked meeting without a fresh
        // confirmation returns here: no claims, no transcript, no personal
        // note, no context, and no handle that could reach any of them.
        let lock = self.meeting_lock(&meeting_id);
        if !crate::meeting_lock::permits(lock, unlocked_meeting_id, &meeting_id) {
            return LibraryNoteResponse {
                state: "locked",
                transcript_handle: None,
                operator_note_handle: None,
                audio_deletion_handle: None,
                microphone_playback_handle: None,
                system_playback_handle: None,
                transcript_deletion_handle: None,
                // The one capability a locked meeting still hands out.
                // Deletion is already double-confirmed and recoverable from
                // Trash for thirty days, and gating it would make a meeting
                // whose confirmation is unavailable permanently undeletable.
                // The lock rides the move to Trash and comes back with it.
                meeting_deletion_handle: self.retain_meeting_deletion_handle(&meeting_id),
                meeting_id: meeting_id.clone(),
                regeneration_source_sha256: None,
                claims: Vec::new(),
                // Content-free by construction, exactly like the `unavailable`
                // and `stale` shapes: retention facts, pause facts, and the
                // capture record all describe this meeting and none of them
                // renders behind the lock.
                audio_retention: Self::unavailable_audio_retention(),
                capture_pauses:
                    local_meeting_notes_session_core::capture_quality::CapturePauseProjection::unchecked(),
                operator_note: crate::operator_note::OperatorNote::none(),
                meeting_context: crate::meeting_context::MeetingContext::none(),
                turns_cited: Vec::new(),
                lock,
                lock_token: None,
                can_confirm_operator: false,
                message: "This meeting is locked on this Mac.".into(),
            };
        }
        let Some((lifecycle, row_transcript_sha256)) = self
            .projection
            .rows()
            .iter()
            .find(|row| row.meeting_id == meeting_id)
            .map(|row| (row.lifecycle(), row.transcript_sha256.clone()))
        else {
            return Self::stale_note(&meeting_id);
        };
        // The exact source pin `regenerate_note` requires. Ready meetings may
        // replace an admitted note; transcript-ready and failed meetings may
        // create their first one.
        let regeneration_source_sha256 = matches!(
            lifecycle,
            MeetingLifecycle::Ready
                | MeetingLifecycle::TranscriptReady
                | MeetingLifecycle::SummaryFailed
        )
        .then_some(row_transcript_sha256)
        .flatten();
        let audio_retention = self.audio_retention(&meeting_id);
        let capture_pauses = Self::capture_pauses(&self.storage, &meeting_id);
        let operator_note = self.operator_note(&meeting_id);
        let meeting_context = self.meeting_context(&meeting_id);
        let operator_note_handle = (!operator_note.unreadable)
            .then(|| self.retain_operator_note_handle(&meeting_id))
            .flatten();
        let audio_deletion_handle =
            self.retain_audio_deletion_handle(&meeting_id, audio_retention.state == "retained");
        let microphone_playback_handle = (audio_retention.state == "retained")
            .then(|| {
                self.retain_audio_playback_handle(
                    &meeting_id,
                    RetainedAudioSource::Microphone,
                    active_meeting_ids,
                )
            })
            .flatten();
        let system_playback_handle = (audio_retention.state == "retained")
            .then(|| {
                self.retain_audio_playback_handle(
                    &meeting_id,
                    RetainedAudioSource::System,
                    active_meeting_ids,
                )
            })
            .flatten();
        let transcript_deletion_handle = self.retain_transcript_deletion_handle(&meeting_id);
        let meeting_deletion_handle = self.retain_meeting_deletion_handle(&meeting_id);
        match lifecycle {
            MeetingLifecycle::SummaryFailed => {
                return LibraryNoteResponse {
                    state: "summary-failed",
                    transcript_handle: self.retain_transcript_handle(&meeting_id),
                    operator_note_handle,
                    audio_deletion_handle,
                    microphone_playback_handle,
                    system_playback_handle,
                    transcript_deletion_handle,
                    meeting_deletion_handle,
                    meeting_id: meeting_id.clone(),
                    regeneration_source_sha256,
                    claims: Vec::new(),
                    audio_retention,
                    capture_pauses: capture_pauses.clone(),
                    operator_note,
                    meeting_context: meeting_context.clone(),
                    turns_cited: Vec::new(),
                    lock,
                    lock_token: None,
                    can_confirm_operator: false,
                    message: "A note was not produced. Retained transcript text remains available."
                        .into(),
                };
            }
            MeetingLifecycle::Ready => {}
            _ => {
                let transcript_handle = self.retain_transcript_handle(&meeting_id);
                return LibraryNoteResponse {
                    state: "transcript-only",
                    transcript_handle: transcript_handle.clone(),
                    operator_note_handle,
                    audio_deletion_handle,
                    microphone_playback_handle,
                    system_playback_handle,
                    transcript_deletion_handle,
                    meeting_deletion_handle,
                    meeting_id: meeting_id.clone(),
                    regeneration_source_sha256,
                    claims: Vec::new(),
                    audio_retention,
                    capture_pauses: capture_pauses.clone(),
                    operator_note,
                    meeting_context: meeting_context.clone(),
                    turns_cited: Vec::new(),
                    lock,
                    lock_token: None,
                    can_confirm_operator: false,
                    message: if transcript_handle.is_some() {
                        "No admitted note is available. Retained transcript text remains available."
                            .into()
                    } else if transcript_deletion_completed(&self.storage, &meeting_id)
                        .unwrap_or(false)
                    {
                        "The transcript and generated notes were permanently deleted. Any retained recording and your personal notes remain."
                            .into()
                    } else {
                        "No transcript was created for this retained meeting.".into()
                    },
                };
            }
        }
        let handles = match self.projection.note_claims(&meeting_id) {
            Ok(handles) => handles,
            Err(_) => return Self::stale_note(&meeting_id),
        };
        // Batched once, before any locator loop, and turned into owned text so
        // the borrow of `self.projection` ends here -- `self.retain_handle`
        // below needs `&mut self`. One pass over this meeting's own turns
        // (already read to answer this same request) is the whole cost; no
        // extra file or projection read per locator.
        let turn_texts: HashMap<u32, (String, bool)> = self
            .projection
            .rows()
            .iter()
            .find(|row| row.meeting_id == meeting_id)
            .map(|row| {
                row.derived()
                    .turns
                    .iter()
                    .map(|turn| (turn.index, (turn.text.to_string(), turn.gated)))
                    .collect()
            })
            .unwrap_or_default();
        let mut claims = Vec::new();
        // Gathered alongside `claims`, from the exact locators this response
        // just opened -- not looked up separately -- then folded into
        // `turns_cited` below. See that function's doc comment for why this
        // is what gives the reverse index its staleness lifecycle for free.
        let mut turn_citations: Vec<(u32, u64)> = Vec::new();
        for hit in handles {
            let handle = self.retain_handle(hit.clone());
            match self.projection.open_snapshot(&hit) {
                Ok(OpenedLibraryHit::Claim {
                    claim_ordinal,
                    claim_type,
                    evidence_state,
                    claim,
                    locators,
                    ..
                }) => {
                    for locator in &locators {
                        turn_citations.push((locator.source_turn_index, claim_ordinal));
                    }
                    let spans = claim_locator_spans(&locators, &turn_texts);
                    claims.push(LibraryClaim {
                        handle,
                        ordinal: claim_ordinal,
                        claim_type: claim_type_name(claim_type),
                        claim,
                        evidence_state: evidence_state_name(evidence_state),
                        locator_count: locators.len(),
                        spans,
                    })
                }
                Ok(_) | Err(_) => return Self::stale_note(&meeting_id),
            }
        }
        LibraryNoteResponse {
            state: "note",
            transcript_handle: self.retain_transcript_handle(&meeting_id),
            operator_note_handle,
            audio_deletion_handle,
            microphone_playback_handle,
            system_playback_handle,
            transcript_deletion_handle,
            meeting_deletion_handle,
            meeting_id: meeting_id.into(),
            regeneration_source_sha256: None,
            claims,
            audio_retention,
            capture_pauses,
            operator_note,
            meeting_context,
            turns_cited: turns_cited(&turn_citations),
            lock,
            lock_token: None,
            can_confirm_operator: false,
            message: "Claim words can be opened against their exact transcript locators.".into(),
        }
    }

    pub(crate) fn open_evidence(
        &mut self,
        handle: &str,
        locator_ordinal: usize,
        active_meeting_ids: &HashSet<String>,
    ) -> LibraryEvidenceResponse {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_evidence();
        }
        self.open_evidence_current(handle, locator_ordinal, active_meeting_ids)
    }

    fn open_evidence_current(
        &mut self,
        handle: &str,
        locator_ordinal: usize,
        active_meeting_ids: &HashSet<String>,
    ) -> LibraryEvidenceResponse {
        let Some(hit) = self.handles.remove(handle) else {
            self.clear_handles();
            return Self::stale_evidence();
        };
        // Showing a source passage spends only source and claim handles. The
        // adjacent destructive controls stay bound to this same immutable
        // projection, so opening evidence cannot turn a visible Delete button
        // into a stale no-op.
        self.handles.clear();
        match self.projection.open_claim_evidence_excluding(
            &self.storage,
            &hit,
            locator_ordinal,
            active_meeting_ids,
        ) {
            Ok(evidence) => {
                let transcript_handle = self.retain_transcript_handle(&evidence.meeting_id);
                LibraryEvidenceResponse {
                    state: "evidence",
                    transcript_handle,
                    meeting_id: Some(evidence.meeting_id),
                    source_turn_index: Some(evidence.source_turn_index),
                    start: Some(evidence.start),
                    end: Some(evidence.end),
                    text: Some(evidence.text),
                    message: "Exact text from the retained transcript locator.".into(),
                }
            }
            Err(_) => Self::stale_evidence(),
        }
    }

    /// Revalidates the opaque transcript handle and runs the bound byte loader
    /// before releasing the same storage sequence. The callback receives only
    /// the already-authorized meeting and artifact identity.
    pub(crate) fn open_transcript_bound<T>(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        load: impl FnOnce(&StorageRoot, &str, &ArtifactRef) -> T,
    ) -> Result<T, LibraryTranscriptAccess> {
        if !self.revalidate(active_meeting_ids) {
            return Err(Self::stale_transcript());
        }
        self.open_transcript_current(handle, active_meeting_ids, load)
    }

    /// Spends an operator-note handle and runs one bounded write callback. The
    /// same snapshot/active-set rules as transcript opening apply, but this
    /// deliberately accepts only a retained meeting handle, never a claim or
    /// transcript handle.
    pub(crate) fn open_operator_note_bound<T>(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        save: impl FnOnce(&StorageRoot, &str) -> T,
    ) -> Result<T, LibraryOperatorNoteAccess> {
        if !self.revalidate(active_meeting_ids) {
            return Err(Self::stale_operator_note());
        }
        let Some(hit) = self.operator_note_handles.remove(handle) else {
            self.clear_handles();
            return Err(Self::stale_operator_note());
        };
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Err(Self::stale_operator_note()),
        };
        if active_meeting_ids.contains(&meeting_id) {
            return Err(Self::stale_operator_note());
        }
        Ok(save(&self.storage, &meeting_id))
    }

    fn open_transcript_current<T>(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        load: impl FnOnce(&StorageRoot, &str, &ArtifactRef) -> T,
    ) -> Result<T, LibraryTranscriptAccess> {
        let Some(hit) = self.handles.remove(handle) else {
            self.clear_handles();
            return Err(Self::stale_transcript());
        };
        // Source opening consumes only its own read authority. A meeting
        // detail can continue to save the operator note or perform a reviewed
        // deletion against the same verified library snapshot.
        self.handles.clear();
        let response = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Transcript {
                meeting_id,
                transcript_artifact,
                ..
            }) => LibraryTranscriptAccess {
                state: "transcript",
                meeting_id: Some(meeting_id),
                transcript_artifact: Some(transcript_artifact),
                message: "Opening the retained canonical transcript.".into(),
                code: None,
            },
            Ok(OpenedLibraryHit::Meeting {
                meeting_id,
                transcript_artifact: Some(transcript_artifact),
                ..
            }) => LibraryTranscriptAccess {
                state: "transcript",
                meeting_id: Some(meeting_id),
                transcript_artifact: Some(transcript_artifact),
                message: "Opening the retained canonical transcript.".into(),
                code: None,
            },
            Ok(OpenedLibraryHit::Meeting { .. }) => Self::stale_transcript(),
            Ok(OpenedLibraryHit::Withheld { .. }) => LibraryTranscriptAccess {
                state: "withheld",
                meeting_id: None,
                transcript_artifact: None,
                message: "A voice check withheld that matching turn from transcript text.".into(),
                code: None,
            },
            Ok(OpenedLibraryHit::Claim { .. }) | Err(_) => Self::stale_transcript(),
        };
        let (Some(meeting_id), Some(transcript_artifact)) = (
            response.meeting_id.as_deref(),
            response.transcript_artifact.as_ref(),
        ) else {
            return Err(response);
        };
        if active_meeting_ids.contains(meeting_id) {
            return Err(Self::stale_transcript());
        }
        Ok(load(&self.storage, meeting_id, transcript_artifact))
    }

    pub(crate) fn unavailable_snapshot() -> LibrarySnapshot {
        LibrarySnapshot {
            state: "unavailable",
            rows: Vec::new(),
            unavailable_count: 0,
            metadata_revision: None,
            folders: Vec::new(),
            total: 0,
            shown: 0,
            filter_active: false,
            message: UNAVAILABLE_MESSAGE.into(),
        }
    }

    pub(crate) fn unavailable_search() -> LibrarySearchResponse {
        LibrarySearchResponse {
            state: "unavailable",
            results: Vec::new(),
            total_matches: 0,
            unavailable_count: 0,
            message: UNAVAILABLE_MESSAGE.into(),
        }
    }

    pub(crate) fn unavailable_note(meeting_id: &str) -> LibraryNoteResponse {
        LibraryNoteResponse {
            state: "unavailable",
            transcript_handle: None,
            operator_note_handle: None,
            audio_deletion_handle: None,
            microphone_playback_handle: None,
            system_playback_handle: None,
            transcript_deletion_handle: None,
            meeting_deletion_handle: None,
            meeting_id: meeting_id.into(),
            regeneration_source_sha256: None,
            claims: Vec::new(),
            audio_retention: Self::unavailable_audio_retention(),
            capture_pauses:
                local_meeting_notes_session_core::capture_quality::CapturePauseProjection::unchecked(),
            operator_note: crate::operator_note::OperatorNote::none(),
            meeting_context: crate::meeting_context::MeetingContext::none(),
            turns_cited: Vec::new(),
            lock: crate::meeting_lock::MeetingLock::none(),
            lock_token: None,
            can_confirm_operator: false,
            message: UNAVAILABLE_MESSAGE.into(),
        }
    }

    pub(crate) fn unavailable_evidence() -> LibraryEvidenceResponse {
        LibraryEvidenceResponse {
            state: "unavailable",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            text: None,
            message: UNAVAILABLE_MESSAGE.into(),
        }
    }

    fn retain_handle(&mut self, hit: LibraryHit) -> String {
        let handle = Uuid::new_v4().to_string();
        self.handles.insert(handle.clone(), hit);
        handle
    }

    fn retain_audio_deletion_handle(&mut self, meeting_id: &str, retained: bool) -> Option<String> {
        if !retained {
            return None;
        }
        let hit = self.projection.meeting_handle(meeting_id).ok()?;
        let handle = Uuid::new_v4().to_string();
        self.audio_deletion_handles.insert(handle.clone(), hit);
        Some(handle)
    }

    /// Mints one native-only capability for the requested retained recording
    /// leg. The caller must already have the current active-meeting snapshot;
    /// this method rechecks it and the exact on-disk artifact before retaining
    /// anything. It never returns an artifact identity to a webview surface.
    pub(crate) fn retain_audio_playback_handle(
        &mut self,
        meeting_id: &str,
        source: RetainedAudioSource,
        active_meeting_ids: &HashSet<String>,
    ) -> Option<String> {
        if !self.revalidate(active_meeting_ids) || active_meeting_ids.contains(meeting_id) {
            return None;
        }
        let hit = self.projection.meeting_handle(meeting_id).ok()?;
        let artifact = Self::retained_audio_artifact(&self.storage, meeting_id, source)?;
        let handle = Uuid::new_v4().to_string();
        self.audio_playback_handles.insert(
            handle.clone(),
            RetainedAudioPlaybackHandle {
                hit,
                source,
                artifact,
            },
        );
        Some(handle)
    }

    /// Consumes an opaque source-specific playback handle and returns only an
    /// already-open, digest-verified native file grant. A fixed player can use
    /// that file; no generic file path or filesystem capability escapes here.
    ///
    /// `unlocked_meeting_id` is roadmap intake I5's gate input: the meeting a
    /// freshly spent `LockedAction::Playback` confirmation authorizes, or
    /// `None`. The check happens **here** rather than in the calling command
    /// because the grant deliberately carries no meeting identity — this is
    /// the last point at which the meeting behind the handle is known.
    pub(crate) fn authorize_audio_playback(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        unlocked_meeting_id: Option<&str>,
    ) -> Result<LibraryAudioPlaybackGrant, LibraryAudioPlaybackAccess> {
        if !self.revalidate(active_meeting_ids) {
            return Err(Self::stale_audio_playback());
        }
        let playback = self.audio_playback_handles.get(handle).cloned();
        // Playback handles are single-use and belong to this exact projection
        // generation. Clear every competing navigation/deletion capability,
        // but preserve the current detail view's personal-note edit handle so
        // listening cannot make an adjacent unsaved note fail.
        self.clear_non_note_handles();
        let Some(playback) = playback else {
            return Err(Self::stale_audio_playback());
        };
        let meeting_id = match self.projection.open_snapshot(&playback.hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Err(Self::stale_audio_playback()),
        };
        if active_meeting_ids.contains(&meeting_id) {
            return Err(Self::stale_audio_playback());
        }
        // Roadmap intake I5. Refused before the file is opened or hashed, so a
        // locked meeting's recording is never read, not merely never played.
        if !crate::meeting_lock::permits(
            self.meeting_lock(&meeting_id),
            unlocked_meeting_id,
            &meeting_id,
        ) {
            return Err(Self::locked_audio_playback());
        }
        let Some(current) =
            Self::retained_audio_artifact(&self.storage, &meeting_id, playback.source)
        else {
            return Err(Self::unavailable_audio_playback());
        };
        // A new meeting record may be valid on its own but is not the artifact
        // this handle was issued for. Require exact continuity across the
        // review-to-playback boundary.
        if current != playback.artifact {
            return Err(Self::stale_audio_playback());
        }
        let Ok(directory) = meeting_dir(&self.storage, &meeting_id) else {
            return Err(Self::unavailable_audio_playback());
        };
        let Ok(path) = resolve_artifact(&directory, &current.relative_path) else {
            return Err(Self::unavailable_audio_playback());
        };
        let Ok(mut file) = open_private_file(&path) else {
            return Err(Self::unavailable_audio_playback());
        };
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let Ok(read) = file.read(&mut buffer) else {
                return Err(Self::unavailable_audio_playback());
            };
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        if format!("{:x}", hasher.finalize()) != current.sha256
            || file.seek(SeekFrom::Start(0)).is_err()
        {
            return Err(Self::unavailable_audio_playback());
        }
        Ok(LibraryAudioPlaybackGrant {
            source: playback.source,
            file,
        })
    }

    fn retain_transcript_deletion_handle(&mut self, meeting_id: &str) -> Option<String> {
        if !self.meeting_has_transcript(meeting_id) {
            return None;
        }
        let hit = self.projection.meeting_handle(meeting_id).ok()?;
        let handle = Uuid::new_v4().to_string();
        self.transcript_deletion_handles.insert(handle.clone(), hit);
        Some(handle)
    }

    /// Issued for any meeting the projection can resolve, unlike the audio
    /// handle which requires retained audio. A meeting whose audio was already
    /// released still has a transcript, a note, and a record to remove.
    fn retain_meeting_deletion_handle(&mut self, meeting_id: &str) -> Option<String> {
        let hit = self.projection.meeting_handle(meeting_id).ok()?;
        let handle = Uuid::new_v4().to_string();
        self.meeting_deletion_handles.insert(handle.clone(), hit);
        Some(handle)
    }

    /// Spends the same opaque handle `open_transcript_bound` accepts — the one
    /// `retain_transcript_handle` mints, already returned to the webview as
    /// `transcriptFileHandle` — and, once the exact meeting still resolves,
    /// hands the bounded callback the storage root, the meeting id, a label to
    /// derive an archive name from, and every claim this meeting's admitted
    /// note holds. Reusing that handle's authority class rather than minting a
    /// new one keeps `open_note_current` (which is what mints every other
    /// per-meeting capability) untouched: export rides the exact same
    /// "meeting, optionally with its transcript" authority a transcript-file
    /// open already spends.
    ///
    /// Claims are empty for any lifecycle other than `Ready`: a
    /// transcript-only or summary-failed meeting still exports honestly, just
    /// without a note. Each claim's locators already carry digest-verified
    /// quoted excerpt text via `open_claim_evidence_excluding` — the same
    /// evidence `preview_library_open_evidence` proves before showing a
    /// source passage. This is the one place export logic reaches into
    /// `self.projection`; the callback receives only already-verified values
    /// and does no further library-projection work itself, keeping
    /// `meeting_export` a pure assembler over data this reader already proved.
    pub(crate) fn open_export_bound<T>(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
        run: impl FnOnce(&StorageRoot, &str, Option<&str>, u64, &[ExportClaim]) -> T,
    ) -> Result<T, LibraryExportAccess> {
        if !self.revalidate(active_meeting_ids) {
            return Err(Self::stale_export());
        }
        let Some(hit) = self.handles.remove(handle) else {
            self.clear_handles();
            return Err(Self::stale_export());
        };
        // Mirrors `open_transcript_current`'s narrower clear: exporting reads
        // this one meeting and must not invalidate an adjacent deletion or
        // audio-playback capability the same detail view already holds.
        self.handles.clear();
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(OpenedLibraryHit::Transcript { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Err(Self::stale_export()),
        };
        if active_meeting_ids.contains(&meeting_id) {
            return Err(Self::stale_export());
        }
        let Some(row) = self
            .projection
            .rows()
            .iter()
            .find(|row| row.meeting_id == meeting_id)
        else {
            return Err(Self::stale_export());
        };
        let label = row.title().map(str::to_owned).or_else(|| row.derived_title());
        let created_at_epoch_seconds = row.created_at_epoch_seconds;
        let claims = match self.export_claims(&meeting_id) {
            Ok(claims) => claims,
            Err(()) => return Err(Self::stale_export()),
        };
        Ok(run(
            &self.storage,
            &meeting_id,
            label.as_deref(),
            created_at_epoch_seconds,
            &claims,
        ))
    }

    /// Every located claim for a `Ready` meeting, with locator turn numbers and
    /// digest-verified quoted excerpt text — the same evidence
    /// `preview_library_open_evidence` proves before showing a source passage,
    /// gathered up front rather than one locator at a time.
    fn export_claims(&self, meeting_id: &str) -> Result<Vec<ExportClaim>, ()> {
        let Some(row) = self
            .projection
            .rows()
            .iter()
            .find(|row| row.meeting_id == meeting_id)
        else {
            return Err(());
        };
        if row.lifecycle() != MeetingLifecycle::Ready {
            return Ok(Vec::new());
        }
        let handles = self.projection.note_claims(meeting_id).map_err(|_| ())?;
        let mut claims = Vec::with_capacity(handles.len());
        for hit in &handles {
            let (ordinal, claim_type, text, locator_count) = match self.projection.open_snapshot(hit)
            {
                Ok(OpenedLibraryHit::Claim {
                    claim_ordinal,
                    claim_type,
                    claim,
                    locators,
                    ..
                }) => (claim_ordinal, claim_type, claim, locators.len()),
                _ => return Err(()),
            };
            let mut export_locators = Vec::with_capacity(locator_count);
            for locator_ordinal in 0..locator_count {
                let evidence = self
                    .projection
                    .open_claim_evidence_excluding(
                        &self.storage,
                        hit,
                        locator_ordinal,
                        &self.excluded_meeting_ids,
                    )
                    .map_err(|_| ())?;
                export_locators.push(ExportClaimLocator {
                    source_turn_index: evidence.source_turn_index,
                    text: evidence.text,
                });
            }
            claims.push(ExportClaim {
                ordinal,
                claim_type: claim_type_name(claim_type),
                text,
                locators: export_locators,
            });
        }
        Ok(claims)
    }

    fn stale_export() -> LibraryExportAccess {
        LibraryExportAccess {
            state: "stale",
            message: "This meeting changed or is no longer available. Reopen it and try again."
                .into(),
            code: Some(crate::error_codes::MEETING_CHANGED_UNAVAILABLE),
        }
    }

    pub(crate) fn authorize_meeting_deletion(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
    ) -> LibraryMeetingDeletionAccess {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_meeting_deletion();
        }
        let hit = self.meeting_deletion_handles.get(handle).cloned();
        // Handles are cleared whether or not this one resolved, so a stale
        // handle cannot be retried against a projection that has since moved.
        self.clear_handles();
        let Some(hit) = hit else {
            return Self::stale_meeting_deletion();
        };
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Self::stale_meeting_deletion(),
        };
        LibraryMeetingDeletionAccess {
            state: "authorized",
            meeting_id: Some(meeting_id),
            message: "The reviewed meeting may be deleted in full.".into(),
            code: None,
        }
    }

    fn stale_meeting_deletion() -> LibraryMeetingDeletionAccess {
        LibraryMeetingDeletionAccess {
            state: "stale",
            meeting_id: None,
            message: STALE_MESSAGE.into(),
            code: Some(crate::error_codes::VIEW_STALE),
        }
    }

    /// Authorization to restore one trash entry.
    ///
    /// Deliberately not handle-based like the deletion accesses above: a trash
    /// entry lives outside `LibraryProjection` (it is, by construction, not a
    /// projected meeting), so there is no snapshot handle to redeem. The
    /// operator already has the exact `meeting_id` from the Trash list this
    /// call re-reads, matching the raw-identifier precedent
    /// `restore_withheld_turn` already uses elsewhere in this app. What this
    /// keeps from the deletion idiom is the state shape — a fresh disk read
    /// authoritative over whatever the shell last rendered — not the handle
    /// mechanism, which has nothing to attach to here.
    pub(crate) fn authorize_trash_restore(&self, meeting_id: &str) -> LibraryTrashRestoreAccess {
        use local_meeting_notes_session_core::meeting_trash::list_trash_entries;
        let found = list_trash_entries(&self.storage)
            .unwrap_or_default()
            .into_iter()
            .any(|entry| entry.meeting_id == meeting_id);
        if found {
            LibraryTrashRestoreAccess {
                state: "authorized",
                message: "This meeting may be restored from Trash.".into(),
            }
        } else {
            LibraryTrashRestoreAccess {
                state: "not-found",
                message: "This meeting is no longer in Trash.".into(),
            }
        }
    }

    pub(crate) fn authorize_transcript_deletion(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
    ) -> LibraryTranscriptDeletionAccess {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_transcript_deletion();
        }
        let hit = self.transcript_deletion_handles.get(handle).cloned();
        self.clear_handles();
        let Some(hit) = hit else {
            return Self::stale_transcript_deletion();
        };
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Self::stale_transcript_deletion(),
        };
        if self.meeting_has_transcript(&meeting_id) {
            return LibraryTranscriptDeletionAccess {
                state: "authorized",
                meeting_id: Some(meeting_id),
                message: "The reviewed meeting transcript may be deleted with its generated notes."
                    .into(),
                code: None,
            };
        }
        if transcript_deletion_completed(&self.storage, &meeting_id).unwrap_or(false) {
            return LibraryTranscriptDeletionAccess {
                state: "already-removed",
                meeting_id: Some(meeting_id),
                message: "This meeting transcript was already deleted.".into(),
                code: None,
            };
        }
        LibraryTranscriptDeletionAccess {
            state: "no-transcript",
            meeting_id: Some(meeting_id),
            message: "This meeting has no retained transcript to delete.".into(),
            code: None,
        }
    }

    fn stale_transcript_deletion() -> LibraryTranscriptDeletionAccess {
        LibraryTranscriptDeletionAccess {
            state: "stale",
            meeting_id: None,
            message: STALE_MESSAGE.into(),
            code: Some(crate::error_codes::VIEW_STALE),
        }
    }

    pub(crate) fn authorize_audio_deletion(
        &mut self,
        handle: &str,
        active_meeting_ids: &HashSet<String>,
    ) -> LibraryAudioDeletionAccess {
        if !self.revalidate(active_meeting_ids) {
            return Self::stale_audio_deletion();
        }
        self.authorize_audio_deletion_current(handle)
    }

    fn authorize_audio_deletion_current(&mut self, handle: &str) -> LibraryAudioDeletionAccess {
        let hit = self.audio_deletion_handles.get(handle).cloned();
        self.clear_handles();
        let Some(hit) = hit else {
            return Self::stale_audio_deletion();
        };
        let meeting_id = match self.projection.open_snapshot(&hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return Self::stale_audio_deletion(),
        };
        match Self::read_audio_retention(&self.storage, &meeting_id).state {
            "retained" => LibraryAudioDeletionAccess {
                state: "authorized",
                meeting_id: Some(meeting_id),
                message: "The reviewed meeting recording may be deleted.".into(),
                code: None,
            },
            "released" => LibraryAudioDeletionAccess {
                state: "already-released",
                meeting_id: Some(meeting_id),
                message: "This meeting recording was already deleted.".into(),
                code: None,
            },
            "deleting" => LibraryAudioDeletionAccess {
                state: "deleting",
                meeting_id: Some(meeting_id),
                message: "This meeting recording is already being deleted.".into(),
                code: None,
            },
            "not-recorded" => LibraryAudioDeletionAccess {
                state: "not-recorded",
                meeting_id: Some(meeting_id),
                message: "This meeting has no retained recording to delete.".into(),
                code: None,
            },
            _ => LibraryAudioDeletionAccess {
                state: "unavailable",
                meeting_id: None,
                message: "Recording deletion is unavailable. Reopen Library and try again.".into(),
                code: Some(crate::error_codes::RECORDING_DELETION_UNAVAILABLE),
            },
        }
    }

    /// Clears handles for every other library action while leaving the current
    /// detail view's personal-note edit capability intact. Reading a source
    /// passage must not make the adjacent note editor fail on its next save.
    fn clear_non_note_handles(&mut self) {
        self.handles.clear();
        self.audio_deletion_handles.clear();
        self.audio_playback_handles.clear();
        self.transcript_deletion_handles.clear();
        self.meeting_deletion_handles.clear();
    }

    /// A new snapshot generation, a changed active set, or opening a different
    /// note invalidates every capability, including an unsaved personal-note
    /// editor from a prior detail view.
    fn clear_handles(&mut self) {
        self.clear_non_note_handles();
        self.operator_note_handles.clear();
    }

    /// A single-use transcript handle for a meeting named by ID rather than
    /// found through this reader.
    ///
    /// Semantic search ranks in the corpus index, which knows meeting IDs and
    /// nothing about handle generations. Minting here rather than letting that
    /// path hand an ID to an open command is the point: the handle is checked
    /// against *this* projection, dies with it, and a meeting with no
    /// transcript gets `None` instead of a handle that opens nothing.
    pub(crate) fn retain_transcript_handle(&mut self, meeting_id: &str) -> Option<String> {
        if !self.meeting_has_transcript(meeting_id) {
            return None;
        }
        self.projection
            .meeting_handle(meeting_id)
            .ok()
            .map(|hit| self.retain_handle(hit))
    }

    /// A single-use handle for exactly one personal-note edit. It shares the
    /// reader's generation with the visible detail view and dies when it is
    /// spent, a new snapshot is minted, or the library revalidates differently.
    pub(crate) fn retain_operator_note_handle(&mut self, meeting_id: &str) -> Option<String> {
        self.projection
            .meeting_handle(meeting_id)
            .ok()
            .map(|hit| {
                let handle = Uuid::new_v4().to_string();
                self.operator_note_handles.insert(handle.clone(), hit);
                handle
            })
    }

    fn meeting_has_transcript(&self, meeting_id: &str) -> bool {
        self.projection
            .rows()
            .iter()
            .any(|row| row.meeting_id == meeting_id && row.transcript_sha256.is_some())
    }

    fn audio_retention(&self, meeting_id: &str) -> LibraryAudioRetention {
        Self::read_audio_retention(&self.storage, meeting_id)
    }

    /// Reads the pause record from the meeting's own capture receipt.
    ///
    /// A meeting whose record cannot be opened, or whose receipt bytes no
    /// longer verify, reports that the check did not happen. It never falls
    /// back to "not paused", which would state an absence Yawn did not confirm.
    fn capture_pauses(
        storage: &StorageRoot,
        meeting_id: &str,
    ) -> local_meeting_notes_session_core::capture_quality::CapturePauseProjection {
        meeting_dir(storage, meeting_id)
            .ok()
            .and_then(|directory| {
                let meeting = load_meeting(&directory).ok()?;
                local_meeting_notes_session_core::capture_quality::project_capture_pauses(
                    &directory, &meeting,
                )
                .ok()
            })
            .unwrap_or_else(
                local_meeting_notes_session_core::capture_quality::CapturePauseProjection::unchecked,
            )
    }

    /// Opens the current meeting record and proves that the requested source
    /// is still retained, regular, non-symlinked, and digest-valid. The core
    /// verifier covers the full record; the returned ref identifies just the
    /// selected audio leg for source-specific handles.
    fn retained_audio_artifact(
        storage: &StorageRoot,
        meeting_id: &str,
        source: RetainedAudioSource,
    ) -> Option<ArtifactRef> {
        let directory = meeting_dir(storage, meeting_id).ok()?;
        let meeting = load_meeting(&directory).ok()?;
        if meeting.retention.state != AudioState::Retained
            || verify_record_artifacts(&directory, &meeting).is_err()
        {
            return None;
        }
        match source {
            RetainedAudioSource::Microphone => meeting.artifacts.microphone_audio.clone(),
            RetainedAudioSource::System => meeting.artifacts.system_audio.clone(),
        }
    }

    /// Read alongside the retention facts, from the same resolved directory and
    /// with the same failure posture: a meeting that cannot be resolved reports
    /// nothing rather than guessing.
    fn operator_note(&self, meeting_id: &str) -> crate::operator_note::OperatorNote {
        match meeting_dir(&self.storage, meeting_id) {
            Ok(directory) => crate::operator_note::read(&directory),
            Err(_) => crate::operator_note::OperatorNote::none(),
        }
    }

    /// Read alongside the operator's own note, from the same resolved
    /// directory and with the same failure posture as `operator_note`: a
    /// meeting that cannot be resolved reports nothing rather than guessing.
    fn meeting_context(&self, meeting_id: &str) -> crate::meeting_context::MeetingContext {
        match meeting_dir(&self.storage, meeting_id) {
            Ok(directory) => crate::meeting_context::read(&directory),
            Err(_) => crate::meeting_context::MeetingContext::none(),
        }
    }

    /// Resolves the meeting behind a snapshot handle **without spending it**,
    /// and reports its lock.
    ///
    /// Roadmap intake I5's three lock commands need to know which meeting they
    /// are about before they do anything, and they must not consume the handle
    /// while finding out: `authorize_locked_action` mints a confirmation for a
    /// meeting the shell is about to open with the same handle, so spending it
    /// would break the very call the confirmation was for. Nothing is minted
    /// here and no handle map is cleared — this is a read.
    ///
    /// It deliberately answers only for the meeting-shaped handles a library
    /// row and a note response carry. A claim, transcript, or withheld handle
    /// resolves to `None`: those name a passage, and locking is a per-meeting
    /// act.
    pub(crate) fn with_locked_meeting<T>(
        &self,
        handle: &str,
        run: impl FnOnce(&StorageRoot, &str, MeetingLock) -> T,
    ) -> Option<T> {
        let hit = self.handles.get(handle)?;
        let meeting_id = match self.projection.open_snapshot(hit) {
            Ok(OpenedLibraryHit::Meeting { meeting_id, .. }) => meeting_id,
            Ok(_) | Err(_) => return None,
        };
        let lock = self.meeting_lock(&meeting_id);
        Some(run(&self.storage, &meeting_id, lock))
    }

    /// Roadmap intake I5's lock state, read from the same resolved directory
    /// as the two sidecars above — with the opposite failure posture.
    ///
    /// A meeting whose directory cannot be resolved reports *unlocked*, not
    /// locked, and that is deliberate rather than an inconsistency: every
    /// caller of this method has already resolved that meeting through the
    /// verified projection, so an unresolvable directory here means the
    /// meeting is gone, and the caller's own stale or unavailable path is what
    /// answers. Reporting a phantom lock would put a locked-looking row in
    /// front of a meeting that no longer exists. The fail-closed decision that
    /// matters lives one level down, in `meeting_lock::read`, where an
    /// unreadable file on a resolvable meeting reads as locked.
    fn meeting_lock(&self, meeting_id: &str) -> crate::meeting_lock::MeetingLock {
        match meeting_dir(&self.storage, meeting_id) {
            Ok(directory) => crate::meeting_lock::read(&directory),
            Err(_) => crate::meeting_lock::MeetingLock::none(),
        }
    }

    pub(crate) fn read_audio_retention(
        storage: &StorageRoot,
        meeting_id: &str,
    ) -> LibraryAudioRetention {
        let Ok(directory) = meeting_dir(storage, meeting_id) else {
            return Self::unavailable_audio_retention();
        };
        let Ok(meeting) = load_meeting(&directory) else {
            return Self::unavailable_audio_retention();
        };
        let (policy, deadline_epoch_seconds) = match meeting.retention.rule {
            AudioRetentionRule::DeleteAfter { .. } => (
                "scheduled",
                meeting.retention.next_deletion_at_epoch_seconds,
            ),
            AudioRetentionRule::UntilManualDeletion => ("manual", None),
        };
        match meeting.retention.state {
            AudioState::Retained => {
                if verify_record_artifacts(&directory, &meeting).is_err() {
                    return Self::unavailable_audio_retention();
                }
                let bytes = [
                    meeting.artifacts.microphone_audio.as_ref(),
                    meeting.artifacts.system_audio.as_ref(),
                ]
                .into_iter()
                .flatten()
                .try_fold(0_u64, |total, artifact| {
                    let path =
                        resolve_artifact(&directory, &artifact.relative_path).map_err(|_| ())?;
                    let size = fs::metadata(path).map_err(|_| ())?.len();
                    total.checked_add(size).ok_or(())
                });
                let Ok(retained_bytes) = bytes else {
                    return Self::unavailable_audio_retention();
                };
                LibraryAudioRetention {
                    state: "retained",
                    policy,
                    deadline_epoch_seconds,
                    retained_bytes: Some(retained_bytes),
                    message: "Meeting audio is retained on this Mac.".into(),
                }
            }
            AudioState::Released => {
                if verify_record_artifacts(&directory, &meeting).is_err() {
                    return Self::unavailable_audio_retention();
                }
                LibraryAudioRetention {
                    state: "released",
                    policy,
                    deadline_epoch_seconds,
                    retained_bytes: None,
                    message:
                        "Meeting audio was deleted. The transcript, note, and evidence remain."
                            .into(),
                }
            }
            AudioState::Deleting => {
                if verify_record_static_artifacts(&directory, &meeting).is_err() {
                    return Self::unavailable_audio_retention();
                }
                LibraryAudioRetention {
                    state: "deleting",
                    policy,
                    deadline_epoch_seconds,
                    retained_bytes: None,
                    message: "Meeting audio deletion is already in progress.".into(),
                }
            }
            AudioState::NeverCreated => LibraryAudioRetention {
                state: "not-recorded",
                policy,
                deadline_epoch_seconds,
                retained_bytes: None,
                message: "This meeting has no retained audio.".into(),
            },
        }
    }

    fn unavailable_audio_retention() -> LibraryAudioRetention {
        LibraryAudioRetention {
            state: "unavailable",
            policy: "unknown",
            deadline_epoch_seconds: None,
            retained_bytes: None,
            message: "Audio retention details are unavailable. Reopen Library and try again."
                .into(),
        }
    }

    fn retain_search_open(
        &mut self,
        hit: LibraryHit,
        state: &'static str,
        meeting_id: Option<String>,
        source_turn_index: Option<u32>,
        start: Option<u64>,
        end: Option<u64>,
        message: &'static str,
    ) -> LibrarySearchOpenResponse {
        self.clear_handles();
        LibrarySearchOpenResponse {
            state,
            transcript_handle: Some(self.retain_handle(hit)),
            meeting_id,
            source_turn_index,
            start,
            end,
            message: message.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_handle_count(&self) -> usize {
        self.handles.len()
    }

    fn stale_search() -> LibrarySearchResponse {
        LibrarySearchResponse {
            state: "stale",
            results: Vec::new(),
            total_matches: 0,
            unavailable_count: 0,
            message: STALE_MESSAGE.into(),
        }
    }

    fn stale_snapshot() -> LibrarySnapshot {
        LibrarySnapshot {
            state: "stale",
            rows: Vec::new(),
            unavailable_count: 0,
            metadata_revision: None,
            folders: Vec::new(),
            total: 0,
            shown: 0,
            filter_active: false,
            message: STALE_MESSAGE.into(),
        }
    }

    fn stale_search_open() -> LibrarySearchOpenResponse {
        LibrarySearchOpenResponse {
            state: "stale",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            message: STALE_MESSAGE.into(),
        }
    }

    fn unavailable_search_open() -> LibrarySearchOpenResponse {
        LibrarySearchOpenResponse {
            state: "unavailable",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            message: UNAVAILABLE_MESSAGE.into(),
        }
    }

    fn stale_note(meeting_id: &str) -> LibraryNoteResponse {
        LibraryNoteResponse {
            state: "stale",
            transcript_handle: None,
            operator_note_handle: None,
            audio_deletion_handle: None,
            microphone_playback_handle: None,
            system_playback_handle: None,
            transcript_deletion_handle: None,
            meeting_deletion_handle: None,
            meeting_id: meeting_id.into(),
            regeneration_source_sha256: None,
            claims: Vec::new(),
            audio_retention: Self::unavailable_audio_retention(),
            capture_pauses:
                local_meeting_notes_session_core::capture_quality::CapturePauseProjection::unchecked(),
            operator_note: crate::operator_note::OperatorNote::none(),
            meeting_context: crate::meeting_context::MeetingContext::none(),
            turns_cited: Vec::new(),
            lock: crate::meeting_lock::MeetingLock::none(),
            lock_token: None,
            can_confirm_operator: false,
            message: STALE_MESSAGE.into(),
        }
    }

    /// Roadmap intake I5's playback refusal. Distinct from `stale` and
    /// `unavailable`, because "reopen it and try again" is wrong advice here —
    /// nothing is out of date, the meeting is locked and needs a confirmation.
    /// Like its siblings it carries no meeting identity.
    fn locked_audio_playback() -> LibraryAudioPlaybackAccess {
        LibraryAudioPlaybackAccess {
            state: "locked",
            message: LOCKED_MESSAGE.into(),
        }
    }

    fn stale_audio_deletion() -> LibraryAudioDeletionAccess {
        LibraryAudioDeletionAccess {
            state: "stale",
            meeting_id: None,
            message: STALE_MESSAGE.into(),
            code: Some(crate::error_codes::VIEW_STALE),
        }
    }

    fn stale_audio_playback() -> LibraryAudioPlaybackAccess {
        LibraryAudioPlaybackAccess {
            state: "stale",
            message: STALE_MESSAGE.into(),
        }
    }

    fn unavailable_audio_playback() -> LibraryAudioPlaybackAccess {
        LibraryAudioPlaybackAccess {
            state: "unavailable",
            message: "Retained audio is unavailable. Reopen Library and try again.".into(),
        }
    }

    fn stale_operator_note() -> LibraryOperatorNoteAccess {
        LibraryOperatorNoteAccess {
            state: "stale",
            message: "This meeting changed or is no longer available. Reopen it and try again."
                .into(),
            code: Some(crate::error_codes::MEETING_CHANGED_UNAVAILABLE),
        }
    }

    fn unavailable_audio_deletion() -> LibraryAudioDeletionAccess {
        LibraryAudioDeletionAccess {
            state: "unavailable",
            meeting_id: None,
            message: "Recording deletion is unavailable. Reopen Library and try again.".into(),
            code: Some(crate::error_codes::RECORDING_DELETION_UNAVAILABLE),
        }
    }

    fn stale_evidence() -> LibraryEvidenceResponse {
        LibraryEvidenceResponse {
            state: "stale",
            transcript_handle: None,
            meeting_id: None,
            source_turn_index: None,
            start: None,
            end: None,
            text: None,
            message: STALE_MESSAGE.into(),
        }
    }

    fn stale_transcript() -> LibraryTranscriptAccess {
        LibraryTranscriptAccess {
            state: "stale",
            meeting_id: None,
            transcript_artifact: None,
            message: STALE_MESSAGE.into(),
            code: Some(crate::error_codes::VIEW_STALE),
        }
    }

    fn unavailable_transcript() -> LibraryTranscriptAccess {
        LibraryTranscriptAccess {
            state: "unavailable",
            meeting_id: None,
            transcript_artifact: None,
            message: UNAVAILABLE_MESSAGE.into(),
            code: Some(crate::error_codes::LIBRARY_UNAVAILABLE),
        }
    }

    fn invalid_search() -> LibrarySearchResponse {
        LibrarySearchResponse {
            state: "invalid",
            results: Vec::new(),
            total_matches: 0,
            unavailable_count: 0,
            message: "Enter at least two characters to search retained text.".into(),
        }
    }
}

fn claim_type_name(value: ClaimType) -> &'static str {
    match value {
        ClaimType::Summary => "summary",
        ClaimType::Decision => "decision",
        ClaimType::Action => "action",
        ClaimType::Proposal => "proposal",
        ClaimType::Question => "question",
        ClaimType::Point => "point",
    }
}

fn evidence_state_name(value: ClaimEvidenceState) -> &'static str {
    match value {
        ClaimEvidenceState::Located => "located",
    }
}

/// Folds `(source_turn_index, claim_ordinal)` pairs -- gathered while opening
/// this response's own `claims`, one pair per locator -- into one entry per
/// cited transcript turn.
///
/// A pure derivation over data the caller already validated: nothing here
/// touches storage, re-derives a claim, or looks anything up a second time.
/// Roadmap intake I4 / design D4's reverse half, "Extends the existing
/// claim→source link on the existing surface": this is that same link, read
/// backwards.
fn turns_cited(citations: &[(u32, u64)]) -> Vec<TurnCitation> {
    let mut by_turn: std::collections::BTreeMap<u32, Vec<u64>> = std::collections::BTreeMap::new();
    for (turn, ordinal) in citations {
        let ordinals = by_turn.entry(*turn).or_default();
        if !ordinals.contains(ordinal) {
            ordinals.push(*ordinal);
        }
    }
    for ordinals in by_turn.values_mut() {
        ordinals.sort_unstable();
    }
    by_turn
        .into_iter()
        .map(|(turn, claim_ordinals)| TurnCitation {
            turn,
            claim_ordinals,
        })
        .collect()
}

/// Re-slices every locator's already-verified turn text, batched for one
/// claim rather than fetched one locator at a time (design intake D5,
/// decision 2).
///
/// A locator is silently skipped -- never turns this whole note `stale`, and
/// never panics -- when its turn cannot currently be re-sliced: gated,
/// missing, or an out-of-range scalar span. Verified (not merely asserted):
/// every one of these three is unreachable through this crate's own
/// projector API, by construction, not by convention --
/// `note_projection::parse_locator` already rejects an out-of-range span or a
/// digest mismatch at claim-projection time, before a claim is ever stored,
/// and `source_turn_index` is assigned only from a turn's `visible_index`,
/// which a gated turn never has. A transcript rewrite that could otherwise
/// desync a stored locator from its turn changes `transcript_sha256`, which
/// fails this response's own snapshot-authority check first. This function
/// stays defensive anyway, matching `open_claim_evidence_excluding`'s own
/// gated check: cheap, and the honest fallback if any of those upstream
/// invariants is ever relaxed. Confirmed empirically while developing this: a
/// projector fixture emitting an out-of-range locator fails
/// `LibraryProjection::rebuild_with_projector` itself with
/// `LibraryReadError::ArtifactUnavailable`, well before this function would
/// ever run.
fn claim_locator_spans(
    locators: &[OpenedClaimLocator],
    turn_texts: &HashMap<u32, (String, bool)>,
) -> Vec<LibraryClaimSpan> {
    let mut spans = Vec::with_capacity(locators.len());
    for locator in locators {
        let Some((text, gated)) = turn_texts.get(&locator.source_turn_index) else {
            continue;
        };
        if *gated {
            continue;
        }
        let Some(sliced) = library_claim_span_slice(text, locator.start, locator.end) else {
            continue;
        };
        spans.push(LibraryClaimSpan {
            source_turn_index: locator.source_turn_index,
            start: locator.start,
            end: locator.end,
            text: sliced,
        });
    }
    spans
}

/// Mirrors `session-core`'s private `scalar_slice` exactly (char-indexed, not
/// byte-indexed, matching how every stored locator's `start`/`end` was
/// produced). Duplicated rather than exposed as `pub` from that crate: this
/// batched read stays local to one reader's note response, re-slicing text
/// this same reader already holds current and validated, not a second digest
/// check or a new cross-crate authority.
fn library_claim_span_slice(text: &str, start: u64, end: u64) -> Option<String> {
    if start >= end || end > text.chars().count() as u64 {
        return None;
    }
    Some(
        text.chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use local_meeting_notes_session_core::meeting::{
        AudioRetention, MeetingArtifacts, MeetingRecord, MeetingSchema, artifact_ref,
        retention_policy_sha256, write_meeting,
    };
    use local_meeting_notes_session_core::storage::{
        StorageRoot, create_private_dir, durable_create_new,
    };
    use serde_json::json;
    use sha2::Digest;
    use tempfile::TempDir;

    use super::*;

    const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";

    struct Fixture {
        _temporary: TempDir,
        storage: StorageRoot,
        directory: std::path::PathBuf,
    }

    /// Its one turn is three words, which is under `MIN_TITLE_WORDS`, so every
    /// test using it sees a meeting with no derivable title. Use
    /// [`fixture_with_turn`] where the title is the subject.
    fn fixture(
        state: AudioState,
        rule: AudioRetentionRule,
        microphone: &[u8],
        system: &[u8],
    ) -> Fixture {
        fixture_with_turn(state, rule, microphone, system, "synthetic exact words")
    }

    fn fixture_with_turn(
        state: AudioState,
        rule: AudioRetentionRule,
        microphone: &[u8],
        system: &[u8],
        turn_text: &str,
    ) -> Fixture {
        let temporary = TempDir::new().unwrap();
        let protected = temporary.path().join("protected");
        create_private_dir(&protected).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &protected).unwrap();
        let directory = storage.path().join("meetings").join(MEETING_ID);
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();
        let policy_sha256 = retention_policy_sha256(&rule);
        let attempt = serde_json::to_vec_pretty(&json!({
            "schema": "capture-attempt/1",
            "meeting_id": MEETING_ID,
            "attempt_id": "22222222-2222-4222-8222-222222222222",
            "created_at_epoch_seconds": 1_728_000_000_u64,
            "application_build_sha256": "a".repeat(64),
            "participant_notice_version": "internal-transcript-alpha/1",
            "operator_attestation": {
                "participantsConsented": true,
                "headphones": true,
                "operatorAlone": true
            },
            "retention_policy_sha256": policy_sha256,
        }))
        .unwrap();
        durable_create_new(&directory.join("attempt.json"), &attempt).unwrap();
        let transcript_bytes = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [{
                "start": 0.0,
                "end": 1.0,
                "speaker": "Me",
                "text": turn_text
            }]
        }))
        .unwrap();
        let transcript_relative = format!(
            "transcript/{:x}.json",
            sha2::Sha256::digest(&transcript_bytes)
        );

        let lifecycle = if state == AudioState::NeverCreated {
            MeetingLifecycle::Incomplete
        } else {
            create_private_dir(&directory.join("deletion")).unwrap();
            create_private_dir(&directory.join("transcript")).unwrap();
            durable_create_new(&directory.join("ownership.json"), b"ownership").unwrap();
            durable_create_new(&directory.join("capture/session.json"), b"session").unwrap();
            durable_create_new(&directory.join("capture/mic.wav"), microphone).unwrap();
            durable_create_new(&directory.join("capture/system.wav"), system).unwrap();
            durable_create_new(&directory.join(&transcript_relative), &transcript_bytes).unwrap();
            MeetingLifecycle::TranscriptReady
        };
        let deletion_receipt = if state == AudioState::Released {
            fs::remove_file(directory.join("capture/mic.wav")).unwrap();
            fs::remove_file(directory.join("capture/system.wav")).unwrap();
            durable_create_new(&directory.join("deletion/audio-deletion.json"), b"released")
                .unwrap();
            Some(artifact_ref(&directory, "deletion/audio-deletion.json").unwrap())
        } else {
            None
        };
        let next_deletion_at_epoch_seconds = match rule {
            AudioRetentionRule::DeleteAfter { .. } if state != AudioState::NeverCreated => {
                Some(1_728_000_060)
            }
            AudioRetentionRule::DeleteAfter { .. } | AudioRetentionRule::UntilManualDeletion => {
                None
            }
        };
        let record = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: MEETING_ID.into(),
            lifecycle,
            retention: AudioRetention {
                policy_sha256,
                rule,
                next_deletion_at_epoch_seconds,
                state,
                deletion_receipt,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: (state != AudioState::NeverCreated)
                    .then(|| artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: (state != AudioState::NeverCreated)
                    .then(|| artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: (state != AudioState::NeverCreated).then(|| ArtifactRef {
                    relative_path: "capture/mic.wav".into(),
                    sha256: format!("{:x}", sha2::Sha256::digest(microphone)),
                }),
                system_audio: (state != AudioState::NeverCreated).then(|| ArtifactRef {
                    relative_path: "capture/system.wav".into(),
                    sha256: format!("{:x}", sha2::Sha256::digest(system)),
                }),
                current_transcript: (state != AudioState::NeverCreated)
                    .then(|| artifact_ref(&directory, &transcript_relative).unwrap()),
                current_note: None,
            },
            pending_storage_operation: (state == AudioState::Deleting).then_some(
                local_meeting_notes_session_core::meeting::PendingStorageOperation::AudioDeletionV1,
            ),
        };
        write_meeting(&directory, &record).unwrap();
        Fixture {
            _temporary: temporary,
            storage,
            directory,
        }
    }

    /// A `Ready` meeting with four transcript turns, one of them withheld, so
    /// a locator's projection-input `turn` (an index into the worker-visible
    /// turns only) and its real `sourceTurnIndex` (an index into every turn,
    /// withheld ones included) genuinely differ. That gap is exactly what a
    /// reverse-index bug would get wrong silently: using the wrong one still
    /// looks plausible unless a withheld turn sits between two visible ones.
    ///
    /// Turns, by their real (source) index: 0 "alpha" (visible), 1 "redacted"
    /// (withheld), 2 "gamma" (visible), 3 "delta" (visible). Fed to the
    /// projector as the visible-only array `["alpha", "gamma", "delta"]`, so
    /// the worker's own `turn` ordinals 0/1/2 name source turns 0/2/3.
    fn claims_fixture() -> Fixture {
        let temporary = TempDir::new().unwrap();
        let protected = temporary.path().join("protected");
        create_private_dir(&protected).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &protected).unwrap();
        let directory = storage.path().join("meetings").join(MEETING_ID);
        for child in ["", "capture", "transcript", "deletion", "notes"] {
            create_private_dir(&directory.join(child)).unwrap();
        }
        let rule = AudioRetentionRule::UntilManualDeletion;
        let policy_sha256 = retention_policy_sha256(&rule);
        let attempt = serde_json::to_vec_pretty(&json!({
            "schema": "capture-attempt/1",
            "meeting_id": MEETING_ID,
            "attempt_id": "22222222-2222-4222-8222-222222222222",
            "created_at_epoch_seconds": 1_728_000_000_u64,
            "application_build_sha256": "a".repeat(64),
            "participant_notice_version": "internal-transcript-alpha/1",
            "operator_attestation": {
                "participantsConsented": true,
                "headphones": true,
                "operatorAlone": true
            },
            "retention_policy_sha256": policy_sha256,
        }))
        .unwrap();
        durable_create_new(&directory.join("attempt.json"), &attempt).unwrap();
        durable_create_new(&directory.join("ownership.json"), b"ownership").unwrap();
        durable_create_new(&directory.join("capture/session.json"), b"session").unwrap();
        durable_create_new(&directory.join("deletion/audio-deletion.json"), b"released").unwrap();
        let transcript_bytes = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [
                {"start": 0.0, "end": 0.5, "speaker": "Me", "text": "alpha", "gated": false},
                {"start": 0.5, "end": 1.0, "speaker": "Me", "text": "redacted", "gated": true},
                {"start": 1.0, "end": 1.5, "speaker": "Me", "text": "gamma", "gated": false},
                {"start": 1.5, "end": 2.0, "speaker": "Me", "text": "delta", "gated": false}
            ]
        }))
        .unwrap();
        let transcript_relative = format!(
            "transcript/{:x}.json",
            sha2::Sha256::digest(&transcript_bytes)
        );
        durable_create_new(&directory.join(&transcript_relative), &transcript_bytes).unwrap();
        let note_json = b"{}\n";
        let note_markdown = b"# private\n";
        let note_json_relative = format!(
            "notes/{:x}.json",
            sha2::Sha256::digest(note_json.as_slice())
        );
        let note_markdown_relative = format!(
            "notes/{:x}.md",
            sha2::Sha256::digest(note_markdown.as_slice())
        );
        durable_create_new(&directory.join(&note_json_relative), note_json).unwrap();
        durable_create_new(&directory.join(&note_markdown_relative), note_markdown).unwrap();
        let transcript_sha256 = format!("{:x}", sha2::Sha256::digest(&transcript_bytes));
        let record = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: MEETING_ID.into(),
            lifecycle: MeetingLifecycle::Ready,
            retention: AudioRetention {
                policy_sha256,
                rule,
                next_deletion_at_epoch_seconds: None,
                state: AudioState::Released,
                deletion_receipt: Some(
                    artifact_ref(&directory, "deletion/audio-deletion.json").unwrap(),
                ),
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                // Metadata only: `Ready` requires both legs recorded, but the
                // bytes need not exist on disk once retention state is
                // `Released` -- `verify_record_artifacts` only opens audio
                // bytes while `Retained`.
                microphone_audio: Some(ArtifactRef {
                    relative_path: "capture/mic.wav".into(),
                    sha256: "b".repeat(64),
                }),
                system_audio: Some(ArtifactRef {
                    relative_path: "capture/system.wav".into(),
                    sha256: "c".repeat(64),
                }),
                current_transcript: Some(artifact_ref(&directory, &transcript_relative).unwrap()),
                current_note: Some(local_meeting_notes_session_core::meeting::NoteRevisionRef {
                    json: artifact_ref(&directory, &note_json_relative).unwrap(),
                    markdown: artifact_ref(&directory, &note_markdown_relative).unwrap(),
                    source_transcript_sha256: transcript_sha256,
                }),
            },
            pending_storage_operation: None,
        };
        write_meeting(&directory, &record).unwrap();
        Fixture {
            _temporary: temporary,
            storage,
            directory,
        }
    }

    /// Projects three claims over [`claims_fixture`]'s transcript, fixed so
    /// the reverse index has one turn multiple claims cite (source turn 0,
    /// "alpha"), one claim citing two different turns (the point, citing
    /// source turns 0 and 3), and one visible turn no claim cites at all
    /// (source turn 2, "gamma") -- the three shapes the merge gate requires.
    struct TurnCitationProjector;

    impl NoteProjector for TurnCitationProjector {
        fn project(
            &self,
            request: &local_meeting_notes_session_core::note_projection::ProjectRequest,
        ) -> Result<
            Vec<u8>,
            local_meeting_notes_session_core::note_projection::ProjectTransportError,
        > {
            let alpha_sha256 = format!("{:x}", sha2::Sha256::digest(b"alpha"));
            let delta_sha256 = format!("{:x}", sha2::Sha256::digest(b"delta"));
            let decision_text = "decide on alpha";
            let action_text = "follow up on alpha too";
            let point_text = "alpha and delta both matter";
            let decision_sha256 = format!("{:x}", sha2::Sha256::digest(decision_text.as_bytes()));
            let action_sha256 = format!("{:x}", sha2::Sha256::digest(action_text.as_bytes()));
            let point_sha256 = format!("{:x}", sha2::Sha256::digest(point_text.as_bytes()));
            // Worker-visible `turn` 0 is source turn 0 ("alpha"); worker-visible
            // `turn` 2 is source turn 3 ("delta"). Worker-visible `turn` 1
            // ("gamma", source turn 2) appears in no locator below.
            let locator_alpha =
                format!("{{\"turn\":0,\"start\":0,\"end\":5,\"text_sha256\":\"{alpha_sha256}\"}}");
            let locator_delta =
                format!("{{\"turn\":2,\"start\":0,\"end\":5,\"text_sha256\":\"{delta_sha256}\"}}");
            let claim_decision = format!(
                "{{\"claim_ordinal\":0,\"claim_sha256\":\"{decision_sha256}\",\"claim_type\":\"decision\",\"evidence_state\":\"located\",\"claim\":\"{decision_text}\",\"locators\":[{locator_alpha}]}}"
            );
            let claim_action = format!(
                "{{\"claim_ordinal\":1,\"claim_sha256\":\"{action_sha256}\",\"claim_type\":\"action\",\"evidence_state\":\"located\",\"claim\":\"{action_text}\",\"locators\":[{locator_alpha}]}}"
            );
            let claim_point = format!(
                "{{\"claim_ordinal\":2,\"claim_sha256\":\"{point_sha256}\",\"claim_type\":\"point\",\"evidence_state\":\"located\",\"claim\":\"{point_text}\",\"locators\":[{locator_alpha},{locator_delta}]}}"
            );
            Ok(format!(
                "{{\"schema\":\"note-projection-result/1\",\"request_id\":\"{}\",\"operation\":\"note.project\",\"outcome\":\"succeeded\",\"projection\":{{\"schema\":\"note-claim-projection/1\",\"note_json_sha256\":\"{}\",\"note_markdown_sha256\":\"{}\",\"transcript_sha256\":\"{}\",\"claims\":[{claim_decision},{claim_action},{claim_point}]}},\"failure\":null}}\n",
                request.request_id,
                request.note_json_sha256,
                request.note_markdown_sha256,
                request.transcript_sha256,
            )
            .into_bytes())
        }
    }

    #[test]
    fn reverse_index_folds_claims_by_the_real_turn_not_the_projection_ordinal() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(TurnCitationProjector),
        )
        .unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let handle = snapshot.rows[0].handle.clone();

        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(note.state, "note");
        assert_eq!(note.claims.len(), 3);

        // Source turn 0 ("alpha") is cited by all three claims.
        let turn_zero = note
            .turns_cited
            .iter()
            .find(|citation| citation.turn == 0)
            .expect("source turn 0 is cited");
        assert_eq!(turn_zero.claim_ordinals, vec![0, 1, 2]);

        // The point claim (ordinal 2) also cites source turn 3 ("delta"),
        // proving one claim can appear under more than one turn.
        let turn_three = note
            .turns_cited
            .iter()
            .find(|citation| citation.turn == 3)
            .expect("source turn 3 is cited");
        assert_eq!(turn_three.claim_ordinals, vec![2]);

        // Source turn 2 ("gamma") is visible but cited by nothing, and must
        // be absent rather than present with an empty list -- absence is how
        // the shell tells "uncited" from "not yet checked."
        assert!(!note.turns_cited.iter().any(|citation| citation.turn == 2));

        // The withheld turn (source index 1) never enters a claim's locators
        // at all, so it cannot appear here either. Asserting there are
        // exactly two entries (turns 0 and 3) closes the set: nothing beyond
        // the two turns actually cited leaked in, in particular not the
        // withheld turn's real index or the projector's own visible-turn
        // ordinal for it (which does not exist, since it was excluded).
        assert_eq!(note.turns_cited.len(), 2);
    }

    #[test]
    fn reverse_index_disappears_with_claims_when_the_projection_goes_stale() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(TurnCitationProjector),
        )
        .unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let handle = snapshot.rows[0].handle.clone();

        let fresh = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(fresh.state, "note");
        assert!(!fresh.turns_cited.is_empty());

        // Change the admitted note's own JSON bytes on disk: the note's
        // digest no longer matches `meeting.json`, so the whole projection --
        // claims and their reverse index alike -- must read as current no
        // longer, not merely thin on data.
        fs::write(fixture.directory.join("notes").join(
            format!("{:x}.json", sha2::Sha256::digest(b"{}\n".as_slice())),
        ), b"{\"changed\":true}\n")
        .unwrap();

        let stale = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(stale.state, "stale");
        assert!(stale.claims.is_empty());
        assert!(
            stale.turns_cited.is_empty(),
            "a stale note must carry no reverse citations, exactly like it carries no claims"
        );
    }

    // Design intake D5, decision 2: every claim's locators arrive with their
    // quoted transcript text already batched onto the note response, so a
    // hover preview needs no per-claim round trip and burns no single-use
    // evidence handle.
    #[test]
    fn claim_spans_batch_every_locator_quoted_text_by_source_turn() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(TurnCitationProjector),
        )
        .unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let handle = snapshot.rows[0].handle.clone();

        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(note.state, "note");

        // The decision claim (ordinal 0) cites one locator, source turn 0
        // ("alpha"). Its span text is the exact retained transcript text this
        // reader already validated -- not re-derived, not truncated.
        let decision = note
            .claims
            .iter()
            .find(|claim| claim.ordinal == 0)
            .expect("decision claim is present");
        assert_eq!(decision.spans.len(), 1);
        assert_eq!(decision.spans[0].source_turn_index, 0);
        assert_eq!(decision.spans[0].start, 0);
        assert_eq!(decision.spans[0].end, 5);
        assert_eq!(decision.spans[0].text, "alpha");

        // The point claim (ordinal 2) cites two locators across two source
        // turns (0 "alpha" and 3 "delta"), proving spans batch every locator
        // of a claim, not only its first.
        let point = note
            .claims
            .iter()
            .find(|claim| claim.ordinal == 2)
            .expect("point claim is present");
        assert_eq!(point.spans.len(), 2);
        assert!(
            point
                .spans
                .iter()
                .any(|span| span.source_turn_index == 0 && span.text == "alpha")
        );
        assert!(
            point
                .spans
                .iter()
                .any(|span| span.source_turn_index == 3 && span.text == "delta")
        );
    }

    /// Projects one overview (`summary`) claim ahead of one decision claim,
    /// over [`claims_fixture`]'s transcript -- the same ordinal precedence
    /// `worker/note_validator.py`'s `admit_note_claims` gives a real note
    /// (every overview sentence admitted before any decision, action,
    /// proposal, or question), which is what lets design intake D1's row
    /// preview trust "first `Summary` claim" as "the overview's first
    /// sentence."
    struct SummaryProjector;

    impl NoteProjector for SummaryProjector {
        fn project(
            &self,
            request: &local_meeting_notes_session_core::note_projection::ProjectRequest,
        ) -> Result<
            Vec<u8>,
            local_meeting_notes_session_core::note_projection::ProjectTransportError,
        > {
            let alpha_sha256 = format!("{:x}", sha2::Sha256::digest(b"alpha"));
            let summary_text = "This meeting covered the alpha rollout timeline in detail. It also touched on staffing.";
            let decision_text = "decide on alpha";
            let summary_sha256 = format!("{:x}", sha2::Sha256::digest(summary_text.as_bytes()));
            let decision_sha256 = format!("{:x}", sha2::Sha256::digest(decision_text.as_bytes()));
            let locator_alpha =
                format!("{{\"turn\":0,\"start\":0,\"end\":5,\"text_sha256\":\"{alpha_sha256}\"}}");
            let claim_summary = format!(
                "{{\"claim_ordinal\":0,\"claim_sha256\":\"{summary_sha256}\",\"claim_type\":\"summary\",\"evidence_state\":\"located\",\"claim\":\"{summary_text}\",\"locators\":[{locator_alpha}]}}"
            );
            let claim_decision = format!(
                "{{\"claim_ordinal\":1,\"claim_sha256\":\"{decision_sha256}\",\"claim_type\":\"decision\",\"evidence_state\":\"located\",\"claim\":\"{decision_text}\",\"locators\":[{locator_alpha}]}}"
            );
            Ok(format!(
                "{{\"schema\":\"note-projection-result/1\",\"request_id\":\"{}\",\"operation\":\"note.project\",\"outcome\":\"succeeded\",\"projection\":{{\"schema\":\"note-claim-projection/1\",\"note_json_sha256\":\"{}\",\"note_markdown_sha256\":\"{}\",\"transcript_sha256\":\"{}\",\"claims\":[{claim_summary},{claim_decision}]}},\"failure\":null}}\n",
                request.request_id,
                request.note_json_sha256,
                request.note_markdown_sha256,
                request.transcript_sha256,
            )
            .into_bytes())
        }
    }

    /// Design intake D1, the present case: a row for a meeting with a current
    /// admitted note carries the overview's first sentence, read from the
    /// same digest-verified claim projection `open_note` already trusts.
    #[test]
    fn library_row_carries_the_note_overviews_first_sentence_as_its_preview() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(SummaryProjector),
        )
        .unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        assert_eq!(
            snapshot.rows[0].note_preview.as_deref(),
            Some("This meeting covered the alpha rollout timeline in detail.")
        );
    }

    /// Design intake D1, the absent case: a meeting with no current admitted
    /// note (here, `TranscriptReady`, produced by [`fixture`] with no
    /// `current_note`) carries no preview field at all -- never a
    /// placeholder line standing in for one.
    #[test]
    fn library_row_carries_no_preview_when_the_meeting_has_no_current_note() {
        let fixture = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert_eq!(row.note_preview, None);
    }

    /// Design intake D1, the failed-verification case. A `Ready` meeting's
    /// claims are projected and digest-verified once, at rebuild time --
    /// there is no separate "list the row, then verify the note" step for a
    /// preview to fall behind -- so a projection that cannot be verified
    /// never reaches the point of producing a row with a blank or guessed
    /// preview. It fails the whole rebuild instead.
    #[test]
    fn a_readys_meeting_note_that_cannot_be_verified_fails_the_whole_rebuild() {
        let fixture = claims_fixture();
        // The default projector (`UnavailableProjector`) always fails to
        // transport, the same shape a broken or refused verification takes.
        match LibraryProjection::rebuild(&fixture.storage, Default::default()) {
            Err(error) => assert_eq!(error, LibraryReadError::ArtifactUnavailable),
            Ok(_) => panic!("an unavailable projector must not silently produce an empty note"),
        }
    }

    /// `note_preview_for` fails closed on a lookup it cannot resolve against
    /// this exact snapshot, rather than guessing -- the same rule a stale or
    /// mismatched handle follows everywhere else in this module.
    #[test]
    fn note_preview_for_returns_none_when_the_meeting_id_does_not_resolve() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(SummaryProjector),
        )
        .unwrap();
        assert_eq!(note_preview_for(&projection, "not-a-real-meeting-id"), None);
    }

    #[test]
    fn first_sentence_preview_leaves_a_short_single_sentence_claim_unchanged() {
        assert_eq!(
            first_sentence_preview("We reviewed Q3 pricing."),
            "We reviewed Q3 pricing."
        );
    }

    #[test]
    fn first_sentence_preview_stops_at_the_first_sentence_terminator() {
        assert_eq!(
            first_sentence_preview(
                "First point stands alone. Second point never appears here."
            ),
            "First point stands alone."
        );
    }

    #[test]
    fn first_sentence_preview_caps_a_long_run_on_claim_at_a_word_boundary_with_an_ellipsis() {
        let long = "word ".repeat(40).trim().to_owned();
        let preview = first_sentence_preview(&long);
        assert!(preview.ends_with('…'));
        assert!(preview.chars().count() <= NOTE_PREVIEW_MAX_CHARS + 1);
        let without_ellipsis = preview.trim_end_matches('…');
        assert!(
            long.starts_with(without_ellipsis),
            "the cut text must be an exact prefix of the source, never a mid-word slice"
        );
        assert!(!without_ellipsis.ends_with(' '));
    }

    #[test]
    fn note_response_carries_written_meeting_context_read_only() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        crate::meeting_context::write(&fixture.directory, "what must get decided").unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader.snapshot(&HashSet::new()).rows[0].handle.clone();

        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(note.meeting_context.text, "what must get decided");
        assert!(!note.meeting_context.unreadable);
    }

    #[test]
    fn note_response_reports_no_context_as_empty_not_unreadable() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader.snapshot(&HashSet::new()).rows[0].handle.clone();

        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert!(note.meeting_context.text.is_empty());
        assert!(!note.meeting_context.unreadable);
    }

    #[test]
    fn note_response_reports_an_unparseable_context_file_as_unreadable_not_missing() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        fs::write(fixture.directory.join("meeting-context.json"), b"{ not context").unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader.snapshot(&HashSet::new()).rows[0].handle.clone();

        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert!(note.meeting_context.unreadable);
        // Distinct from empty: an unreadable context must never surface bytes
        // from a file this build could not parse.
        assert!(note.meeting_context.text.is_empty());
    }

    fn tree_bytes(path: &Path) -> Vec<(String, Vec<u8>)> {
        fn visit(root: &Path, path: &Path, entries: &mut Vec<(String, Vec<u8>)>) {
            let mut children = fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                if child.is_dir() {
                    visit(root, &child, entries);
                } else {
                    entries.push((
                        child.strip_prefix(root).unwrap().display().to_string(),
                        fs::read(&child).unwrap(),
                    ));
                }
            }
        }
        let mut entries = Vec::new();
        visit(path, path, &mut entries);
        entries
    }

    #[test]
    fn retention_projection_reports_exact_two_leg_bytes_and_scheduled_deadline() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::DeleteAfter { seconds: 60 },
            &[1; 19],
            &[2; 23],
        );

        let retention = LibraryReader::read_audio_retention(&fixture.storage, MEETING_ID);

        assert_eq!(retention.state, "retained");
        assert_eq!(retention.policy, "scheduled");
        assert_eq!(retention.deadline_epoch_seconds, Some(1_728_000_060));
        assert_eq!(retention.retained_bytes, Some(42));
        assert!(
            !serde_json::to_string(&retention)
                .unwrap()
                .contains("capture/")
        );
    }

    /// The three label sources, in the order a row consults them.
    ///
    /// Written because the row that shipped before this had exactly one
    /// source, it had no writer, and so every meeting in the library read
    /// `Untitled meeting` — the fallback was unreachable and the identical
    /// rows were the product.
    #[test]
    fn a_row_is_named_by_the_operator_then_by_its_own_first_words_then_by_nothing() {
        let short = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&short.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(short.storage.clone(), projection);
        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert_eq!(row.label, None, "three words do not name a meeting");
        assert_eq!(row.label_source, "date");

        let spoken = fixture_with_turn(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
            "We should walk through the migration plan today. Ravi has the numbers.",
        );
        let projection = LibraryProjection::rebuild(&spoken.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(spoken.storage.clone(), projection);
        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert_eq!(
            row.label.as_deref(),
            Some("We should walk through the migration plan today")
        );
        assert_eq!(row.label_source, "derived");

        let library = spoken.storage.path().join("library");
        create_private_dir(&library).unwrap();
        durable_create_new(
            &library.join("metadata.json"),
            format!(
                r#"{{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{{"meeting_id":"{MEETING_ID}","title":"Migration review","folder_id":null}}]}}"#
            )
            .as_bytes(),
        )
        .unwrap();
        let projection = LibraryProjection::rebuild(&spoken.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(spoken.storage.clone(), projection);
        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert_eq!(
            row.label.as_deref(),
            Some("Migration review"),
            "a title the operator wrote outranks one the meeting said"
        );
        assert_eq!(row.label_source, "operator");
    }

    /// "A shell that never lies" is a shipped property, and a list quietly
    /// showing a fraction of itself breaks it. The counts and the sentence are
    /// both asserted, because a number in a corner is easy to miss and the
    /// message is where the reader is already looking.
    #[test]
    fn a_filtered_snapshot_states_how_many_it_is_hiding() {
        let fixture = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let all = reader.snapshot(&HashSet::new());
        assert_eq!((all.total, all.shown), (1, 1));
        assert!(!all.filter_active);
        assert!(!all.message.contains("Showing"));

        // A range that excludes the only meeting.
        let args = LibraryFilterArgs {
            start_epoch_seconds: Some(1_900_000_000),
            ..Default::default()
        };
        let filtered = reader.snapshot_filtered(&HashSet::new(), &args.to_filter());
        assert_eq!((filtered.total, filtered.shown), (1, 0));
        assert!(filtered.filter_active);
        assert!(filtered.rows.is_empty());
        assert!(
            filtered.message.contains("No meeting matches this filter"),
            "{}",
            filtered.message
        );
        assert!(
            filtered.message.contains("1 are retained"),
            "an empty filtered list must not read as an empty library"
        );
    }

    #[test]
    fn unfiled_wins_over_a_folder_id_because_the_two_contradict() {
        let args = LibraryFilterArgs {
            folder_id: Some("11111111-1111-4111-8111-111111111111".into()),
            unfiled: Some(true),
            ..Default::default()
        };
        assert_eq!(args.to_filter().folder, FolderFilter::Unfiled);

        let args = LibraryFilterArgs {
            folder_id: Some("11111111-1111-4111-8111-111111111111".into()),
            unfiled: Some(false),
            ..Default::default()
        };
        assert!(matches!(args.to_filter().folder, FolderFilter::Named(_)));
    }

    #[test]
    fn a_whitespace_only_title_box_is_not_a_question() {
        for value in ["", "   ", "\t"] {
            let args = LibraryFilterArgs {
                title: Some(value.into()),
                ..Default::default()
            };
            assert_eq!(args.to_filter().title, None, "{value:?}");
            assert!(args.to_filter().is_empty());
        }
    }

    #[test]
    fn retained_meeting_gets_a_single_use_deletion_handle_not_generic_handle_authority() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let generic_handle = snapshot.rows[0].handle.clone();

        let refused = reader.authorize_audio_deletion(&generic_handle, &HashSet::new());
        assert_eq!(refused.state, "stale");
        assert_eq!(refused.meeting_id, None);

        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let deletion_handle = note
            .audio_deletion_handle
            .expect("retained meeting deletion handle");
        let authorized = reader.authorize_audio_deletion(&deletion_handle, &HashSet::new());
        assert_eq!(authorized.state, "authorized");
        assert_eq!(authorized.meeting_id.as_deref(), Some(MEETING_ID));

        let reused = reader.authorize_audio_deletion(&deletion_handle, &HashSet::new());
        assert_eq!(reused.state, "stale");
        assert_eq!(reused.meeting_id, None);
    }

    #[test]
    fn opening_a_source_keeps_same_snapshot_deletion_controls_authorized() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let transcript_handle = note.transcript_handle.unwrap();
        let audio_handle = note.audio_deletion_handle.unwrap();
        reader
            .open_transcript_bound(&transcript_handle, &HashSet::new(), |_, _, _| ())
            .unwrap();
        assert_eq!(
            reader
                .authorize_audio_deletion(&audio_handle, &HashSet::new())
                .state,
            "authorized"
        );

        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let transcript_handle = note.transcript_handle.unwrap();
        let deletion_handle = note.transcript_deletion_handle.unwrap();
        reader
            .open_transcript_bound(&transcript_handle, &HashSet::new(), |_, _, _| ())
            .unwrap();
        assert_eq!(
            reader
                .authorize_transcript_deletion(&deletion_handle, &HashSet::new())
                .state,
            "authorized"
        );
    }

    #[test]
    fn retention_identities_do_not_invalidate_the_navigated_generation() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let handle = snapshot.rows[0].handle.clone();

        let identities = reader.retention_identities(&HashSet::new()).unwrap();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].0, MEETING_ID);

        // The overview read minted no handle and cleared none: the snapshot
        // generation the operator is navigating still opens.
        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert_ne!(note.state, "stale");

        let retention = LibraryReader::read_audio_retention(&fixture.storage, MEETING_ID);
        assert_eq!(retention.state, "retained");
        assert_eq!(retention.retained_bytes, Some(19 + 23));
    }

    #[test]
    fn active_set_change_stales_even_an_empty_search_before_input_validation() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage, projection);

        let response = reader.search("", &HashSet::from([MEETING_ID.to_owned()]));

        assert_eq!(response.state, "stale");
        assert!(response.results.is_empty());
        assert_eq!(reader.retained_handle_count(), 0);
    }

    #[test]
    fn active_set_change_stales_search_result_and_note_handles() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let changed = HashSet::from([MEETING_ID.to_owned()]);

        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let search = reader.search("exact", &HashSet::new());
        assert_eq!(search.state, "results");
        assert_eq!(
            reader
                .open_search_result(&search.results[0].handle, &changed)
                .state,
            "stale"
        );

        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage, projection);
        let snapshot = reader.snapshot(&HashSet::new());
        assert_eq!(
            reader.open_note(&snapshot.rows[0].handle, &changed, None).state,
            "stale"
        );
    }

    /// Roadmap intake I5's search-path hardening, end to end through the real
    /// `meeting_lock` module rather than a synthetic exclusion set (that layer
    /// is proven directly in `library_read::tests`). This backend has no
    /// reachable caller today (`preview_library_search` /
    /// `preview_library_open_search_result` stay unregistered in `main.rs`),
    /// but the exclusion must hold structurally for whenever it does.
    #[test]
    fn locking_a_meeting_removes_it_from_search_and_a_stale_hit_refuses_to_open() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let before = reader.search("exact", &HashSet::new());
        assert_eq!(before.state, "results");
        assert_eq!(before.results.len(), 1);
        let stale_handle = before.results[0].handle.clone();

        crate::meeting_lock::write(&fixture.directory, true).unwrap();

        // The already-sealed hit refuses rather than reopening the meeting it
        // named -- the race where a meeting was unlocked when a search sealed
        // this handle and became locked before it was opened.
        assert_eq!(
            reader
                .open_search_result(&stale_handle, &HashSet::new())
                .state,
            "stale"
        );

        // And the meeting is gone from search entirely, not merely
        // unreachable through the one handle that predates the lock.
        let locked = reader.search("exact", &HashSet::new());
        assert_ne!(locked.state, "results");
        assert!(locked.results.is_empty());

        // Unlocking re-admits it on the next search -- no separate
        // re-admission path.
        crate::meeting_lock::write(&fixture.directory, false).unwrap();
        let after = reader.search("exact", &HashSet::new());
        assert_eq!(after.state, "results");
        assert_eq!(after.results.len(), 1);
    }

    #[test]
    fn transcript_loader_runs_only_for_an_inactive_current_generation() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let transcript_handle = note.transcript_handle.unwrap();
        let called = Cell::new(false);

        let loaded = reader
            .open_transcript_bound(&transcript_handle, &HashSet::new(), |_, meeting_id, _| {
                called.set(true);
                meeting_id.to_owned()
            })
            .unwrap();
        assert_eq!(loaded, MEETING_ID);
        assert!(called.get());

        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage, projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let transcript_handle = note.transcript_handle.unwrap();
        called.set(false);
        let refused = reader.open_transcript_bound(
            &transcript_handle,
            &HashSet::from([MEETING_ID.to_owned()]),
            |_, _, _| called.set(true),
        );
        assert_eq!(refused.unwrap_err().state, "stale");
        assert!(!called.get());
    }

    #[test]
    fn a_retained_meeting_note_edit_handle_is_single_use_and_scoped() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let generic_handle = reader.snapshot(&HashSet::new()).rows[0].handle.clone();
        let called = Cell::new(false);
        let generic_refused = reader.open_operator_note_bound(
            &generic_handle,
            &HashSet::new(),
            |_, _| called.set(true),
        );
        assert_eq!(generic_refused.unwrap_err().state, "stale");
        assert!(!called.get());

        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let note_handle = note
            .operator_note_handle
            .expect("readable meeting gets an edit handle");
        let playback_handle = note
            .microphone_playback_handle
            .expect("retained meeting gets a microphone playback handle");
        let transcript_handle = note
            .transcript_handle
            .expect("retained meeting gets a transcript handle");
        reader
            .open_transcript_bound(&transcript_handle, &HashSet::new(), |_, _, _| ())
            .unwrap();
        reader
            .authorize_audio_playback(&playback_handle, &HashSet::new(), None)
            .expect("listening keeps the adjacent personal-note handle valid");
        let saved = reader
            .open_operator_note_bound(&note_handle, &HashSet::new(), |storage, meeting_id| {
                called.set(true);
                let directory = meeting_dir(storage, meeting_id).unwrap();
                crate::operator_note::write(&directory, "capture the budget risk").unwrap();
                crate::operator_note::read(&directory)
            })
            .unwrap();
        assert!(called.get());
        assert_eq!(saved.text, "capture the budget risk");

        let reused = reader.open_operator_note_bound(&note_handle, &HashSet::new(), |_, _| ());
        assert_eq!(reused.unwrap_err().state, "stale");

        let snapshot = reader.snapshot(&HashSet::new());
        let refreshed = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        assert_eq!(refreshed.operator_note.text, "capture the budget risk");
        let active_handle = refreshed.operator_note_handle.unwrap();
        called.set(false);
        let active_refused = reader.open_operator_note_bound(
            &active_handle,
            &HashSet::from([MEETING_ID.to_owned()]),
            |_, _| called.set(true),
        );
        assert_eq!(active_refused.unwrap_err().state, "stale");
        assert!(!called.get());
    }

    #[test]
    fn response_reads_preserve_exact_storage_bytes() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let before = tree_bytes(fixture.storage.path());
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);
        let transcript_handle = note.transcript_handle.unwrap();
        let transcript_bytes = reader
            .open_transcript_bound(
                &transcript_handle,
                &HashSet::new(),
                |storage, meeting_id, artifact| {
                    let directory = meeting_dir(storage, meeting_id).unwrap();
                    fs::read(directory.join(&artifact.relative_path)).unwrap()
                },
            )
            .unwrap();
        assert!(!transcript_bytes.is_empty());
        assert_eq!(reader.search("", &HashSet::new()).state, "invalid");

        assert_eq!(tree_bytes(fixture.storage.path()), before);
    }

    #[test]
    fn released_meeting_has_no_audio_deletion_handle() {
        let fixture = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 19],
            &[2; 23],
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage, projection);
        let snapshot = reader.snapshot(&HashSet::new());
        let note = reader.open_note(&snapshot.rows[0].handle, &HashSet::new(), None);

        assert_eq!(note.audio_retention.state, "released");
        assert_eq!(note.audio_deletion_handle, None);
    }

    #[test]
    fn manual_released_never_created_and_deleting_are_content_free_states() {
        let manual = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            &[1; 44],
            &[2; 48],
        );
        let released = fixture(
            AudioState::Released,
            AudioRetentionRule::DeleteAfter { seconds: 60 },
            &[1; 44],
            &[2; 48],
        );
        let never_created = fixture(
            AudioState::NeverCreated,
            AudioRetentionRule::UntilManualDeletion,
            &[],
            &[],
        );
        let deleting = fixture(
            AudioState::Deleting,
            AudioRetentionRule::DeleteAfter { seconds: 60 },
            &[1; 44],
            &[2; 48],
        );

        let manual = LibraryReader::read_audio_retention(&manual.storage, MEETING_ID);
        let released = LibraryReader::read_audio_retention(&released.storage, MEETING_ID);
        let never_created = LibraryReader::read_audio_retention(&never_created.storage, MEETING_ID);
        let deleting = LibraryReader::read_audio_retention(&deleting.storage, MEETING_ID);

        assert_eq!(
            (manual.state, manual.policy, manual.deadline_epoch_seconds),
            ("retained", "manual", None)
        );
        assert_eq!(released.state, "released");
        assert_eq!(released.retained_bytes, None);
        assert_eq!(never_created.state, "not-recorded");
        assert_eq!(deleting.state, "deleting");
    }

    #[test]
    fn changed_missing_or_symlinked_audio_fails_closed() {
        for mutation in ["changed", "missing", "symlink"] {
            let fixture = fixture(
                AudioState::Retained,
                AudioRetentionRule::UntilManualDeletion,
                &[1; 44],
                &[2; 48],
            );
            let microphone = fixture.directory.join("capture/mic.wav");
            match mutation {
                "changed" => fs::write(&microphone, b"changed").unwrap(),
                "missing" => fs::remove_file(&microphone).unwrap(),
                "symlink" => {
                    fs::remove_file(&microphone).unwrap();
                    let target = fixture.directory.join("capture/not-audio");
                    durable_create_new(&target, b"not audio").unwrap();
                    symlink(target, microphone).unwrap();
                }
                _ => unreachable!(),
            }

            let retention = LibraryReader::read_audio_retention(&fixture.storage, MEETING_ID);
            assert_eq!(retention.state, "unavailable", "{mutation}");
            assert_eq!(retention.retained_bytes, None, "{mutation}");
        }
    }

    #[test]
    fn retained_audio_playback_grant_is_source_scoped_and_single_use() {
        let microphone = b"microphone recording";
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            microphone,
            b"system recording",
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage, projection);

        let active = HashSet::from([MEETING_ID.to_owned()]);
        assert_eq!(
            reader.retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &active,
            ),
            None,
            "an active meeting cannot mint playback authority"
        );

        let handle = reader
            .retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &HashSet::new(),
            )
            .expect("verified retained microphone gets a native handle");
        let grant = reader
            .authorize_audio_playback(&handle, &HashSet::new(), None)
            .expect("current verified artifact grants playback");
        assert_eq!(grant.source(), RetainedAudioSource::Microphone);
        let mut file = grant.file();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, microphone);

        let system_handle = reader
            .retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::System,
                &HashSet::new(),
            )
            .expect("verified retained system audio gets a distinct native handle");
        let system_grant = reader
            .authorize_audio_playback(&system_handle, &HashSet::new(), None)
            .expect("current verified system artifact grants playback");
        assert_eq!(system_grant.source(), RetainedAudioSource::System);
        let mut system_file = system_grant.file();
        let mut system_bytes = Vec::new();
        system_file.read_to_end(&mut system_bytes).unwrap();
        assert_eq!(system_bytes, b"system recording");

        assert_eq!(
            reader
                .authorize_audio_playback(&handle, &HashSet::new(), None)
                .unwrap_err()
                .state,
            "stale"
        );
    }

    /// Roadmap intake I5 (a): a locked row shows its title and date and
    /// nothing about what the meeting said. The preview is generated content
    /// about the meeting's outcome, so it is suppressed at source rather than
    /// hidden by the shell -- Bear's obscured-previews move.
    #[test]
    fn a_locked_row_carries_no_note_preview_while_the_same_meeting_unlocked_does() {
        let fixture = claims_fixture();
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            std::sync::Arc::new(SummaryProjector),
        )
        .unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let unlocked = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert!(!unlocked.locked);
        assert!(unlocked.note_preview.is_some());

        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let locked = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert!(locked.locked);
        assert_eq!(locked.note_preview, None);
        // The title and the date are exactly what a locked row still shows.
        assert_eq!(locked.label, unlocked.label);
        assert_eq!(
            locked.created_at_epoch_seconds,
            unlocked.created_at_epoch_seconds
        );
        // And `transcript_available` stays truthful. A field asserting "no
        // transcript" about a meeting that has one would be the same class of
        // thing this product forbids for a withheld turn; the shell's locked
        // branch simply does not render that line.
        assert_eq!(locked.transcript_available, unlocked.transcript_available);
    }

    /// Roadmap intake I5's honest-ceiling follow-up: locking a meeting must
    /// not just suppress its list-row preview (proven above) -- it must remove
    /// whatever `sync_corpus_index` already wrote about it into the derived
    /// corpus index, which lives *outside* the meeting's own directory. This
    /// exercises the real hook end to end: `LibraryReader::rebuild` is what
    /// `lock_meeting`'s invalidation (`main.rs`) causes to run again next, and
    /// `sync_corpus_index` inside it is what actually drops the rows -- not a
    /// bespoke test-only removal path.
    #[test]
    fn locking_a_meeting_drops_its_words_from_the_next_corpus_sync_and_unlocking_restores_them() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );

        // Unlocked: the first rebuild is exactly what happens when the
        // library is opened, and it is the path that syncs the corpus index.
        LibraryReader::rebuild(fixture.storage.clone(), &HashSet::new()).unwrap();
        {
            let index = CorpusIndex::open(&fixture.storage).unwrap();
            assert_eq!(index.meeting_count().unwrap(), 1);
        }

        // Locking, then the next rebuild -- standing in for `lock_meeting`'s
        // `with_preview_library_invalidated` plus the shell's next
        // `library_snapshot` -- must remove it, with no other file touched.
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        LibraryReader::rebuild(fixture.storage.clone(), &HashSet::new()).unwrap();
        {
            let index = CorpusIndex::open(&fixture.storage).unwrap();
            assert_eq!(
                index.meeting_count().unwrap(),
                0,
                "a locked meeting's derived content stayed in the corpus index"
            );
        }

        // Unlocking re-admits it on the next rebuild -- no separate re-add
        // path either.
        crate::meeting_lock::write(&fixture.directory, false).unwrap();
        LibraryReader::rebuild(fixture.storage.clone(), &HashSet::new()).unwrap();
        {
            let index = CorpusIndex::open(&fixture.storage).unwrap();
            assert_eq!(index.meeting_count().unwrap(), 1);
        }
    }

    /// The corpus-index exclusion fails closed exactly like every other
    /// locked-meeting gate: a sidecar this build cannot read is `locked: true`
    /// (`meeting_lock::read`), so it must be excluded from the corpus index
    /// the same as a meeting locked through a normal, readable sidecar.
    #[test]
    fn an_unreadable_lock_sidecar_excludes_the_meeting_from_the_corpus_index_too() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );
        LibraryReader::rebuild(fixture.storage.clone(), &HashSet::new()).unwrap();
        {
            let index = CorpusIndex::open(&fixture.storage).unwrap();
            assert_eq!(index.meeting_count().unwrap(), 1);
        }

        std::fs::write(fixture.directory.join("meeting-lock.json"), b"{ not a lock").unwrap();
        LibraryReader::rebuild(fixture.storage.clone(), &HashSet::new()).unwrap();
        let index = CorpusIndex::open(&fixture.storage).unwrap();
        assert_eq!(
            index.meeting_count().unwrap(),
            0,
            "an unreadable lock sidecar left the meeting's words in the corpus index"
        );
    }

    /// Roadmap intake I5 (b): opening a locked meeting without a confirmation
    /// returns no content **and no capability**. The refusal is not the shell
    /// declining to draw controls -- it is the response never carrying the
    /// authority those controls would spend.
    #[test]
    fn a_locked_meeting_opens_with_no_content_and_no_handle_but_one() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );
        crate::operator_note::write(&fixture.directory, "my private note").unwrap();
        crate::meeting_context::write(&fixture.directory, "why we met").unwrap();
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let handle = reader.snapshot(&HashSet::new()).rows.remove(0).handle;
        let note = reader.open_note(&handle, &HashSet::new(), None);
        assert_eq!(note.state, "locked");
        assert!(note.lock.locked);
        assert!(!note.lock.unreadable);
        assert_eq!(note.transcript_handle, None);
        assert_eq!(note.operator_note_handle, None);
        assert_eq!(note.audio_deletion_handle, None);
        assert_eq!(note.microphone_playback_handle, None);
        assert_eq!(note.system_playback_handle, None);
        assert_eq!(note.transcript_deletion_handle, None);
        assert_eq!(note.regeneration_source_sha256, None);
        assert!(note.claims.is_empty());
        assert!(note.turns_cited.is_empty());
        // The operator's own words -- note and context alike -- do not travel
        // in a locked response.
        assert_eq!(note.operator_note.text, "");
        assert_eq!(note.meeting_context.text, "");
        // Decision 7: deletion is the one act a lock never blocks. Blocking it
        // would make a meeting nobody can confirm for permanently immortal,
        // and moving to Trash is already double-confirmed and recoverable.
        assert!(note.meeting_deletion_handle.is_some());
    }

    /// The same open, with the confirmation the shell just spent. Content
    /// comes back, and so does the full set of capabilities.
    #[test]
    fn a_locked_meeting_opens_normally_for_the_meeting_its_confirmation_names() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );
        crate::operator_note::write(&fixture.directory, "my private note").unwrap();
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let handle = reader.snapshot(&HashSet::new()).rows.remove(0).handle;
        let note = reader.open_note(&handle, &HashSet::new(), Some(MEETING_ID));
        assert_ne!(note.state, "locked");
        assert!(note.lock.locked, "reading it does not clear the lock");
        assert_eq!(note.operator_note.text, "my private note");
        assert!(note.transcript_handle.is_some());
        assert!(note.microphone_playback_handle.is_some());
    }

    /// A confirmation names one meeting. Presenting it against a different
    /// meeting's handle refuses -- which is what stops a token minted for a
    /// meeting the operator is allowed to read from opening another.
    #[test]
    fn a_confirmation_for_another_meeting_does_not_open_this_one() {
        let fixture = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone",
            b"system",
        );
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader.snapshot(&HashSet::new()).rows.remove(0).handle;
        assert_eq!(
            reader
                .open_note(&handle, &HashSet::new(), Some("some-other-meeting"))
                .state,
            "locked"
        );
    }

    /// An unreadable lock file gates exactly like a readable one, and says so
    /// -- the shell needs the second fact to explain why removing the lock is
    /// the only way forward.
    #[test]
    fn an_unreadable_lock_file_still_closes_the_meeting_and_reports_itself() {
        let fixture = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone",
            b"system",
        );
        std::fs::write(fixture.directory.join("meeting-lock.json"), b"{ broken").unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert!(row.locked);
        let note = reader.open_note(&row.handle, &HashSet::new(), None);
        assert_eq!(note.state, "locked");
        assert!(note.lock.unreadable);
    }

    /// Roadmap intake I5 (c): playback is refused for a locked meeting with no
    /// confirmation, and refused with its own state -- "reopen it and try
    /// again" is wrong advice when nothing is out of date.
    #[test]
    fn locked_retained_audio_refuses_without_a_confirmation_and_plays_with_one() {
        let microphone = b"microphone recording";
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            microphone,
            b"system recording",
        );
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let handle = reader
            .retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &HashSet::new(),
            )
            .expect("a handle still mints; the gate is at authorization");
        let refused = reader
            .authorize_audio_playback(&handle, &HashSet::new(), None)
            .expect_err("a locked meeting refuses playback");
        assert_eq!(refused.state, "locked");
        assert_eq!(refused.message, LOCKED_MESSAGE);

        let handle = reader
            .retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &HashSet::new(),
            )
            .unwrap();
        let grant = reader
            .authorize_audio_playback(&handle, &HashSet::new(), Some(MEETING_ID))
            .expect("a confirmation for this meeting plays it");
        let mut bytes = Vec::new();
        grant.file().read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, microphone);
    }

    /// Decision 3: the unlocked default path takes no friction anywhere. This
    /// is the case that would silently regress if a gate were ever written to
    /// require a token unconditionally.
    #[test]
    fn an_unlocked_meeting_needs_no_confirmation_for_reading_or_playback() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);

        let row = reader.snapshot(&HashSet::new()).rows.remove(0);
        assert!(!row.locked);
        let note = reader.open_note(&row.handle, &HashSet::new(), None);
        assert_ne!(note.state, "locked");
        assert!(!note.lock.locked);
        let playback = note
            .microphone_playback_handle
            .expect("retained audio offers playback with no lock in the way");
        assert!(
            reader
                .authorize_audio_playback(&playback, &HashSet::new(), None)
                .is_ok()
        );
    }

    /// Deleting a locked meeting's audio proceeds and leaves the lock exactly
    /// as it was. The retention promise outranks the lock: a meeting nobody
    /// can confirm for must still release its recording on schedule.
    #[test]
    fn releasing_a_locked_meetings_audio_is_not_gated_and_does_not_disturb_the_lock() {
        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone recording",
            b"system recording",
        );
        crate::meeting_lock::write(&fixture.directory, true).unwrap();
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader
            .retain_audio_deletion_handle(MEETING_ID, true)
            .expect("a locked meeting still mints audio-deletion authority");
        let access = reader.authorize_audio_deletion(&handle, &HashSet::new());
        assert_eq!(
            access.state, "authorized",
            "audio release is never behind the lock"
        );
        assert!(crate::meeting_lock::read(&fixture.directory).locked);
    }

    #[test]
    fn retained_audio_playback_rejects_released_and_changed_artifacts() {
        let released = fixture(
            AudioState::Released,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone",
            b"system",
        );
        let projection = LibraryProjection::rebuild(&released.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(released.storage, projection);
        assert_eq!(
            reader.retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &HashSet::new(),
            ),
            None
        );

        let fixture = fixture(
            AudioState::Retained,
            AudioRetentionRule::UntilManualDeletion,
            b"microphone",
            b"system",
        );
        let projection = LibraryProjection::rebuild(&fixture.storage, Default::default()).unwrap();
        let mut reader = LibraryReader::new(fixture.storage.clone(), projection);
        let handle = reader
            .retain_audio_playback_handle(
                MEETING_ID,
                RetainedAudioSource::Microphone,
                &HashSet::new(),
            )
            .unwrap();
        fs::write(fixture.directory.join("capture/mic.wav"), b"changed").unwrap();
        assert_eq!(
            reader
                .authorize_audio_playback(&handle, &HashSet::new(), None)
                .unwrap_err()
                .state,
            "unavailable"
        );
    }
}
