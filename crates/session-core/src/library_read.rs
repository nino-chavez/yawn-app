//! Read-only, rebuildable exact retrieval for the private meeting library.
//!
//! This is deliberately not a command or persistence API.  It owns no writer,
//! persists no index, and refuses to turn malformed private bytes into search
//! authority.  Library metadata is deliberately outside this transcript-only slice.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use icu_normalizer::ComposingNormalizer;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;

use crate::library_metadata::{MetadataIdentity, MetadataState, read_library_metadata};
use crate::meeting::{
    ArtifactRef, MAX_MEETING_RECORD_BYTES, MAX_RECEIPT_BYTES, MeetingLifecycle, artifact_ref,
    load_meeting, open_private_file, require_private_directory, valid_opaque_id,
    verify_artifact_ref, verify_record_static_artifacts,
};
use crate::note_projection::{
    NoteProjector, ProjectRequest, ProjectionError, UnavailableProjector, project_claims,
};
use crate::storage::StorageRoot;
use crate::transcript_restoration::{TranscriptArtifactError, resolve_stored_transcript_with};

const MAX_TRANSCRIPT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TURNS: usize = 20_000;
pub const MAX_SEARCH_RESULTS: usize = 100;
const NOTICE_VERSION: &str = "internal-transcript-alpha/1";
/// Rust 1.94.0 source identity used by `char::to_lowercase` in
/// `search-normalization/1`.
pub const SEARCH_NORMALIZATION_RUST_COMMIT: &str = "4a4ef493e3a1488c6e321570238084b38948f6db";

/// Bounds every input this reader accepts.  A bound failure rejects the complete
/// build; a caller must not expose a prefix as a library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    pub max_meetings: usize,
    pub max_total_bytes: u64,
    pub max_transcript_bytes: u64,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_meetings: 100,
            max_total_bytes: 64 * 1024 * 1024,
            max_transcript_bytes: MAX_TRANSCRIPT_BYTES,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LibraryReadError {
    #[error("library read capacity exceeded")]
    CapacityExceeded,
    #[error("library snapshot is stale")]
    SnapshotStale,
    #[error("library request is invalid")]
    InvalidRequest,
    #[error("library artifact is unavailable")]
    ArtifactUnavailable,
}

/// Which meetings a surface is currently looking at.
///
/// **Applied in memory, over the projection this snapshot already validated.**
/// The corpus index carries the same folder and capture-time columns and could
/// answer these in SQL, and doing that today would buy nothing: the app builds
/// the full projection before anything else happens, so the rows are already
/// here and the query would be a second walk over data in hand. The index earns
/// its read path when US-13.6 stops the scan from being the entry point — at
/// which point *not* having built the projection is the whole advantage. Until
/// then, filtering here is the honest implementation and no commit claims a
/// speed it does not have.
///
/// Text matching runs through `normalized_matches`, the same transform search
/// uses, rather than a second lowercase-and-contains. `search-normalization/1`
/// pins Unicode 17.0.0, three crate checksums and a Rust commit for
/// `char::to_lowercase`; a filter with its own rule would disagree with search
/// on composed `é` and expanding `İ`, and two owners for one rule is the drift
/// that pin exists to prevent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    pub folder: FolderFilter,
    /// Inclusive, in UTC epoch seconds, matching the capture-time range the
    /// contract already specifies for search.
    pub start_epoch_seconds: Option<u64>,
    pub end_epoch_seconds: Option<u64>,
    /// Narrows by the meeting's *name* — the operator's title if there is one,
    /// and the meeting's own opening line if there is not.
    ///
    /// Deliberately not a transcript search. Search already covers transcript
    /// text and returns spans; this narrows a list of meetings, and a filter
    /// that quietly searched everything would return meetings whose names do
    /// not contain what was typed.
    pub title: Option<String>,
}

/// `Any` and `Unfiled` are different questions and a nullable folder cannot ask
/// both: "no folder constraint" and "only meetings in no folder" would collapse
/// into one value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FolderFilter {
    #[default]
    Any,
    Unfiled,
    Named(String),
}

impl LibraryFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A page of search hits, and how many there were before it was cut.
///
/// Two numbers rather than one. `hits` is the result — what a person can open.
/// `total` is a diagnostic that says whether narrowing the query is worth doing,
/// and it must never be rendered in the result position: a search that reported
/// "4,312 matches" as its headline would be reporting how much text the library
/// holds, not how well it answered.
#[derive(Debug)]
pub struct SearchHits {
    pub hits: Vec<LibraryHit>,
    pub total: usize,
}

pub struct LibraryProjection {
    snapshot_id: Uuid,
    rows: Vec<LibraryRow>,
    quarantined_meetings: usize,
    excluded_meeting_ids: HashSet<String>,
    limits: ReadLimits,
    metadata: MetadataState,
    hits: RefCell<BTreeMap<String, SealedHit>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct LibraryRow {
    pub meeting_id: String,
    pub created_at_epoch_seconds: u64,
    pub transcript_sha256: Option<String>,
    lifecycle: MeetingLifecycle,
    meeting_record_sha256: String,
    transcript_relative_path: Option<String>,
    turns: Vec<StoredTurn>,
    note_json_sha256: Option<String>,
    note_markdown_sha256: Option<String>,
    claims: Vec<StoredClaim>,
    attempt_sha256: String,
    title: Option<String>,
    folder: Option<String>,
    /// The folder's identifier, beside its resolved name.
    ///
    /// Two folders may carry the same name — that is the operator's business
    /// and the writer allows it — so a filter that matched on the name would
    /// silently union them. Nothing displays this; it exists so a filter can
    /// mean one folder.
    folder_id: Option<String>,
}

impl LibraryRow {
    /// Lifecycle is snapshot data only. The reader adapter uses it to distinguish
    /// a transcript that remains readable after a note-generation refusal.
    pub fn lifecycle(&self) -> MeetingLifecycle {
        self.lifecycle
    }

    /// Metadata is already part of this projection's exact-search authority.
    /// Exposing the accepted title does not create a broader metadata reader.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Which folder the operator filed this meeting in, if any.
    ///
    /// The identifier, not the name: two folders may share a name and a surface
    /// that offered to move a meeting "to Clients" could not say which.
    pub fn folder_id(&self) -> Option<&str> {
        self.folder_id.as_deref()
    }

    /// The meeting's own opening line, when it has a transcript with one.
    ///
    /// Recomputed from the validated turns rather than stored, and it is not a
    /// search field: every span it can return is already indexed as part of the
    /// retained turn it came from, so indexing it again would return the same
    /// text twice under two hit kinds. See [`crate::meeting_title`] for why it
    /// is extracted rather than composed.
    pub fn derived_title(&self) -> Option<String> {
        crate::meeting_title::derived_title(
            self.turns
                .iter()
                .map(|turn| (turn.text.as_str(), turn.gated)),
        )
    }

    /// Everything a derived index is permitted to persist, and nothing else.
    ///
    /// A cache downstream of this method cannot hold a field this module did
    /// not first validate against canonical files, which is what keeps
    /// `corpus_index` a rebuildable cache rather than a second authority. Claims
    /// are deliberately absent: they come from an out-of-process projector, not
    /// from a file. Accepted claims may be replayed inside this in-memory
    /// projection to validate their digest-bound source without relaunching the
    /// projector, but they are never persisted into the derived cache.
    pub fn derived(&self) -> DerivedRow<'_> {
        DerivedRow {
            meeting_id: &self.meeting_id,
            created_at_epoch_seconds: self.created_at_epoch_seconds,
            lifecycle: self.lifecycle,
            meeting_record_sha256: &self.meeting_record_sha256,
            attempt_sha256: &self.attempt_sha256,
            transcript_sha256: self.transcript_sha256.as_deref(),
            transcript_relative_path: self.transcript_relative_path.as_deref(),
            note_json_sha256: self.note_json_sha256.as_deref(),
            note_markdown_sha256: self.note_markdown_sha256.as_deref(),
            title: self.title.as_deref(),
            folder: self.folder.as_deref(),
            turns: self
                .turns
                .iter()
                .map(|turn| DerivedTurn {
                    index: turn.index,
                    visible_index: turn.visible_index,
                    text: &turn.text,
                    gated: turn.gated,
                })
                .collect(),
        }
    }
}

/// One validated meeting, in the exact shape a derived index may store.
#[derive(Clone, PartialEq, Eq)]
pub struct DerivedRow<'a> {
    pub meeting_id: &'a str,
    pub created_at_epoch_seconds: u64,
    pub lifecycle: MeetingLifecycle,
    pub meeting_record_sha256: &'a str,
    pub attempt_sha256: &'a str,
    pub transcript_sha256: Option<&'a str>,
    pub transcript_relative_path: Option<&'a str>,
    pub note_json_sha256: Option<&'a str>,
    pub note_markdown_sha256: Option<&'a str>,
    pub title: Option<&'a str>,
    pub folder: Option<&'a str>,
    pub turns: Vec<DerivedTurn<'a>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DerivedTurn<'a> {
    pub index: u32,
    pub visible_index: Option<u64>,
    pub text: &'a str,
    pub gated: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct StoredTurn {
    index: u32,
    visible_index: Option<u64>,
    text: String,
    gated: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct StoredClaim {
    ordinal: u64,
    sha256: String,
    claim_type: crate::note_projection::ClaimType,
    text: String,
    locators: Vec<StoredLocator>,
}

#[derive(Clone, PartialEq, Eq)]
struct StoredLocator {
    projected: crate::note_projection::Locator,
    source_turn_index: u32,
}

#[derive(Clone, Copy)]
enum ClaimSource<'a> {
    Projector(&'a dyn NoteProjector),
    Accepted(&'a [LibraryRow]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHit {
    projection_id: Uuid,
    key: String,
}

#[derive(Clone, PartialEq, Eq)]
enum SealedHit {
    Claim {
        key: String,
        authority: HitAuthority,
        claim: StoredClaim,
        normalized_query: String,
    },
    Transcript {
        key: String,
        authority: HitAuthority,
        source_turn_index: u32,
        original_scalar_start: u64,
        original_scalar_end: u64,
        normalized_query: String,
    },
    Withheld {
        key: String,
        authority: HitAuthority,
        source_turn_index: u32,
        normalized_query: String,
    },
    Meeting {
        key: String,
        authority: HitAuthority,
        normalized_query: Option<String>,
    },
}

#[derive(Clone, PartialEq, Eq)]
struct HitAuthority {
    meeting_id: String,
    created_at_epoch_seconds: u64,
    meeting_record_sha256: String,
    lifecycle: MeetingLifecycle,
    attempt_sha256: String,
    transcript_sha256: Option<String>,
    transcript_relative_path: Option<String>,
    note_json_sha256: Option<String>,
    note_markdown_sha256: Option<String>,
    metadata_identity: MetadataIdentity,
}

#[derive(Clone, PartialEq, Eq)]
pub enum OpenedLibraryHit {
    Claim {
        meeting_id: String,
        claim_ordinal: u64,
        claim_sha256: String,
        claim_type: crate::note_projection::ClaimType,
        evidence_state: ClaimEvidenceState,
        claim: String,
        locators: Vec<OpenedClaimLocator>,
        note_json_sha256: String,
        note_markdown_sha256: String,
        transcript_sha256: String,
    },
    Transcript {
        meeting_id: String,
        source_turn_index: u32,
        original_scalar_start: u64,
        original_scalar_end: u64,
        text: String,
        transcript_artifact: ArtifactRef,
    },
    Withheld {
        meeting_id: String,
        source_turn_index: u32,
    },
    Meeting {
        meeting_id: String,
        title: Option<String>,
        folder: Option<String>,
        transcript_artifact: Option<ArtifactRef>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClaimEvidenceState {
    Located,
}

#[derive(Clone, PartialEq, Eq)]
pub struct OpenedClaimLocator {
    pub turn: u64,
    pub source_turn_index: u32,
    pub start: u64,
    pub end: u64,
    pub text_sha256: String,
}

/// Exact, scalar-bound evidence from one located claim locator.
///
/// This is intentionally narrower than a transcript reader: a caller can only
/// request evidence through a still-valid claim handle and a locator ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedClaimEvidence {
    pub meeting_id: String,
    pub source_turn_index: u32,
    pub start: u64,
    pub end: u64,
    pub text: String,
}

impl LibraryProjection {
    pub fn rebuild(storage: &StorageRoot, limits: ReadLimits) -> Result<Self, LibraryReadError> {
        Self::rebuild_excluding(storage, limits, &HashSet::new())
    }

    /// Rebuilds while refusing to inspect meeting directories currently owned
    /// by a writer. The exact exclusion set remains part of this snapshot's
    /// authority and must match every response-scoped revalidation.
    pub fn rebuild_excluding(
        storage: &StorageRoot,
        limits: ReadLimits,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Self, LibraryReadError> {
        Self::rebuild_with_projector_excluding(
            storage,
            limits,
            Arc::new(UnavailableProjector),
            excluded_meeting_ids,
        )
    }

    /// Rebuilds through an injected, read-only `note.project` transport.  The
    /// snapshot retains no child output beyond the accepted current claims.
    pub fn rebuild_with_projector(
        storage: &StorageRoot,
        limits: ReadLimits,
        projector: Arc<dyn NoteProjector>,
    ) -> Result<Self, LibraryReadError> {
        Self::rebuild_with_projector_excluding(storage, limits, projector, &HashSet::new())
    }

    pub fn rebuild_with_projector_excluding(
        storage: &StorageRoot,
        limits: ReadLimits,
        projector: Arc<dyn NoteProjector>,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Self, LibraryReadError> {
        Self::rebuild_with_claim_source_excluding(
            storage,
            limits,
            ClaimSource::Projector(projector.as_ref()),
            excluded_meeting_ids,
        )
    }

    fn rebuild_with_accepted_claims_excluding(
        storage: &StorageRoot,
        limits: ReadLimits,
        accepted: &[LibraryRow],
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Self, LibraryReadError> {
        Self::rebuild_with_claim_source_excluding(
            storage,
            limits,
            ClaimSource::Accepted(accepted),
            excluded_meeting_ids,
        )
    }

    fn rebuild_with_claim_source_excluding(
        storage: &StorageRoot,
        limits: ReadLimits,
        claim_source: ClaimSource<'_>,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Self, LibraryReadError> {
        if limits.max_meetings == 0
            || limits.max_total_bytes == 0
            || limits.max_transcript_bytes == 0
        {
            return Err(LibraryReadError::CapacityExceeded);
        }
        let meetings_path = storage
            .resolve(Path::new("meetings"))
            .map_err(|_| LibraryReadError::ArtifactUnavailable)?;
        require_private_directory(&meetings_path)
            .map_err(|_| LibraryReadError::ArtifactUnavailable)?;
        let mut directories = Vec::new();
        for entry in
            fs::read_dir(&meetings_path).map_err(|_| LibraryReadError::ArtifactUnavailable)?
        {
            let entry = entry.map_err(|_| LibraryReadError::ArtifactUnavailable)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if valid_opaque_id(name)
                && !excluded_meeting_ids.contains(name)
                && entry
                    .file_type()
                    .map_err(|_| LibraryReadError::ArtifactUnavailable)?
                    .is_dir()
                && !entry
                    .file_type()
                    .map_err(|_| LibraryReadError::ArtifactUnavailable)?
                    .is_symlink()
            {
                directories.push((name.to_owned(), entry.path()));
            }
        }
        directories.sort_by(|left, right| left.0.cmp(&right.0));
        if directories.len() > limits.max_meetings {
            return Err(LibraryReadError::CapacityExceeded);
        }

        let mut total = 0_u64;
        let mut rows = Vec::new();
        let mut quarantined = 0;
        for (_, directory) in &directories {
            match inspect_meeting(directory, limits, &mut total, claim_source) {
                Ok(Some(row)) => rows.push(row),
                Ok(None) => continue,
                Err(MeetingInspectionError::CapacityExceeded) => {
                    return Err(LibraryReadError::CapacityExceeded);
                }
                Err(MeetingInspectionError::Unavailable) => {
                    return Err(LibraryReadError::ArtifactUnavailable);
                }
                Err(MeetingInspectionError::Quarantine) => quarantined += 1,
            }
        }
        rows.sort_by(meeting_order);
        let metadata = read_library_metadata(storage);
        if let Some(document) = metadata.document() {
            // Metadata gains authority only when every row targets a safely
            // projected meeting.  A sparse record may omit any meeting.
            if document.meetings.iter().all(|row| {
                excluded_meeting_ids.contains(&row.meeting_id)
                    || rows
                        .iter()
                        .any(|meeting| meeting.meeting_id == row.meeting_id)
            }) {
                for row in &mut rows {
                    if let Some(label) = document
                        .meetings
                        .iter()
                        .find(|label| label.meeting_id == row.meeting_id)
                    {
                        row.title = label.title.clone();
                        row.folder = label
                            .folder_id
                            .as_ref()
                            .and_then(|id| document.folders.iter().find(|folder| &folder.id == id))
                            .map(|folder| folder.name.clone());
                        row.folder_id = label.folder_id.clone();
                    }
                }
            } else {
                // Metadata rows cannot hide invalid or unavailable meetings.
                // Preserve a content-free unavailable state instead.
                return Ok(Self {
                    snapshot_id: Uuid::new_v4(),
                    rows,
                    quarantined_meetings: quarantined,
                    excluded_meeting_ids: excluded_meeting_ids.clone(),
                    limits,
                    metadata: metadata.unavailable_after_relative_validation(),
                    hits: RefCell::new(BTreeMap::new()),
                });
            }
        }
        Ok(Self {
            snapshot_id: Uuid::new_v4(),
            rows,
            quarantined_meetings: quarantined,
            excluded_meeting_ids: excluded_meeting_ids.clone(),
            limits,
            metadata,
            hits: RefCell::new(BTreeMap::new()),
        })
    }

    pub fn rows(&self) -> &[LibraryRow] {
        &self.rows
    }
    pub fn quarantined_meetings(&self) -> usize {
        self.quarantined_meetings
    }

    /// Rows this filter admits, in the projection's own order.
    ///
    /// Returns references into the snapshot rather than clones: a filtered view
    /// is a way of looking at one immutable projection, not a second copy of it
    /// that could drift.
    pub fn filtered_rows(&self, filter: &LibraryFilter) -> Vec<&LibraryRow> {
        self.rows
            .iter()
            .filter(|row| Self::admits(row, filter))
            .collect()
    }

    /// Whether one row survives the filter.
    ///
    /// Every clause is a narrowing, so an empty filter admits everything and a
    /// surface that has not chosen anything sees the whole library.
    fn admits(row: &LibraryRow, filter: &LibraryFilter) -> bool {
        match &filter.folder {
            FolderFilter::Any => {}
            FolderFilter::Unfiled => {
                if row.folder_id.is_some() {
                    return false;
                }
            }
            FolderFilter::Named(id) => {
                if row.folder_id.as_deref() != Some(id.as_str()) {
                    return false;
                }
            }
        }
        if filter
            .start_epoch_seconds
            .is_some_and(|start| row.created_at_epoch_seconds < start)
            || filter
                .end_epoch_seconds
                .is_some_and(|end| row.created_at_epoch_seconds > end)
        {
            return false;
        }
        if let Some(needle) = &filter.title {
            // Normalized here, once, through the same transform the field is
            // normalized by. An empty needle after normalization admits
            // everything rather than nothing: a surface with an empty box has
            // not asked a question.
            let Ok(normalized) = normalize_query(needle) else {
                return true;
            };
            let name = row
                .title
                .clone()
                .or_else(|| row.derived_title())
                .unwrap_or_default();
            if normalized_matches(&name, &normalized).is_empty() {
                return false;
            }
        }
        true
    }

    /// Search, narrowed to the meetings the filter admits.
    ///
    /// The filter selects *meetings*; it never changes what counts as a match
    /// inside one. A hit in an excluded meeting is dropped whole rather than
    /// reported without its meeting.
    /// Match, filter, then cut — in that order, and the order is the point.
    ///
    /// **A broad query is answered rather than refused.** Until 2026-08-08 more
    /// than [`MAX_SEARCH_RESULTS`] matches returned `CapacityExceeded` for the
    /// whole query, and the shell rendered it as "That search has too many
    /// matches." Measured the same day: five synthetic meetings already exceed
    /// the cap on a common word, so any word appearing more than a hundred times
    /// across a library returned nothing. The cap bounds how many handles one
    /// response holds open, and truncating to it bounds that exactly as well as
    /// refusing did — a response still seals at most a hundred hits. What
    /// changes is what a person gets for a common word: a hundred results
    /// instead of none.
    ///
    /// The order is recency first (see `hit_order`), so a cut page is the most
    /// recent matches rather than an arbitrary hundred.
    pub fn search_filtered(
        &self,
        query: &str,
        filter: &LibraryFilter,
    ) -> Result<SearchHits, LibraryReadError> {
        self.search_filtered_excluding(query, filter, &HashSet::new())
    }

    /// Same as [`Self::search_filtered`], but a meeting named in
    /// `excluded_meeting_ids` contributes no hit at all -- no claim, no
    /// transcript span, no withheld-turn marker, no title/folder match -- and
    /// none of its matches count toward `total`.
    ///
    /// This is the dormant search backend's own roadmap-I5 hook (this backend
    /// has no reachable caller today -- `preview_library_search` and
    /// `preview_library_open_search_result` in `main.rs` are unregistered --
    /// but a lock this session-core crate cannot itself observe must still be
    /// structurally impossible for a future caller to bypass, the same way the
    /// corpus index's own exclusion is). The desktop crate is the layer that
    /// can read `meeting_lock`, so it resolves the excluded set and passes it
    /// in here; this crate treats the set as opaque and exists to make its
    /// exclusion airtight rather than to decide who belongs in it.
    ///
    /// Filtering happens on the raw match set, before `total` is computed --
    /// not merely before `seal` mints handles -- so a locked meeting's hit
    /// count leaks nothing either. A caller that filtered only the returned
    /// page would still tell a reader "3 results" for a query none of which
    /// they may open.
    pub fn search_filtered_excluding(
        &self,
        query: &str,
        filter: &LibraryFilter,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<SearchHits, LibraryReadError> {
        let mut hits = self.matches(query)?;
        if !excluded_meeting_ids.is_empty() {
            hits.retain(|hit| {
                !excluded_meeting_ids.contains(hit_authority(hit).meeting_id.as_str())
            });
        }
        if !filter.is_empty() {
            let admitted: HashSet<&str> = self
                .filtered_rows(filter)
                .into_iter()
                .map(|row| row.meeting_id.as_str())
                .collect();
            hits.retain(|hit| admitted.contains(hit_authority(hit).meeting_id.as_str()));
        }
        let total = hits.len();
        hits.truncate(MAX_SEARCH_RESULTS);
        Ok(SearchHits {
            hits: self.seal(hits),
            total,
        })
    }

    /// The organization revision this snapshot was built against, or `None`
    /// when the record is unreadable.
    ///
    /// Every mutation carries an `expected_revision` and refuses on mismatch,
    /// so a surface needs the number it is editing from. `None` is not zero: a
    /// missing record *is* revision zero and can be written, while an
    /// unreadable one refuses every mutation, and collapsing the two would
    /// invite a caller to write over a record it could not read.
    pub fn metadata_revision(&self) -> Option<u64> {
        match &self.metadata {
            MetadataState::Missing { .. } => Some(0),
            MetadataState::Valid(document) => Some(document.revision),
            MetadataState::Unavailable { .. } => None,
        }
    }

    /// Folders as the operator named them, ordered as the record stores them.
    pub fn folders(&self) -> Vec<(&str, &str)> {
        self.metadata
            .document()
            .map(|document| {
                document
                    .folders
                    .iter()
                    .map(|folder| (folder.id.as_str(), folder.name.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every match, in order, before any cut. Retains no handle.
    ///
    /// Split out from [`Self::search`] on 2026-08-08 so that truncation can
    /// happen **after** filtering. Capping first and filtering second would
    /// answer a folder search with "the hundred most recent matches anywhere,
    /// minus the ones outside this folder" — three results where four hundred
    /// exist. The old code capped first and could not have that bug, because
    /// past the cap it refused the whole query instead of answering it.
    fn matches(&self, query: &str) -> Result<Vec<SealedHit>, LibraryReadError> {
        let normalized = normalize_search_query(query)?;
        let mut hits = Vec::new();
        for row in &self.rows {
            let authority = self.authority(row);
            for claim in &row.claims {
                if claim_matches(row, claim, &normalized) {
                    hits.push(SealedHit::Claim {
                        key: format!(
                            "{}:c:{}:{}",
                            row.meeting_id,
                            row.note_json_sha256.as_deref().unwrap_or_default(),
                            claim.ordinal
                        ),
                        authority: authority.clone(),
                        claim: claim.clone(),
                        normalized_query: normalized.clone(),
                    });
                }
            }
            for turn in &row.turns {
                for (start, end) in normalized_matches(&turn.text, &normalized) {
                    if turn.gated {
                        hits.push(SealedHit::Withheld {
                            key: format!("{}:w:{}", row.meeting_id, turn.index),
                            authority: authority.clone(),
                            source_turn_index: turn.index,
                            normalized_query: normalized.clone(),
                        });
                    } else {
                        hits.push(SealedHit::Transcript {
                            key: format!("{}:t:{}:{}:{}", row.meeting_id, turn.index, start, end),
                            authority: authority.clone(),
                            source_turn_index: turn.index,
                            original_scalar_start: start,
                            original_scalar_end: end,
                            normalized_query: normalized.clone(),
                        });
                    }
                }
            }
            if row
                .title
                .as_ref()
                .is_some_and(|title| !normalized_matches(title, &normalized).is_empty())
                || row
                    .folder
                    .as_ref()
                    .is_some_and(|folder| !normalized_matches(folder, &normalized).is_empty())
            {
                hits.push(SealedHit::Meeting {
                    key: format!("{}:m", row.meeting_id),
                    authority: authority.clone(),
                    normalized_query: Some(normalized.clone()),
                });
            }
        }
        hits.sort_by(hit_order);
        hits.dedup_by(|left, right| hit_key(left) == hit_key(right));
        Ok(hits)
    }

    /// Seals a page of hits into handles, replacing whatever was retained.
    ///
    /// One handle per returned hit and no more: the retained map is what bounds
    /// how much private snapshot state a response holds open, so it must be cut
    /// to the page rather than to the match set.
    fn seal(&self, hits: Vec<SealedHit>) -> Vec<LibraryHit> {
        let mut retained = self.hits.borrow_mut();
        retained.clear();
        hits.into_iter()
            .map(|hit| {
                let key = format!("{}:{}", self.snapshot_id, hit_key(&hit));
                retained.insert(key.clone(), hit);
                LibraryHit {
                    projection_id: self.snapshot_id,
                    key,
                }
            })
            .collect()
    }

    pub fn search(&self, query: &str) -> Result<SearchHits, LibraryReadError> {
        self.search_filtered(query, &LibraryFilter::default())
    }

    /// The hits alone, for assertions that are about hit content rather than
    /// about the cut. Test-only and a plain forward: a test that cares how many
    /// matched calls [`Self::search`] and reads `total`.
    #[cfg(test)]
    fn search_hits(&self, query: &str) -> Result<Vec<LibraryHit>, LibraryReadError> {
        self.search(query).map(|found| found.hits)
    }

    #[cfg(test)]
    fn search_filtered_hits(
        &self,
        query: &str,
        filter: &LibraryFilter,
    ) -> Result<Vec<LibraryHit>, LibraryReadError> {
        self.search_filtered(query, filter).map(|found| found.hits)
    }

    /// Returns the current projected claims for one already-listed meeting.
    /// The returned handles retain the same stale-snapshot checks as search
    /// handles, but do not create a broad transcript enumeration API.
    pub fn note_claims(&self, meeting_id: &str) -> Result<Vec<LibraryHit>, LibraryReadError> {
        let row = self
            .rows
            .iter()
            .find(|row| row.meeting_id == meeting_id)
            .ok_or(LibraryReadError::InvalidRequest)?;
        let authority = self.authority(row);
        let mut retained = self.hits.borrow_mut();
        retained.clear();
        row.claims
            .iter()
            .map(|claim| {
                let normalized_query = normalize_query(&claim.text)?;
                let hit = SealedHit::Claim {
                    key: format!(
                        "{}:c:{}:{}",
                        row.meeting_id,
                        row.note_json_sha256.as_deref().unwrap_or_default(),
                        claim.ordinal
                    ),
                    authority: authority.clone(),
                    claim: claim.clone(),
                    normalized_query,
                };
                let key = format!("{}:{}", self.snapshot_id, hit_key(&hit));
                retained.insert(key.clone(), hit);
                Ok(LibraryHit {
                    projection_id: self.snapshot_id,
                    key,
                })
            })
            .collect()
    }

    /// Retains one already-listed meeting as an opaque, projection-owned open
    /// target. It is not a pathname or a general transcript enumeration API.
    pub fn meeting_handle(&self, meeting_id: &str) -> Result<LibraryHit, LibraryReadError> {
        let row = self
            .rows
            .iter()
            .find(|row| row.meeting_id == meeting_id)
            .ok_or(LibraryReadError::InvalidRequest)?;
        let hit = SealedHit::Meeting {
            key: format!("{}:o", row.meeting_id),
            authority: self.authority(row),
            normalized_query: None,
        };
        let key = format!("{}:{}", self.snapshot_id, hit_key(&hit));
        self.hits.borrow_mut().insert(key.clone(), hit);
        Ok(LibraryHit {
            projection_id: self.snapshot_id,
            key,
        })
    }

    /// Confirms that the complete read-only snapshot remains current once.
    /// Callers may then inspect several already-owned hits without rebuilding
    /// the library once per result.
    pub fn validate_snapshot(&self, storage: &StorageRoot) -> Result<(), LibraryReadError> {
        self.validate_snapshot_excluding(storage, &self.excluded_meeting_ids)
    }

    pub fn validate_snapshot_excluding(
        &self,
        storage: &StorageRoot,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<(), LibraryReadError> {
        if excluded_meeting_ids != &self.excluded_meeting_ids {
            return Err(LibraryReadError::SnapshotStale);
        }
        let rebuilt = Self::rebuild_with_accepted_claims_excluding(
            storage,
            self.limits,
            &self.rows,
            excluded_meeting_ids,
        )?;
        if rebuilt.rows != self.rows
            || rebuilt.quarantined_meetings != self.quarantined_meetings
            || rebuilt.metadata != self.metadata
        {
            return Err(LibraryReadError::SnapshotStale);
        }
        Ok(())
    }

    pub fn open(
        &self,
        storage: &StorageRoot,
        handle: &LibraryHit,
    ) -> Result<OpenedLibraryHit, LibraryReadError> {
        self.open_excluding(storage, handle, &self.excluded_meeting_ids)
    }

    pub fn open_excluding(
        &self,
        storage: &StorageRoot,
        handle: &LibraryHit,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<OpenedLibraryHit, LibraryReadError> {
        if excluded_meeting_ids != &self.excluded_meeting_ids {
            return Err(LibraryReadError::SnapshotStale);
        }
        self.open_inner(Some(storage), handle)
    }

    /// Inspects a projection-owned hit after `validate_snapshot` has already
    /// established that the exact snapshot remains current.
    pub fn open_snapshot(&self, handle: &LibraryHit) -> Result<OpenedLibraryHit, LibraryReadError> {
        self.open_inner(None, handle)
    }

    fn open_inner(
        &self,
        storage: Option<&StorageRoot>,
        handle: &LibraryHit,
    ) -> Result<OpenedLibraryHit, LibraryReadError> {
        if handle.projection_id != self.snapshot_id {
            return Err(LibraryReadError::SnapshotStale);
        }
        let hit = self
            .hits
            .borrow()
            .get(&handle.key)
            .cloned()
            .ok_or(LibraryReadError::SnapshotStale)?;
        let rebuilt = storage
            .map(|storage| {
                Self::rebuild_with_accepted_claims_excluding(
                    storage,
                    self.limits,
                    &self.rows,
                    &self.excluded_meeting_ids,
                )
            })
            .transpose()
            .map_err(|_| LibraryReadError::SnapshotStale)?;
        let current = rebuilt.as_ref().unwrap_or(self);
        let row = current
            .rows
            .iter()
            .find(|row| row.meeting_id == hit_meeting_id(&hit))
            .ok_or(LibraryReadError::SnapshotStale)?;
        if current.authority(row) != *hit_authority(&hit) {
            return Err(LibraryReadError::SnapshotStale);
        }
        match hit {
            SealedHit::Claim {
                claim,
                normalized_query,
                ..
            } => {
                let current = row
                    .claims
                    .iter()
                    .find(|candidate| candidate.ordinal == claim.ordinal)
                    .ok_or(LibraryReadError::SnapshotStale)?;
                if current != &claim || !claim_matches(row, current, &normalized_query) {
                    return Err(LibraryReadError::SnapshotStale);
                }
                Ok(OpenedLibraryHit::Claim {
                    meeting_id: row.meeting_id.clone(),
                    claim_ordinal: current.ordinal,
                    claim_sha256: current.sha256.clone(),
                    claim_type: current.claim_type,
                    evidence_state: ClaimEvidenceState::Located,
                    claim: current.text.clone(),
                    locators: current
                        .locators
                        .iter()
                        .map(|locator| OpenedClaimLocator {
                            turn: locator.projected.turn,
                            source_turn_index: locator.source_turn_index,
                            start: locator.projected.start,
                            end: locator.projected.end,
                            text_sha256: locator.projected.text_sha256.clone(),
                        })
                        .collect(),
                    note_json_sha256: row
                        .note_json_sha256
                        .clone()
                        .ok_or(LibraryReadError::SnapshotStale)?,
                    note_markdown_sha256: row
                        .note_markdown_sha256
                        .clone()
                        .ok_or(LibraryReadError::SnapshotStale)?,
                    transcript_sha256: row
                        .transcript_sha256
                        .clone()
                        .ok_or(LibraryReadError::SnapshotStale)?,
                })
            }
            SealedHit::Transcript {
                source_turn_index,
                original_scalar_start,
                original_scalar_end,
                normalized_query,
                ..
            } => {
                let turn = row
                    .turns
                    .iter()
                    .find(|turn| turn.index == source_turn_index && !turn.gated)
                    .ok_or(LibraryReadError::SnapshotStale)?;
                if !span_is_valid(&turn.text, original_scalar_start, original_scalar_end) {
                    return Err(LibraryReadError::SnapshotStale);
                }
                if !normalized_matches(&turn.text, &normalized_query)
                    .contains(&(original_scalar_start, original_scalar_end))
                {
                    return Err(LibraryReadError::SnapshotStale);
                }
                Ok(OpenedLibraryHit::Transcript {
                    meeting_id: row.meeting_id.clone(),
                    source_turn_index,
                    original_scalar_start,
                    original_scalar_end,
                    text: scalar_slice(&turn.text, original_scalar_start, original_scalar_end)
                        .ok_or(LibraryReadError::SnapshotStale)?,
                    transcript_artifact: transcript_artifact(row)?
                        .ok_or(LibraryReadError::SnapshotStale)?,
                })
            }
            SealedHit::Withheld {
                source_turn_index,
                normalized_query,
                ..
            } => {
                if !row
                    .turns
                    .iter()
                    .any(|turn| turn.index == source_turn_index && turn.gated)
                {
                    return Err(LibraryReadError::SnapshotStale);
                }
                let turn = row
                    .turns
                    .iter()
                    .find(|turn| turn.index == source_turn_index)
                    .ok_or(LibraryReadError::SnapshotStale)?;
                if normalized_matches(&turn.text, &normalized_query).is_empty() {
                    return Err(LibraryReadError::SnapshotStale);
                }
                Ok(OpenedLibraryHit::Withheld {
                    meeting_id: row.meeting_id.clone(),
                    source_turn_index,
                })
            }
            SealedHit::Meeting {
                normalized_query, ..
            } => {
                if let Some(normalized_query) = normalized_query {
                    if row
                        .title
                        .as_ref()
                        .is_none_or(|title| normalized_matches(title, &normalized_query).is_empty())
                        && row.folder.as_ref().is_none_or(|folder| {
                            normalized_matches(folder, &normalized_query).is_empty()
                        })
                    {
                        return Err(LibraryReadError::SnapshotStale);
                    }
                }
                Ok(OpenedLibraryHit::Meeting {
                    meeting_id: row.meeting_id.clone(),
                    title: row.title.clone(),
                    folder: row.folder.clone(),
                    transcript_artifact: transcript_artifact(row)?,
                })
            }
        }
    }

    /// Opens precisely one current claim locator.  The evidence is re-derived
    /// from the transcript only after the claim handle has passed the normal
    /// snapshot and artifact checks.
    pub fn open_claim_evidence(
        &self,
        storage: &StorageRoot,
        handle: &LibraryHit,
        locator_ordinal: usize,
    ) -> Result<OpenedClaimEvidence, LibraryReadError> {
        self.open_claim_evidence_excluding(
            storage,
            handle,
            locator_ordinal,
            &self.excluded_meeting_ids,
        )
    }

    pub fn open_claim_evidence_excluding(
        &self,
        storage: &StorageRoot,
        handle: &LibraryHit,
        locator_ordinal: usize,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<OpenedClaimEvidence, LibraryReadError> {
        let OpenedLibraryHit::Claim {
            meeting_id,
            note_json_sha256,
            note_markdown_sha256,
            transcript_sha256,
            locators,
            ..
        } = self.open_excluding(storage, handle, excluded_meeting_ids)?
        else {
            return Err(LibraryReadError::InvalidRequest);
        };
        let locator = locators
            .get(locator_ordinal)
            .ok_or(LibraryReadError::InvalidRequest)?;
        let rebuilt = Self::rebuild_with_accepted_claims_excluding(
            storage,
            self.limits,
            &self.rows,
            excluded_meeting_ids,
        )
        .map_err(|_| LibraryReadError::SnapshotStale)?;
        let row = rebuilt
            .rows
            .iter()
            .find(|row| row.meeting_id == meeting_id)
            .ok_or(LibraryReadError::SnapshotStale)?;
        if row.note_json_sha256.as_deref() != Some(&note_json_sha256)
            || row.note_markdown_sha256.as_deref() != Some(&note_markdown_sha256)
            || row.transcript_sha256.as_deref() != Some(&transcript_sha256)
        {
            return Err(LibraryReadError::SnapshotStale);
        }
        let turn = row
            .turns
            .iter()
            .find(|turn| turn.index == locator.source_turn_index && !turn.gated)
            .ok_or(LibraryReadError::SnapshotStale)?;
        let text = scalar_slice(&turn.text, locator.start, locator.end)
            .ok_or(LibraryReadError::SnapshotStale)?;
        Ok(OpenedClaimEvidence {
            meeting_id,
            source_turn_index: locator.source_turn_index,
            start: locator.start,
            end: locator.end,
            text,
        })
    }

    fn authority(&self, row: &LibraryRow) -> HitAuthority {
        HitAuthority {
            meeting_id: row.meeting_id.clone(),
            created_at_epoch_seconds: row.created_at_epoch_seconds,
            meeting_record_sha256: row.meeting_record_sha256.clone(),
            lifecycle: row.lifecycle,
            attempt_sha256: row.attempt_sha256.clone(),
            transcript_sha256: row.transcript_sha256.clone(),
            transcript_relative_path: row.transcript_relative_path.clone(),
            note_json_sha256: row.note_json_sha256.clone(),
            note_markdown_sha256: row.note_markdown_sha256.clone(),
            metadata_identity: self.metadata.identity(),
        }
    }

    #[cfg(test)]
    fn sealed(&self, handle: &LibraryHit) -> Option<SealedHit> {
        self.hits.borrow().get(&handle.key).cloned()
    }
}

enum MeetingInspectionError {
    Quarantine,
    CapacityExceeded,
    Unavailable,
}

impl From<LibraryReadError> for MeetingInspectionError {
    fn from(value: LibraryReadError) -> Self {
        match value {
            LibraryReadError::CapacityExceeded => Self::CapacityExceeded,
            LibraryReadError::ArtifactUnavailable
            | LibraryReadError::InvalidRequest
            | LibraryReadError::SnapshotStale => Self::Quarantine,
        }
    }
}

fn inspect_meeting(
    directory: &Path,
    limits: ReadLimits,
    total: &mut u64,
    claim_source: ClaimSource<'_>,
) -> Result<Option<LibraryRow>, MeetingInspectionError> {
    let _meeting_bytes = bounded_read(
        &directory.join("meeting.json"),
        MAX_MEETING_RECORD_BYTES,
        total,
        limits,
    )?;
    let meeting = load_meeting(directory).map_err(|_| MeetingInspectionError::Quarantine)?;
    verify_record_static_artifacts(directory, &meeting)
        .map_err(|_| MeetingInspectionError::Quarantine)?;
    if let Some(receipt) = &meeting.retention.deletion_receipt {
        verify_artifact_ref(directory, receipt).map_err(|_| MeetingInspectionError::Quarantine)?;
        account_reference(directory, receipt, total, limits)?;
    }
    for reference in [
        meeting.artifacts.ownership.as_ref(),
        meeting.artifacts.capture_session.as_ref(),
        meeting
            .artifacts
            .current_note
            .as_ref()
            .map(|note| &note.json),
        meeting
            .artifacts
            .current_note
            .as_ref()
            .map(|note| &note.markdown),
    ]
    .into_iter()
    .flatten()
    {
        account_reference(directory, reference, total, limits)?;
    }
    let meeting_record =
        artifact_ref(directory, "meeting.json").map_err(|_| MeetingInspectionError::Quarantine)?;
    let attempt =
        artifact_ref(directory, "attempt.json").map_err(|_| MeetingInspectionError::Quarantine)?;
    if attempt != meeting.artifacts.attempt {
        return Err(MeetingInspectionError::Quarantine);
    }
    let attempt_bytes = bounded_read(
        &directory.join("attempt.json"),
        MAX_RECEIPT_BYTES,
        total,
        limits,
    )?;
    let attempt_data: Attempt =
        serde_json::from_slice(&attempt_bytes).map_err(|_| MeetingInspectionError::Quarantine)?;
    if attempt_data.schema != "capture-attempt/1"
        || attempt_data.meeting_id != meeting.meeting_id
        || Uuid::parse_str(&attempt_data.attempt_id).is_err()
        || !valid_digest(&attempt_data.application_build_sha256)
        || attempt_data.participant_notice_version != NOTICE_VERSION
        || !attempt_data.operator_attestation.participants_consented
        || !attempt_data.operator_attestation.headphones
        || !attempt_data.operator_attestation.operator_alone
        || attempt_data.retention_policy_sha256 != meeting.retention.policy_sha256
    {
        return Err(MeetingInspectionError::Quarantine);
    }
    let (transcript_sha256, transcript_relative_path, turns, current_artifact) = if matches!(
        meeting.lifecycle,
        MeetingLifecycle::TranscriptReady
            | MeetingLifecycle::SummaryFailed
            | MeetingLifecycle::Ready
    ) {
        let current = meeting
            .artifacts
            .current_transcript
            .as_ref()
            .ok_or(MeetingInspectionError::Quarantine)?;
        let actual = artifact_ref(directory, &current.relative_path)
            .map_err(|_| MeetingInspectionError::Quarantine)?;
        if &actual != current {
            return Err(MeetingInspectionError::Quarantine);
        }
        let bytes = bounded_read(
            &directory.join(&current.relative_path),
            limits.max_transcript_bytes,
            total,
            limits,
        )?;
        let is_view = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|document| {
                document
                    .get("schema")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some("transcript-view/1");
        let (base_bytes, restored) = if is_view {
            // A restored view is resolved by the audited chain walker; every
            // hop is read under this projection's own byte budget, and the
            // walker re-verifies each hop's content address itself. The head
            // revision is served from the bytes already read and charged
            // above, and the rest of the chain draws on a fixed per-meeting
            // allowance, so one hostile chain overspends into its own
            // quarantine instead of failing the whole library build.
            let meeting_uuid = Uuid::parse_str(&meeting.meeting_id)
                .map_err(|_| MeetingInspectionError::Quarantine)?;
            let mut read_failure: Option<MeetingInspectionError> = None;
            let resolved = {
                let mut primed = Some(bytes);
                let mut chain_allowance = limits.max_transcript_bytes.saturating_mul(2);
                let mut read = |digest: &str| {
                    if digest == current.sha256 {
                        if let Some(head) = primed.take() {
                            return Ok(head);
                        }
                    }
                    if chain_allowance == 0 {
                        read_failure = Some(MeetingInspectionError::Quarantine);
                        return Err(TranscriptArtifactError::TooDeep);
                    }
                    let relative = format!("transcript/{digest}.json");
                    match bounded_read(
                        &directory.join(&relative),
                        limits.max_transcript_bytes.min(chain_allowance),
                        total,
                        limits,
                    ) {
                        Ok(hop) => {
                            chain_allowance = chain_allowance.saturating_sub(hop.len() as u64);
                            Ok(hop)
                        }
                        // A chain hop that oversteps its allowance is a
                        // defective chain, not global budget exhaustion; it
                        // quarantines this meeting alone. If the shared
                        // budget is genuinely spent, the next meeting's
                        // ordinary read still aborts the build.
                        Err(LibraryReadError::CapacityExceeded) => {
                            read_failure = Some(MeetingInspectionError::Quarantine);
                            Err(TranscriptArtifactError::TooDeep)
                        }
                        Err(error) => {
                            read_failure = Some(MeetingInspectionError::from(error));
                            Err(TranscriptArtifactError::Changed)
                        }
                    }
                };
                resolve_stored_transcript_with(&mut read, meeting_uuid, &current.sha256)
            };
            let resolved = resolved.map_err(|_| {
                read_failure
                    .take()
                    .unwrap_or(MeetingInspectionError::Quarantine)
            })?;
            let restored: BTreeSet<u32> = resolved
                .inspection
                .restored_source_turn_indices
                .iter()
                .copied()
                .collect();
            (resolved.base_bytes, restored)
        } else {
            // The view branch re-verifies its bytes inside the walker; this
            // branch must not trust a file that changed between the
            // artifact_ref hash above and the read here.
            if format!("{:x}", Sha256::digest(&bytes)) != current.sha256 {
                return Err(MeetingInspectionError::Quarantine);
            }
            (bytes, BTreeSet::new())
        };
        let document: Transcript =
            serde_json::from_slice(&base_bytes).map_err(|_| MeetingInspectionError::Quarantine)?;
        (
            Some(current.sha256.clone()),
            Some(current.relative_path.clone()),
            validate_transcript(document, &restored)?,
            Some(actual),
        )
    } else {
        // The closed lifecycle table admits these rows for metadata only.  No
        // transcript bytes or transcript search authority are inferred.
        (None, None, Vec::new(), None)
    };
    let (note_json_sha256, note_markdown_sha256, claims) = if meeting.lifecycle
        == MeetingLifecycle::Ready
    {
        let note = meeting
            .artifacts
            .current_note
            .as_ref()
            .ok_or(MeetingInspectionError::Quarantine)?;
        let request = ProjectRequest {
            request_id: Uuid::new_v4(),
            meeting_id: meeting.meeting_id.clone(),
            note_json_sha256: note.json.sha256.clone(),
            note_markdown_sha256: note.markdown.sha256.clone(),
            transcript_sha256: transcript_sha256
                .clone()
                .ok_or(MeetingInspectionError::Quarantine)?,
        };
        let claims = match claim_source {
            ClaimSource::Accepted(rows) => rows
                .iter()
                .find(|row| {
                    row.meeting_id == request.meeting_id
                        && row.note_json_sha256.as_deref()
                            == Some(request.note_json_sha256.as_str())
                        && row.note_markdown_sha256.as_deref()
                            == Some(request.note_markdown_sha256.as_str())
                        && row.transcript_sha256.as_deref()
                            == Some(request.transcript_sha256.as_str())
                })
                .map(|row| row.claims.clone())
                .ok_or(MeetingInspectionError::Unavailable)?,
            ClaimSource::Projector(projector) => {
                let transcript_text: Vec<_> = turns
                    .iter()
                    .filter(|turn| !turn.gated)
                    .map(|turn| turn.text.clone())
                    .collect();
                project_claims(projector, &request, &transcript_text)
                    .map_err(|error| match error {
                        ProjectionError::ArtifactMissing
                        | ProjectionError::ArtifactInvalid
                        | ProjectionError::ArtifactChanged => MeetingInspectionError::Quarantine,
                        ProjectionError::CapacityExceeded => {
                            MeetingInspectionError::CapacityExceeded
                        }
                        ProjectionError::Unavailable => MeetingInspectionError::Unavailable,
                    })?
                    .into_iter()
                    .map(|claim| {
                        let locators = claim
                            .locators
                            .into_iter()
                            .map(|projected| {
                                let source_turn_index = turns
                                    .iter()
                                    .find(|turn| turn.visible_index == Some(projected.turn))
                                    .map(|turn| turn.index)
                                    .ok_or(MeetingInspectionError::Unavailable)?;
                                Ok(StoredLocator {
                                    projected,
                                    source_turn_index,
                                })
                            })
                            .collect::<Result<_, MeetingInspectionError>>()?;
                        Ok(StoredClaim {
                            ordinal: claim.ordinal,
                            sha256: claim.sha256,
                            claim_type: claim.claim_type,
                            text: claim.text,
                            locators,
                        })
                    })
                    .collect::<Result<Vec<_>, MeetingInspectionError>>()?
            }
        };
        (
            Some(note.json.sha256.clone()),
            Some(note.markdown.sha256.clone()),
            claims,
        )
    } else {
        (None, None, Vec::new())
    };
    // Re-read the pointers after inspection before allowing this candidate into a snapshot.
    let again = load_meeting(directory).map_err(|_| MeetingInspectionError::Quarantine)?;
    if again != meeting
        || artifact_ref(directory, "meeting.json")
            .map_err(|_| MeetingInspectionError::Quarantine)?
            != meeting_record
        || artifact_ref(directory, "attempt.json")
            .map_err(|_| MeetingInspectionError::Quarantine)?
            != attempt
        || current_artifact.as_ref().is_some_and(|actual| {
            artifact_ref(directory, &actual.relative_path)
                .map(|again| again != *actual)
                .unwrap_or(true)
        })
        || meeting.artifacts.current_note.as_ref().is_some_and(|note| {
            artifact_ref(directory, &note.json.relative_path)
                .map(|again| again != note.json)
                .unwrap_or(true)
                || artifact_ref(directory, &note.markdown.relative_path)
                    .map(|again| again != note.markdown)
                    .unwrap_or(true)
        })
    {
        return Err(MeetingInspectionError::Quarantine);
    }
    Ok(Some(LibraryRow {
        meeting_id: meeting.meeting_id.clone(),
        created_at_epoch_seconds: attempt_data.created_at_epoch_seconds,
        transcript_sha256,
        lifecycle: meeting.lifecycle,
        meeting_record_sha256: meeting_record.sha256,
        transcript_relative_path,
        turns,
        note_json_sha256,
        note_markdown_sha256,
        claims,
        attempt_sha256: attempt.sha256,
        title: None,
        folder: None,
        folder_id: None,
    }))
}

fn bounded_read(
    path: &Path,
    maximum: u64,
    total: &mut u64,
    limits: ReadLimits,
) -> Result<Vec<u8>, LibraryReadError> {
    let mut file = open_private_file(path).map_err(|_| LibraryReadError::ArtifactUnavailable)?;
    let length = file
        .metadata()
        .map_err(|_| LibraryReadError::ArtifactUnavailable)?
        .len();
    if length > maximum {
        return Err(LibraryReadError::CapacityExceeded);
    }
    *total = total
        .checked_add(length)
        .ok_or(LibraryReadError::CapacityExceeded)?;
    if *total > limits.max_total_bytes {
        return Err(LibraryReadError::CapacityExceeded);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| LibraryReadError::ArtifactUnavailable)?;
    if bytes.len() as u64 != length {
        return Err(LibraryReadError::ArtifactUnavailable);
    }
    Ok(bytes)
}

fn account_reference(
    directory: &Path,
    reference: &crate::meeting::ArtifactRef,
    total: &mut u64,
    limits: ReadLimits,
) -> Result<(), LibraryReadError> {
    let _ = bounded_read(
        &directory.join(&reference.relative_path),
        limits.max_total_bytes,
        total,
        limits,
    )?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    schema: String,
    meeting_id: String,
    attempt_id: String,
    created_at_epoch_seconds: u64,
    application_build_sha256: String,
    participant_notice_version: String,
    operator_attestation: Attestation,
    retention_policy_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attestation {
    participants_consented: bool,
    headphones: bool,
    operator_alone: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript {
    schema: String,
    source: String,
    attribution: String,
    #[serde(rename = "bleed")]
    _bleed: Option<serde_json::Value>,
    #[serde(rename = "voiceprint")]
    _voiceprint: Option<serde_json::Value>,
    capture_health: serde_json::Value,
    turns: Vec<Turn>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    start: f64,
    end: f64,
    speaker: Option<String>,
    text: String,
    gated: Option<bool>,
    gate_score: Option<f64>,
    gate_reason: Option<String>,
}

fn validate_transcript(
    document: Transcript,
    restored: &BTreeSet<u32>,
) -> Result<Vec<StoredTurn>, LibraryReadError> {
    if document.schema != "capture-transcript/1"
        || document.source.is_empty()
        || !matches!(document.attribution.as_str(), "channel" | "none")
        || !document.capture_health.is_object()
        || document.turns.len() > MAX_TURNS
    {
        return Err(LibraryReadError::ArtifactUnavailable);
    }
    let mut visible_index = 0_u64;
    document
        .turns
        .into_iter()
        .enumerate()
        .map(|(index, turn)| {
            if !turn.start.is_finite()
                || !turn.end.is_finite()
                || turn.start < 0.0
                || turn.end < turn.start
                || turn.text.len() > 100_000
                || turn
                    .speaker
                    .as_ref()
                    .is_some_and(|speaker| speaker.len() > 256)
                || (document.attribution == "channel"
                    && turn
                        .speaker
                        .as_deref()
                        .is_some_and(|speaker| !matches!(speaker, "Me" | "Them")))
                || (document.attribution == "channel"
                    && turn.gated == Some(true)
                    && turn.speaker.as_deref() != Some("Me"))
                || ((turn.gate_score.is_some() || turn.gate_reason.is_some())
                    && turn.gated != Some(true))
            {
                return Err(LibraryReadError::ArtifactUnavailable);
            }
            let restored_here = restored.contains(&(index as u32));
            if restored_here && turn.gated != Some(true) {
                // The chain walker already refuses this; a second refusal here
                // keeps the invariant local to the visibility decision.
                return Err(LibraryReadError::ArtifactUnavailable);
            }
            let gated = turn.gated == Some(true) && !restored_here;
            let current_visible_index = (!gated).then(|| {
                let current = visible_index;
                visible_index += 1;
                current
            });
            Ok(StoredTurn {
                index: index as u32,
                visible_index: current_visible_index,
                text: turn.text,
                gated,
            })
        })
        .collect()
}

fn normalize_query(query: &str) -> Result<String, LibraryReadError> {
    if query.chars().any(is_forbidden) {
        return Err(LibraryReadError::InvalidRequest);
    }
    let trimmed = query.trim_matches(char::is_whitespace);
    if trimmed.is_empty() || trimmed.chars().count() > 256 {
        return Err(LibraryReadError::InvalidRequest);
    }
    Ok(normalize(trimmed).0)
}

fn normalize_search_query(query: &str) -> Result<String, LibraryReadError> {
    let normalized = normalize_query(query)?;
    if normalized.chars().count() < 2 {
        return Err(LibraryReadError::InvalidRequest);
    }
    Ok(normalized)
}

fn normalized_matches(field: &str, needle: &str) -> Vec<(u64, u64)> {
    let (haystack, origins) = normalize(field);
    if needle.is_empty() {
        return Vec::new();
    }
    let chars: Vec<_> = haystack.chars().collect();
    let needle: Vec<_> = needle.chars().collect();
    chars
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle.as_slice())
        .map(|(start, _)| {
            let span = &origins[start..start + needle.len()];
            (
                span.iter().map(|range| range.0).min().unwrap_or(0),
                span.iter().map(|range| range.1).max().unwrap_or(0),
            )
        })
        .collect()
}

/// Frozen `search-normalization/1`: grapheme -> ICU NFC -> Rust lowercase,
/// with every normalized scalar retaining its complete original scalar range.
fn normalize(value: &str) -> (String, Vec<(u64, u64)>) {
    let nfc = ComposingNormalizer::new_nfc();
    let mut normalized = String::new();
    let mut origins = Vec::new();
    let mut scalar = 0_u64;
    for grapheme in value.graphemes(true) {
        let count = grapheme.chars().count() as u64;
        let range = (scalar, scalar + count);
        scalar += count;
        let mut nfc_text = String::new();
        nfc.normalize_to(grapheme, &mut nfc_text)
            .expect("string write");
        for character in nfc_text.chars().flat_map(char::to_lowercase) {
            normalized.push(character);
            origins.push(range);
        }
    }
    (normalized, origins)
}

fn is_forbidden(character: char) -> bool {
    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn span_is_valid(text: &str, start: u64, end: u64) -> bool {
    start < end && end <= text.chars().count() as u64
}
fn transcript_artifact(row: &LibraryRow) -> Result<Option<ArtifactRef>, LibraryReadError> {
    match (&row.transcript_relative_path, &row.transcript_sha256) {
        (Some(relative_path), Some(sha256)) => Ok(Some(ArtifactRef {
            relative_path: relative_path.clone(),
            sha256: sha256.clone(),
        })),
        (None, None) => Ok(None),
        _ => Err(LibraryReadError::SnapshotStale),
    }
}
fn scalar_slice(text: &str, start: u64, end: u64) -> Option<String> {
    if !span_is_valid(text, start, end) {
        return None;
    }
    Some(
        text.chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect(),
    )
}
fn claim_matches(row: &LibraryRow, claim: &StoredClaim, normalized_query: &str) -> bool {
    !normalized_matches(&claim.text, normalized_query).is_empty()
        || claim.locators.iter().any(|locator| {
            row.turns
                .iter()
                .find(|turn| turn.index == locator.source_turn_index && !turn.gated)
                .and_then(|turn| {
                    scalar_slice(&turn.text, locator.projected.start, locator.projected.end)
                })
                .is_some_and(|text| !normalized_matches(&text, normalized_query).is_empty())
        })
}
#[cfg(test)]
fn digest_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn meeting_order(left: &LibraryRow, right: &LibraryRow) -> std::cmp::Ordering {
    right
        .created_at_epoch_seconds
        .cmp(&left.created_at_epoch_seconds)
        .then_with(|| left.meeting_id.cmp(&right.meeting_id))
}
fn hit_key(hit: &SealedHit) -> &str {
    match hit {
        SealedHit::Claim { key, .. }
        | SealedHit::Transcript { key, .. }
        | SealedHit::Withheld { key, .. }
        | SealedHit::Meeting { key, .. } => key,
    }
}
fn hit_meeting_id(hit: &SealedHit) -> &str {
    match hit {
        SealedHit::Claim { authority, .. }
        | SealedHit::Transcript { authority, .. }
        | SealedHit::Withheld { authority, .. }
        | SealedHit::Meeting { authority, .. } => &authority.meeting_id,
    }
}
fn hit_authority(hit: &SealedHit) -> &HitAuthority {
    match hit {
        SealedHit::Claim { authority, .. }
        | SealedHit::Transcript { authority, .. }
        | SealedHit::Withheld { authority, .. }
        | SealedHit::Meeting { authority, .. } => authority,
    }
}
fn hit_order(left: &SealedHit, right: &SealedHit) -> std::cmp::Ordering {
    authority_created(hit_authority(right))
        .cmp(&authority_created(hit_authority(left)))
        .then_with(|| hit_meeting_id(left).cmp(hit_meeting_id(right)))
        .then_with(|| hit_location(left).cmp(&hit_location(right)))
        .then_with(|| hit_kind(left).cmp(&hit_kind(right)))
        .then_with(|| hit_claim_digest(left).cmp(hit_claim_digest(right)))
        .then_with(|| hit_claim_ordinal(left).cmp(&hit_claim_ordinal(right)))
        .then_with(|| hit_key(left).cmp(hit_key(right)))
}
fn hit_kind(hit: &SealedHit) -> u8 {
    match hit {
        SealedHit::Claim { .. } => 0,
        SealedHit::Transcript { .. } => 1,
        SealedHit::Withheld { .. } => 2,
        SealedHit::Meeting { .. } => 3,
    }
}
fn hit_location(hit: &SealedHit) -> (u64, u64) {
    match hit {
        SealedHit::Claim { claim, .. } => claim
            .locators
            .first()
            .map(|locator| (locator.projected.turn, locator.projected.start))
            .unwrap_or((u64::MAX, u64::MAX)),
        SealedHit::Transcript {
            source_turn_index,
            original_scalar_start,
            ..
        } => (*source_turn_index as u64, *original_scalar_start),
        SealedHit::Withheld {
            source_turn_index, ..
        } => (*source_turn_index as u64, u64::MAX),
        SealedHit::Meeting { .. } => (u64::MAX, u64::MAX),
    }
}
fn hit_claim_digest(hit: &SealedHit) -> &str {
    match hit {
        SealedHit::Claim { claim, .. } => &claim.sha256,
        _ => "",
    }
}
fn hit_claim_ordinal(hit: &SealedHit) -> u64 {
    match hit {
        SealedHit::Claim { claim, .. } => claim.ordinal,
        _ => 0,
    }
}
fn authority_created(authority: &HitAuthority) -> u64 {
    authority.created_at_epoch_seconds
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::meeting::{
        ArtifactRef, AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts,
        MeetingRecord, MeetingSchema, artifact_ref, retention_policy_sha256, write_meeting,
    };
    use crate::note_projection::{NoteProjector, ProjectRequest, ProjectTransportError};
    use crate::storage::{create_private_dir, durable_create_new};

    macro_rules! assert_stale {
        ($result:expr) => {
            assert!(matches!($result, Err(LibraryReadError::SnapshotStale)));
        };
    }

    struct Fixture {
        temp: TempDir,
        storage: StorageRoot,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let repository = temp.path().join("repository");
            create_private_dir(&repository).unwrap();
            let storage = StorageRoot::create(&temp.path().join("data"), &repository).unwrap();
            Self { temp, storage }
        }

        fn meeting(&self, id: &str, created: u64, turns: &[(&str, bool)]) -> PathBuf {
            let directory = self.storage.path().join("meetings").join(id);
            for child in ["", "capture", "transcript", "deletion"] {
                create_private_dir(&directory.join(child)).unwrap();
            }
            let rule = AudioRetentionRule::DeleteAfter { seconds: 60 };
            let policy = retention_policy_sha256(&rule);
            let attempt = json!({
                "schema": "capture-attempt/1", "meeting_id": id,
                "attempt_id": "11111111-1111-4111-8111-111111111111",
                "created_at_epoch_seconds": created,
                "application_build_sha256": "a".repeat(64),
                "participant_notice_version": NOTICE_VERSION,
                "operator_attestation": {"participantsConsented": true, "headphones": true, "operatorAlone": true},
                "retention_policy_sha256": policy,
            });
            durable_create_new(
                &directory.join("attempt.json"),
                &serde_json::to_vec_pretty(&attempt).unwrap(),
            )
            .unwrap();
            for path in [
                "ownership.json",
                "capture/session.json",
                "deletion/audio-deletion.json",
            ] {
                durable_create_new(&directory.join(path), b"{}\n").unwrap();
            }
            let turns: Vec<_> = turns.iter().enumerate().map(|(index, (text, gated))| json!({
                "start": index as f64, "end": index as f64 + 0.5, "speaker": "Me", "text": text,
                "gated": gated,
            })).collect();
            let transcript = serde_json::to_vec_pretty(&json!({
                "schema": "capture-transcript/1", "source": "synthetic", "attribution": "channel",
                "bleed": null, "voiceprint": null, "capture_health": {}, "turns": turns,
            }))
            .unwrap();
            let digest = format!("{:x}", Sha256::digest(&transcript));
            let relative = format!("transcript/{digest}.json");
            durable_create_new(&directory.join(&relative), &transcript).unwrap();
            let reference = |path: &str| artifact_ref(&directory, path).unwrap();
            let record = MeetingRecord {
                schema: MeetingSchema::V2,
                meeting_id: id.into(),
                lifecycle: MeetingLifecycle::TranscriptReady,
                retention: AudioRetention {
                    rule,
                    policy_sha256: policy,
                    next_deletion_at_epoch_seconds: Some(created + 60),
                    state: AudioState::Released,
                    deletion_receipt: Some(reference("deletion/audio-deletion.json")),
                },
                artifacts: MeetingArtifacts {
                    attempt: reference("attempt.json"),
                    ownership: Some(reference("ownership.json")),
                    capture_session: Some(reference("capture/session.json")),
                    microphone_audio: Some(ArtifactRef {
                        relative_path: "capture/mic.wav".into(),
                        sha256: "b".repeat(64),
                    }),
                    system_audio: Some(ArtifactRef {
                        relative_path: "capture/system.wav".into(),
                        sha256: "c".repeat(64),
                    }),
                    current_transcript: Some(reference(&relative)),
                    current_note: None,
                },
                pending_storage_operation: None,
            };
            write_meeting(&directory, &record).unwrap();
            directory
        }

        fn metadata(&self, bytes: &[u8]) -> PathBuf {
            let library = self.storage.path().join("library");
            create_private_dir(&library).unwrap();
            let path = library.join("metadata.json");
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            durable_create_new(&path, bytes).unwrap();
            path
        }

        fn ready_meeting(&self, id: &str, created: u64) -> PathBuf {
            self.ready_meeting_with_turns(
                id,
                created,
                &[
                    ("alpha", false),
                    ("beta", false),
                    ("gamma", false),
                    ("aé🙂z", false),
                ],
            )
        }

        fn ready_meeting_with_turns(
            &self,
            id: &str,
            created: u64,
            turns: &[(&str, bool)],
        ) -> PathBuf {
            let directory = self.meeting(id, created, turns);
            let mut record = load_meeting(&directory).unwrap();
            create_private_dir(&directory.join("notes")).unwrap();
            let json = b"{}\n";
            let markdown = b"# private\n";
            let json_digest = digest_bytes(json);
            let markdown_digest = digest_bytes(markdown);
            let json_path = format!("notes/{json_digest}.json");
            let markdown_path = format!("notes/{markdown_digest}.md");
            durable_create_new(&directory.join(&json_path), json).unwrap();
            durable_create_new(&directory.join(&markdown_path), markdown).unwrap();
            let transcript = record
                .artifacts
                .current_transcript
                .as_ref()
                .unwrap()
                .sha256
                .clone();
            record.lifecycle = MeetingLifecycle::Ready;
            record.artifacts.current_note = Some(crate::meeting::NoteRevisionRef {
                json: artifact_ref(&directory, &json_path).unwrap(),
                markdown: artifact_ref(&directory, &markdown_path).unwrap(),
                source_transcript_sha256: transcript,
            });
            write_meeting(&directory, &record).unwrap();
            directory
        }

        fn metadata_only_meeting(
            &self,
            id: &str,
            created: u64,
            lifecycle: MeetingLifecycle,
        ) -> PathBuf {
            let directory = self.meeting(id, created, &[("unsearched", false)]);
            let mut record = load_meeting(&directory).unwrap();
            record.lifecycle = lifecycle;
            record.artifacts.current_transcript = None;
            record.artifacts.current_note = None;
            match lifecycle {
                MeetingLifecycle::Incomplete => {
                    record.artifacts.ownership = None;
                    record.artifacts.capture_session = None;
                    record.artifacts.microphone_audio = None;
                    record.artifacts.system_audio = None;
                    record.retention.state = AudioState::NeverCreated;
                    record.retention.deletion_receipt = None;
                }
                MeetingLifecycle::Captured | MeetingLifecycle::TranscriptionFailed => {}
                MeetingLifecycle::RecoveredInterrupted => {
                    record.artifacts.ownership = None;
                    record.artifacts.capture_session = None;
                    record.artifacts.microphone_audio = None;
                    record.artifacts.system_audio = None;
                    record.retention.state = AudioState::NeverCreated;
                    record.retention.deletion_receipt = None;
                }
                _ => panic!("fixture only supports metadata-only lifecycles"),
            }
            write_meeting(&directory, &record).unwrap();
            directory
        }
    }

    #[derive(Clone, Copy)]
    enum ProjectorMode {
        Success,
        Refusal(&'static str),
        Transport,
        /// A frame the worker calls successful, carrying a claim this side
        /// will not parse. Distinct from `Refusal`: nothing is declared
        /// wrong, so the reader learns of the disagreement only by failing to
        /// read it.
        Divergent,
    }

    struct FixtureProjector(Mutex<ProjectorMode>);

    impl FixtureProjector {
        fn new(mode: ProjectorMode) -> Self {
            Self(Mutex::new(mode))
        }
        fn set(&self, mode: ProjectorMode) {
            *self.0.lock().unwrap() = mode;
        }
    }

    impl NoteProjector for FixtureProjector {
        fn project(&self, request: &ProjectRequest) -> Result<Vec<u8>, ProjectTransportError> {
            match *self.0.lock().unwrap() {
                ProjectorMode::Transport => Err(ProjectTransportError::Unavailable),
                ProjectorMode::Refusal(code) => {
                    Ok(format!("{{\"schema\":\"note-projection-result/1\",\"request_id\":\"{}\",\"operation\":\"note.project\",\"outcome\":\"refused\",\"projection\":null,\"failure\":{{\"code\":\"{code}\",\"recoverable\":{}}}}}\n", request.request_id, code == "artifact-missing").into_bytes())
                }
                ProjectorMode::Divergent => {
                    // A control character in claim text. `parse_claim` checks
                    // the text rules before the digest, so the digest here is
                    // the true one: the only thing wrong with this frame is
                    // the rule under test, not a second defect that would let
                    // the test pass for the wrong reason.
                    let locator = "{\"turn\":0,\"start\":0,\"end\":5,\"text_sha256\":\"8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8\"}";
                    let claim = format!(
                        "{{\"claim_ordinal\":0,\"claim_sha256\":\"4958e6f016634210e3858a8d3680ee15a58c8fea265af6448aa3751e1b5946a0\",\"claim_type\":\"decision\",\"evidence_state\":\"located\",\"claim\":\"claim\\u0007a\",\"locators\":[{locator}]}}"
                    );
                    Ok(format!("{{\"schema\":\"note-projection-result/1\",\"request_id\":\"{}\",\"operation\":\"note.project\",\"outcome\":\"succeeded\",\"projection\":{{\"schema\":\"note-claim-projection/1\",\"note_json_sha256\":\"{}\",\"note_markdown_sha256\":\"{}\",\"transcript_sha256\":\"{}\",\"claims\":[{claim}]}},\"failure\":null}}\n", request.request_id, request.note_json_sha256, request.note_markdown_sha256, request.transcript_sha256).into_bytes())
                }
                ProjectorMode::Success => {
                    let locator = "{\"turn\":0,\"start\":0,\"end\":5,\"text_sha256\":\"8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8\"}";
                    let claims = (0..3)
                        .map(|ordinal| format!("{{\"claim_ordinal\":{ordinal},\"claim_sha256\":\"b8eccd74fe344c3e91654493ed1cc69e9b637cf8dd0ed2d2884f2375bd15c4f2\",\"claim_type\":\"decision\",\"evidence_state\":\"located\",\"claim\":\"claim-a\",\"locators\":[{locator}]}}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    Ok(format!("{{\"schema\":\"note-projection-result/1\",\"request_id\":\"{}\",\"operation\":\"note.project\",\"outcome\":\"succeeded\",\"projection\":{{\"schema\":\"note-claim-projection/1\",\"note_json_sha256\":\"{}\",\"note_markdown_sha256\":\"{}\",\"transcript_sha256\":\"{}\",\"claims\":[{claims}]}},\"failure\":null}}\n", request.request_id, request.note_json_sha256, request.note_markdown_sha256, request.transcript_sha256).into_bytes())
                }
            }
        }
    }

    #[test]
    fn rebuild_is_read_only_and_quarantines_tampered_meetings() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("safe token", false)]);
        let bad = fixture.meeting("meeting-b", 9, &[("tampered token", false)]);
        fs::write(bad.join("attempt.json"), b"not a receipt").unwrap();
        let before = tree_digest(fixture.storage.path());
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.rows().len(), 1);
        assert_eq!(projection.quarantined_meetings(), 1);
        assert_eq!(before, tree_digest(fixture.storage.path()));
        assert!(fixture.temp.path().exists());
    }

    #[test]
    fn exclusion_set_skips_active_directory_and_is_snapshot_authority() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-stable", 10, &[("stable words", false)]);
        let active = fixture.meeting("meeting-active", 20, &[("partial words", false)]);
        fs::write(
            active.join("attempt.json"),
            b"writer is replacing this receipt",
        )
        .unwrap();
        let excluded = HashSet::from(["meeting-active".to_owned()]);

        let projection = LibraryProjection::rebuild_excluding(
            &fixture.storage,
            ReadLimits::default(),
            &excluded,
        )
        .unwrap();

        assert_eq!(projection.rows().len(), 1);
        assert_eq!(projection.rows()[0].meeting_id, "meeting-stable");
        assert_eq!(projection.quarantined_meetings(), 0);
        let handle = projection.search_hits("stable").unwrap().remove(0);
        assert!(
            projection
                .open_excluding(&fixture.storage, &handle, &excluded)
                .is_ok()
        );
        assert_stale!(projection.open_excluding(&fixture.storage, &handle, &HashSet::new()));
        assert_stale!(projection.validate_snapshot_excluding(&fixture.storage, &HashSet::new()));
    }

    #[test]
    fn unchanged_snapshot_validation_reuses_accepted_claims_without_calling_the_projector() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-a", 10);
        let projector = Arc::new(FixtureProjector::new(ProjectorMode::Success));
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            projector.clone(),
        )
        .unwrap();
        projector.set(ProjectorMode::Transport);

        assert_eq!(projection.validate_snapshot(&fixture.storage), Ok(()));
    }

    #[test]
    fn current_claims_preserve_duplicate_ordinals_and_revalidate_sources_without_relaunching() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-a", 10);
        let projector = Arc::new(FixtureProjector::new(ProjectorMode::Success));
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            projector.clone(),
        )
        .unwrap();
        let hits = projection.search_hits("claim-a").unwrap();
        assert!(matches!(
            projection.sealed(&hits[0]),
            Some(SealedHit::Claim {
                claim: StoredClaim { ordinal: 0, .. },
                ..
            })
        ));
        let claim_hits = projection.search_hits("claim-a").unwrap();
        assert_eq!(claim_hits.len(), 3, "same text remains distinct by ordinal");
        for (ordinal, hit) in claim_hits.iter().enumerate() {
            assert!(
                matches!(projection.open(&fixture.storage, hit).unwrap(), OpenedLibraryHit::Claim { claim_ordinal, ref claim, .. } if claim_ordinal == ordinal as u64 && claim == "claim-a")
            );
        }
        projector.set(ProjectorMode::Refusal("artifact-changed"));
        assert!(
            projection.open(&fixture.storage, &claim_hits[0]).is_ok(),
            "an already accepted claim remains readable when its digest-bound artifacts are unchanged"
        );

        let fixture = Fixture::new();
        let directory = fixture.ready_meeting("meeting-a", 10);
        let projector = Arc::new(FixtureProjector::new(ProjectorMode::Success));
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            projector,
        )
        .unwrap();
        let hit = projection.search_hits("claim-a").unwrap().remove(0);
        let note = load_meeting(&directory)
            .unwrap()
            .artifacts
            .current_note
            .unwrap();
        fs::write(directory.join(note.json.relative_path), b"changed").unwrap();
        assert_stale!(projection.open(&fixture.storage, &hit));
    }

    #[test]
    fn note_claims_and_exact_evidence_keep_the_existing_snapshot_boundary() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-a", 10);
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            Arc::new(FixtureProjector::new(ProjectorMode::Success)),
        )
        .unwrap();

        let claims = projection.note_claims("meeting-a").unwrap();
        assert_eq!(claims.len(), 3);
        assert!(matches!(
            projection.open(&fixture.storage, &claims[0]).unwrap(),
            OpenedLibraryHit::Claim {
                claim_ordinal: 0,
                ..
            }
        ));
        assert!(matches!(
            projection.open_claim_evidence(&fixture.storage, &claims[0], 0).unwrap(),
            OpenedClaimEvidence { source_turn_index: 0, ref text, .. } if text == "alpha"
        ));
        assert_eq!(
            projection.note_claims("missing").unwrap_err(),
            LibraryReadError::InvalidRequest
        );
        assert_eq!(
            projection
                .open_claim_evidence(&fixture.storage, &claims[0], 1)
                .unwrap_err(),
            LibraryReadError::InvalidRequest
        );
    }

    #[test]
    fn claim_search_is_claim_first_deduplicated_and_opens_complete_evidence() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-a", 10);
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            Arc::new(FixtureProjector::new(ProjectorMode::Success)),
        )
        .unwrap();

        let cited_span_hits = projection.search_hits("alpha").unwrap();
        assert_eq!(cited_span_hits.len(), 4);
        assert!(
            cited_span_hits
                .iter()
                .filter(|hit| matches!(projection.sealed(hit), Some(SealedHit::Claim { .. })))
                .count()
                == 3
        );
        assert!(
            cited_span_hits
                .iter()
                .any(|hit| matches!(projection.sealed(hit), Some(SealedHit::Transcript { .. })))
        );

        let claim_and_span_hits = projection.search_hits("claim-a").unwrap();
        assert_eq!(
            claim_and_span_hits
                .iter()
                .filter(|hit| matches!(projection.sealed(hit), Some(SealedHit::Claim { .. })))
                .count(),
            3,
            "claim text plus cited span still yields one hit per claim ordinal"
        );

        let opened = projection
            .open(&fixture.storage, &cited_span_hits[0])
            .unwrap();
        assert!(matches!(
            opened,
            OpenedLibraryHit::Claim {
                meeting_id,
                claim_ordinal: 0,
                claim_sha256,
                claim_type: crate::note_projection::ClaimType::Decision,
                evidence_state: ClaimEvidenceState::Located,
                claim,
                locators,
                note_json_sha256,
                note_markdown_sha256,
                transcript_sha256,
            } if meeting_id == "meeting-a"
                && claim_sha256 == "b8eccd74fe344c3e91654493ed1cc69e9b637cf8dd0ed2d2884f2375bd15c4f2"
                && claim == "claim-a"
                && locators.len() == 1
                && locators[0].turn == 0
                && locators[0].source_turn_index == 0
                && locators[0].start == 0
                && locators[0].end == 5
                && locators[0].text_sha256 == "8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8"
                && valid_digest(&note_json_sha256)
                && valid_digest(&note_markdown_sha256)
                && valid_digest(&transcript_sha256)
        ));
    }

    #[test]
    fn projected_visible_turn_maps_back_to_original_turn_after_withheld_input() {
        let fixture = Fixture::new();
        fixture.ready_meeting_with_turns(
            "meeting-a",
            10,
            &[("withheld", true), ("alpha", false), ("beta", false)],
        );
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            Arc::new(FixtureProjector::new(ProjectorMode::Success)),
        )
        .unwrap();
        let hit = projection.search_hits("alpha").unwrap().remove(0);
        assert!(matches!(
            projection.open(&fixture.storage, &hit).unwrap(),
            OpenedLibraryHit::Claim { locators, .. }
                if locators[0].turn == 0 && locators[0].source_turn_index == 1
        ));
    }

    #[test]
    fn hit_order_is_recency_id_location_kind_digest_ordinal_then_id() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-z", 11);
        fixture.ready_meeting("meeting-a", 10);
        fixture.ready_meeting("meeting-b", 10);
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            Arc::new(FixtureProjector::new(ProjectorMode::Success)),
        )
        .unwrap();
        let ordered: Vec<_> = projection
            .search_hits("claim-a")
            .unwrap()
            .iter()
            .map(|hit| match projection.sealed(hit).unwrap() {
                SealedHit::Claim {
                    authority, claim, ..
                } => (authority.meeting_id, claim.ordinal),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            ordered,
            vec![
                ("meeting-z".into(), 0),
                ("meeting-z".into(), 1),
                ("meeting-z".into(), 2),
                ("meeting-a".into(), 0),
                ("meeting-a".into(), 1),
                ("meeting-a".into(), 2),
                ("meeting-b".into(), 0),
                ("meeting-b".into(), 1),
                ("meeting-b".into(), 2),
            ]
        );

        let authority = projection.authority(&projection.rows[0]);
        let locator = StoredLocator {
            projected: crate::note_projection::Locator {
                turn: 0,
                start: 0,
                end: 5,
                text_sha256: "8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8"
                    .into(),
            },
            source_turn_index: 0,
        };
        let claim = |key: &str, digest: char, ordinal| SealedHit::Claim {
            key: key.into(),
            authority: authority.clone(),
            claim: StoredClaim {
                ordinal,
                sha256: digest.to_string().repeat(64),
                claim_type: crate::note_projection::ClaimType::Decision,
                text: "claim".into(),
                locators: vec![locator.clone()],
            },
            normalized_query: "claim".into(),
        };
        let mut same_location = [
            SealedHit::Transcript {
                key: "t-b".into(),
                authority: authority.clone(),
                source_turn_index: 0,
                original_scalar_start: 0,
                original_scalar_end: 1,
                normalized_query: "a".into(),
            },
            claim("c-b-2", 'b', 2),
            claim("c-a-2", 'a', 2),
            claim("c-a-1", 'a', 1),
            SealedHit::Transcript {
                key: "t-a".into(),
                authority,
                source_turn_index: 0,
                original_scalar_start: 0,
                original_scalar_end: 1,
                normalized_query: "a".into(),
            },
        ];
        same_location.sort_by(hit_order);
        assert_eq!(
            same_location.iter().map(hit_key).collect::<Vec<_>>(),
            vec!["c-a-1", "c-a-2", "c-b-2", "t-a", "t-b"]
        );
    }

    #[test]
    fn projection_failures_never_publish_partial_ready_meetings() {
        for (mode, expected) in [
            (
                ProjectorMode::Refusal("projection-capacity-exceeded"),
                LibraryReadError::CapacityExceeded,
            ),
            (
                ProjectorMode::Refusal("invalid-request"),
                LibraryReadError::ArtifactUnavailable,
            ),
            (
                ProjectorMode::Transport,
                LibraryReadError::ArtifactUnavailable,
            ),
        ] {
            let fixture = Fixture::new();
            fixture.ready_meeting("meeting-a", 10);
            fixture.ready_meeting("meeting-b", 9);
            let projector = Arc::new(FixtureProjector::new(mode));
            assert!(matches!(
                LibraryProjection::rebuild_with_projector(
                    &fixture.storage,
                    ReadLimits::default(),
                    projector,
                ),
                Err(actual) if actual == expected
            ));
        }
    }

    /// Refusing and diverging are different failures with different costs,
    /// and the gap between them is why the claim rules are duplicated on both
    /// sides of the boundary rather than delegated to this one.
    ///
    /// A declared refusal quarantines the affected meetings and the rebuild
    /// continues. A frame the worker calls successful, carrying a claim this
    /// side will not parse, is a content-free `Unavailable` that aborts the
    /// whole rebuild and names no meeting -- so a disagreement about one
    /// claim surfaces as the library being unavailable.
    ///
    /// The asymmetry is the argument for duplicating the rules: refusing too
    /// much locally costs a meeting, admitting something the other side will
    /// not parse costs the library. Nothing else asserts the two branches
    /// stay distinct, so a refactor collapsing them would be invisible.
    #[test]
    fn a_divergent_claim_costs_the_library_while_a_declared_refusal_costs_a_meeting() {
        let diverged = Fixture::new();
        diverged.ready_meeting("meeting-a", 10);
        diverged.ready_meeting("meeting-b", 9);
        assert!(matches!(
            LibraryProjection::rebuild_with_projector(
                &diverged.storage,
                ReadLimits::default(),
                Arc::new(FixtureProjector::new(ProjectorMode::Divergent)),
            ),
            Err(LibraryReadError::ArtifactUnavailable)
        ));

        let refused = Fixture::new();
        refused.ready_meeting("meeting-a", 10);
        refused.ready_meeting("meeting-b", 9);
        let projection = LibraryProjection::rebuild_with_projector(
            &refused.storage,
            ReadLimits::default(),
            Arc::new(FixtureProjector::new(ProjectorMode::Refusal(
                "artifact-invalid",
            ))),
        )
        .expect("a declared refusal must not fail the whole rebuild");
        assert_eq!(projection.quarantined_meetings(), 2);
    }

    #[test]
    fn artifact_refusals_quarantine_only_the_affected_ready_meeting() {
        for code in ["artifact-missing", "artifact-invalid", "artifact-changed"] {
            let fixture = Fixture::new();
            fixture.ready_meeting("meeting-a", 10);
            let projector = Arc::new(FixtureProjector::new(ProjectorMode::Refusal(code)));
            let projection = LibraryProjection::rebuild_with_projector(
                &fixture.storage,
                ReadLimits::default(),
                projector,
            )
            .unwrap();
            assert!(projection.rows().is_empty(), "{code}");
            assert_eq!(projection.quarantined_meetings(), 1, "{code}");
        }
    }

    #[test]
    fn missing_metadata_keeps_transcript_authority_and_valid_metadata_coalesces_meeting_hits() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
        let missing = LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(missing.search_hits("transcript").unwrap().len(), 1);
        assert!(missing.search_hits("project").unwrap().is_empty());

        fixture.metadata(br#"{"schema":"library-metadata/1","revision":4,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"Project Atlas"}],"meetings":[{"meeting_id":"meeting-a","title":"Project kickoff","folder_id":"11111111-1111-4111-8111-111111111111"}]}"#);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let title = projection.search_hits("project").unwrap();
        assert_eq!(title.len(), 1, "title and folder are one meeting hit");
        assert!(
            matches!(projection.open(&fixture.storage, &title[0]).unwrap(), OpenedLibraryHit::Meeting { ref title, ref folder, .. } if title.as_deref() == Some("Project kickoff") && folder.as_deref() == Some("Project Atlas"))
        );
        assert_eq!(projection.search_hits("transcript").unwrap().len(), 1);
    }

    #[test]
    fn a_derived_title_is_the_first_long_enough_visible_turn_and_is_never_a_search_field() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[
                ("Hi.", false),
                ("The insurer refused the claim again this week.", true),
                ("We should walk through the migration plan today.", false),
            ],
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let row = &projection.rows()[0];
        assert_eq!(
            row.derived_title().as_deref(),
            Some("We should walk through the migration plan today"),
            "the greeting is too short and the withheld turn is not a label source"
        );
        assert!(
            projection.search_hits("insurer").unwrap().len() == 1
                && matches!(
                    projection
                        .open(
                            &fixture.storage,
                            &projection.search_hits("insurer").unwrap()[0]
                        )
                        .unwrap(),
                    OpenedLibraryHit::Withheld { .. }
                ),
            "the withheld turn stays withheld, which is what it must not leak past"
        );

        // A derived title is a span of a retained turn, so the turn already
        // answers for it. One hit, of the transcript kind — not a second
        // meeting hit for the same words under a label.
        let hits = projection.search_hits("migration plan").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(matches!(
            projection.open(&fixture.storage, &hits[0]).unwrap(),
            OpenedLibraryHit::Transcript { .. }
        ));
    }

    #[test]
    fn an_operator_title_and_a_derived_title_are_separate_readings_of_the_same_row() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[("We should walk through the migration plan today.", false)],
        );
        fixture.metadata(
            br#"{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"Migration review","folder_id":null}]}"#,
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let row = &projection.rows()[0];
        assert_eq!(row.title(), Some("Migration review"));
        assert_eq!(
            row.derived_title().as_deref(),
            Some("We should walk through the migration plan today"),
            "the operator's title is authority over what is displayed, not over \
             what the transcript says"
        );
    }

    #[test]
    fn a_meeting_with_no_transcript_has_no_derived_title() {
        let fixture = Fixture::new();
        fixture.metadata_only_meeting("meeting-a", 10, MeetingLifecycle::Captured);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.rows()[0].derived_title(), None);
    }

    #[test]
    fn a_folder_filter_means_one_folder_even_when_two_share_a_name() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("alpha turn text here", false)]);
        fixture.meeting("meeting-b", 20, &[("beta turn text here", false)]);
        fixture.meeting("meeting-c", 30, &[("gamma turn text here", false)]);
        // Two folders, same name, different ids. The writer permits this and a
        // filter that matched on the name would union them.
        fixture.metadata(
            br#"{"schema":"library-metadata/1","revision":1,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"Clients"},{"id":"22222222-2222-4222-8222-222222222222","name":"Clients"}],"meetings":[{"meeting_id":"meeting-a","title":null,"folder_id":"11111111-1111-4111-8111-111111111111"},{"meeting_id":"meeting-b","title":null,"folder_id":"22222222-2222-4222-8222-222222222222"}]}"#,
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();

        let one = LibraryFilter {
            folder: FolderFilter::Named("11111111-1111-4111-8111-111111111111".into()),
            ..Default::default()
        };
        let rows = projection.filtered_rows(&one);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].meeting_id, "meeting-a");

        let unfiled = LibraryFilter {
            folder: FolderFilter::Unfiled,
            ..Default::default()
        };
        let rows = projection.filtered_rows(&unfiled);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].meeting_id, "meeting-c");

        assert_eq!(
            projection.filtered_rows(&LibraryFilter::default()).len(),
            3,
            "an empty filter is not a narrowing"
        );
    }

    #[test]
    fn a_capture_time_range_is_inclusive_at_both_ends() {
        let fixture = Fixture::new();
        for (id, created) in [("meeting-a", 10), ("meeting-b", 20), ("meeting-c", 30)] {
            fixture.meeting(id, created, &[("some turn text goes here", false)]);
        }
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let filter = LibraryFilter {
            start_epoch_seconds: Some(20),
            end_epoch_seconds: Some(30),
            ..Default::default()
        };
        let ids: Vec<_> = projection
            .filtered_rows(&filter)
            .into_iter()
            .map(|row| row.meeting_id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["meeting-c", "meeting-b"],
            "newest first, both ends in"
        );

        let single = LibraryFilter {
            start_epoch_seconds: Some(20),
            end_epoch_seconds: Some(20),
            ..Default::default()
        };
        assert_eq!(projection.filtered_rows(&single).len(), 1);
    }

    /// The filter and search must agree about what a character is.
    ///
    /// `search-normalization/1` pins Unicode 17.0.0, three crate checksums and a
    /// Rust commit for `char::to_lowercase`. A filter written with its own
    /// `to_lowercase().contains()` would disagree with search on exactly these
    /// inputs, which is why the filter runs through `normalized_matches` rather
    /// than growing a second rule.
    #[test]
    fn the_title_filter_normalizes_the_same_way_search_does() {
        let fixture = Fixture::new();
        // Composed é in the title, decomposed é in the needle.
        fixture.meeting("meeting-a", 10, &[("nothing relevant in this turn", false)]);
        fixture.metadata(
            "{\"schema\":\"library-metadata/1\",\"revision\":1,\"folders\":[],\"meetings\":[{\"meeting_id\":\"meeting-a\",\"title\":\"Caf\u{00e9} review\",\"folder_id\":null}]}".as_bytes(),
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();

        for needle in ["Caf\u{00e9}", "cafe\u{0301}", "CAF\u{00c9} REVIEW"] {
            let filter = LibraryFilter {
                title: Some(needle.into()),
                ..Default::default()
            };
            assert_eq!(
                projection.filtered_rows(&filter).len(),
                1,
                "{needle:?} did not match a title search would have matched"
            );
        }

        let miss = LibraryFilter {
            title: Some("warehouse".into()),
            ..Default::default()
        };
        assert!(projection.filtered_rows(&miss).is_empty());
    }

    #[test]
    fn the_title_filter_reads_the_derived_name_when_no_operator_title_exists() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[("We should walk through the migration plan today.", false)],
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let filter = LibraryFilter {
            title: Some("migration".into()),
            ..Default::default()
        };
        assert_eq!(projection.filtered_rows(&filter).len(), 1);
    }

    #[test]
    fn a_filter_narrows_which_meetings_search_answers_from_and_not_what_matches() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[("the needle appears in this turn", false)],
        );
        fixture.meeting("meeting-b", 20, &[("the needle appears here too", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();

        assert_eq!(projection.search_hits("needle").unwrap().len(), 2);
        let filter = LibraryFilter {
            start_epoch_seconds: Some(20),
            ..Default::default()
        };
        let hits = projection.search_filtered_hits("needle", &filter).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(matches!(
            projection.open(&fixture.storage, &hits[0]).unwrap(),
            OpenedLibraryHit::Transcript { ref meeting_id, .. } if meeting_id == "meeting-b"
        ));
    }

    /// Roadmap intake I5's search-path hardening. `search_filtered_excluding`
    /// is the hook a locked-meeting-aware caller uses (the desktop crate,
    /// which can read `meeting_lock` -- this crate cannot); this pins that
    /// every kind of hit `matches` can produce for one meeting -- transcript
    /// span, withheld-turn marker, and title match -- disappears when that
    /// meeting is excluded, and that the count a caller would render also
    /// drops, not merely the page of hits.
    #[test]
    fn search_filtered_excluding_removes_transcript_withheld_and_title_hits_for_one_meeting() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[
                ("shared secret sentence", false),
                ("withheld secret aside", true),
            ],
        );
        fixture.meeting("meeting-b", 20, &[("shared secret sentence", false)]);
        crate::library_metadata::set_meeting_title(
            &fixture.storage,
            0,
            "meeting-a",
            Some("Shared Title"),
        )
        .unwrap();
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();

        // Sanity: both meetings match before any exclusion, so the assertions
        // below are about exclusion and not about the fixture failing to
        // produce a hit in the first place.
        assert_eq!(projection.search_hits("shared secret").unwrap().len(), 2);
        assert_eq!(projection.search_hits("withheld secret").unwrap().len(), 1);
        assert_eq!(projection.search_hits("Shared Title").unwrap().len(), 1);

        let excluded = HashSet::from(["meeting-a".to_owned()]);

        let transcript = projection
            .search_filtered_excluding(
                "shared secret",
                &LibraryFilter::default(),
                &excluded,
            )
            .unwrap();
        assert_eq!(
            transcript.total, 1,
            "an excluded meeting's transcript span still counted"
        );
        for hit in &transcript.hits {
            assert!(matches!(
                projection.open(&fixture.storage, hit).unwrap(),
                OpenedLibraryHit::Transcript { ref meeting_id, .. } if meeting_id == "meeting-b"
            ));
        }

        let withheld = projection
            .search_filtered_excluding(
                "withheld secret",
                &LibraryFilter::default(),
                &excluded,
            )
            .unwrap();
        assert_eq!(
            withheld.total, 0,
            "an excluded meeting's withheld-turn marker still surfaced"
        );

        let titled = projection
            .search_filtered_excluding("Shared Title", &LibraryFilter::default(), &excluded)
            .unwrap();
        assert_eq!(
            titled.total, 0,
            "an excluded meeting's title match still surfaced"
        );

        // Lifting the exclusion restores every one of them -- unlocking
        // re-admits on the next search, with no separate re-admission path.
        let restored = projection
            .search_filtered_excluding(
                "shared secret",
                &LibraryFilter::default(),
                &HashSet::new(),
            )
            .unwrap();
        assert_eq!(restored.total, 2);
    }

    /// Same requirement, for a claim hit rather than a transcript/title one --
    /// a locked meeting's generated note content must be just as unreachable
    /// through search as its raw transcript.
    #[test]
    fn search_filtered_excluding_removes_claim_hits_for_an_excluded_meeting() {
        let fixture = Fixture::new();
        fixture.ready_meeting("meeting-a", 10);
        fixture.ready_meeting("meeting-b", 20);
        let projector = Arc::new(FixtureProjector::new(ProjectorMode::Success));
        let projection = LibraryProjection::rebuild_with_projector(
            &fixture.storage,
            ReadLimits::default(),
            projector,
        )
        .unwrap();

        let unfiltered = projection.search_hits("claim-a").unwrap();
        assert!(
            unfiltered.len() >= 2,
            "sanity: both meetings should produce a claim-a hit"
        );

        let excluded = HashSet::from(["meeting-a".to_owned()]);
        let found = projection
            .search_filtered_excluding("claim-a", &LibraryFilter::default(), &excluded)
            .unwrap();
        assert!(!found.hits.is_empty(), "meeting-b's own claim hit vanished too");
        for hit in &found.hits {
            assert!(matches!(
                projection.open(&fixture.storage, hit).unwrap(),
                OpenedLibraryHit::Claim { ref meeting_id, .. } if meeting_id == "meeting-b"
            ));
        }
    }

    /// The semantic-retrieval probe measures against exact search, so exact
    /// search's actual behaviour on these fixtures is asserted here rather than
    /// written down in the fixture file from memory.
    ///
    /// `exact_helps` is the load-bearing field: it says whether today's search
    /// finds the meeting a person wanted, given the one word they would type.
    /// Five questions where it does and five where it does not — a suite of only
    /// the second kind would reward any non-empty retrieval at all.
    #[test]
    fn the_semantic_probe_fixtures_describe_what_exact_search_actually_does() {
        let document: serde_json::Value = serde_json::from_str(include_str!(
            "../../../notes/semantic_retrieval_fixtures.json"
        ))
        .expect("fixtures parse");
        assert_eq!(document["schema"], "semantic-retrieval-fixtures/1");

        let fixture = Fixture::new();
        let meetings = document["meetings"].as_array().expect("meetings");
        assert!(
            meetings.len() >= 8,
            "too few meetings to make top-1 meaningful"
        );
        for meeting in meetings {
            let turns: Vec<(&str, bool)> = meeting["turns"]
                .as_array()
                .expect("turns")
                .iter()
                .map(|turn| (turn.as_str().expect("turn text"), false))
                .collect();
            fixture.meeting(
                meeting["id"].as_str().expect("id"),
                meeting["created_at_epoch_seconds"]
                    .as_u64()
                    .expect("created"),
                &turns,
            );
        }
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.rows().len(), meetings.len());

        let questions = document["questions"].as_array().expect("questions");
        let mut helped = 0;
        for question in questions {
            let keyword = question["keyword"].as_str().expect("keyword");
            let intended = question["intended_meeting"].as_str().expect("intended");
            let claims_help = question["exact_helps"].as_bool().expect("exact_helps");

            let hits = projection.search_hits(keyword).unwrap();
            let found: BTreeSet<String> = hits
                .iter()
                .map(
                    |hit| match projection.open(&fixture.storage, hit).unwrap() {
                        OpenedLibraryHit::Claim { meeting_id, .. }
                        | OpenedLibraryHit::Transcript { meeting_id, .. }
                        | OpenedLibraryHit::Withheld { meeting_id, .. }
                        | OpenedLibraryHit::Meeting { meeting_id, .. } => meeting_id,
                    },
                )
                .collect();
            assert_eq!(
                found.contains(intended),
                claims_help,
                "{keyword:?}: exact_helps is {claims_help} and exact search {} the intended meeting",
                if found.contains(intended) {
                    "found"
                } else {
                    "missed"
                }
            );
            helped += usize::from(claims_help);
        }
        assert_eq!(
            (helped, questions.len() - helped),
            (5, 5),
            "the suite must be balanced between questions exact search answers and questions it cannot"
        );
    }

    #[test]
    fn every_safe_metadata_only_lifecycle_is_projected_without_transcript_authority() {
        let fixture = Fixture::new();
        let cases = [
            ("meeting-a", MeetingLifecycle::Incomplete),
            ("meeting-b", MeetingLifecycle::Captured),
            ("meeting-c", MeetingLifecycle::TranscriptionFailed),
            ("meeting-d", MeetingLifecycle::RecoveredInterrupted),
        ];
        for (offset, (id, lifecycle)) in cases.iter().enumerate() {
            fixture.metadata_only_meeting(id, 10 + offset as u64, *lifecycle);
        }
        fixture.meeting("meeting-z", 20, &[("transcript token", false)]);
        fixture.metadata(br#"{"schema":"library-metadata/1","revision":1,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"Project Atlas"}],"meetings":[{"meeting_id":"meeting-a","title":"Organizer a","folder_id":"11111111-1111-4111-8111-111111111111"},{"meeting_id":"meeting-b","title":"Organizer b","folder_id":null},{"meeting_id":"meeting-c","title":"Organizer c","folder_id":null},{"meeting_id":"meeting-d","title":"Organizer d","folder_id":null}]}"#);

        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.rows().len(), 5);
        assert_eq!(projection.search_hits("organizer").unwrap().len(), 4);
        let folder = projection.search_hits("Atlas").unwrap();
        assert_eq!(folder.len(), 1);
        assert!(matches!(
            projection.open(&fixture.storage, &folder[0]).unwrap(),
            OpenedLibraryHit::Meeting { ref meeting_id, ref folder, .. }
                if meeting_id == "meeting-a" && folder.as_deref() == Some("Project Atlas")
        ));
        assert_eq!(projection.search_hits("transcript").unwrap().len(), 1);
        for (id, lifecycle) in cases {
            let row = projection
                .rows()
                .iter()
                .find(|row| row.meeting_id == id)
                .unwrap();
            assert_eq!(row.lifecycle, lifecycle);
            assert_eq!(row.transcript_sha256, None);
            assert!(row.turns.is_empty());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn metadata_fifo_never_blocks_the_projection() {
        use std::ffi::CString;
        use std::time::{Duration, Instant};
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
        let library = fixture.storage.path().join("library");
        create_private_dir(&library).unwrap();
        let fifo =
            CString::new(library.join("metadata.json").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let started = Instant::now();
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(projection.search_hits("transcript").unwrap().len(), 1);
        assert!(projection.search_hits("metadata").unwrap().is_empty());
    }

    #[test]
    fn malformed_metadata_loses_only_label_authority() {
        let cases: &[&[u8]] = &[
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[],"extra":true}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"name":"x","id":"11111111-1111-4111-8111-111111111111"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"e\u0301"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"a"},{"id":"00000000-0000-4000-8000-000000000000","name":"b"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"a"},{"id":"11111111-1111-4111-8111-111111111111","name":"b"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-4111-111111111111","name":"a"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-11111111111A","name":"a"}],"meetings":[]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"a"}],"meetings":[{"meeting_id":"meeting-a","title":null,"folder_id":"11111111-1111-4111-8111-111111111111"},{"meeting_id":"meeting-a","title":null,"folder_id":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"a"}],"meetings":[{"meeting_id":"meeting-a","title":null,"folder_id":"11111111-1111-4111-8111-111111111111"},{"meeting_id":"meeting-a","title":null,"folder_id":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[{"id":"11111111-1111-4111-8111-111111111111","name":"a"}],"meetings":[{"meeting_id":"meeting-a","title":null,"folder_id":"22222222-2222-4222-8222-222222222222"}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"unknown","title":"x","folder_id":null}]}"#,
            br#"{"schema":"library-metadata/1","revision":0,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"x/y","folder_id":null}]}"#,
        ];
        for bytes in cases {
            let fixture = Fixture::new();
            fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
            fixture.metadata(bytes);
            let projection =
                LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
            assert!(projection.search_hits("xx").unwrap().is_empty());
            assert_eq!(projection.search_hits("transcript").unwrap().len(), 1);
        }
    }

    #[test]
    fn every_metadata_state_change_stales_prior_hits() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
        let missing = LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let missing_hit = missing.search_hits("transcript").unwrap().remove(0);
        fixture.metadata(br#"{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"first","folder_id":null}]}"#);
        assert_stale!(missing.open(&fixture.storage, &missing_hit));

        let variants: &[Option<&[u8]>] = &[
            Some(br#"{ "schema" : "library-metadata/1", "revision" : 1, "folders" : [], "meetings" : [{ "meeting_id" : "meeting-a", "title" : "first", "folder_id" : null }] }"#),
            Some(br#"{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"other","folder_id":null}]}"#),
            Some(br#"{"schema":"library-metadata/1","revision":2,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"first","folder_id":null}]}"#),
            None,
        ];
        for replacement in variants {
            let fixture = Fixture::new();
            fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
            fixture.metadata(br#"{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"first","folder_id":null}]}"#);
            let projection =
                LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
            let hit = projection.search_hits("first").unwrap().remove(0);
            let path = fixture.storage.path().join("library/metadata.json");
            match replacement {
                Some(bytes) => {
                    fs::remove_file(&path).unwrap();
                    durable_create_new(&path, bytes).unwrap();
                }
                None => fs::remove_file(&path).unwrap(),
            }
            assert_stale!(projection.open(&fixture.storage, &hit));
        }
    }

    #[test]
    fn unsafe_metadata_nodes_fail_closed_without_losing_transcript_rows() {
        for fault in ["symlink", "hard-link", "mode", "oversize"] {
            let fixture = Fixture::new();
            fixture.meeting("meeting-a", 10, &[("transcript token", false)]);
            let path = fixture.metadata(br#"{"schema":"library-metadata/1","revision":1,"folders":[],"meetings":[{"meeting_id":"meeting-a","title":"private title","folder_id":null}]}"#);
            match fault {
                "symlink" => {
                    let target = fixture.storage.path().join("library/target.json");
                    fs::rename(&path, &target).unwrap();
                    std::os::unix::fs::symlink("target.json", &path).unwrap();
                }
                "hard-link" => {
                    fs::hard_link(&path, fixture.storage.path().join("library/other.json"))
                        .unwrap();
                }
                "mode" => {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
                }
                "oversize" => {
                    fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
                }
                _ => unreachable!(),
            }
            let projection =
                LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
            assert!(
                projection.search_hits("private").unwrap().is_empty(),
                "{fault}"
            );
            assert_eq!(
                projection.search_hits("transcript").unwrap().len(),
                1,
                "{fault}"
            );
        }
    }

    #[test]
    fn normalized_overlapping_search_has_stable_natural_keys() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("e\u{301}chooooo", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let hits = projection.search_hits(" ÉCHO ").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(matches!(
            projection.sealed(&hits[0]),
            Some(SealedHit::Transcript {
                original_scalar_start: 0,
                original_scalar_end: 5,
                ..
            })
        ));
        assert_eq!(
            hit_key(&projection.sealed(&hits[0]).unwrap()),
            hit_key(
                &projection
                    .sealed(&projection.search_hits("écho").unwrap()[0])
                    .unwrap()
            )
        );
        assert_eq!(normalized_matches("oooo", "oo").len(), 3);
    }

    #[test]
    fn withheld_opens_without_text_and_retained_opens_with_original_span() {
        let fixture = Fixture::new();
        fixture.meeting(
            "meeting-a",
            10,
            &[("visible token", false), ("hidden token", true)],
        );
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let retained = projection.search_hits("visible").unwrap().remove(0);
        assert!(matches!(
            projection.open(&fixture.storage, &retained).unwrap(),
            OpenedLibraryHit::Transcript {
                original_scalar_start: 0,
                original_scalar_end: 7,
                ..
            }
        ));
        let withheld = projection.search_hits("hidden").unwrap().remove(0);
        assert!(matches!(
            projection.open(&fixture.storage, &withheld).unwrap(),
            OpenedLibraryHit::Withheld { .. }
        ));
    }

    #[test]
    fn restored_view_resolves_instead_of_quarantining_and_frees_the_turn() {
        let fixture = Fixture::new();
        let meeting_id = "22222222-2222-4222-8222-222222222222";
        let directory = fixture.meeting(
            meeting_id,
            10,
            &[
                ("visible token", false),
                ("freed token", true),
                ("hidden token", true),
            ],
        );
        let mut record = load_meeting(&directory).unwrap();
        let base = record.artifacts.current_transcript.clone().unwrap();
        let view = crate::operations::TranscriptView {
            schema: crate::operations::TranscriptViewSchema::V1,
            meeting_id: Uuid::parse_str(meeting_id).unwrap(),
            base_transcript_sha256: base.sha256.clone(),
            parent_transcript_sha256: base.sha256.clone(),
            restored_source_turn_indices: vec![1],
        };
        let view_bytes = serde_json::to_vec_pretty(&view).unwrap();
        let view_digest = format!("{:x}", Sha256::digest(&view_bytes));
        let relative = format!("transcript/{view_digest}.json");
        durable_create_new(&directory.join(&relative), &view_bytes).unwrap();
        record.artifacts.current_transcript = Some(artifact_ref(&directory, &relative).unwrap());
        crate::meeting::write_meeting(&directory, &record).unwrap();

        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let freed = projection.search_hits("freed").unwrap().remove(0);
        assert!(matches!(
            projection.open(&fixture.storage, &freed).unwrap(),
            OpenedLibraryHit::Transcript { .. }
        ));
        let withheld = projection.search_hits("hidden").unwrap().remove(0);
        assert!(matches!(
            projection.open(&fixture.storage, &withheld).unwrap(),
            OpenedLibraryHit::Withheld { .. }
        ));
    }

    #[test]
    fn a_hostile_view_chain_quarantines_its_meeting_without_failing_the_library() {
        let fixture = Fixture::new();
        fixture.meeting(
            "33333333-3333-4333-8333-333333333333",
            20,
            &[("good token", false)],
        );
        let meeting_id = "44444444-4444-4444-8444-444444444444";
        let directory = fixture.meeting(
            meeting_id,
            10,
            &[("bad token", false), ("gated token", true)],
        );
        let mut record = load_meeting(&directory).unwrap();
        let base = record.artifacts.current_transcript.clone().unwrap();
        // The view's parent digest names an oversized file: the chain must
        // quarantine this one meeting, never abort the whole build.
        let big = vec![b'x'; (ReadLimits::default().max_transcript_bytes + 1) as usize];
        let big_digest = format!("{:x}", Sha256::digest(&big));
        durable_create_new(
            &directory.join(format!("transcript/{big_digest}.json")),
            &big,
        )
        .unwrap();
        let view = crate::operations::TranscriptView {
            schema: crate::operations::TranscriptViewSchema::V1,
            meeting_id: Uuid::parse_str(meeting_id).unwrap(),
            base_transcript_sha256: base.sha256.clone(),
            parent_transcript_sha256: big_digest,
            restored_source_turn_indices: vec![1],
        };
        let view_bytes = serde_json::to_vec_pretty(&view).unwrap();
        let view_digest = format!("{:x}", Sha256::digest(&view_bytes));
        let relative = format!("transcript/{view_digest}.json");
        durable_create_new(&directory.join(&relative), &view_bytes).unwrap();
        record.artifacts.current_transcript = Some(artifact_ref(&directory, &relative).unwrap());
        crate::meeting::write_meeting(&directory, &record).unwrap();

        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.search_hits("good").unwrap().len(), 1);
        assert!(projection.search_hits("bad").unwrap().is_empty());
    }

    #[test]
    fn channel_attribution_withholds_only_microphone_turns() {
        let fixture = Fixture::new();
        let directory = fixture.meeting("meeting-a", 10, &[("hidden token", true)]);
        let mut record = load_meeting(&directory).unwrap();
        let transcript = serde_json::to_vec_pretty(&json!({
            "schema": "capture-transcript/1",
            "source": "synthetic",
            "attribution": "channel",
            "bleed": null,
            "voiceprint": null,
            "capture_health": {},
            "turns": [{
                "start": 0.0,
                "end": 0.5,
                "speaker": "Them",
                "text": "hidden token",
                "gated": true,
            }],
        }))
        .unwrap();
        let digest = format!("{:x}", Sha256::digest(&transcript));
        let relative = format!("transcript/{digest}.json");
        durable_create_new(&directory.join(&relative), &transcript).unwrap();
        record.artifacts.current_transcript = Some(artifact_ref(&directory, &relative).unwrap());
        write_meeting(&directory, &record).unwrap();

        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert!(projection.rows().is_empty());
        assert_eq!(projection.quarantined_meetings(), 1);
    }

    #[test]
    fn open_fails_closed_when_any_bound_artifact_drifts() {
        for target in [
            "meeting",
            "attempt",
            "ownership",
            "capture",
            "deletion",
            "transcript",
        ] {
            let fixture = Fixture::new();
            let directory = fixture.meeting("meeting-a", 10, &[("stable token", false)]);
            let projection =
                LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
            let hit = projection.search_hits("stable").unwrap().remove(0);
            match target {
                "meeting" => fs::write(directory.join("meeting.json"), b"{}").unwrap(),
                "attempt" => fs::write(directory.join("attempt.json"), b"{}").unwrap(),
                "ownership" => fs::write(directory.join("ownership.json"), b"{}").unwrap(),
                "capture" => fs::write(directory.join("capture/session.json"), b"{}").unwrap(),
                "deletion" => {
                    fs::write(directory.join("deletion/audio-deletion.json"), b"{}").unwrap()
                }
                "transcript" => {
                    let current = load_meeting(&directory)
                        .unwrap()
                        .artifacts
                        .current_transcript
                        .unwrap();
                    fs::write(directory.join(current.relative_path), b"changed").unwrap();
                }
                _ => unreachable!(),
            }
            assert_stale!(projection.open(&fixture.storage, &hit));
        }
    }

    #[test]
    fn handles_are_projection_owned_and_unknown_handles_are_stale() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &[("stable token", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let handle = projection.search_hits("stable").unwrap().remove(0);
        let forged = LibraryHit {
            projection_id: projection.snapshot_id,
            key: "forged".into(),
        };
        assert_stale!(projection.open(&fixture.storage, &forged));
        let other = LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_stale!(projection.open(&fixture.storage, &other.search_hits("stable").unwrap()[0]));
        assert!(projection.open(&fixture.storage, &handle).is_ok());
    }

    #[test]
    fn lifecycle_and_current_transcript_pointer_drift_are_stale() {
        let fixture = Fixture::new();
        let directory = fixture.meeting("meeting-a", 10, &[("stable token", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let handle = projection.search_hits("stable").unwrap().remove(0);
        let mut record = load_meeting(&directory).unwrap();
        record.lifecycle = MeetingLifecycle::SummaryFailed;
        write_meeting(&directory, &record).unwrap();
        assert_stale!(projection.open(&fixture.storage, &handle));

        let fixture = Fixture::new();
        let directory = fixture.meeting("meeting-a", 10, &[("stable token", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let handle = projection.search_hits("stable").unwrap().remove(0);
        let mut record = load_meeting(&directory).unwrap();
        durable_create_new(&directory.join("capture/mic.wav"), b"mic").unwrap();
        durable_create_new(&directory.join("capture/system.wav"), b"system").unwrap();
        record.artifacts.microphone_audio =
            Some(artifact_ref(&directory, "capture/mic.wav").unwrap());
        record.artifacts.system_audio =
            Some(artifact_ref(&directory, "capture/system.wav").unwrap());
        record.retention.state = AudioState::Retained;
        record.retention.deletion_receipt = None;
        write_meeting(&directory, &record).unwrap();
        assert_stale!(projection.open(&fixture.storage, &handle));

        let fixture = Fixture::new();
        let directory = fixture.meeting("meeting-a", 10, &[("stable token", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let handle = projection.search_hits("stable").unwrap().remove(0);
        let mut record = load_meeting(&directory).unwrap();
        let original = record.artifacts.current_transcript.clone().unwrap();
        let bytes = fs::read(directory.join(&original.relative_path)).unwrap();
        let alternate = format!("{}\n", String::from_utf8(bytes).unwrap());
        let digest = format!("{:x}", Sha256::digest(alternate.as_bytes()));
        let relative = format!("transcript/{digest}.json");
        durable_create_new(&directory.join(&relative), alternate.as_bytes()).unwrap();
        record.artifacts.current_transcript = Some(artifact_ref(&directory, &relative).unwrap());
        write_meeting(&directory, &record).unwrap();
        assert_stale!(projection.open(&fixture.storage, &handle));
    }

    #[test]
    fn transcript_search_and_limits_refuse_whole_build() {
        let fixture = Fixture::new();
        let directory = fixture.meeting("meeting-a", 10, &[("token", false)]);
        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert_eq!(projection.search_hits("token").unwrap().len(), 1);
        assert_eq!(
            projection.search_hits("t"),
            Err(LibraryReadError::InvalidRequest)
        );
        assert!(matches!(
            LibraryProjection::rebuild(
                &fixture.storage,
                ReadLimits {
                    max_total_bytes: 1,
                    ..ReadLimits::default()
                }
            ),
            Err(LibraryReadError::CapacityExceeded)
        ));
        assert!(matches!(
            LibraryProjection::rebuild(
                &fixture.storage,
                ReadLimits {
                    max_transcript_bytes: 1,
                    ..ReadLimits::default()
                }
            ),
            Err(LibraryReadError::CapacityExceeded)
        ));
        let meeting_bytes = fs::metadata(directory.join("meeting.json")).unwrap().len();
        let deletion_bytes = fs::metadata(directory.join("deletion/audio-deletion.json"))
            .unwrap()
            .len();
        assert!(matches!(
            LibraryProjection::rebuild(
                &fixture.storage,
                ReadLimits {
                    max_total_bytes: meeting_bytes + deletion_bytes,
                    ..ReadLimits::default()
                }
            ),
            Err(LibraryReadError::CapacityExceeded)
        ));
        for (input, expected, origins) in NORMALIZATION_FIXTURES {
            let actual = normalize(input);
            assert_eq!(actual.0, *expected);
            assert_eq!(actual.1, *origins);
        }
        assert_eq!(
            normalized_matches("aaaa", "aa"),
            vec![(0, 2), (1, 3), (2, 4)]
        );
        assert!(normalize_query(&"a".repeat(256)).is_ok());
        assert_eq!(
            normalize_query(&"a".repeat(257)),
            Err(LibraryReadError::InvalidRequest)
        );
        assert_eq!(
            normalize_query(" \n token"),
            Err(LibraryReadError::InvalidRequest)
        );

        let fixture = Fixture::new();
        for ordinal in 0..=MAX_SEARCH_RESULTS {
            fixture.meeting(
                &format!("meeting-{ordinal:03}"),
                ordinal as u64,
                &[("common phrase", false)],
            );
        }
        let projection = LibraryProjection::rebuild(
            &fixture.storage,
            ReadLimits {
                max_meetings: MAX_SEARCH_RESULTS + 1,
                ..ReadLimits::default()
            },
        )
        .unwrap();
        // Until 2026-08-08 this asserted `Err(CapacityExceeded)` — a query with
        // more than a hundred matches refused entirely, and the shell showed
        // "That search has too many matches" with no results. Measured the same
        // day: five synthetic meetings already cross the cap on a common word,
        // so that refusal was reachable in a first week rather than at scale.
        //
        // It follows the constant rather than a literal hundred, because it is a
        // test of live behaviour and not of a frozen artifact.
        let found = projection
            .search("common")
            .expect("a broad query is answered");
        assert_eq!(found.hits.len(), MAX_SEARCH_RESULTS);
        assert_eq!(found.total, MAX_SEARCH_RESULTS + 1);
        // Recency first, so the page a person sees is the newest matches rather
        // than an arbitrary hundred of them.
        assert!(
            found.hits[0]
                .key
                .contains(&format!("meeting-{MAX_SEARCH_RESULTS:03}")),
            "the cut page did not start at the most recent match: {}",
            found.hits[0].key
        );
        // One handle per returned hit, not per match. The cap's job was always
        // to bound retained snapshot state, and truncating bounds it exactly as
        // refusing did.
        assert_eq!(projection.hits.borrow().len(), MAX_SEARCH_RESULTS);
    }

    /// Filter first, then cut — and the order is only visible when the filter
    /// selects meetings the global page would not contain.
    ///
    /// A hundred and twenty meetings, all matching the word, ordered newest
    /// first. The filter admits the twenty *oldest*. Cutting to a hundred before
    /// filtering leaves nothing of them, so a folder or date search would report
    /// no matches while twenty exist. The old code could not have this bug
    /// because past the cap it refused the whole query instead of answering it;
    /// truncation is what makes the order matter.
    #[test]
    fn a_filtered_search_pages_within_the_filter_and_not_across_it() {
        let fixture = Fixture::new();
        for ordinal in 0..120_u64 {
            fixture.meeting(
                &format!("meeting-{ordinal:03}"),
                100 + ordinal,
                &[("common phrase", false)],
            );
        }
        let projection = LibraryProjection::rebuild(
            &fixture.storage,
            ReadLimits {
                max_meetings: 200,
                ..ReadLimits::default()
            },
        )
        .unwrap();

        let oldest_twenty = LibraryFilter {
            end_epoch_seconds: Some(119),
            ..LibraryFilter::default()
        };
        let found = projection
            .search_filtered("common", &oldest_twenty)
            .expect("a filtered query is answered");
        assert_eq!(
            found.total, 20,
            "the total counted matches outside the filter"
        );
        assert_eq!(found.hits.len(), 20);
        assert_eq!(projection.hits.borrow().len(), 20);
    }

    #[test]
    fn pinned_rust_compiler_matches_normalization_contract() {
        let output = std::process::Command::new("rustc")
            .arg("-Vv")
            .output()
            .expect("rustc must be on PATH for Rust tests");
        let version = String::from_utf8(output.stdout).expect("rustc version is UTF-8");
        assert!(output.status.success());
        assert!(version.contains("release: 1.94.0"));
        assert!(version.contains(SEARCH_NORMALIZATION_RUST_COMMIT));
    }

    fn tree_digest(path: &Path) -> Vec<(String, String)> {
        fn walk(root: &Path, path: &Path, output: &mut Vec<(String, String)>) {
            let metadata = fs::symlink_metadata(path).unwrap();
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            if metadata.is_dir() {
                output.push((
                    format!("d:{relative}"),
                    format!("{:o}", metadata.permissions().mode() & 0o777),
                ));
                let mut children: Vec<_> =
                    fs::read_dir(path).unwrap().map(Result::unwrap).collect();
                children.sort_by_key(|entry| entry.file_name());
                for child in children {
                    walk(root, &child.path(), output);
                }
            } else {
                output.push((
                    format!("f:{relative}"),
                    digest_bytes(&fs::read(path).unwrap()),
                ));
            }
        }
        use std::os::unix::fs::PermissionsExt;
        let mut output = Vec::new();
        walk(path, path, &mut output);
        output
    }

    type NormalizationFixture = (&'static str, &'static str, &'static [(u64, u64)]);
    const NORMALIZATION_FIXTURES: &[NormalizationFixture] = &[
        ("é", "é", &[(0, 1)]),
        ("e\u{301}", "é", &[(0, 2)]),
        ("İ", "i\u{307}", &[(0, 1), (0, 1)]),
        ("👩‍💻", "👩‍💻", &[(0, 3), (0, 3), (0, 3)]),
    ];
}
