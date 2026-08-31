//! Pre-meeting context: what the operator says the meeting is for, typed
//! before or during capture.
//!
//! Roadmap intake I3, governing constraint verbatim: "Context is a labeled
//! operator input; it never appears as transcript-backed generated content."
//! A generic AI overview misses the meeting's stated purpose; context fixes
//! that without polluting evidence.
//!
//! **This mirrors `operator_note.rs` decision for decision.** Context is
//! interpretation, not evidence, exactly like the operator's own note: nothing
//! in this product cites it, so it is a fixed path in the meeting directory,
//! replaced atomically, rather than a digest-bound artifact threaded through
//! `meeting.json`. The frozen `meeting/2` contract stays untouched, so a
//! meeting carrying context stays readable by a build that predates this
//! module.
//!
//! **The text is operator content and is treated like transcript text.** It is
//! never logged, never placed in a diagnostic, and never carried in an error
//! string. The errors here name the failure, not the context.
//!
//! Where this differs from the operator note: context is allowed to reach a
//! note-generation prompt (see `product_coordinator.rs`'s regeneration
//! re-attestation and `NoteCreateWorkerArgs::pre_meeting_context`). The
//! operator note is never read by generation at all. That is why context gets
//! a smaller byte ceiling than the note: it is a hint carried into a prompt on
//! every regeneration, not a growing personal scratchpad.

use std::path::Path;

use local_meeting_notes_session_core::meeting::read_private_bytes;
use local_meeting_notes_session_core::storage::durable_replace;
use serde::{Deserialize, Serialize};

/// The most context may hold, in bytes of UTF-8.
///
/// Context is a hint, not a document: a few sentences of "why we're meeting"
/// and "what must get decided," not a place to paste an agenda or a document.
/// 16 KiB is generous for that and small next to the operator note's quarter
/// megabyte, because every byte here rides into a note-generation prompt on
/// every regeneration rather than sitting inert until read.
const MAX_TEXT_BYTES: usize = 16 * 1024;

/// The read bound, generous over `MAX_TEXT_BYTES` for the envelope and JSON
/// string escaping, mirroring `operator_note.rs`'s margin exactly.
const MAX_FILE_BYTES: u64 = 64 * 1024;

const FILE_NAME: &str = "meeting-context.json";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum ContextSchema {
    #[serde(rename = "meeting-context/1")]
    V1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredContext {
    schema: ContextSchema,
    text: String,
}

/// What the surface is told. Deliberately not the context plus a status
/// string, mirroring `OperatorNote`: the shell decides how to present an empty
/// context, and conflating "nothing written" with "nothing readable" is the
/// mistake `operator_note.rs` was already corrected for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingContext {
    pub text: String,
    /// True when context exists on disk but could not be read or parsed.
    ///
    /// Separate from empty context, because the consequences are opposite: an
    /// empty context may be typed into, and an unreadable one must not be --
    /// saving over it would destroy whatever it holds. The shell refuses
    /// editing on this, which is the only reason the flag exists.
    pub unreadable: bool,
}

pub fn read(meeting_dir: &Path) -> MeetingContext {
    let path = meeting_dir.join(FILE_NAME);
    if !path.exists() {
        return MeetingContext {
            text: String::new(),
            unreadable: false,
        };
    }
    // A symlink here is not meeting context. `read_private_bytes` enforces the
    // rest of the safe-file predicate; this module adds no exception to it.
    match read_private_bytes(&path, MAX_FILE_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredContext>(&bytes).ok())
    {
        Some(stored) => MeetingContext {
            text: stored.text,
            unreadable: false,
        },
        None => MeetingContext {
            text: String::new(),
            unreadable: true,
        },
    }
}

pub fn write(meeting_dir: &Path, text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES {
        return Err("That context is too long to save.".into());
    }
    // Refuse to overwrite something this build could not read. The operator
    // did not see its contents, so they cannot have meant to replace them.
    if read(meeting_dir).unreadable {
        return Err("This meeting's context could not be read, so it was not replaced.".into());
    }
    let bytes = serde_json::to_vec(&StoredContext {
        schema: ContextSchema::V1,
        text: text.to_owned(),
    })
    .map_err(|_| "That context could not be saved.".to_string())?;
    // Atomic replace: a crash mid-write leaves the previous context, never a
    // half-written one. The window this does lose is the typing since the
    // last save, which is what the shell's autosave interval bounds.
    durable_replace(&meeting_dir.join(FILE_NAME), &bytes)
        .map_err(|_| "That context could not be saved.".to_string())
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
    fn a_meeting_with_no_context_reads_empty_and_not_unreadable() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        assert_eq!(
            read(&directory),
            MeetingContext {
                text: String::new(),
                unreadable: false,
            }
        );
    }

    #[test]
    fn context_round_trips_and_replaces_in_place() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "why we're meeting").unwrap();
        assert_eq!(read(&directory).text, "why we're meeting");
        // Replacement, not accumulation: one file, and the previous contents
        // are gone rather than left beside it under another name.
        write(&directory, "what must get decided, revised").unwrap();
        assert_eq!(read(&directory).text, "what must get decided, revised");
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[test]
    fn an_unreadable_context_is_not_silently_replaced() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(directory.join(FILE_NAME), b"{ not context").unwrap();

        let found = read(&directory);
        assert!(found.unreadable);
        // Empty text, so nothing a corrupt file happens to contain reaches a
        // surface -- and distinctly flagged, so the shell does not offer to
        // type over it.
        assert!(found.text.is_empty());

        assert!(write(&directory, "replacement").is_err());
        assert_eq!(
            std::fs::read(directory.join(FILE_NAME)).unwrap(),
            b"{ not context",
            "the unreadable bytes must survive a refused write"
        );
    }

    #[test]
    fn context_past_the_ceiling_is_refused_rather_than_truncated() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        write(&directory, "kept").unwrap();
        assert!(write(&directory, &"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
        // Half an operator's context is worse than none, so the refusal
        // leaves the last good one intact.
        assert_eq!(read(&directory).text, "kept");
        assert!(write(&directory, &"x".repeat(MAX_TEXT_BYTES)).is_ok());
    }

    #[test]
    fn context_carrying_an_unknown_field_is_refused_rather_than_partly_read() {
        let temporary = meeting();
        let directory = temporary.path().join("meeting");
        std::fs::write(
            directory.join(FILE_NAME),
            br#"{"schema":"meeting-context/1","text":"words","extra":1}"#,
        )
        .unwrap();
        // `deny_unknown_fields` again: context written by a later build
        // carrying a field this one does not understand is unreadable, not
        // partly readable. Failing closed keeps this build from dropping
        // whatever that field held.
        assert!(read(&directory).unreadable);
    }
}
