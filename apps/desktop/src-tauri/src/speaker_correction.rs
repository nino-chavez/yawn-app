//! Durable, local speaker-name corrections for one retained transcript.
//!
//! The capture transcript remains immutable. Corrections live in a bounded
//! sidecar and are applied only when its source digest still names the
//! transcript being projected. Each change is retained as an operation so a
//! later correction does not erase the earlier human decision.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use local_meeting_notes_session_core::meeting::read_private_bytes;
use local_meeting_notes_session_core::storage::durable_replace;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const FILE_NAME: &str = "speaker-corrections.json";
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_OPERATIONS: usize = 1_000;
const MAX_LABEL_BYTES: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SpeakerCorrectionsSchema {
    #[serde(rename = "speaker-corrections/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCorrection {
    operation_id: Uuid,
    source_transcript_sha256: String,
    source_speaker: Option<String>,
    replacement: String,
    applied_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCorrections {
    schema: SpeakerCorrectionsSchema,
    meeting_id: Uuid,
    operations: Vec<StoredCorrection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpeakerCorrectionOperation {
    pub(crate) operation_id: Uuid,
    pub(crate) source_transcript_sha256: String,
    pub(crate) source_speaker: Option<String>,
    pub(crate) replacement: String,
    pub(crate) applied_at_epoch_seconds: u64,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_source_speaker(value: Option<&str>) -> bool {
    value.is_none_or(|speaker| matches!(speaker, "Me" | "Them"))
}

pub(crate) fn normalize_replacement(value: &str) -> Result<String, String> {
    let replacement = value.trim();
    if replacement.is_empty()
        || replacement.len() > MAX_LABEL_BYTES
        || replacement.chars().any(char::is_control)
    {
        return Err("Use a speaker name between 1 and 80 characters.".into());
    }
    Ok(replacement.to_owned())
}

fn validate_document(document: &StoredCorrections, meeting_id: Uuid) -> Result<(), ()> {
    if document.meeting_id != meeting_id || document.operations.len() > MAX_OPERATIONS {
        return Err(());
    }
    let mut operation_ids = HashSet::new();
    for operation in &document.operations {
        if !operation_ids.insert(operation.operation_id)
            || !valid_digest(&operation.source_transcript_sha256)
            || !valid_source_speaker(operation.source_speaker.as_deref())
            || operation.applied_at_epoch_seconds == 0
            || normalize_replacement(&operation.replacement).as_deref()
                != Ok(operation.replacement.as_str())
        {
            return Err(());
        }
    }
    Ok(())
}

fn read_document(meeting_dir: &Path, meeting_id: Uuid) -> Result<Option<StoredCorrections>, ()> {
    let path = meeting_dir.join(FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_private_bytes(&path, MAX_FILE_BYTES).map_err(|_| ())?;
    let document: StoredCorrections = serde_json::from_slice(&bytes).map_err(|_| ())?;
    validate_document(&document, meeting_id)?;
    Ok(Some(document))
}

pub(crate) fn current_labels(
    meeting_dir: &Path,
    meeting_id: Uuid,
    source_transcript_sha256: &str,
) -> Result<BTreeMap<Option<String>, String>, ()> {
    if !valid_digest(source_transcript_sha256) {
        return Err(());
    }
    let mut labels = BTreeMap::new();
    let Some(document) = read_document(meeting_dir, meeting_id)? else {
        return Ok(labels);
    };
    for operation in document.operations {
        if operation.source_transcript_sha256 == source_transcript_sha256 {
            let source_label = operation
                .source_speaker
                .as_deref()
                .unwrap_or("Unattributed");
            if operation.replacement == source_label {
                labels.remove(&operation.source_speaker);
            } else {
                labels.insert(operation.source_speaker, operation.replacement);
            }
        }
    }
    Ok(labels)
}

pub(crate) fn append(
    meeting_dir: &Path,
    meeting_id: Uuid,
    operation: SpeakerCorrectionOperation,
) -> Result<(), String> {
    if !valid_digest(&operation.source_transcript_sha256)
        || !valid_source_speaker(operation.source_speaker.as_deref())
        || operation.applied_at_epoch_seconds == 0
    {
        return Err(
            "That speaker correction is no longer valid. Reopen the meeting and try again.".into(),
        );
    }
    let replacement = normalize_replacement(&operation.replacement)?;
    let mut document = match read_document(meeting_dir, meeting_id) {
        Ok(Some(document)) => document,
        Ok(None) => StoredCorrections {
            schema: SpeakerCorrectionsSchema::V1,
            meeting_id,
            operations: Vec::new(),
        },
        Err(()) => {
            return Err(
                "Saved speaker corrections could not be read, so they were not replaced.".into(),
            );
        }
    };
    if document.operations.len() >= MAX_OPERATIONS {
        return Err("This meeting has too many saved speaker corrections to add another.".into());
    }
    if document
        .operations
        .iter()
        .any(|stored| stored.operation_id == operation.operation_id)
    {
        return Err("That speaker correction was already recorded.".into());
    }
    document.operations.push(StoredCorrection {
        operation_id: operation.operation_id,
        source_transcript_sha256: operation.source_transcript_sha256,
        source_speaker: operation.source_speaker,
        replacement,
        applied_at_epoch_seconds: operation.applied_at_epoch_seconds,
    });
    let bytes = serde_json::to_vec(&document)
        .map_err(|_| "That speaker correction could not be saved.".to_string())?;
    durable_replace(&meeting_dir.join(FILE_NAME), &bytes)
        .map_err(|_| "That speaker correction could not be saved.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use local_meeting_notes_session_core::storage::create_private_dir;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, std::path::PathBuf, Uuid) {
        let temporary = TempDir::new().unwrap();
        let directory = temporary.path().join("meeting");
        create_private_dir(&directory).unwrap();
        (temporary, directory, Uuid::new_v4())
    }

    fn operation(source_speaker: Option<&str>, replacement: &str) -> SpeakerCorrectionOperation {
        SpeakerCorrectionOperation {
            operation_id: Uuid::new_v4(),
            source_transcript_sha256: "a".repeat(64),
            source_speaker: source_speaker.map(str::to_owned),
            replacement: replacement.into(),
            applied_at_epoch_seconds: 1,
        }
    }

    #[test]
    fn a_correction_round_trips_without_changing_another_source_group() {
        let (_temporary, directory, meeting_id) = fixture();
        append(&directory, meeting_id, operation(Some("Them"), "Alex")).unwrap();

        let labels = current_labels(&directory, meeting_id, &"a".repeat(64)).unwrap();
        assert_eq!(
            labels.get(&Some("Them".into())).map(String::as_str),
            Some("Alex")
        );
        assert!(!labels.contains_key(&Some("Me".into())));
        assert!(
            current_labels(&directory, meeting_id, &"b".repeat(64))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_later_correction_wins_without_erasing_the_first_operation() {
        let (_temporary, directory, meeting_id) = fixture();
        append(&directory, meeting_id, operation(Some("Them"), "Alex")).unwrap();
        append(&directory, meeting_id, operation(Some("Them"), "Taylor")).unwrap();

        let labels = current_labels(&directory, meeting_id, &"a".repeat(64)).unwrap();
        assert_eq!(
            labels.get(&Some("Them".into())).map(String::as_str),
            Some("Taylor")
        );
        assert_eq!(
            read_document(&directory, meeting_id)
                .unwrap()
                .unwrap()
                .operations
                .len(),
            2
        );
    }

    #[test]
    fn restoring_the_source_label_clears_the_projection_but_keeps_history() {
        let (_temporary, directory, meeting_id) = fixture();
        append(&directory, meeting_id, operation(Some("Them"), "Alex")).unwrap();
        append(&directory, meeting_id, operation(Some("Them"), "Them")).unwrap();

        assert!(
            current_labels(&directory, meeting_id, &"a".repeat(64))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            read_document(&directory, meeting_id)
                .unwrap()
                .unwrap()
                .operations
                .len(),
            2
        );
    }

    #[test]
    fn unreadable_history_is_never_silently_replaced() {
        let (_temporary, directory, meeting_id) = fixture();
        std::fs::write(directory.join(FILE_NAME), b"{not corrections").unwrap();
        assert!(append(&directory, meeting_id, operation(Some("Me"), "Nino")).is_err());
        assert_eq!(
            std::fs::read(directory.join(FILE_NAME)).unwrap(),
            b"{not corrections"
        );
    }

    #[test]
    fn labels_are_trimmed_and_bounded() {
        assert_eq!(normalize_replacement("  Alex  ").unwrap(), "Alex");
        assert!(normalize_replacement("").is_err());
        assert!(normalize_replacement("Alex\nTaylor").is_err());
        assert!(normalize_replacement(&"x".repeat(MAX_LABEL_BYTES + 1)).is_err());
    }
}
