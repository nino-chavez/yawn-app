//! Roadmap intake I7+I8: a completed meeting exports as a folder of plain
//! per-item files a person can read with no Yawn installed, plus one compact
//! archive of the same content.
//!
//! Governing constraints (`docs/roadmap.md`, Category-review intake): "The
//! claim→evidence structure survives export; an export is a local file, not
//! sharing" and "an app-independent format; plain-file trust stated in
//! product copy."
//!
//! **This module is pure assembly.** Every byte it writes is either:
//! - a verbatim copy of an artifact this module has itself digest-verified
//!   against `meeting.json` immediately before reading it (receipts), or
//! - freshly derived from data the caller already digest-verified
//!   (`note.md` from `library_reader`'s `ExportClaim`s, whose claim text and
//!   locator excerpts were proven through
//!   `LibraryProjection::open_claim_evidence_excluding`; `transcript.md` from
//!   `crate::load_transcript_projection`, which re-checks the transcript's
//!   sha256 against the meeting record before parsing it).
//!
//! No artifact here is re-flattened through the note-generation model. An
//! artifact that fails verification is withheld — never exported — and every
//! withholding is named in both `README.txt` and the command's result, never
//! silently.
//!
//! `export/` is replaced wholesale on every export: the previous folder and
//! archive are removed first, so a stale or partial export never survives
//! next to a fresh one.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use local_meeting_notes_session_core::meeting::{
    ArtifactRef, MAX_RECEIPT_BYTES, load_meeting, read_private_bytes, resolve_artifact,
    verify_artifact_ref,
};
use local_meeting_notes_session_core::retention::meeting_dir;
use local_meeting_notes_session_core::storage::{StorageRoot, create_private_dir, durable_create_new};

use crate::library_reader::ExportClaim;
use crate::{TranscriptTurn, load_transcript_projection};

const EXPORT_DIR_NAME: &str = "export";

/// What `export_meeting` actually did, reported back to the operator. Holding
/// only the withheld-artifact manifest — never a path — keeps the command
/// result honest without handing the webview general filesystem knowledge.
pub(crate) struct ExportOutcome {
    pub(crate) withheld: Vec<String>,
}

/// Exports one meeting. `claims` must already be digest-verified (this is
/// exactly what `LibraryReader::open_export_bound` hands the caller) —
/// this function re-verifies every other artifact itself before reading it.
pub(crate) fn export_meeting(
    storage: &StorageRoot,
    meeting_id: &str,
    label: Option<&str>,
    created_at_epoch_seconds: u64,
    claims: &[ExportClaim],
) -> Result<ExportOutcome, String> {
    let directory = meeting_dir(storage, meeting_id)
        .map_err(|_| "This meeting could not be found on this Mac.".to_string())?;
    let meeting = load_meeting(&directory)
        .map_err(|_| "This meeting's record could not be read.".to_string())?;

    let mut withheld: Vec<String> = Vec::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    match &meeting.artifacts.current_transcript {
        Some(reference) => {
            match load_transcript_projection(&directory, meeting_id, reference) {
                Ok((turns, _warnings)) => {
                    files.push(("transcript.md".to_string(), transcript_markdown(&turns).into_bytes()));
                }
                Err(_) => withheld.push(
                    "transcript.md was not included: the retained transcript failed verification."
                        .to_string(),
                ),
            }
        }
        None => withheld.push(
            "transcript.md was not included: this meeting has no retained transcript.".to_string(),
        ),
    }

    if !claims.is_empty() {
        files.push(("note.md".to_string(), note_markdown(claims).into_bytes()));
    } else if meeting.artifacts.current_note.is_some() {
        withheld.push(
            "note.md was not included: this meeting's generated note has no located claims."
                .to_string(),
        );
    } else {
        withheld.push(
            "note.md was not included: no generated note exists for this meeting.".to_string(),
        );
    }

    let operator_note = crate::operator_note::read(&directory);
    if operator_note.unreadable {
        withheld
            .push("your-notes.txt was not included: your saved note could not be read.".to_string());
    } else if !operator_note.text.trim().is_empty() {
        files.push((
            "your-notes.txt".to_string(),
            provenance_text(
                "Written by the operator; not generated.",
                &operator_note.text,
            ),
        ));
    }

    let context = crate::meeting_context::read(&directory);
    if context.unreadable {
        withheld.push(
            "context.txt was not included: the saved pre-meeting context could not be read."
                .to_string(),
        );
    } else if !context.text.trim().is_empty() {
        files.push((
            "context.txt".to_string(),
            provenance_text("Written by the operator; not generated.", &context.text),
        ));
    }

    export_receipt(
        &directory,
        &meeting.artifacts.attempt,
        "receipts/capture-attempt.json",
        &mut files,
        &mut withheld,
    );
    if let Some(session) = &meeting.artifacts.capture_session {
        export_receipt(
            &directory,
            session,
            "receipts/capture-session.json",
            &mut files,
            &mut withheld,
        );
    }
    if let Some(deletion) = &meeting.retention.deletion_receipt {
        export_receipt(
            &directory,
            deletion,
            "receipts/audio-deletion.json",
            &mut files,
            &mut withheld,
        );
    }

    files.push(("README.txt".to_string(), readme_text(&withheld).into_bytes()));

    let export_dir = write_export_folder(&directory, &files)?;
    let archive_name = format!("{}.zip", archive_base_name(label, created_at_epoch_seconds));
    let zip_bytes = build_zip(&files)?;
    durable_create_new(&export_dir.join(&archive_name), &zip_bytes)
        .map_err(|_| "The export archive could not be written.".to_string())?;

    // Revealing the folder is a convenience, not the deliverable: the files
    // are already durably on disk by this point, so a Finder hiccup must not
    // turn a completed export into a reported failure.
    let _ = reveal_in_finder(&export_dir);

    Ok(ExportOutcome { withheld })
}

fn export_receipt(
    directory: &Path,
    reference: &ArtifactRef,
    relative_path: &'static str,
    files: &mut Vec<(String, Vec<u8>)>,
    withheld: &mut Vec<String>,
) {
    match verify_artifact_ref(directory, reference)
        .map_err(|_| ())
        .and_then(|()| resolve_artifact(directory, &reference.relative_path).map_err(|_| ()))
        .and_then(|path| read_private_bytes(&path, MAX_RECEIPT_BYTES).map_err(|_| ()))
    {
        Ok(bytes) => files.push((relative_path.to_string(), bytes)),
        Err(()) => withheld.push(format!("{relative_path} was not included: it failed verification.")),
    }
}

fn provenance_text(provenance: &str, text: &str) -> Vec<u8> {
    format!("{provenance}\n\n{text}").into_bytes()
}

/// Footnote-style `[T{turn}]` markers name transcript turn numbers, matching
/// the numbering `transcript_markdown` gives each line — a reader with only
/// these two files can always resolve a marker by searching for it.
fn note_markdown(claims: &[ExportClaim]) -> String {
    let mut out = String::new();
    out.push_str("# Meeting note\n\n");
    out.push_str(
        "Generated by Yawn from the retained transcript on this Mac. Markers such as [T3] \
         name transcript turn numbers from transcript.md; the Sources section below quotes \
         the exact retained words each claim cites.\n",
    );

    let summary: Vec<&ExportClaim> = claims.iter().filter(|c| c.claim_type == "summary").collect();
    if !summary.is_empty() {
        out.push_str("\n## Overview\n\n");
        for claim in &summary {
            out.push_str(&claim.text);
            out.push_str(&footnote_markers(claim));
            out.push_str("\n\n");
        }
    }

    for (claim_type, title) in [
        ("decision", "Decisions"),
        ("action", "Follow-ups"),
        ("proposal", "Ideas discussed"),
        ("question", "Open questions"),
    ] {
        let group: Vec<&ExportClaim> = claims.iter().filter(|c| c.claim_type == claim_type).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {title}\n\n"));
        for claim in &group {
            out.push_str("- ");
            out.push_str(&claim.text);
            out.push_str(&footnote_markers(claim));
            out.push('\n');
        }
    }

    let highlights: Vec<&ExportClaim> = claims.iter().filter(|c| c.claim_type == "point").collect();
    if !highlights.is_empty() {
        out.push_str("\n## Transcript highlights\n\n");
        for claim in &highlights {
            out.push_str("- ");
            out.push_str(&claim.text);
            out.push_str(&footnote_markers(claim));
            out.push('\n');
        }
    }

    out.push_str("\n## Sources\n");
    for claim in claims {
        out.push_str(&format!("\n### Claim {}\n\n", claim.ordinal + 1));
        for locator in &claim.locators {
            out.push_str(&format!(
                "- [T{turn}] Turn {turn}: \"{text}\"\n",
                turn = locator.source_turn_index,
                text = locator.text
            ));
        }
    }
    out
}

fn footnote_markers(claim: &ExportClaim) -> String {
    let mut markers = String::new();
    for locator in &claim.locators {
        markers.push_str(&format!(" [T{}]", locator.source_turn_index));
    }
    markers
}

/// One line per turn, in the same reading order the app shows. A withheld
/// turn renders as withheld — never omitted, never guessed — matching the
/// product's standing rule that a gap in the record is always stated plainly.
fn transcript_markdown(turns: &[TranscriptTurn]) -> String {
    let mut out = String::new();
    out.push_str("# Meeting transcript\n\n");
    out.push_str(
        "The complete retained record from this Mac. A turn a voice check withheld is marked \
         withheld here, never guessed or left out.\n\n",
    );
    for turn in turns {
        let stamp = timestamp(turn.start);
        if turn.withheld {
            out.push_str(&format!(
                "[T{}] {stamp} (withheld — a voice check set this turn aside)\n\n",
                turn.source_turn_index
            ));
            continue;
        }
        let speaker = turn
            .speaker
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Unattributed");
        out.push_str(&format!(
            "[T{}] {stamp} {speaker}: {}\n\n",
            turn.source_turn_index, turn.text
        ));
    }
    out
}

fn timestamp(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds.floor() as u64
    } else {
        0
    };
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn readme_text(withheld: &[String]) -> String {
    let mut out = String::new();
    out.push_str("This folder was created on this Mac by Yawn.\n");
    out.push_str("note.md is the generated meeting note; transcript.md is the full retained record.\n");
    out.push_str("your-notes.txt and context.txt, if present, are what you personally wrote.\n");
    out.push_str("receipts/ holds plain records of what happened during capture.\n");
    out.push_str("These are ordinary text files you own. Nothing here needs Yawn to be read.\n");
    if !withheld.is_empty() {
        out.push_str("\nNot exported:\n");
        for line in withheld {
            out.push_str("- ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Replaces `export/` wholesale so a stale or partial export never survives
/// next to a fresh one, then writes every file through the same durable,
/// private-mode primitives the rest of the meeting directory is written with.
fn write_export_folder(
    directory: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<PathBuf, String> {
    let export_dir = directory.join(EXPORT_DIR_NAME);
    if export_dir.exists() {
        fs::remove_dir_all(&export_dir)
            .map_err(|_| "The previous export could not be replaced.".to_string())?;
    }
    create_private_dir(&export_dir)
        .map_err(|_| "The export folder could not be created.".to_string())?;
    for (relative, bytes) in files {
        let path = export_dir.join(relative);
        durable_create_new(&path, bytes)
            .map_err(|_| "A file in the export could not be written.".to_string())?;
    }
    Ok(export_dir)
}

/// Builds the archive from the exact same `(relative_path, bytes)` pairs
/// written to disk, so the folder and the archive can never drift apart.
fn build_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write as _};
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (relative, bytes) in files {
            writer
                .start_file(relative.as_str(), options)
                .map_err(|_| "The export archive could not be built.".to_string())?;
            writer
                .write_all(bytes)
                .map_err(|_| "The export archive could not be built.".to_string())?;
        }
        writer
            .finish()
            .map_err(|_| "The export archive could not be built.".to_string())?;
    }
    Ok(cursor.into_inner())
}

#[cfg(target_os = "macos")]
fn reveal_in_finder(path: &Path) -> Result<(), ()> {
    Command::new("/usr/bin/open")
        .arg(path)
        .status()
        .map_err(|_| ())
        .and_then(|status| if status.success() { Ok(()) } else { Err(()) })
}

#[cfg(not(target_os = "macos"))]
fn reveal_in_finder(_path: &Path) -> Result<(), ()> {
    Err(())
}

fn archive_base_name(label: Option<&str>, created_at_epoch_seconds: u64) -> String {
    label
        .map(slug)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("meeting-{}", date_ymd(created_at_epoch_seconds)))
}

fn slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// A dependency-free civil (proleptic Gregorian) date for an epoch-seconds
/// value, used only to name the export archive when no title is available.
/// Howard Hinnant's `civil_from_days` — public-domain, integer-only, and
/// correct for every date this product can produce (no dates before 1970).
fn date_ymd(epoch_seconds: u64) -> String {
    let days = (epoch_seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use local_meeting_notes_session_core::meeting::{
        AudioRetention, MeetingArtifacts, MeetingLifecycle, MeetingRecord, MeetingSchema,
        artifact_ref, retention_policy_sha256, write_meeting,
    };
    use local_meeting_notes_session_core::meeting::{AudioRetentionRule, AudioState};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    const MEETING_ID: &str = "11111111-1111-4111-8111-111111111111";

    /// A minimal but real on-disk meeting: a `TranscriptReady` record with one
    /// retained turn and a capture-session receipt, nothing else. Individual
    /// tests corrupt or add to this before calling `export_meeting`, which is
    /// the only way to exercise its own digest verification and file-writing
    /// rather than the pure Markdown builders above.
    struct Fixture {
        _temporary: TempDir,
        storage: StorageRoot,
        directory: PathBuf,
    }

    fn fixture() -> Fixture {
        let temporary = TempDir::new().unwrap();
        let protected = temporary.path().join("protected");
        create_private_dir(&protected).unwrap();
        let storage = StorageRoot::create(&temporary.path().join("app-data"), &protected).unwrap();
        let directory = storage.path().join("meetings").join(MEETING_ID);
        create_private_dir(&directory).unwrap();
        create_private_dir(&directory.join("capture")).unwrap();

        durable_create_new(
            &directory.join("attempt.json"),
            br#"{"schema":"capture-attempt/1"}"#,
        )
        .unwrap();
        durable_create_new(&directory.join("ownership.json"), b"ownership").unwrap();
        durable_create_new(
            &directory.join("capture/session.json"),
            br#"{"schema":"capture-session/2","paused":false}"#,
        )
        .unwrap();
        let microphone: &[u8] = b"synthetic-microphone-wav-bytes";
        let system: &[u8] = b"synthetic-system-wav-bytes";
        durable_create_new(&directory.join("capture/mic.wav"), microphone).unwrap();
        durable_create_new(&directory.join("capture/system.wav"), system).unwrap();

        let transcript_bytes = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [
                {"start": 0.0, "end": 1.0, "speaker": "Me", "text": "hello there"},
                {"start": 1.0, "end": 2.0, "speaker": "Them", "text": "withheld words", "gated": true},
            ]
        }))
        .unwrap();
        let transcript_relative = format!("transcript/{:x}.json", Sha256::digest(&transcript_bytes));
        durable_create_new(&directory.join(&transcript_relative), &transcript_bytes).unwrap();

        let rule = AudioRetentionRule::UntilManualDeletion;
        let record = MeetingRecord {
            schema: MeetingSchema::V2,
            meeting_id: MEETING_ID.into(),
            lifecycle: MeetingLifecycle::TranscriptReady,
            retention: AudioRetention {
                policy_sha256: retention_policy_sha256(&rule),
                rule,
                next_deletion_at_epoch_seconds: None,
                state: AudioState::Retained,
                deletion_receipt: None,
            },
            artifacts: MeetingArtifacts {
                attempt: artifact_ref(&directory, "attempt.json").unwrap(),
                ownership: Some(artifact_ref(&directory, "ownership.json").unwrap()),
                capture_session: Some(artifact_ref(&directory, "capture/session.json").unwrap()),
                microphone_audio: Some(ArtifactRef {
                    relative_path: "capture/mic.wav".into(),
                    sha256: format!("{:x}", Sha256::digest(microphone)),
                }),
                system_audio: Some(ArtifactRef {
                    relative_path: "capture/system.wav".into(),
                    sha256: format!("{:x}", Sha256::digest(system)),
                }),
                current_transcript: Some(artifact_ref(&directory, &transcript_relative).unwrap()),
                current_note: None,
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

    fn sample_claims() -> Vec<ExportClaim> {
        vec![ExportClaim {
            ordinal: 0,
            claim_type: "decision",
            text: "Ship the beta Friday.".to_string(),
            locators: vec![crate::library_reader::ExportClaimLocator {
                source_turn_index: 0,
                text: "hello there".to_string(),
            }],
        }]
    }

    #[test]
    fn export_meeting_withholds_a_tampered_transcript_and_names_it_in_the_result() {
        let fixture = fixture();
        let meeting = load_meeting(&fixture.directory).unwrap();
        let transcript = meeting.artifacts.current_transcript.as_ref().unwrap();
        // Tamper the retained transcript bytes without touching meeting.json,
        // so its recorded sha256 no longer matches what is on disk -- the
        // same failure mode `open_verified_transcript_file` guards against.
        fs::write(
            fixture.directory.join(&transcript.relative_path),
            b"{ not the verified transcript",
        )
        .unwrap();

        let outcome = export_meeting(&fixture.storage, MEETING_ID, None, 0, &[]).unwrap();

        assert!(outcome.withheld.iter().any(|line| line
            .contains("transcript.md was not included: the retained transcript failed verification.")));
        let export_dir = fixture.directory.join(EXPORT_DIR_NAME);
        assert!(!export_dir.join("transcript.md").exists());
        let readme = fs::read_to_string(export_dir.join("README.txt")).unwrap();
        assert!(readme.contains("transcript.md was not included: the retained transcript failed verification."));
    }

    #[test]
    fn re_export_replaces_the_export_folder_wholesale() {
        let fixture = fixture();
        export_meeting(&fixture.storage, MEETING_ID, Some("Kickoff"), 0, &sample_claims()).unwrap();
        let export_dir = fixture.directory.join(EXPORT_DIR_NAME);
        assert!(export_dir.join("note.md").exists());

        // A stray file simulating leftover state from an earlier export shape
        // (or simply a file the operator dropped in there themselves) must
        // not survive a fresh export.
        fs::write(export_dir.join("stray.txt"), b"leftover").unwrap();
        assert!(export_dir.join("stray.txt").exists());

        export_meeting(&fixture.storage, MEETING_ID, Some("Kickoff"), 0, &sample_claims()).unwrap();
        assert!(!export_dir.join("stray.txt").exists());
        assert!(export_dir.join("note.md").exists());
    }

    #[test]
    fn export_zip_contains_exactly_the_written_folder_files() {
        let fixture = fixture();
        export_meeting(&fixture.storage, MEETING_ID, Some("Kickoff Call"), 0, &sample_claims()).unwrap();
        let export_dir = fixture.directory.join(EXPORT_DIR_NAME);

        let zip_path = export_dir.join("kickoff-call.zip");
        assert!(zip_path.exists());
        let zip_bytes = fs::read(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
        let mut zipped: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        zipped.sort();

        let mut on_disk: Vec<String> = Vec::new();
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().is_some_and(|extension| extension == "zip") {
                    continue;
                }
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        walk(&export_dir, &export_dir, &mut on_disk);
        on_disk.sort();

        assert_eq!(zipped, on_disk);
    }

    #[test]
    fn operator_authored_files_export_only_when_present_with_provenance_headers() {
        let fixture = fixture();
        export_meeting(&fixture.storage, MEETING_ID, None, 0, &[]).unwrap();
        let export_dir = fixture.directory.join(EXPORT_DIR_NAME);
        assert!(!export_dir.join("your-notes.txt").exists());
        assert!(!export_dir.join("context.txt").exists());

        crate::operator_note::write(&fixture.directory, "remember the follow-up").unwrap();
        crate::meeting_context::write(&fixture.directory, "client is evaluating two vendors").unwrap();
        export_meeting(&fixture.storage, MEETING_ID, None, 0, &[]).unwrap();

        let your_notes = fs::read_to_string(export_dir.join("your-notes.txt")).unwrap();
        assert!(your_notes.starts_with("Written by the operator; not generated."));
        assert!(your_notes.contains("remember the follow-up"));
        let context = fs::read_to_string(export_dir.join("context.txt")).unwrap();
        assert!(context.starts_with("Written by the operator; not generated."));
        assert!(context.contains("client is evaluating two vendors"));
    }

    #[test]
    fn date_ymd_matches_known_epoch_seconds() {
        assert_eq!(date_ymd(0), "1970-01-01");
        assert_eq!(date_ymd(1_735_689_600), "2025-01-01");
        assert_eq!(date_ymd(951_782_400), "2000-02-29");
    }

    #[test]
    fn slug_lowercases_and_collapses_separators() {
        assert_eq!(slug("Kickoff: Q3 Plan!!"), "kickoff-q3-plan");
        assert_eq!(slug("   "), "");
        assert_eq!(slug("Café Meeting"), "caf-meeting");
    }

    #[test]
    fn archive_base_name_falls_back_to_date_without_a_label() {
        assert_eq!(archive_base_name(Some("Kickoff"), 0), "kickoff");
        assert_eq!(archive_base_name(Some("   "), 0), "meeting-1970-01-01");
        assert_eq!(archive_base_name(None, 0), "meeting-1970-01-01");
    }

    fn claim(
        ordinal: u64,
        claim_type: &'static str,
        text: &str,
        locators: Vec<(u32, &str)>,
    ) -> ExportClaim {
        ExportClaim {
            ordinal,
            claim_type,
            text: text.to_string(),
            locators: locators
                .into_iter()
                .map(|(turn, text)| crate::library_reader::ExportClaimLocator {
                    source_turn_index: turn,
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn note_markdown_preserves_every_claim_with_its_locator_turns_and_excerpts() {
        let claims = vec![
            claim(0, "summary", "Kickoff went well.", vec![(0, "we kicked off")]),
            claim(
                1,
                "decision",
                "Ship the beta Friday.",
                vec![(2, "we will ship Friday"), (4, "beta scope is frozen")],
            ),
            claim(2, "point", "an unclassified excerpt", vec![(6, "an unclassified excerpt")]),
        ];
        let rendered = note_markdown(&claims);

        assert!(rendered.contains("Kickoff went well. [T0]"));
        assert!(rendered.contains("## Decisions"));
        assert!(rendered.contains("Ship the beta Friday. [T2] [T4]"));
        assert!(rendered.contains("## Transcript highlights"));
        assert!(rendered.contains("an unclassified excerpt [T6]"));

        // Every claim gets its own Sources subsection, and a claim with two
        // locators keeps both — this is the "multiple locators per claim"
        // case the merge gate requires.
        assert!(rendered.contains("### Claim 2\n\n- [T2] Turn 2: \"we will ship Friday\"\n- [T4] Turn 4: \"beta scope is frozen\"\n"));
        assert!(rendered.contains("### Claim 1\n\n- [T0] Turn 0: \"we kicked off\"\n"));
        assert!(rendered.contains("### Claim 3\n\n- [T6] Turn 6: \"an unclassified excerpt\"\n"));
    }

    fn turn(index: u32, speaker: Option<&str>, text: &str, withheld: bool) -> TranscriptTurn {
        TranscriptTurn {
            source_turn_index: index,
            source_speaker: speaker.map(str::to_string),
            speaker: speaker.map(str::to_string),
            speaker_corrected: false,
            start: (index as f64) * 10.0,
            text: text.to_string(),
            withheld,
        }
    }

    #[test]
    fn transcript_markdown_renders_withheld_turns_as_withheld_never_silently() {
        let turns = vec![
            turn(0, Some("Me"), "hello there", false),
            turn(1, None, "", true),
            turn(2, Some("Them"), "sounds good", false),
        ];
        let rendered = transcript_markdown(&turns);
        assert!(rendered.contains("[T0] 00:00 Me: hello there"));
        assert!(rendered.contains("[T1] 00:10 (withheld — a voice check set this turn aside)"));
        assert!(!rendered.contains("[T1]") || rendered.contains("(withheld"));
        assert!(rendered.contains("[T2] 00:20 Them: sounds good"));
        // The withheld turn's text never appears, even though it was set to
        // an empty string here -- this asserts the withheld branch is taken
        // rather than falling through to the ordinary rendering path.
        assert!(!rendered.contains("Them: \n"));
    }

    #[test]
    fn readme_names_every_withheld_artifact_and_stays_honest_when_nothing_is_withheld() {
        let clean = readme_text(&[]);
        assert!(!clean.contains("Not exported"));
        assert!(clean.contains("ordinary text files you own"));

        let partial = readme_text(&["transcript.md was not included: the retained transcript failed verification.".to_string()]);
        assert!(partial.contains("Not exported:"));
        assert!(partial.contains("transcript.md was not included: the retained transcript failed verification."));
    }

    #[test]
    fn build_zip_contains_exactly_the_given_files() {
        let files = vec![
            ("README.txt".to_string(), b"hello".to_vec()),
            ("receipts/capture-attempt.json".to_string(), b"{}".to_vec()),
        ];
        let bytes = build_zip(&files).expect("zip builds");
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("archive reads back");
        let mut names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        names.sort();
        let mut expected: Vec<String> = files.iter().map(|(name, _)| name.clone()).collect();
        expected.sort();
        assert_eq!(names, expected);

        let mut readme = archive.by_name("README.txt").unwrap();
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut readme, &mut contents).unwrap();
        assert_eq!(contents, "hello");
    }
}
