//! Read-only validation for optional anonymous speaker suggestions.
//!
//! The worker writes a content-addressed sidecar beside a meeting. This module
//! does not start diarization, choose a person, or change a transcript. It
//! admits exactly one fully checked suggestion set for the transcript the
//! reader already opened; anything stale, malformed, or ambiguous stays out
//! of the presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use local_meeting_notes_session_core::meeting::{read_private_bytes, require_private_directory};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DIRECTORY: &str = "speaker-suggestions";
const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 16;
const MAX_INTERVALS: usize = 50_000;
const MAX_CLUSTERS: usize = 8;
const MAX_DURATION_SECONDS: f64 = 24.0 * 60.0 * 60.0;
const PRESENCE_ROUNDING_TOLERANCE: f64 = 0.000_000_5;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionTurn {
    pub(crate) source_turn_index: u32,
    pub(crate) clusters: Vec<String>,
    pub(crate) has_overlap: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionSet {
    pub(crate) turns: Vec<SuggestionTurn>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub(crate) enum SuggestionsRead {
    None,
    ReviewRequired { suggestion: SuggestionSet },
    Unavailable { message: String },
}

#[derive(Clone, Copy)]
pub(crate) struct TranscriptMicTurn {
    pub(crate) source_turn_index: u32,
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) is_microphone_turn: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSuggestion {
    schema: String,
    status: String,
    source: StoredSource,
    duration_seconds: f64,
    anonymous_cluster_count: usize,
    intervals: Vec<StoredInterval>,
    mic_turn_suggestions: Vec<StoredTurnSuggestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSource {
    transcript_sha256: String,
    microphone_audio_sha256: String,
    diarizer_executable_sha256: String,
    diarizer_model_sha256: String,
    diarizer_output_sha256: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredInterval {
    start: f64,
    end: f64,
    cluster: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTurnSuggestion {
    source_turn_index: u32,
    speaker_presence: Vec<StoredPresence>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPresence {
    cluster: String,
    seconds: f64,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_cluster(value: &str) -> bool {
    value
        .strip_prefix("cluster-")
        .and_then(|suffix| suffix.parse::<usize>().ok())
        .is_some_and(|number| (1..=MAX_CLUSTERS).contains(&number))
}

fn content_addressed_name(name: &str, bytes: &[u8]) -> bool {
    let Some(digest) = name.strip_suffix(".json") else {
        return false;
    };
    valid_digest(digest) && format!("{:x}", Sha256::digest(bytes)) == digest
}

fn finite(value: f64) -> bool {
    value.is_finite()
}

fn union_seconds(ranges: &mut [(f64, f64)]) -> f64 {
    ranges.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.total_cmp(&right.1)));
    let mut total = 0.0;
    let mut current: Option<(f64, f64)> = None;
    for (start, end) in ranges.iter().copied() {
        match current {
            None => current = Some((start, end)),
            Some((low, high)) if start > high => {
                total += high - low;
                current = Some((start, end));
            }
            Some((low, high)) => current = Some((low, high.max(end))),
        }
    }
    total + current.map_or(0.0, |(start, end)| end - start)
}

fn intervals_overlap_in_turn(intervals: &[StoredInterval], turn: TranscriptMicTurn) -> bool {
    let mut active = Vec::new();
    for interval in intervals {
        let start = interval.start.max(turn.start);
        let end = interval.end.min(turn.end);
        if end > start {
            active.push((start, end, interval.cluster.as_str()));
        }
    }
    active.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut latest_end_by_cluster: BTreeMap<&str, f64> = BTreeMap::new();
    for (start, end, cluster) in active {
        if latest_end_by_cluster
            .iter()
            .any(|(other, latest_end)| *other != cluster && *latest_end > start)
        {
            return true;
        }
        latest_end_by_cluster
            .entry(cluster)
            .and_modify(|latest_end| *latest_end = latest_end.max(end))
            .or_insert(end);
    }
    false
}

fn validate_and_project(
    document: StoredSuggestion,
    transcript_sha256: &str,
    microphone_audio_sha256: &str,
    turns: &[TranscriptMicTurn],
) -> Result<(bool, SuggestionSet), ()> {
    if document.schema != "speaker-diarization-suggestion/1"
        || !valid_digest(transcript_sha256)
        || !valid_digest(microphone_audio_sha256)
        || document.source.transcript_sha256 != transcript_sha256
        || document.source.microphone_audio_sha256 != microphone_audio_sha256
        || ![
            &document.source.diarizer_executable_sha256,
            &document.source.diarizer_model_sha256,
            &document.source.diarizer_output_sha256,
        ]
        .into_iter()
        .all(|digest| valid_digest(digest))
        || !finite(document.duration_seconds)
        || !(0.0..=MAX_DURATION_SECONDS).contains(&document.duration_seconds)
        || document.duration_seconds == 0.0
        || document.intervals.len() > MAX_INTERVALS
        || document.anonymous_cluster_count > MAX_CLUSTERS
        || !matches!(document.status.as_str(), "review-required" | "no-speakers")
    {
        return Err(());
    }

    let mut clusters = BTreeSet::new();
    for interval in &document.intervals {
        if !finite(interval.start)
            || !finite(interval.end)
            || interval.start < 0.0
            || interval.end <= interval.start
            || interval.start >= document.duration_seconds
            || interval.end > document.duration_seconds
            || !valid_cluster(&interval.cluster)
        {
            return Err(());
        }
        clusters.insert(interval.cluster.clone());
    }
    if clusters.len() != document.anonymous_cluster_count
        || clusters.len() > MAX_CLUSTERS
        || clusters
            .iter()
            .enumerate()
            .any(|(index, cluster)| cluster != &format!("cluster-{}", index + 1))
        || (document.status == "review-required") != !clusters.is_empty()
    {
        return Err(());
    }
    let review_required = document.status == "review-required";

    let microphone_turns: BTreeMap<_, _> = turns
        .iter()
        .copied()
        .filter(|turn| {
            turn.is_microphone_turn
                && finite(turn.start)
                && finite(turn.end)
                && turn.start >= 0.0
                && turn.end > turn.start
        })
        .map(|turn| (turn.source_turn_index, turn))
        .collect();
    if microphone_turns.len() != turns.iter().filter(|turn| turn.is_microphone_turn).count()
        || document.mic_turn_suggestions.len() != microphone_turns.len()
    {
        return Err(());
    }

    let mut projected = Vec::with_capacity(document.mic_turn_suggestions.len());
    let mut seen_turns = BTreeSet::new();
    for suggestion in document.mic_turn_suggestions {
        let Some(turn) = microphone_turns.get(&suggestion.source_turn_index).copied() else {
            return Err(());
        };
        if !seen_turns.insert(suggestion.source_turn_index)
            || suggestion.speaker_presence.len() > clusters.len()
        {
            return Err(());
        }
        let mut expected: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
        for interval in &document.intervals {
            let start = interval.start.max(turn.start);
            let end = interval.end.min(turn.end);
            if end > start {
                expected
                    .entry(interval.cluster.clone())
                    .or_default()
                    .push((start, end));
            }
        }
        let mut actual = BTreeMap::new();
        for presence in suggestion.speaker_presence {
            if !clusters.contains(&presence.cluster)
                || !finite(presence.seconds)
                || presence.seconds <= 0.0
                || actual.insert(presence.cluster, presence.seconds).is_some()
            {
                return Err(());
            }
        }
        if actual.len() != expected.len() {
            return Err(());
        }
        for (cluster, mut ranges) in expected {
            let Some(seconds) = actual.get(&cluster) else {
                return Err(());
            };
            if (union_seconds(&mut ranges) - seconds).abs() > PRESENCE_ROUNDING_TOLERANCE {
                return Err(());
            }
        }
        projected.push(SuggestionTurn {
            source_turn_index: suggestion.source_turn_index,
            clusters: actual.into_keys().collect(),
            has_overlap: intervals_overlap_in_turn(&document.intervals, turn),
        });
    }
    if seen_turns.len() != microphone_turns.len() {
        return Err(());
    }
    Ok((review_required, SuggestionSet { turns: projected }))
}

/// Reads one current, fully checked sidecar. Historical sidecars for another
/// transcript or microphone hash are intentionally ignored. More than one
/// current candidate is unavailable: the UI must not pick a run on behalf of
/// the operator.
pub(crate) fn read_current(
    meeting_dir: &Path,
    transcript_sha256: &str,
    microphone_audio_sha256: &str,
    turns: &[TranscriptMicTurn],
) -> SuggestionsRead {
    let directory = meeting_dir.join(DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return SuggestionsRead::None,
        Ok(_) if require_private_directory(&directory).is_err() => {
            return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() };
        }
        Err(_) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data cannot be read.".into() },
        Ok(_) => {}
    }
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(_) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data cannot be read.".into() },
    };
    let mut current = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DOCUMENTS {
            return SuggestionsRead::Unavailable {
                message:
                    "Speaker suggestions are unavailable because too many review files were found."
                        .into(),
            };
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() },
        };
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() },
        };
        let path = entry.path();
        let bytes = match read_private_bytes(&path, MAX_DOCUMENT_BYTES) {
            Ok(bytes) if content_addressed_name(&name, &bytes) => bytes,
            _ => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() },
        };
        let document: StoredSuggestion = match serde_json::from_slice(&bytes) {
            Ok(document) => document,
            Err(_) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() },
        };
        if document.source.transcript_sha256 != transcript_sha256
            || document.source.microphone_audio_sha256 != microphone_audio_sha256
        {
            continue;
        }
        match validate_and_project(document, transcript_sha256, microphone_audio_sha256, turns) {
            Ok(suggestion) => current.push(suggestion),
            Err(()) => return SuggestionsRead::Unavailable { message: "Speaker suggestions are unavailable because their local review data is invalid.".into() },
        }
    }
    match current.len() {
        0 => SuggestionsRead::None,
        1 => match current.pop().expect("checked length") {
            (true, suggestion) => SuggestionsRead::ReviewRequired { suggestion },
            (false, _) => SuggestionsRead::None,
        },
        _ => SuggestionsRead::Unavailable {
            message: "Speaker suggestions need a single review set before they can be shown."
                .into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use local_meeting_notes_session_core::storage::create_private_dir;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn turns() -> Vec<TranscriptMicTurn> {
        vec![
            TranscriptMicTurn {
                source_turn_index: 0,
                start: 0.0,
                end: 2.0,
                is_microphone_turn: true,
            },
            TranscriptMicTurn {
                source_turn_index: 1,
                start: 2.0,
                end: 3.0,
                is_microphone_turn: false,
            },
            TranscriptMicTurn {
                source_turn_index: 2,
                start: 3.0,
                end: 5.0,
                is_microphone_turn: true,
            },
        ]
    }

    fn write_document(root: &Path, value: serde_json::Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        std::fs::write(root.join(format!("{digest}.json")), bytes).unwrap();
        std::fs::set_permissions(
            root.join(format!("{digest}.json")),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }

    fn document() -> serde_json::Value {
        json!({
            "schema": "speaker-diarization-suggestion/1",
            "status": "review-required",
            "source": {
                "transcript_sha256": "a".repeat(64),
                "microphone_audio_sha256": "b".repeat(64),
                "diarizer_executable_sha256": "c".repeat(64),
                "diarizer_model_sha256": "d".repeat(64),
                "diarizer_output_sha256": "e".repeat(64),
            },
            "duration_seconds": 5.0,
            "anonymous_cluster_count": 2,
            "intervals": [
                {"start": 0.5, "end": 1.5, "cluster": "cluster-2"},
                {"start": 1.0, "end": 2.0, "cluster": "cluster-1"},
                {"start": 3.5, "end": 4.5, "cluster": "cluster-2"},
            ],
            "mic_turn_suggestions": [
                {"source_turn_index": 0, "speaker_presence": [
                    {"cluster": "cluster-1", "seconds": 1.0},
                    {"cluster": "cluster-2", "seconds": 1.0},
                ]},
                {"source_turn_index": 2, "speaker_presence": [
                    {"cluster": "cluster-2", "seconds": 1.0},
                ]},
            ],
        })
    }

    #[test]
    fn projects_one_hash_bound_review_set_and_marks_actual_overlap() {
        let temporary = TempDir::new().unwrap();
        let directory = temporary.path().join(DIRECTORY);
        create_private_dir(&directory).unwrap();
        write_document(&directory, document());

        let SuggestionsRead::ReviewRequired { suggestion } =
            read_current(temporary.path(), &"a".repeat(64), &"b".repeat(64), &turns())
        else {
            panic!("expected checked review suggestion")
        };
        assert_eq!(suggestion.turns[0].clusters, ["cluster-1", "cluster-2"]);
        assert!(suggestion.turns[0].has_overlap);
        assert_eq!(suggestion.turns[1].clusters, ["cluster-2"]);
        assert!(!suggestion.turns[1].has_overlap);
    }

    #[test]
    fn refuses_a_content_addressed_file_when_its_turn_presence_does_not_match_intervals() {
        let temporary = TempDir::new().unwrap();
        let directory = temporary.path().join(DIRECTORY);
        create_private_dir(&directory).unwrap();
        let mut invalid = document();
        invalid["mic_turn_suggestions"][0]["speaker_presence"][0]["seconds"] = json!(0.2);
        write_document(&directory, invalid);

        assert!(matches!(
            read_current(temporary.path(), &"a".repeat(64), &"b".repeat(64), &turns()),
            SuggestionsRead::Unavailable { .. }
        ));
    }

    #[test]
    fn ignores_a_valid_historical_sidecar_without_promoting_it_to_the_current_transcript() {
        let temporary = TempDir::new().unwrap();
        let directory = temporary.path().join(DIRECTORY);
        create_private_dir(&directory).unwrap();
        write_document(&directory, document());

        assert!(matches!(
            read_current(temporary.path(), &"f".repeat(64), &"b".repeat(64), &turns()),
            SuggestionsRead::None
        ));
    }

    #[test]
    fn repeated_intervals_from_one_cluster_are_not_speaker_overlap() {
        let turn = turns()[0];
        let same_cluster = vec![
            StoredInterval {
                start: 0.1,
                end: 1.2,
                cluster: "cluster-1".into(),
            },
            StoredInterval {
                start: 0.8,
                end: 1.8,
                cluster: "cluster-1".into(),
            },
        ];
        assert!(!intervals_overlap_in_turn(&same_cluster, turn));
        let different_clusters = vec![
            same_cluster[0].clone(),
            StoredInterval {
                start: 0.8,
                end: 1.8,
                cluster: "cluster-2".into(),
            },
        ];
        assert!(intervals_overlap_in_turn(&different_clusters, turn));
    }
}
