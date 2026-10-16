//! § D. The operator's own note, typed during the meeting.
//!
//! What a person chooses to write down while
//! listening is not recoverable from the recording afterwards, which is the whole
//! argument for the surface.
//!
//! **This is interpretation, not evidence, and it is stored as such.** Every
//! evidence artifact in this product is digest-named and bound into
//! `meeting.json`, so a citation can only ever resolve to bytes that were
//! verified. Nothing cites an operator note. Binding it the same way would
//! rewrite the meeting record on every autosave and strand an orphaned
//! digest-named file behind each one, so it is a fixed path in the meeting
//! directory, replaced atomically. The frozen `meeting/2` contract is untouched,
//! which also means a note can never make a meeting unreadable to a build that
//! predates this module.
//!
//! **The text is operator content and is treated like transcript text.** It is
//! never logged, never placed in a diagnostic, and never carried in an error
//! string. The errors here name the failure, not the note.

use std::path::Path;

use local_meeting_notes_session_core::meeting::read_private_bytes;
use local_meeting_notes_session_core::storage::durable_replace;
use serde::{Deserialize, Serialize};

/// The most a note may hold, in bytes of UTF-8.
///
/// Roughly a quarter of a million characters — far past any meeting's worth of
/// typing, and far under the transcript ceiling. It exists so a runaway writer
/// cannot fill a disk through an autosave, and so the read below is bounded by
/// something other than hope.
const MAX_TEXT_BYTES: usize = 256 * 1024;

/// The read bound, which must exceed `MAX_TEXT_BYTES` by enough for the envelope
/// and JSON string escaping. Escaping can expand a byte to six (`\u00XX`), so the
/// margin is generous rather than tight; a file past this is refused rather than
/// truncated, because half an operator's note is worse than none.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

const FILE_NAME: &str = "operator-note.json";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum NoteSchema {
    #[serde(rename = "operator-note/1")]
    V1,
    /// Adds `moments`. Written only when a note carries at least one moment,
    /// so a note without them stays byte-for-byte a V1 file that an older
    /// build reads as before (product brief, "Note moments on the Mac").
    #[serde(rename = "operator-note/2")]
    V2,
}

/// The moment a line of the operator's note was first typed, in seconds by
/// this Mac's clock since the recording started, pauses included; the
/// capture ledger's pause spans convert it to a position in the audio after
/// the meeting. A line typed after Stop has no
/// moment and is simply absent here. `line` is the zero-based index of the
/// paragraph in `text` split on newlines.
///
/// A moment is a fact about when the operator wrote, never a claim about what
/// was said; nothing here binds it to a transcript turn. The shell pairs a
/// moment with the nearest turn at render time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NoteMoment {
    pub line: u32,
    pub seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredNote {
    schema: NoteSchema,
    text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    moments: Vec<NoteMoment>,
}

/// What the surface is told. Deliberately not the note plus a status string: the
/// shell decides how to present an empty note, and conflating "nothing written"
/// with "nothing readable" is the mistake first run's permission surface had to
/// be corrected for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperatorNote {
    pub text: String,
    /// The moments of the lines typed during capture, ascending by line.
    /// Empty for a note saved before moments existed, and for every line
    /// typed after Stop.
    pub moments: Vec<NoteMoment>,
    /// True when a note exists on disk but could not be read or parsed.
    ///
    /// Separate from an empty note, because the consequences are opposite: an
    /// empty note may be typed into, and an unreadable one must not be — saving
    /// over it would destroy whatever it holds. The shell refuses editing on
    /// this, which is the only reason the flag exists.
    pub unreadable: bool,
}

impl OperatorNote {
    /// Nothing to report: no meeting resolved, so neither a note nor a failure
    /// to read one. Distinct from a meeting with an empty note only in what
    /// produced it, and identical in what a surface should show.
    pub fn none() -> Self {
        Self {
            text: String::new(),
            moments: Vec::new(),
            unreadable: false,
        }
    }
}

pub fn read(meeting_dir: &Path) -> OperatorNote {
    let path = meeting_dir.join(FILE_NAME);
    if !path.exists() {
        return OperatorNote::none();
    }
    // A symlink here is not an operator note. `read_private_bytes` enforces the
    // rest of the safe-file predicate; this module adds no exception to it.
    match read_private_bytes(&path, MAX_FILE_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredNote>(&bytes).ok())
    {
        Some(stored) => OperatorNote {
            text: stored.text,
            moments: stored.moments,
            unreadable: false,
        },
        None => OperatorNote {
            text: String::new(),
            moments: Vec::new(),
            unreadable: true,
        },
    }
}

/// Saves the note with the moments of its timed lines. Moments must name
/// lines that exist in `text`, each line at most once, ascending; anything
/// else is refused rather than stored, because a moment pointing past the
/// text would later pin the wrong line to the wrong second.
pub fn write(meeting_dir: &Path, text: &str, moments: &[NoteMoment]) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES {
        return Err("That note is too long to save.".into());
    }
    let line_count = text.split('\n').count();
    let mut previous: Option<u32> = None;
    for moment in moments {
        let in_order = previous.map_or(true, |last| moment.line > last);
        if !in_order || moment.line as usize >= line_count {
            return Err("That note's moments do not match its lines, so it was not saved.".into());
        }
        previous = Some(moment.line);
    }
    // Refuse to overwrite something this build could not read. The operator did
    // not see its contents, so they cannot have meant to replace them.
    if read(meeting_dir).unreadable {
        return Err("This meeting's note could not be read, so it was not replaced.".into());
    }
    let bytes = serde_json::to_vec(&StoredNote {
        schema: if moments.is_empty() { NoteSchema::V1 } else { NoteSchema::V2 },
        text: text.to_owned(),
        moments: moments.to_vec(),
    })
    .map_err(|_| "That note could not be saved.".to_string())?;
    // Atomic replace: a crash mid-write leaves the previous note, never a
    // half-written one. The window this does lose is the typing since the last
    // save, which is what the shell's autosave interval bounds.
    durable_replace(&meeting_dir.join(FILE_NAME), &bytes)
        .map_err(|_| "That note could not be saved.".to_string())
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
    fn a_meeting_with_no_note_reads_empty_and_not_unreadable() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        assert_eq!(
            read(&directory),
            OperatorNote {
                text: String::new(),
                moments: Vec::new(),
                unreadable: false,
            }
        );
    }

    #[test]
    fn a_note_round_trips_and_replaces_in_place() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "first pass", &[]).unwrap();
        assert_eq!(read(&directory).text, "first pass");
        // Replacement, not accumulation: one file, and the previous contents are
        // gone rather than left beside it under another name.
        write(&directory, "second pass, longer", &[]).unwrap();
        assert_eq!(read(&directory).text, "second pass, longer");
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn an_unreadable_note_is_not_silently_replaced() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(directory.join(FILE_NAME), b"{ not a note").unwrap();

        let found = read(&directory);
        assert!(found.unreadable);
        // Empty text, so nothing a corrupt file happens to contain reaches a
        // surface — and distinctly flagged, so the shell does not offer to type
        // over it.
        assert!(found.text.is_empty());

        assert!(write(&directory, "replacement", &[]).is_err());
        assert_eq!(
            std::fs::read(directory.join(FILE_NAME)).unwrap(),
            b"{ not a note",
            "the unreadable bytes must survive a refused write"
        );
    }

    #[test]
    fn a_note_past_the_ceiling_is_refused_rather_than_truncated() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "kept", &[]).unwrap();
        assert!(write(&directory, &"x".repeat(MAX_TEXT_BYTES + 1), &[]).is_err());
        // Half an operator's note is worse than none, so the refusal leaves the
        // last good one intact.
        assert_eq!(read(&directory).text, "kept");
        assert!(write(&directory, &"x".repeat(MAX_TEXT_BYTES), &[]).is_ok());
    }

    #[test]
    fn a_note_carrying_an_unknown_field_is_refused_rather_than_partly_read() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"operator-note/1","text":"words","extra":1}"#,
        )
        .unwrap();
        // `deny_unknown_fields` again: a note written by a later build carrying a
        // field this one does not understand is unreadable, not partly readable.
        // Failing closed keeps this build from dropping whatever that field held.
        assert!(read(&directory).unreadable);
    }

    fn moments(pairs: &[(u32, u32)]) -> Vec<NoteMoment> {
        pairs.iter().map(|&(line, seconds)| NoteMoment { line, seconds }).collect()
    }

    #[test]
    fn a_note_without_moments_is_still_a_v1_file() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "plain", &[]).unwrap();
        let bytes = std::fs::read(directory.join(FILE_NAME)).unwrap();
        assert_eq!(bytes, br#"{"schema":"operator-note/1","text":"plain"}"#);
        assert!(read(&directory).moments.is_empty());
    }

    #[test]
    fn moments_round_trip_as_v2_and_a_v1_file_reads_with_none() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        let timed = moments(&[(0, 48), (2, 245)]);
        write(&directory, "first\nsecond\nthird", &timed).unwrap();
        let found = read(&directory);
        assert_eq!(found.text, "first\nsecond\nthird");
        assert_eq!(found.moments, timed);
        assert!(!found.unreadable);
        let bytes = std::fs::read(directory.join(FILE_NAME)).unwrap();
        assert!(bytes.starts_with(br#"{"schema":"operator-note/2","#), "{}", String::from_utf8_lossy(&bytes));

        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"operator-note/1","text":"older"}"#,
        )
        .unwrap();
        let older = read(&directory);
        assert_eq!(older.text, "older");
        assert!(older.moments.is_empty());
        assert!(!older.unreadable);
    }

    #[test]
    fn moments_that_do_not_match_the_lines_are_refused() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "kept", &[]).unwrap();
        // Past the end.
        assert!(write(&directory, "one\ntwo", &moments(&[(2, 10)])).is_err());
        // Twice for one line.
        assert!(write(&directory, "one\ntwo", &moments(&[(1, 10), (1, 12)])).is_err());
        // Out of order.
        assert!(write(&directory, "one\ntwo", &moments(&[(1, 10), (0, 12)])).is_err());
        assert_eq!(read(&directory).text, "kept", "a refused save leaves the last good note");
        // The last line is a valid target, including an empty trailing one.
        assert!(write(&directory, "one\ntwo\n", &moments(&[(2, 10)])).is_ok());
    }

    #[test]
    fn a_moment_with_an_unknown_field_is_refused() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"operator-note/2","text":"w","moments":[{"line":0,"seconds":1,"turn":3}]}"#,
        )
        .unwrap();
        assert!(read(&directory).unreadable);
    }
}
