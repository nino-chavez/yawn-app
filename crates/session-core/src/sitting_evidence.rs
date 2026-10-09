//! Crash-safe evidence store for guided voice-enrolment sittings.
//!
//! A sitting counts as **saved** only when its
//! derived voice material (embeddings plus encoder identity) is durably
//! stored, the dedicated raw recording is deleted under a digest-bound
//! cleanup receipt, and the temporary work directory is gone. Raw audio
//! deleted before derivation leaves nothing a profile can be built from, so
//! that path is labelled a **rehearsal** and never advances enrolment.
//!
//! Layout under the bound app-data root:
//!
//! ```text
//! $APP_DATA/enrollment/
//!   work/<sitting-id>/          crash-safe temporary storage
//!     audio.raw                 the dedicated raw recording (synthetic in tests)
//!     audio.raw.staged          present only mid-cleanup
//!   sittings/<sitting-id>/      durable write-once evidence
//!     sitting.json              identity row: kind, source class, start time
//!     capture.json              finalized raw artifact digest and capture time
//!     segments.json             content-free segment timings bound to the raw digest
//!     embeddings.bin            derived embeddings, bound by digest below
//!     derived.json              encoder + ONNX artifact identity, embeddings digest
//!     cleanup.json              deleting -> staged -> removed receipt (mutable)
//!     rehearsal.json            terminal pre-derivation deletion label
//! ```
//!
//! State is derived from file presence the way `operation_store` derives
//! receipt state, and the cleanup receipt copies the `audio-deletion/1`
//! three-state shape from `retention`: the receipt is written before bytes
//! move, every transition revalidates the presence table, and an on-disk
//! receipt is reconciled before any new work is authorized. The alternating
//! profile-lifecycle journal was deliberately not reused: sittings are an
//! append-only set of write-once rows, not a single mutating authority slot.
//!
//! Everything stored here is content-free evidence: digests, byte counts,
//! segment timings, and identities. No transcript text and no audio content
//! ever enters a JSON row. Raw bytes live only under `work/` and are gone by
//! the time a sitting reports saved.
//!
//! This store is not an admission authority. Its projection feeds
//! `enrollment_guidance::evaluate_enrollment_evidence`, and the canonical
//! loader in `spike/speaker_gate.py` refuses again at profile-build time.
//! Until the preferred ONNX encoder passes both admission checks recorded in
//! `spike/encoder-packaging/RESULTS.md`, only synthetic fixtures may flow
//! through the derivation seam.

#![cfg(target_os = "macos")]

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::enrollment_guidance::{
    DerivedVoiceMaterial, EnrollmentEvidence, MIN_SCORABLE_SECONDS, NegativeSourceEvidence,
    SittingEvidence,
};
use crate::storage::{
    StorageRoot, create_private_dir, durable_create_new, durable_replace, sync_directory,
};

const ENROLLMENT_DIR: &str = "enrollment";
const WORK_DIR: &str = "work";
const SITTINGS_DIR: &str = "sittings";
const RAW_AUDIO_NAME: &str = "audio.raw";
const RAW_STAGED_NAME: &str = "audio.raw.staged";
const SITTING_FILE: &str = "sitting.json";
const CAPTURE_FILE: &str = "capture.json";
const SEGMENTS_FILE: &str = "segments.json";
const EMBEDDINGS_FILE: &str = "embeddings.bin";
const DERIVED_FILE: &str = "derived.json";
const CLEANUP_FILE: &str = "cleanup.json";
const REHEARSAL_FILE: &str = "rehearsal.json";

/// JSON rows share the profile-lifecycle receipt bound.
const RECORD_MAX_BYTES: u64 = 16_384;
/// Segment timing lists grow with sitting length; one hour of dense speech
/// stays far below this.
const SEGMENTS_MAX_BYTES: u64 = 1_048_576;
/// Embedding payloads are `count * dim * 4` bytes; 16 MiB covers thousands of
/// segments at dimension 192.
const EMBEDDINGS_MAX_BYTES: u64 = 16_777_216;
/// One dedicated sitting recording; an hour of 16 kHz float mono is ~230 MB.
const RAW_MAX_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Error)]
pub enum SittingEvidenceError {
    #[error("sitting identifier is not a canonical UUID")]
    MalformedIdentifier,
    #[error("sitting evidence storage is unavailable")]
    Storage,
    #[error("sitting evidence refused: {0}")]
    Refused(&'static str),
    #[error("sitting evidence needs attention: {0}")]
    Quarantined(&'static str),
}

impl From<io::Error> for SittingEvidenceError {
    fn from(_: io::Error) -> Self {
        SittingEvidenceError::Storage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SittingKind {
    OperatorSitting,
    NegativeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SittingLifecycleState {
    /// `sitting.json` exists but no finalized capture does. Never projected as
    /// evidence; a crash here reconciles to a rehearsal.
    RecordingInProgress,
    /// A finalized raw recording awaits derivation. Projects with
    /// `derived: None` — app-side pending work, never an operator errand.
    RawRetained,
    /// Derived material is durably stored but the raw recording is not yet
    /// fully deleted under its receipt. Still projects with `derived: None`,
    /// because the saved-sitting promise includes the deletion.
    CleanupPending,
    /// Derived material stored, raw deleted under a terminal receipt, work
    /// directory absent. The only state that projects derived material.
    Saved,
    /// Deleted before derivation. Terminal; never advances enrolment.
    Rehearsal,
}

/// Content-free per-sitting summary for surfaces that list recorder state.
#[derive(Debug, Clone, PartialEq)]
pub struct SittingRecordSummary {
    pub sitting_id: String,
    pub kind: SittingKind,
    pub source_class: Option<String>,
    pub state: SittingLifecycleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SittingSchema {
    #[serde(rename = "sitting-evidence/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum CaptureSchema {
    #[serde(rename = "sitting-capture/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SegmentsSchema {
    #[serde(rename = "sitting-segments/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum DerivedSchema {
    #[serde(rename = "sitting-derived/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum CleanupSchema {
    #[serde(rename = "sitting-cleanup/1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RehearsalSchema {
    #[serde(rename = "sitting-rehearsal/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    relative_name: String,
    byte_size: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SittingStartedRecord {
    schema: SittingSchema,
    sitting_id: String,
    kind: SittingKind,
    source_class: Option<String>,
    started_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureRecord {
    schema: CaptureSchema,
    sitting_id: String,
    raw: RawArtifact,
    captured_at_epoch_seconds: u64,
}

/// Content-free segment timing. Text never appears here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentSpan {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentsRecord {
    schema: SegmentsSchema,
    sitting_id: String,
    raw_sha256: String,
    segments: Vec<SegmentSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DerivedRecord {
    schema: DerivedSchema,
    sitting_id: String,
    raw_sha256: String,
    encoder_sha256: String,
    /// The converted `.onnx` artifact digest, once the preferred ONNX
    /// candidate is admitted. The checkpoint fingerprint alone does not
    /// identify the converted artifact, so both travel together.
    onnx_artifact_sha256: Option<String>,
    embeddings_sha256: String,
    embedding_count: u32,
    embedding_dim: u32,
    derived_at_epoch_seconds: u64,
}

/// The worker's `sitting.derive` output (`worker/sitting_derivation.py`),
/// read strictly from the WORK directory. It is a claim, never a row: every
/// field is re-verified against this store's own capture record and the
/// runtime manifest before anything durable is written.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerDerivationDocument {
    schema: WorkerDerivationSchema,
    sitting_id: String,
    raw_sha256: String,
    sample_rate: u32,
    samples: u64,
    segments: Vec<SegmentSpan>,
    embedding: WorkerDerivationEmbedding,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
enum WorkerDerivationSchema {
    #[serde(rename = "sitting-derivation/1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerDerivationEmbedding {
    count: u32,
    dim: u32,
    sha256: String,
    encoder_sha256: String,
    onnx_artifact_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CleanupState {
    Deleting,
    Staged,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupReceipt {
    schema: CleanupSchema,
    sitting_id: String,
    raw_sha256: String,
    state: CleanupState,
    artifacts: Vec<RawArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RehearsalReason {
    Abandoned,
    CrashIncomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RehearsalRecord {
    schema: RehearsalSchema,
    sitting_id: String,
    reason: RehearsalReason,
    recorded_raw: Option<RawArtifact>,
    labeled_at_epoch_seconds: u64,
}

fn require_canonical_uuid(value: &str) -> Result<(), SittingEvidenceError> {
    match Uuid::parse_str(value) {
        Ok(parsed) if parsed.to_string() == value => Ok(()),
        _ => Err(SittingEvidenceError::MalformedIdentifier),
    }
}

fn resolve(storage: &StorageRoot, parts: &[&str]) -> Result<PathBuf, SittingEvidenceError> {
    let mut relative = PathBuf::new();
    for part in parts {
        relative.push(part);
    }
    storage
        .resolve(&relative)
        .map_err(|_| SittingEvidenceError::Refused("evidence path was refused"))
}

fn sitting_dir(storage: &StorageRoot, id: &str) -> Result<PathBuf, SittingEvidenceError> {
    resolve(storage, &[ENROLLMENT_DIR, SITTINGS_DIR, id])
}

fn work_dir(storage: &StorageRoot, id: &str) -> Result<PathBuf, SittingEvidenceError> {
    resolve(storage, &[ENROLLMENT_DIR, WORK_DIR, id])
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, SittingEvidenceError> {
    serde_json::to_vec_pretty(value).map_err(|_| SittingEvidenceError::Storage)
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Streamed digest with a hard size bound, for raw audio that should never be
/// held in memory whole. Symlinks are refused twice: a deterministic lstat
/// check first, then `O_NOFOLLOW` on the open guards the race — a link swapped
/// in for a raw artifact must quarantine, because cleanup's rename and unlink
/// would otherwise delete only the link while the receipt claims the bytes.
fn digest_file_bounded(path: &Path, max_bytes: u64) -> Result<(u64, String), SittingEvidenceError> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(SittingEvidenceError::Quarantined(
            "a raw artifact is not a bounded regular file",
        ));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            if error.raw_os_error() == Some(libc::ELOOP) {
                SittingEvidenceError::Quarantined("a raw artifact is not a bounded regular file")
            } else {
                SittingEvidenceError::from(error)
            }
        })?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(SittingEvidenceError::Quarantined(
            "a raw artifact is not a bounded regular file",
        ));
    }
    let mut hasher = Sha256::new();
    let mut reader = io::BufReader::new(file);
    let mut buffer = [0_u8; 65_536];
    let mut total: u64 = 0;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > max_bytes {
            return Err(SittingEvidenceError::Quarantined(
                "a raw artifact grew past its bound during inspection",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

/// Write-side bound: a row the read side would refuse under its size bound
/// must be refused here, at write time, as a bad argument — otherwise one
/// oversized row turns into a permanent fail-closed store on every later read.
fn require_writable_size(bytes: &[u8], max_bytes: u64) -> Result<(), SittingEvidenceError> {
    if bytes.len() as u64 > max_bytes {
        return Err(SittingEvidenceError::Refused(
            "an evidence row exceeds its storage bound",
        ));
    }
    Ok(())
}

/// Write-once with the `operation_store` identity rule: an existing file must
/// hold byte-identical content; different bytes are a conflict, never an
/// overwrite.
fn write_once(path: &Path, bytes: &[u8]) -> Result<(), SittingEvidenceError> {
    match fs::read(path) {
        Ok(existing) => {
            if existing == bytes {
                Ok(())
            } else {
                Err(SittingEvidenceError::Quarantined(
                    "an evidence row already exists with different bytes",
                ))
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            durable_create_new(path, bytes).map_err(SittingEvidenceError::from)
        }
        Err(_) => Err(SittingEvidenceError::Storage),
    }
}

fn read_canonical<T>(path: &Path, max_bytes: u64) -> Result<T, SittingEvidenceError>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(SittingEvidenceError::Quarantined(
            "an evidence row is not a bounded regular file",
        ));
    }
    let bytes = fs::read(path)?;
    let value: T = serde_json::from_slice(&bytes)
        .map_err(|_| SittingEvidenceError::Quarantined("an evidence row failed to decode"))?;
    if canonical_bytes(&value)? != bytes {
        return Err(SittingEvidenceError::Quarantined(
            "an evidence row is not in canonical form",
        ));
    }
    Ok(value)
}

fn path_present(path: &Path) -> Result<bool, SittingEvidenceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(SittingEvidenceError::Storage),
    }
}

fn child_names(path: &Path) -> Result<BTreeSet<String>, SittingEvidenceError> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| SittingEvidenceError::Quarantined("an evidence name is not unicode"))?;
        names.insert(name);
    }
    Ok(names)
}

fn scorable_segments(segments: &[SegmentSpan]) -> (u32, f64) {
    let mut count: u32 = 0;
    let mut seconds: f64 = 0.0;
    for span in segments {
        let duration = span.end_seconds - span.start_seconds;
        if duration >= MIN_SCORABLE_SECONDS {
            count += 1;
            seconds += duration;
        }
    }
    (count, seconds)
}

fn validate_segments(segments: &[SegmentSpan]) -> Result<(), SittingEvidenceError> {
    let mut previous_end = 0.0_f64;
    for span in segments {
        if !span.start_seconds.is_finite()
            || !span.end_seconds.is_finite()
            || span.start_seconds < 0.0
            || span.end_seconds <= span.start_seconds
        {
            return Err(SittingEvidenceError::Refused(
                "segment timings must be finite, ordered, and non-negative",
            ));
        }
        // Segments feed the enrolment-sufficiency thresholds, so duplicated
        // or overlapping spans would inflate evidence. Whisper segments are
        // sequential; require the same here.
        if span.start_seconds < previous_end {
            return Err(SittingEvidenceError::Refused(
                "segment timings must not overlap or repeat",
            ));
        }
        previous_end = span.end_seconds;
    }
    Ok(())
}

struct SittingPaths {
    durable: PathBuf,
    work: PathBuf,
}

fn sitting_paths(storage: &StorageRoot, id: &str) -> Result<SittingPaths, SittingEvidenceError> {
    require_canonical_uuid(id)?;
    Ok(SittingPaths {
        durable: sitting_dir(storage, id)?,
        work: work_dir(storage, id)?,
    })
}

fn ensure_store_directories(storage: &StorageRoot) -> Result<(), SittingEvidenceError> {
    create_private_dir(&resolve(storage, &[ENROLLMENT_DIR])?)?;
    create_private_dir(&resolve(storage, &[ENROLLMENT_DIR, WORK_DIR])?)?;
    create_private_dir(&resolve(storage, &[ENROLLMENT_DIR, SITTINGS_DIR])?)?;
    // Fsyncing a directory persists its entries, not its own link in the
    // parent — so the enrollment directory and the storage root must sync
    // too, or on a fresh store the `work/` chain can survive power loss while
    // the `sittings/` chain is lost, quarantining the store as an orphan
    // work directory.
    sync_directory(&resolve(storage, &[ENROLLMENT_DIR])?)?;
    sync_directory(storage.path())?;
    Ok(())
}

fn create_exclusive_private_dir(path: &Path) -> Result<(), SittingEvidenceError> {
    fs::create_dir(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            SittingEvidenceError::Refused("the sitting already exists")
        } else {
            SittingEvidenceError::Storage
        }
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Starts a sitting: the durable identity row is written before the work
/// directory exists, so an orphan work directory is always a structural
/// surprise rather than an ambiguous crash artifact.
pub(crate) fn begin_sitting(
    storage: &StorageRoot,
    sitting_id: &str,
    kind: SittingKind,
    source_class: Option<&str>,
    started_at_epoch_seconds: u64,
) -> Result<(), SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    match (kind, source_class) {
        (SittingKind::OperatorSitting, None) | (SittingKind::NegativeSource, Some(_)) => {}
        _ => {
            return Err(SittingEvidenceError::Refused(
                "a source class is required exactly for negative-source material",
            ));
        }
    }
    ensure_store_directories(storage)?;
    let record = SittingStartedRecord {
        schema: SittingSchema::V1,
        sitting_id: sitting_id.to_string(),
        kind,
        source_class: source_class.map(str::to_string),
        started_at_epoch_seconds,
    };
    let record_bytes = canonical_bytes(&record)?;
    require_writable_size(&record_bytes, RECORD_MAX_BYTES)?;
    create_exclusive_private_dir(&paths.durable)?;
    // The identity row must be durable — including the directory entries above
    // it — before the work directory can exist on disk after power loss;
    // otherwise startup would find an orphan work directory and quarantine.
    sync_directory(&resolve(storage, &[ENROLLMENT_DIR, SITTINGS_DIR])?)?;
    write_once(&paths.durable.join(SITTING_FILE), &record_bytes)?;
    create_exclusive_private_dir(&paths.work)?;
    sync_directory(&resolve(storage, &[ENROLLMENT_DIR, WORK_DIR])?)?;
    Ok(())
}

/// Appends raw bytes to the dedicated recording. Dev-lane seam: tests feed
/// synthetic fixture bytes; a real recorder feeds capture output. Refused once
/// the capture is finalized.
pub(crate) fn append_raw_audio(
    storage: &StorageRoot,
    sitting_id: &str,
    bytes: &[u8],
) -> Result<(), SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    if !path_present(&paths.durable.join(SITTING_FILE))? {
        return Err(SittingEvidenceError::Refused(
            "the sitting was never started",
        ));
    }
    if path_present(&paths.durable.join(CAPTURE_FILE))? {
        return Err(SittingEvidenceError::Refused(
            "the capture is already finalized",
        ));
    }
    if path_present(&paths.durable.join(REHEARSAL_FILE))? {
        return Err(SittingEvidenceError::Refused("the sitting is a rehearsal"));
    }
    let raw = paths.work.join(RAW_AUDIO_NAME);
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&raw)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Finalizes the capture: digests the raw recording and writes the durable
/// capture row. From here the sitting is raw-retained until derivation.
pub(crate) fn finalize_capture(
    storage: &StorageRoot,
    sitting_id: &str,
    captured_at_epoch_seconds: u64,
) -> Result<(), SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    if !path_present(&paths.durable.join(SITTING_FILE))? {
        return Err(SittingEvidenceError::Refused(
            "the sitting was never started",
        ));
    }
    if path_present(&paths.durable.join(REHEARSAL_FILE))? {
        return Err(SittingEvidenceError::Refused("the sitting is a rehearsal"));
    }
    let raw = paths.work.join(RAW_AUDIO_NAME);
    if !path_present(&raw)? {
        return Err(SittingEvidenceError::Refused(
            "no raw recording exists to finalize",
        ));
    }
    // The raw recording's directory entry must be durable before the capture
    // row is: after power loss, `capture.json` without `audio.raw` would read
    // as unexplained raw loss forever.
    sync_directory(&paths.work)?;
    let (byte_size, sha256) = digest_file_bounded(&raw, RAW_MAX_BYTES)?;
    if byte_size == 0 {
        return Err(SittingEvidenceError::Refused(
            "an empty recording cannot be finalized",
        ));
    }
    let record = CaptureRecord {
        schema: CaptureSchema::V1,
        sitting_id: sitting_id.to_string(),
        raw: RawArtifact {
            relative_name: RAW_AUDIO_NAME.to_string(),
            byte_size,
            sha256,
        },
        captured_at_epoch_seconds,
    };
    write_once(
        &paths.durable.join(CAPTURE_FILE),
        &canonical_bytes(&record)?,
    )?;
    sync_directory(&paths.durable)?;
    Ok(())
}

/// Records content-free segment timings bound to the finalized raw digest.
pub(crate) fn record_segments(
    storage: &StorageRoot,
    sitting_id: &str,
    segments: &[SegmentSpan],
) -> Result<(), SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    validate_segments(segments)?;
    let capture: CaptureRecord =
        read_canonical(&paths.durable.join(CAPTURE_FILE), RECORD_MAX_BYTES)
            .map_err(|_| SittingEvidenceError::Refused("the capture is not finalized"))?;
    let record = SegmentsRecord {
        schema: SegmentsSchema::V1,
        sitting_id: sitting_id.to_string(),
        raw_sha256: capture.raw.sha256,
        segments: segments.to_vec(),
    };
    let record_bytes = canonical_bytes(&record)?;
    require_writable_size(&record_bytes, SEGMENTS_MAX_BYTES)?;
    write_once(&paths.durable.join(SEGMENTS_FILE), &record_bytes)?;
    sync_directory(&paths.durable)?;
    Ok(())
}

/// Persists derived voice material and immediately begins raw cleanup. The
/// derived rows are durable before the first raw byte moves; a cleanup failure
/// leaves the sitting cleanup-pending and separately retryable, mirroring
/// `ProfileEnrollmentCompletion::CleanupPending`.
///
/// The embeddings are opaque bytes bound by digest. Their semantic validity is
/// the worker's verdict at profile-build time; this store only guarantees the
/// bytes cannot change identity unnoticed.
pub(crate) fn store_derived_material(
    storage: &StorageRoot,
    sitting_id: &str,
    embeddings: &[u8],
    encoder_sha256: &str,
    onnx_artifact_sha256: Option<&str>,
    embedding_dim: u32,
    derived_at_epoch_seconds: u64,
) -> Result<SittingLifecycleState, SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    if path_present(&paths.durable.join(REHEARSAL_FILE))? {
        return Err(SittingEvidenceError::Refused("the sitting is a rehearsal"));
    }
    let capture: CaptureRecord =
        read_canonical(&paths.durable.join(CAPTURE_FILE), RECORD_MAX_BYTES)
            .map_err(|_| SittingEvidenceError::Refused("the capture is not finalized"))?;
    let segments: SegmentsRecord =
        read_canonical(&paths.durable.join(SEGMENTS_FILE), SEGMENTS_MAX_BYTES)
            .map_err(|_| SittingEvidenceError::Refused("segment evidence is missing"))?;
    if encoder_sha256.is_empty() {
        return Err(SittingEvidenceError::Refused(
            "derived material requires an encoder identity",
        ));
    }
    if embedding_dim == 0 || embeddings.is_empty() {
        return Err(SittingEvidenceError::Refused(
            "derived material requires embeddings",
        ));
    }
    if embeddings.len() as u64 > EMBEDDINGS_MAX_BYTES {
        return Err(SittingEvidenceError::Refused(
            "embeddings exceed the storage bound",
        ));
    }
    let stride = embedding_dim as usize * 4;
    if !embeddings.len().is_multiple_of(stride) {
        return Err(SittingEvidenceError::Refused(
            "embedding bytes do not divide into whole vectors",
        ));
    }
    let embedding_count = (embeddings.len() / stride) as u32;
    let (scorable_count, _) = scorable_segments(&segments.segments);
    if embedding_count != scorable_count {
        return Err(SittingEvidenceError::Refused(
            "one embedding is required per scorable segment",
        ));
    }
    write_once(&paths.durable.join(EMBEDDINGS_FILE), embeddings)?;
    let record = DerivedRecord {
        schema: DerivedSchema::V1,
        sitting_id: sitting_id.to_string(),
        raw_sha256: capture.raw.sha256.clone(),
        encoder_sha256: encoder_sha256.to_string(),
        onnx_artifact_sha256: onnx_artifact_sha256.map(str::to_string),
        embeddings_sha256: digest_bytes(embeddings),
        embedding_count,
        embedding_dim,
        derived_at_epoch_seconds,
    };
    let record_bytes = canonical_bytes(&record)?;
    require_writable_size(&record_bytes, RECORD_MAX_BYTES)?;
    write_once(&paths.durable.join(DERIVED_FILE), &record_bytes)?;
    sync_directory(&paths.durable)?;
    // A transient storage failure leaves cleanup separately retryable without
    // rolling back the durable derivation, mirroring
    // `ProfileEnrollmentCompletion::CleanupPending`. A detected identity
    // violation is not retryable and must not report as benign pending work.
    match run_cleanup(&paths, &capture) {
        Ok(()) => Ok(SittingLifecycleState::Saved),
        Err(SittingEvidenceError::Storage) => Ok(SittingLifecycleState::CleanupPending),
        Err(error) => Err(error),
    }
}

/// Admits the worker's `sitting.derive` output into the store's durable rows.
///
/// The whole join is verified before the first durable write, so a defective
/// worker artifact can never strand a sitting half-recorded: the document must
/// name this sitting and the exact raw bytes the capture row finalized, the
/// embedding bytes must match their claimed digest and divide into one vector
/// per scorable segment under this store's own rule, and the encoder identity
/// must equal what the runtime manifest expects — the worker's claim about
/// which encoder ran is never taken on its word. Re-admitting an
/// already-derived sitting routes to cleanup retry instead of re-deriving,
/// because the derived rows are write-once and the first admission's
/// timestamp is part of their identity.
pub(crate) fn admit_worker_derivation(
    storage: &StorageRoot,
    sitting_id: &str,
    expected_encoder_sha256: &str,
    derived_at_epoch_seconds: u64,
) -> Result<SittingLifecycleState, SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    if path_present(&paths.durable.join(REHEARSAL_FILE))? {
        return Err(SittingEvidenceError::Refused("the sitting is a rehearsal"));
    }
    if expected_encoder_sha256.is_empty() {
        return Err(SittingEvidenceError::Refused(
            "derived material requires an encoder identity",
        ));
    }
    if path_present(&paths.durable.join(DERIVED_FILE))? {
        return retry_sitting_cleanup(storage, sitting_id);
    }
    let capture: CaptureRecord =
        read_canonical(&paths.durable.join(CAPTURE_FILE), RECORD_MAX_BYTES)
            .map_err(|_| SittingEvidenceError::Refused("the capture is not finalized"))?;

    let document_bytes = read_work_artifact(&paths.work.join(SEGMENTS_FILE), SEGMENTS_MAX_BYTES)?;
    let document: WorkerDerivationDocument =
        serde_json::from_slice(&document_bytes).map_err(|_| {
            SittingEvidenceError::Refused("the worker derivation document is malformed")
        })?;
    let WorkerDerivationSchema::V1 = document.schema;
    if document.sitting_id != sitting_id {
        return Err(SittingEvidenceError::Refused(
            "the worker derivation names another sitting",
        ));
    }
    if document.raw_sha256 != capture.raw.sha256 {
        return Err(SittingEvidenceError::Refused(
            "the worker derivation names different raw bytes",
        ));
    }
    if document.sample_rate != 16_000 || document.samples == 0 {
        return Err(SittingEvidenceError::Refused(
            "the worker derivation names a foreign sample format",
        ));
    }
    if document.embedding.encoder_sha256 != expected_encoder_sha256 {
        return Err(SittingEvidenceError::Refused(
            "the worker derivation encoder disagrees with the runtime manifest",
        ));
    }
    validate_segments(&document.segments)?;
    let (scorable_count, _) = scorable_segments(&document.segments);
    if document.embedding.count == 0
        || document.embedding.dim == 0
        || document.embedding.count != scorable_count
    {
        return Err(SittingEvidenceError::Refused(
            "one embedding is required per scorable segment",
        ));
    }

    let embeddings = read_work_artifact(&paths.work.join(EMBEDDINGS_FILE), EMBEDDINGS_MAX_BYTES)?;
    if digest_bytes(&embeddings) != document.embedding.sha256 {
        return Err(SittingEvidenceError::Refused(
            "derived embeddings disagree with their claimed digest",
        ));
    }
    let stride = document.embedding.dim as usize * 4;
    if embeddings.len() != document.embedding.count as usize * stride {
        return Err(SittingEvidenceError::Refused(
            "the worker derivation embedding join is incoherent",
        ));
    }

    // The cleanup walk rightly refuses to remove a work directory holding
    // entries no receipt names, so the worker's artifacts are consumed here —
    // deleted durably before the rows are written. A crash inside this window
    // loses only the work copies: re-admission refuses on the missing
    // artifacts and the worker's idempotent re-derivation restores them.
    for name in [SEGMENTS_FILE, EMBEDDINGS_FILE] {
        fs::remove_file(paths.work.join(name))?;
    }
    sync_directory(&paths.work)?;

    record_segments(storage, sitting_id, &document.segments)?;
    store_derived_material(
        storage,
        sitting_id,
        &embeddings,
        expected_encoder_sha256,
        document.embedding.onnx_artifact_sha256.as_deref(),
        document.embedding.dim,
        derived_at_epoch_seconds,
    )
}

/// A worker output in the work directory: bounded, regular, never a symlink.
/// Absence is a refusal (the derivation has not run), not a quarantine — the
/// durable rows carry no claim about work-directory contents.
fn read_work_artifact(path: &Path, max_bytes: u64) -> Result<Vec<u8>, SittingEvidenceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(SittingEvidenceError::Refused(
                "the worker derivation artifacts are missing",
            ));
        }
        Err(_) => return Err(SittingEvidenceError::Storage),
    };
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(SittingEvidenceError::Refused(
            "a worker derivation artifact is not a bounded regular file",
        ));
    }
    Ok(fs::read(path)?)
}

/// Retries a pending raw cleanup without touching the durable derivation.
pub(crate) fn retry_sitting_cleanup(
    storage: &StorageRoot,
    sitting_id: &str,
) -> Result<SittingLifecycleState, SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    let capture: CaptureRecord =
        read_canonical(&paths.durable.join(CAPTURE_FILE), RECORD_MAX_BYTES)?;
    read_canonical::<DerivedRecord>(&paths.durable.join(DERIVED_FILE), RECORD_MAX_BYTES)
        .map_err(|_| SittingEvidenceError::Refused("no derived material to clean up after"))?;
    run_cleanup(&paths, &capture)?;
    Ok(SittingLifecycleState::Saved)
}

/// The three-state receipt walk, copied in shape from `audio-deletion/1`:
/// receipt before bytes move, presence table revalidated at every state,
/// staged rename before removal, terminal state only after the work
/// directory itself is gone.
fn run_cleanup(paths: &SittingPaths, capture: &CaptureRecord) -> Result<(), SittingEvidenceError> {
    let receipt_path = paths.durable.join(CLEANUP_FILE);
    let live = paths.work.join(RAW_AUDIO_NAME);
    let staged = paths.work.join(RAW_STAGED_NAME);
    let mut receipt: CleanupReceipt = if path_present(&receipt_path)? {
        read_canonical(&receipt_path, RECORD_MAX_BYTES)?
    } else {
        // Without a receipt the live raw must exist; its absence is an
        // unexplained loss, not transient storage trouble — reporting it as
        // retryable would strand the sitting in cleanup-pending forever.
        if !path_present(&live)? {
            return Err(SittingEvidenceError::Quarantined(
                "raw material vanished without a cleanup receipt",
            ));
        }
        let (byte_size, sha256) = digest_file_bounded(&live, RAW_MAX_BYTES)?;
        if sha256 != capture.raw.sha256 || byte_size != capture.raw.byte_size {
            return Err(SittingEvidenceError::Quarantined(
                "the raw recording no longer matches its capture row",
            ));
        }
        let receipt = CleanupReceipt {
            schema: CleanupSchema::V1,
            sitting_id: capture.sitting_id.clone(),
            raw_sha256: capture.raw.sha256.clone(),
            state: CleanupState::Deleting,
            artifacts: vec![capture.raw.clone()],
        };
        durable_create_new(&receipt_path, &canonical_bytes(&receipt)?)?;
        receipt
    };
    if receipt.raw_sha256 != capture.raw.sha256 {
        return Err(SittingEvidenceError::Quarantined(
            "the cleanup receipt is bound to a different recording",
        ));
    }
    if receipt.state == CleanupState::Deleting {
        let live_present = path_present(&live)?;
        let staged_present = path_present(&staged)?;
        if live_present == staged_present {
            return Err(SittingEvidenceError::Quarantined(
                "raw material moved without a matching receipt state",
            ));
        }
        let present = if live_present { &live } else { &staged };
        let (byte_size, sha256) = digest_file_bounded(present, RAW_MAX_BYTES)?;
        if sha256 != receipt.raw_sha256 || byte_size != capture.raw.byte_size {
            return Err(SittingEvidenceError::Quarantined(
                "raw material changed identity during cleanup",
            ));
        }
        if live_present {
            fs::rename(&live, &staged)?;
            sync_directory(&paths.work)?;
        }
        receipt.state = CleanupState::Staged;
        durable_replace(&receipt_path, &canonical_bytes(&receipt)?)?;
    }
    if receipt.state == CleanupState::Staged {
        if path_present(&live)? {
            return Err(SittingEvidenceError::Quarantined(
                "live raw material reappeared after staging",
            ));
        }
        if path_present(&staged)? {
            let (byte_size, sha256) = digest_file_bounded(&staged, RAW_MAX_BYTES)?;
            if sha256 != receipt.raw_sha256 || byte_size != capture.raw.byte_size {
                return Err(SittingEvidenceError::Quarantined(
                    "staged raw material changed identity",
                ));
            }
            fs::remove_file(&staged)?;
            sync_directory(&paths.work)?;
        }
        if path_present(&paths.work)? {
            fs::remove_dir(&paths.work).map_err(|_| {
                SittingEvidenceError::Quarantined(
                    "the work directory holds unexpected entries after cleanup",
                )
            })?;
            if let Some(parent) = paths.work.parent() {
                sync_directory(parent)?;
            }
        }
        receipt.state = CleanupState::Removed;
        durable_replace(&receipt_path, &canonical_bytes(&receipt)?)?;
        sync_directory(&paths.durable)?;
    }
    if path_present(&paths.work)? {
        return Err(SittingEvidenceError::Quarantined(
            "the work directory survived a terminal cleanup receipt",
        ));
    }
    Ok(())
}

/// Labels a pre-derivation deletion as a rehearsal and deletes the raw
/// material. The label is durable before bytes move, so a crash mid-deletion
/// reconciles by finishing the deletion, never by un-labelling it. Refused
/// once derived material exists — from there the only path is cleanup.
pub(crate) fn abandon_sitting(
    storage: &StorageRoot,
    sitting_id: &str,
    labeled_at_epoch_seconds: u64,
) -> Result<(), SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    if !path_present(&paths.durable.join(SITTING_FILE))? {
        return Err(SittingEvidenceError::Refused(
            "the sitting was never started",
        ));
    }
    if path_present(&paths.durable.join(DERIVED_FILE))? {
        return Err(SittingEvidenceError::Refused(
            "derived material exists; deletion now runs under the cleanup receipt",
        ));
    }
    write_rehearsal_and_sweep(
        &paths,
        sitting_id,
        RehearsalReason::Abandoned,
        labeled_at_epoch_seconds,
    )
}

fn write_rehearsal_and_sweep(
    paths: &SittingPaths,
    sitting_id: &str,
    reason: RehearsalReason,
    labeled_at_epoch_seconds: u64,
) -> Result<(), SittingEvidenceError> {
    let rehearsal_path = paths.durable.join(REHEARSAL_FILE);
    if !path_present(&rehearsal_path)? {
        let live = paths.work.join(RAW_AUDIO_NAME);
        let recorded_raw = if path_present(&live)? {
            let (byte_size, sha256) = digest_file_bounded(&live, RAW_MAX_BYTES)?;
            Some(RawArtifact {
                relative_name: RAW_AUDIO_NAME.to_string(),
                byte_size,
                sha256,
            })
        } else {
            None
        };
        let record = RehearsalRecord {
            schema: RehearsalSchema::V1,
            sitting_id: sitting_id.to_string(),
            reason,
            recorded_raw,
            labeled_at_epoch_seconds,
        };
        durable_create_new(&rehearsal_path, &canonical_bytes(&record)?)?;
        sync_directory(&paths.durable)?;
    }
    sweep_rehearsal_work(paths)
}

/// Finishes a rehearsal's physical deletion. Idempotent; safe to re-run after
/// any crash because the durable label already exists.
///
/// Worker derivation output is swept alongside the raw artifacts: a sitting
/// abandoned after `sitting.derive` ran but before its admission leaves
/// `segments.json`/`embeddings.bin` in the work directory, and the rehearsal
/// label is precisely the authority to delete derivation output — refusing
/// here would leave the label durable, the sweep failing, and startup
/// reconciliation quarantining the whole store on every launch. The worker's
/// own recovery instruction for a disagreeing re-derivation is to abandon, so
/// this window is a documented path, not a theoretical interleaving.
fn sweep_rehearsal_work(paths: &SittingPaths) -> Result<(), SittingEvidenceError> {
    if !path_present(&paths.work)? {
        return Ok(());
    }
    for name in [
        RAW_AUDIO_NAME,
        RAW_STAGED_NAME,
        SEGMENTS_FILE,
        EMBEDDINGS_FILE,
    ] {
        let path = paths.work.join(name);
        if path_present(&path)? {
            fs::remove_file(&path)?;
        }
    }
    sync_directory(&paths.work)?;
    fs::remove_dir(&paths.work).map_err(|_| {
        SittingEvidenceError::Quarantined("a rehearsal work directory holds unexpected entries")
    })?;
    if let Some(parent) = paths.work.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

struct ClassifiedSitting {
    started: SittingStartedRecord,
    capture: Option<CaptureRecord>,
    segments: Option<SegmentsRecord>,
    derived: Option<DerivedRecord>,
    state: SittingLifecycleState,
}

/// Derives a sitting's lifecycle state from file presence and validates every
/// cross-row digest binding. Fails closed: any surprise is `Quarantined`.
fn classify_sitting(
    storage: &StorageRoot,
    sitting_id: &str,
) -> Result<ClassifiedSitting, SittingEvidenceError> {
    let paths = sitting_paths(storage, sitting_id)?;
    let names = child_names(&paths.durable)?;
    let allowed: BTreeSet<String> = [
        SITTING_FILE,
        CAPTURE_FILE,
        SEGMENTS_FILE,
        EMBEDDINGS_FILE,
        DERIVED_FILE,
        CLEANUP_FILE,
        REHEARSAL_FILE,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    if !names.is_subset(&allowed) {
        return Err(SittingEvidenceError::Quarantined(
            "a sitting record holds unexpected entries",
        ));
    }
    let started: SittingStartedRecord =
        read_canonical(&paths.durable.join(SITTING_FILE), RECORD_MAX_BYTES)?;
    if started.sitting_id != sitting_id {
        return Err(SittingEvidenceError::Quarantined(
            "a sitting row names a different sitting",
        ));
    }
    match (started.kind, &started.source_class) {
        (SittingKind::OperatorSitting, None) | (SittingKind::NegativeSource, Some(_)) => {}
        _ => {
            return Err(SittingEvidenceError::Quarantined(
                "a sitting row carries an inconsistent source class",
            ));
        }
    }
    let has = |name: &str| names.contains(name);
    let capture: Option<CaptureRecord> = if has(CAPTURE_FILE) {
        Some(read_canonical(
            &paths.durable.join(CAPTURE_FILE),
            RECORD_MAX_BYTES,
        )?)
    } else {
        None
    };
    let segments: Option<SegmentsRecord> = if has(SEGMENTS_FILE) {
        Some(read_canonical(
            &paths.durable.join(SEGMENTS_FILE),
            SEGMENTS_MAX_BYTES,
        )?)
    } else {
        None
    };
    let derived: Option<DerivedRecord> = if has(DERIVED_FILE) {
        Some(read_canonical(
            &paths.durable.join(DERIVED_FILE),
            RECORD_MAX_BYTES,
        )?)
    } else {
        None
    };
    if let (Some(capture), Some(segments)) = (&capture, &segments)
        && segments.raw_sha256 != capture.raw.sha256
    {
        return Err(SittingEvidenceError::Quarantined(
            "segment evidence is bound to a different recording",
        ));
    }
    if segments.is_some() && capture.is_none() {
        return Err(SittingEvidenceError::Quarantined(
            "segment evidence exists without a finalized capture",
        ));
    }
    if let Some(derived) = &derived {
        let capture = capture.as_ref().ok_or(SittingEvidenceError::Quarantined(
            "derived material exists without a finalized capture",
        ))?;
        let segments = segments.as_ref().ok_or(SittingEvidenceError::Quarantined(
            "derived material exists without segment evidence",
        ))?;
        if derived.raw_sha256 != capture.raw.sha256 {
            return Err(SittingEvidenceError::Quarantined(
                "derived material is bound to a different recording",
            ));
        }
        if !has(EMBEDDINGS_FILE) {
            return Err(SittingEvidenceError::Quarantined(
                "a derived row exists without its embeddings",
            ));
        }
        let embeddings_path = paths.durable.join(EMBEDDINGS_FILE);
        let metadata = fs::symlink_metadata(&embeddings_path)?;
        if !metadata.is_file() || metadata.len() > EMBEDDINGS_MAX_BYTES {
            return Err(SittingEvidenceError::Quarantined(
                "stored embeddings are not a bounded regular file",
            ));
        }
        let bytes = fs::read(&embeddings_path)?;
        if digest_bytes(&bytes) != derived.embeddings_sha256 {
            return Err(SittingEvidenceError::Quarantined(
                "stored embeddings changed identity",
            ));
        }
        let stride = derived.embedding_dim as usize * 4;
        if stride == 0
            || bytes.len() % stride != 0
            || (bytes.len() / stride) as u32 != derived.embedding_count
        {
            return Err(SittingEvidenceError::Quarantined(
                "stored embeddings disagree with their derived row",
            ));
        }
        let (scorable_count, _) = scorable_segments(&segments.segments);
        if derived.embedding_count != scorable_count {
            return Err(SittingEvidenceError::Quarantined(
                "derived material disagrees with segment evidence",
            ));
        }
    }
    if has(REHEARSAL_FILE) {
        if derived.is_some() || has(CLEANUP_FILE) || has(EMBEDDINGS_FILE) {
            return Err(SittingEvidenceError::Quarantined(
                "a rehearsal label coexists with derived material",
            ));
        }
        let rehearsal: RehearsalRecord =
            read_canonical(&paths.durable.join(REHEARSAL_FILE), RECORD_MAX_BYTES)?;
        if rehearsal.sitting_id != sitting_id {
            return Err(SittingEvidenceError::Quarantined(
                "a rehearsal row names a different sitting",
            ));
        }
        return Ok(ClassifiedSitting {
            started,
            capture,
            segments,
            derived: None,
            state: SittingLifecycleState::Rehearsal,
        });
    }
    if has(EMBEDDINGS_FILE) && derived.is_none() {
        return Err(SittingEvidenceError::Quarantined(
            "embeddings exist without their derived row",
        ));
    }
    if has(CLEANUP_FILE) && derived.is_none() {
        return Err(SittingEvidenceError::Quarantined(
            "a cleanup receipt exists without derived material",
        ));
    }
    let work_present = path_present(&paths.work)?;
    let state = if let Some(derived_record) = &derived {
        let receipt: Option<CleanupReceipt> = if has(CLEANUP_FILE) {
            Some(read_canonical(
                &paths.durable.join(CLEANUP_FILE),
                RECORD_MAX_BYTES,
            )?)
        } else {
            None
        };
        if let Some(receipt) = &receipt {
            if receipt.raw_sha256 != derived_record.raw_sha256 {
                return Err(SittingEvidenceError::Quarantined(
                    "the cleanup receipt is bound to a different recording",
                ));
            }
            if receipt.state == CleanupState::Removed {
                if work_present {
                    return Err(SittingEvidenceError::Quarantined(
                        "the work directory survived a terminal cleanup receipt",
                    ));
                }
                SittingLifecycleState::Saved
            } else {
                SittingLifecycleState::CleanupPending
            }
        } else {
            // No receipt yet means no byte has moved: the live raw must
            // still be present and whole, or the loss is unexplained.
            if !work_present || !path_present(&paths.work.join(RAW_AUDIO_NAME))? {
                return Err(SittingEvidenceError::Quarantined(
                    "raw material vanished without a cleanup receipt",
                ));
            }
            SittingLifecycleState::CleanupPending
        }
    } else if let Some(capture) = &capture {
        if !work_present || !path_present(&paths.work.join(RAW_AUDIO_NAME))? {
            return Err(SittingEvidenceError::Quarantined(
                "raw material vanished without a receipt or rehearsal label",
            ));
        }
        let (byte_size, sha256) =
            digest_file_bounded(&paths.work.join(RAW_AUDIO_NAME), RAW_MAX_BYTES)?;
        if sha256 != capture.raw.sha256 || byte_size != capture.raw.byte_size {
            return Err(SittingEvidenceError::Quarantined(
                "the retained raw recording no longer matches its capture row",
            ));
        }
        SittingLifecycleState::RawRetained
    } else {
        SittingLifecycleState::RecordingInProgress
    };
    Ok(ClassifiedSitting {
        started,
        capture,
        segments,
        derived,
        state,
    })
}

fn list_sitting_ids(storage: &StorageRoot) -> Result<Vec<String>, SittingEvidenceError> {
    let sittings = resolve(storage, &[ENROLLMENT_DIR, SITTINGS_DIR])?;
    if !path_present(&sittings)? {
        return Ok(Vec::new());
    }
    let mut ids: Vec<String> = child_names(&sittings)?.into_iter().collect();
    for id in &ids {
        require_canonical_uuid(id).map_err(|_| {
            SittingEvidenceError::Quarantined("the sitting store holds an unexpected entry")
        })?;
    }
    ids.sort();
    Ok(ids)
}

/// Startup crash recovery. Every incomplete recording becomes a labelled
/// rehearsal, every pending cleanup resumes toward its terminal receipt, and
/// any orphan work directory fails the whole store closed.
pub(crate) fn reconcile_sitting_evidence(
    storage: &StorageRoot,
    reconciled_at_epoch_seconds: u64,
) -> Result<(), SittingEvidenceError> {
    let enrollment = resolve(storage, &[ENROLLMENT_DIR])?;
    if !path_present(&enrollment)? {
        return Ok(());
    }
    let sittings_dir = resolve(storage, &[ENROLLMENT_DIR, SITTINGS_DIR])?;
    let work_root = resolve(storage, &[ENROLLMENT_DIR, WORK_DIR])?;
    let ids = list_sitting_ids(storage)?;
    if path_present(&work_root)? {
        for name in child_names(&work_root)? {
            let matching = sittings_dir.join(&name).join(SITTING_FILE);
            if !path_present(&matching)? {
                return Err(SittingEvidenceError::Quarantined(
                    "a work directory exists for no started sitting",
                ));
            }
        }
    }
    for id in ids {
        let paths = sitting_paths(storage, &id)?;
        if child_names(&paths.durable)?.is_empty() {
            // A crash between directory creation and the identity row left
            // pre-identity debris: nothing was recorded and no identity was
            // committed, so removal is safe and needs no rehearsal label.
            fs::remove_dir(&paths.durable)?;
            sync_directory(&sittings_dir)?;
            continue;
        }
        let classified = classify_sitting(storage, &id)?;
        match classified.state {
            SittingLifecycleState::RecordingInProgress => {
                write_rehearsal_and_sweep(
                    &paths,
                    &id,
                    RehearsalReason::CrashIncomplete,
                    reconciled_at_epoch_seconds,
                )?;
            }
            SittingLifecycleState::CleanupPending => {
                let capture =
                    classified
                        .capture
                        .as_ref()
                        .ok_or(SittingEvidenceError::Quarantined(
                            "a cleanup-pending sitting lost its capture row",
                        ))?;
                run_cleanup(&paths, capture)?;
            }
            SittingLifecycleState::Rehearsal => {
                sweep_rehearsal_work(&paths)?;
            }
            SittingLifecycleState::RawRetained | SittingLifecycleState::Saved => {}
        }
    }
    Ok(())
}

/// Projects the store into the content-free evidence the guidance evaluator
/// consumes, plus per-sitting summaries for recorder surfaces.
///
/// Only `Saved` projects derived material: a cleanup-pending sitting reports
/// `derived: None`, because the saved-sitting promise includes the deletion —
/// the evaluator names it app-side pending work, never an operator errand.
/// Rehearsals and in-progress recordings never project at all.
pub(crate) fn read_sitting_evidence(
    storage: &StorageRoot,
) -> Result<(EnrollmentEvidence, Vec<SittingRecordSummary>), SittingEvidenceError> {
    let mut evidence = EnrollmentEvidence::default();
    let mut summaries = Vec::new();
    for id in list_sitting_ids(storage)? {
        let classified = classify_sitting(storage, &id)?;
        summaries.push(SittingRecordSummary {
            sitting_id: id.clone(),
            kind: classified.started.kind,
            source_class: classified.started.source_class.clone(),
            state: classified.state,
        });
        let capture = match &classified.capture {
            Some(capture) => capture,
            None => continue,
        };
        if matches!(
            classified.state,
            SittingLifecycleState::Rehearsal | SittingLifecycleState::RecordingInProgress
        ) {
            continue;
        }
        // A finalized capture without segment evidence yet still projects —
        // with zero scorable material — so retained raw audio is never
        // invisible to the evaluator or to the refuse-without-encoder gate.
        // The canonical gate would refuse a segmentless sitting as too thin,
        // so zero is also the honest count.
        let (scorable_count, scorable_seconds) = classified
            .segments
            .as_ref()
            .map(|record| scorable_segments(&record.segments))
            .unwrap_or((0, 0.0));
        let derived = if classified.state == SittingLifecycleState::Saved {
            classified
                .derived
                .as_ref()
                .map(|record| DerivedVoiceMaterial {
                    encoder_sha256: record.encoder_sha256.clone(),
                })
        } else {
            None
        };
        match classified.started.kind {
            SittingKind::OperatorSitting => evidence.sittings.push(SittingEvidence {
                captured_at_epoch: Some(capture.captured_at_epoch_seconds),
                audio_sha256: capture.raw.sha256.clone(),
                scorable_segments: scorable_count,
                scorable_seconds,
                derived,
            }),
            SittingKind::NegativeSource => evidence.negative_sources.push(NegativeSourceEvidence {
                source_class: classified.started.source_class.clone().unwrap_or_default(),
                audio_sha256: capture.raw.sha256.clone(),
                scorable_segments: scorable_count,
                scorable_seconds,
                derived,
            }),
        }
    }
    Ok((evidence, summaries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enrollment_guidance::{EnrollmentShortfall, evaluate_enrollment_evidence};
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        storage: StorageRoot,
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        Fixture {
            _temp: temp,
            storage,
        }
    }

    const SID: &str = "6f7dce4e-93a9-4c05-a9c6-9c1f60ec2c68";
    const SID_B: &str = "0e0aa6f8-1cbb-4d43-9d3f-3f2d0c8b7a11";
    const ENCODER: &str = "0575cb64845e6b9a10db9bcb74d5ac32b326b8dc90352671d345e2ee3d0126a2";

    fn spans(count: usize) -> Vec<SegmentSpan> {
        (0..count)
            .map(|index| SegmentSpan {
                start_seconds: index as f64 * 4.0,
                end_seconds: index as f64 * 4.0 + 3.0,
            })
            .collect()
    }

    fn embeddings_for(count: usize) -> Vec<u8> {
        vec![7_u8; count * 192 * 4]
    }

    fn record_through_segments(fixture: &Fixture, id: &str, kind: SittingKind) {
        let class = match kind {
            SittingKind::OperatorSitting => None,
            SittingKind::NegativeSource => Some("public-or-licensed"),
        };
        begin_sitting(&fixture.storage, id, kind, class, 1_000).unwrap();
        append_raw_audio(&fixture.storage, id, b"synthetic fixture audio bytes").unwrap();
        finalize_capture(&fixture.storage, id, 1_100).unwrap();
        record_segments(&fixture.storage, id, &spans(5)).unwrap();
    }

    fn sitting_state(fixture: &Fixture, id: &str) -> SittingLifecycleState {
        classify_sitting(&fixture.storage, id).unwrap().state
    }

    #[test]
    fn a_sitting_saves_only_after_derivation_cleanup_and_absent_work_directory() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::RawRetained
        );
        let state = store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            Some("onnx-artifact-digest"),
            192,
            1_200,
        )
        .unwrap();
        assert_eq!(state, SittingLifecycleState::Saved);
        assert_eq!(sitting_state(&fixture, SID), SittingLifecycleState::Saved);
        assert!(!work_dir(&fixture.storage, SID).unwrap().exists());
        let (evidence, summaries) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].state, SittingLifecycleState::Saved);
        assert_eq!(evidence.sittings.len(), 1);
        let sitting = &evidence.sittings[0];
        assert_eq!(sitting.scorable_segments, 5);
        assert_eq!(sitting.derived.as_ref().unwrap().encoder_sha256, ENCODER);
    }

    #[test]
    fn raw_retained_projects_without_derived_material() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let (evidence, _) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(evidence.sittings.len(), 1);
        assert!(evidence.sittings[0].derived.is_none());
        let status = evaluate_enrollment_evidence(&evidence, Some(ENCODER));
        assert!(status.shortfalls.iter().any(|shortfall| matches!(
            shortfall,
            EnrollmentShortfall::AwaitingVoiceDerivation { .. }
        )));
    }

    #[test]
    fn crash_before_capture_reconciles_to_a_labelled_rehearsal() {
        let fixture = fixture();
        begin_sitting(
            &fixture.storage,
            SID,
            SittingKind::OperatorSitting,
            None,
            1_000,
        )
        .unwrap();
        append_raw_audio(&fixture.storage, SID, b"partial bytes").unwrap();
        reconcile_sitting_evidence(&fixture.storage, 2_000).unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::Rehearsal
        );
        assert!(!work_dir(&fixture.storage, SID).unwrap().exists());
        let rehearsal: RehearsalRecord = read_canonical(
            &sitting_dir(&fixture.storage, SID)
                .unwrap()
                .join(REHEARSAL_FILE),
            RECORD_MAX_BYTES,
        )
        .unwrap();
        assert_eq!(rehearsal.reason, RehearsalReason::CrashIncomplete);
        assert!(rehearsal.recorded_raw.is_some());
        let (evidence, summaries) = read_sitting_evidence(&fixture.storage).unwrap();
        assert!(evidence.sittings.is_empty());
        assert_eq!(summaries[0].state, SittingLifecycleState::Rehearsal);
    }

    #[test]
    fn abandonment_labels_a_rehearsal_and_never_advances_enrollment() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        abandon_sitting(&fixture.storage, SID, 3_000).unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::Rehearsal
        );
        let (evidence, _) = read_sitting_evidence(&fixture.storage).unwrap();
        assert!(evidence.sittings.is_empty());
        let status = evaluate_enrollment_evidence(&evidence, Some(ENCODER));
        assert_eq!(status.sittings_recorded, 0);
    }

    #[test]
    fn abandonment_after_derivation_is_refused() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        )
        .unwrap();
        let refused = abandon_sitting(&fixture.storage, SID, 3_000);
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
    }

    #[test]
    fn crash_between_rehearsal_label_and_deletion_finishes_the_deletion() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let paths = SittingPaths {
            durable: sitting_dir(&fixture.storage, SID).unwrap(),
            work: work_dir(&fixture.storage, SID).unwrap(),
        };
        // Simulate the crash window: durable label written, raw still present.
        let record = RehearsalRecord {
            schema: RehearsalSchema::V1,
            sitting_id: SID.to_string(),
            reason: RehearsalReason::Abandoned,
            recorded_raw: None,
            labeled_at_epoch_seconds: 3_000,
        };
        durable_create_new(
            &paths.durable.join(REHEARSAL_FILE),
            &canonical_bytes(&record).unwrap(),
        )
        .unwrap();
        assert!(paths.work.join(RAW_AUDIO_NAME).exists());
        reconcile_sitting_evidence(&fixture.storage, 4_000).unwrap();
        assert!(!paths.work.exists());
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::Rehearsal
        );
    }

    #[test]
    fn crash_after_derivation_resumes_cleanup_to_saved() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        // Write derived rows exactly as store_derived_material does, then stop
        // before any cleanup, simulating a crash at the widest window.
        let paths = SittingPaths {
            durable: sitting_dir(&fixture.storage, SID).unwrap(),
            work: work_dir(&fixture.storage, SID).unwrap(),
        };
        let capture: CaptureRecord =
            read_canonical(&paths.durable.join(CAPTURE_FILE), RECORD_MAX_BYTES).unwrap();
        let embeddings = embeddings_for(5);
        write_once(&paths.durable.join(EMBEDDINGS_FILE), &embeddings).unwrap();
        let derived = DerivedRecord {
            schema: DerivedSchema::V1,
            sitting_id: SID.to_string(),
            raw_sha256: capture.raw.sha256.clone(),
            encoder_sha256: ENCODER.to_string(),
            onnx_artifact_sha256: None,
            embeddings_sha256: digest_bytes(&embeddings),
            embedding_count: 5,
            embedding_dim: 192,
            derived_at_epoch_seconds: 1_200,
        };
        write_once(
            &paths.durable.join(DERIVED_FILE),
            &canonical_bytes(&derived).unwrap(),
        )
        .unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::CleanupPending
        );
        let (evidence, _) = read_sitting_evidence(&fixture.storage).unwrap();
        assert!(
            evidence.sittings[0].derived.is_none(),
            "cleanup-pending must not report saved"
        );
        reconcile_sitting_evidence(&fixture.storage, 5_000).unwrap();
        assert_eq!(sitting_state(&fixture, SID), SittingLifecycleState::Saved);
        assert!(!paths.work.exists());
    }

    #[test]
    fn crash_mid_cleanup_with_staged_raw_resumes_to_saved() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        )
        .unwrap();
        // Rewind the terminal receipt to the staged state with a staged file
        // present, simulating a crash between staging and removal.
        let paths = SittingPaths {
            durable: sitting_dir(&fixture.storage, SID).unwrap(),
            work: work_dir(&fixture.storage, SID).unwrap(),
        };
        let mut receipt: CleanupReceipt =
            read_canonical(&paths.durable.join(CLEANUP_FILE), RECORD_MAX_BYTES).unwrap();
        receipt.state = CleanupState::Staged;
        fs::create_dir_all(&paths.work).unwrap();
        fs::write(
            paths.work.join(RAW_STAGED_NAME),
            b"synthetic fixture audio bytes",
        )
        .unwrap();
        durable_replace(
            &paths.durable.join(CLEANUP_FILE),
            &canonical_bytes(&receipt).unwrap(),
        )
        .unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::CleanupPending
        );
        reconcile_sitting_evidence(&fixture.storage, 6_000).unwrap();
        assert_eq!(sitting_state(&fixture, SID), SittingLifecycleState::Saved);
    }

    #[test]
    fn raw_loss_without_receipt_or_label_quarantines() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        fs::remove_file(
            work_dir(&fixture.storage, SID)
                .unwrap()
                .join(RAW_AUDIO_NAME),
        )
        .unwrap();
        fs::remove_dir(work_dir(&fixture.storage, SID).unwrap()).unwrap();
        let refused = read_sitting_evidence(&fixture.storage);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn tampered_raw_bytes_quarantine_instead_of_projecting() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        fs::write(
            work_dir(&fixture.storage, SID)
                .unwrap()
                .join(RAW_AUDIO_NAME),
            b"different bytes entirely",
        )
        .unwrap();
        let refused = read_sitting_evidence(&fixture.storage);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn tampered_embeddings_quarantine() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        )
        .unwrap();
        let path = sitting_dir(&fixture.storage, SID)
            .unwrap()
            .join(EMBEDDINGS_FILE);
        fs::write(&path, embeddings_for(4)).unwrap();
        let refused = read_sitting_evidence(&fixture.storage);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn orphan_work_directory_fails_the_store_closed() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let orphan = resolve(&fixture.storage, &[ENROLLMENT_DIR, WORK_DIR, SID_B]).unwrap();
        fs::create_dir_all(&orphan).unwrap();
        let refused = reconcile_sitting_evidence(&fixture.storage, 7_000);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn unexpected_entries_in_a_sitting_record_quarantine() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        fs::write(
            sitting_dir(&fixture.storage, SID)
                .unwrap()
                .join("extra.json"),
            b"{}",
        )
        .unwrap();
        let refused = read_sitting_evidence(&fixture.storage);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn embedding_count_must_match_scorable_segments() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let refused = store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(4),
            ENCODER,
            None,
            192,
            1_200,
        );
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
    }

    #[test]
    fn negative_source_projects_with_its_source_class() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::NegativeSource);
        store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        )
        .unwrap();
        let (evidence, _) = read_sitting_evidence(&fixture.storage).unwrap();
        assert!(evidence.sittings.is_empty());
        assert_eq!(evidence.negative_sources.len(), 1);
        assert_eq!(
            evidence.negative_sources[0].source_class,
            "public-or-licensed"
        );
        assert!(evidence.negative_sources[0].derived.is_some());
    }

    #[test]
    fn operator_sitting_with_a_source_class_is_refused() {
        let fixture = fixture();
        let refused = begin_sitting(
            &fixture.storage,
            SID,
            SittingKind::OperatorSitting,
            Some("public-or-licensed"),
            1_000,
        );
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
    }

    #[test]
    fn two_saved_sittings_and_a_negative_project_into_guidance_counts() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        )
        .unwrap();
        record_through_segments(&fixture, SID_B, SittingKind::NegativeSource);
        store_derived_material(
            &fixture.storage,
            SID_B,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            9_000,
        )
        .unwrap();
        let (evidence, summaries) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(summaries.len(), 2);
        let status = evaluate_enrollment_evidence(&evidence, Some(ENCODER));
        assert_eq!(status.sittings_recorded, 1);
        assert_eq!(status.negatives_awaiting_derivation, 0);
    }

    #[test]
    fn non_canonical_identifier_is_refused() {
        let fixture = fixture();
        let refused = begin_sitting(
            &fixture.storage,
            "NOT-A-UUID",
            SittingKind::OperatorSitting,
            None,
            1_000,
        );
        assert!(matches!(
            refused,
            Err(SittingEvidenceError::MalformedIdentifier)
        ));
    }

    #[test]
    fn empty_sitting_directory_is_pre_identity_debris_and_removed() {
        let fixture = fixture();
        let debris = sitting_dir(&fixture.storage, SID).unwrap();
        fs::create_dir_all(&debris).unwrap();
        reconcile_sitting_evidence(&fixture.storage, 8_000).unwrap();
        assert!(!debris.exists());
    }

    #[test]
    fn append_after_finalize_is_refused() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let refused = append_raw_audio(&fixture.storage, SID, b"more");
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
    }

    #[test]
    fn oversized_rows_are_refused_at_write_time_not_bricked_at_read_time() {
        let fixture = fixture();
        let refused = begin_sitting(
            &fixture.storage,
            SID,
            SittingKind::NegativeSource,
            Some(&"x".repeat(20_000)),
            1_000,
        );
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));

        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let oversized: Vec<SegmentSpan> = (0..20_000)
            .map(|index| SegmentSpan {
                start_seconds: index as f64 * 4.0,
                end_seconds: index as f64 * 4.0 + 3.0,
            })
            .collect();
        let refused = record_segments(&fixture.storage, SID, &oversized);
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
        // The store must still be fully readable after both refusals.
        read_sitting_evidence(&fixture.storage).unwrap();
    }

    #[test]
    fn overlapping_or_repeated_segments_are_refused() {
        let fixture = fixture();
        begin_sitting(
            &fixture.storage,
            SID,
            SittingKind::OperatorSitting,
            None,
            1_000,
        )
        .unwrap();
        append_raw_audio(&fixture.storage, SID, b"synthetic fixture audio bytes").unwrap();
        finalize_capture(&fixture.storage, SID, 1_100).unwrap();
        let overlapping = [
            SegmentSpan {
                start_seconds: 0.0,
                end_seconds: 3.0,
            },
            SegmentSpan {
                start_seconds: 2.0,
                end_seconds: 5.0,
            },
        ];
        let refused = record_segments(&fixture.storage, SID, &overlapping);
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
        let repeated = [
            SegmentSpan {
                start_seconds: 0.0,
                end_seconds: 3.0,
            },
            SegmentSpan {
                start_seconds: 0.0,
                end_seconds: 3.0,
            },
        ];
        let refused = record_segments(&fixture.storage, SID, &repeated);
        assert!(matches!(refused, Err(SittingEvidenceError::Refused(_))));
    }

    #[test]
    fn a_capture_without_segments_still_projects_as_retained_evidence() {
        let fixture = fixture();
        begin_sitting(
            &fixture.storage,
            SID,
            SittingKind::OperatorSitting,
            None,
            1_000,
        )
        .unwrap();
        append_raw_audio(&fixture.storage, SID, b"synthetic fixture audio bytes").unwrap();
        finalize_capture(&fixture.storage, SID, 1_100).unwrap();
        reconcile_sitting_evidence(&fixture.storage, 2_000).unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::RawRetained
        );
        let (evidence, _) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(
            evidence.sittings.len(),
            1,
            "retained raw audio must never be invisible"
        );
        assert_eq!(evidence.sittings[0].scorable_segments, 0);
        assert!(evidence.sittings[0].derived.is_none());
    }

    #[test]
    fn a_symlinked_raw_artifact_quarantines() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        let raw = work_dir(&fixture.storage, SID)
            .unwrap()
            .join(RAW_AUDIO_NAME);
        let copy = fixture.storage.path().join("diagnostics/decoy-bytes");
        fs::copy(&raw, &copy).unwrap();
        fs::remove_file(&raw).unwrap();
        std::os::unix::fs::symlink(&copy, &raw).unwrap();
        let refused = read_sitting_evidence(&fixture.storage);
        assert!(matches!(refused, Err(SittingEvidenceError::Quarantined(_))));
    }

    #[test]
    fn raw_vanished_before_cleanup_quarantines_instead_of_pending_forever() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        fs::remove_file(
            work_dir(&fixture.storage, SID)
                .unwrap()
                .join(RAW_AUDIO_NAME),
        )
        .unwrap();
        let outcome = store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        );
        assert!(
            matches!(outcome, Err(SittingEvidenceError::Quarantined(_))),
            "a vanished raw file is unexplained loss, not retryable pending work"
        );
        // The stuck shape — derived rows durable, no receipt, raw gone — must
        // also quarantine at read and reconcile time rather than looping as
        // transient storage failure.
        let read = read_sitting_evidence(&fixture.storage);
        assert!(matches!(read, Err(SittingEvidenceError::Quarantined(_))));
        let reconcile = reconcile_sitting_evidence(&fixture.storage, 9_000);
        assert!(matches!(
            reconcile,
            Err(SittingEvidenceError::Quarantined(_))
        ));
    }

    #[test]
    fn a_detected_identity_violation_during_cleanup_is_not_reported_as_pending() {
        let fixture = fixture();
        record_through_segments(&fixture, SID, SittingKind::OperatorSitting);
        fs::write(
            work_dir(&fixture.storage, SID)
                .unwrap()
                .join(RAW_AUDIO_NAME),
            b"tampered bytes that no longer match the capture row",
        )
        .unwrap();
        let outcome = store_derived_material(
            &fixture.storage,
            SID,
            &embeddings_for(5),
            ENCODER,
            None,
            192,
            1_200,
        );
        assert!(matches!(outcome, Err(SittingEvidenceError::Quarantined(_))));
        // The durable derivation must survive the refused cleanup.
        assert!(
            sitting_dir(&fixture.storage, SID)
                .unwrap()
                .join(DERIVED_FILE)
                .exists()
        );
    }

    const RAW_FIXTURE_AUDIO: &[u8] = b"synthetic fixture audio bytes";

    fn record_through_finalize(fixture: &Fixture, id: &str) {
        begin_sitting(
            &fixture.storage,
            id,
            SittingKind::OperatorSitting,
            None,
            1_000,
        )
        .unwrap();
        append_raw_audio(&fixture.storage, id, RAW_FIXTURE_AUDIO).unwrap();
        finalize_capture(&fixture.storage, id, 1_100).unwrap();
    }

    fn plant_worker_derivation(
        fixture: &Fixture,
        id: &str,
        mutate: impl FnOnce(&mut serde_json::Value),
    ) {
        let embeddings = embeddings_for(5);
        let mut document = serde_json::json!({
            "schema": "sitting-derivation/1",
            "sitting_id": id,
            "raw_sha256": digest_bytes(RAW_FIXTURE_AUDIO),
            "sample_rate": 16_000,
            "samples": 160_000,
            "segments": spans(5),
            "embedding": {
                "count": 5,
                "dim": 192,
                "sha256": digest_bytes(&embeddings),
                "encoder_sha256": ENCODER,
                "onnx_artifact_sha256": null,
            },
        });
        mutate(&mut document);
        let work = work_dir(&fixture.storage, id).unwrap();
        fs::write(
            work.join(SEGMENTS_FILE),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        fs::write(work.join(EMBEDDINGS_FILE), &embeddings).unwrap();
    }

    fn assert_nothing_durable_was_written(fixture: &Fixture, id: &str) {
        let durable = sitting_dir(&fixture.storage, id).unwrap();
        assert!(!durable.join(SEGMENTS_FILE).exists());
        assert!(!durable.join(DERIVED_FILE).exists());
        assert!(!durable.join(EMBEDDINGS_FILE).exists());
        assert_eq!(
            sitting_state(fixture, id),
            SittingLifecycleState::RawRetained
        );
    }

    #[test]
    fn worker_derivation_admits_through_the_full_join_and_saves() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |_| {});
        let state = admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300).unwrap();
        assert_eq!(state, SittingLifecycleState::Saved);
        assert!(!work_dir(&fixture.storage, SID).unwrap().exists());
        let (evidence, summaries) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(summaries[0].state, SittingLifecycleState::Saved);
        let sitting = &evidence.sittings[0];
        assert_eq!(sitting.scorable_segments, 5);
        assert_eq!(sitting.derived.as_ref().unwrap().encoder_sha256, ENCODER);

        // Re-admission routes to cleanup retry rather than re-deriving; the
        // first admission's rows are the identity and stay untouched.
        assert_eq!(
            admit_worker_derivation(&fixture.storage, SID, ENCODER, 9_999).unwrap(),
            SittingLifecycleState::Saved
        );
    }

    #[test]
    fn worker_derivation_naming_foreign_raw_bytes_writes_nothing() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |document| {
            document["raw_sha256"] = serde_json::json!("a".repeat(64));
        });
        let outcome = admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300);
        assert!(matches!(
            outcome,
            Err(SittingEvidenceError::Refused(
                "the worker derivation names different raw bytes"
            ))
        ));
        assert_nothing_durable_was_written(&fixture, SID);
    }

    #[test]
    fn worker_derivation_with_a_foreign_encoder_identity_writes_nothing() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |document| {
            document["embedding"]["encoder_sha256"] = serde_json::json!("f".repeat(64));
        });
        let outcome = admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300);
        assert!(matches!(
            outcome,
            Err(SittingEvidenceError::Refused(
                "the worker derivation encoder disagrees with the runtime manifest"
            ))
        ));
        assert_nothing_durable_was_written(&fixture, SID);
    }

    #[test]
    fn worker_derivation_embedding_defects_write_nothing() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |document| {
            document["embedding"]["sha256"] = serde_json::json!("b".repeat(64));
        });
        assert!(matches!(
            admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300),
            Err(SittingEvidenceError::Refused(
                "derived embeddings disagree with their claimed digest"
            ))
        ));
        assert_nothing_durable_was_written(&fixture, SID);

        // Count that disagrees with the store's own scorable rule.
        plant_worker_derivation(&fixture, SID, |document| {
            document["embedding"]["count"] = serde_json::json!(4);
        });
        assert!(matches!(
            admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300),
            Err(SittingEvidenceError::Refused(
                "one embedding is required per scorable segment"
            ))
        ));
        assert_nothing_durable_was_written(&fixture, SID);
    }

    #[test]
    fn worker_derivation_documents_outside_the_closed_schema_write_nothing() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |document| {
            document["transcript_text"] = serde_json::json!("words must never be here");
        });
        assert!(matches!(
            admit_worker_derivation(&fixture.storage, SID, ENCODER, 1_300),
            Err(SittingEvidenceError::Refused(
                "the worker derivation document is malformed"
            ))
        ));
        assert_nothing_durable_was_written(&fixture, SID);

        let missing = fixture;
        fs::remove_file(work_dir(&missing.storage, SID).unwrap().join(SEGMENTS_FILE)).unwrap();
        assert!(matches!(
            admit_worker_derivation(&missing.storage, SID, ENCODER, 1_300),
            Err(SittingEvidenceError::Refused(
                "the worker derivation artifacts are missing"
            ))
        ));
    }

    /// The worker's documented recovery for a disagreeing re-derivation is to
    /// abandon the sitting. That abandonment lands while derivation output
    /// still sits in the work directory, so the rehearsal sweep must own those
    /// artifacts too — a sweep that refuses would leave the durable rehearsal
    /// label pointing at an undeletable directory and fail the whole store's
    /// startup reconciliation on every launch (review of 409b82d).
    #[test]
    fn abandoning_after_derivation_but_before_admission_sweeps_cleanly() {
        let fixture = fixture();
        record_through_finalize(&fixture, SID);
        plant_worker_derivation(&fixture, SID, |_| {});

        abandon_sitting(&fixture.storage, SID, 1_400).unwrap();
        assert_eq!(
            sitting_state(&fixture, SID),
            SittingLifecycleState::Rehearsal
        );
        assert!(!work_dir(&fixture.storage, SID).unwrap().exists());

        // The store stays healthy: reconciliation completes and a neighbor
        // sitting still projects.
        record_through_segments(&fixture, SID_B, SittingKind::OperatorSitting);
        reconcile_sitting_evidence(&fixture.storage, 1_500).unwrap();
        let (_, summaries) = read_sitting_evidence(&fixture.storage).unwrap();
        assert_eq!(summaries.len(), 2);
        assert!(
            summaries
                .iter()
                .any(|summary| summary.state == SittingLifecycleState::Rehearsal)
        );
        assert!(
            summaries
                .iter()
                .any(|summary| summary.state == SittingLifecycleState::RawRetained)
        );
    }
}
