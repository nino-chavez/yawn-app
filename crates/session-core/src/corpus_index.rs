//! A rebuildable SQLite index over the canonical meeting corpus.
//!
//! # What this is not
//!
//! It is not an authority. Every row here is derived from files that
//! [`crate::library_read`] already validated against their content addresses,
//! and it is reachable only through [`LibraryRow::derived`], which hands out
//! exactly the fields a cache may hold. Deleting this database costs time and
//! never information: [`CorpusIndex::replace_from_projection`] rebuilds it from
//! the files, and `rebuild_from_files_equals_the_live_index` pins that equality.
//! A column with no canonical file behind it fails that test, which is the
//! point of it.
//!
//! **One table is exempt and the exemption is the interesting part.**
//! `corpus_window_vector` holds numbers no file determines: they are a function of the
//! files *and a model*, so rebuilding from files alone cannot reproduce them and
//! [`CorpusIndex::fingerprint`] deliberately does not hash them. That is the
//! only sense in which deleting this file is not free — every vector would have
//! to be recomputed, which is minutes of work rather than a lost meeting. The
//! windows those vectors hang from *are* derived, are hashed, and do rebuild.
//!
//! The binding that keeps this safe is `corpus_window.text_sha256`. A vector records
//! the digest it was computed from; a sync that re-derives a window with
//! different text leaves the old vector matching nothing, and the prune inside
//! `replace_from_projection` removes it. A stale vector is worse than no vector,
//! because it answers with a meeting that no longer says what it said.
//!
//! This is a rebuildable cache used only after measured library size makes the
//! scan a problem. No derived
//! index may become the sole copy of a meeting, transcript, note, locator, or
//! library label". Both clauses survive. The measurement that fired the first
//! one is `src/bin/corpus-scan-bench.rs`.
//!
//! # Why it exists
//!
//! The scan answers `rows()` and one exact query. It cannot answer the
//! questions the corpus features need — meetings in a folder, meetings in a
//! date range, how many matches exist when there are more than a screenful —
//! because it holds no queryable structure and refuses any query matching more
//! than `MAX_SEARCH_RESULTS` spans. A single 200-turn meeting already crosses
//! that for a common word.
//!
//! # Private material
//!
//! This database holds transcript text. It lives inside the 0700 storage root
//! at `library/corpus.sqlite3`, mode 0600, and never enters Git. `secure_delete`
//! is on so a removed meeting's pages are overwritten rather than left legible
//! in free space, which matches how audio is treated everywhere else here.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::corpus_window::{self, CorpusWindow, EmbedderIdentity, WindowSegment};
use crate::library_read::{DerivedRow, LibraryProjection};
use crate::meeting::MeetingLifecycle;
use crate::storage::{StorageRoot, create_private_dir};

/// Bumped only when a migration changes what a row means. The identity row
/// records the version that wrote the file, so an older binary meeting a newer
/// file refuses instead of misreading it.
pub const CORPUS_INDEX_SCHEMA: &str = "corpus-index/2";
const APPLIED_MIGRATION: i64 = 2;
const DATABASE_NAME: &str = "corpus.sqlite3";

#[derive(Debug, Error)]
pub enum CorpusIndexError {
    #[error("corpus index storage is unavailable")]
    StorageUnavailable,
    #[error("corpus index file is not private")]
    NotPrivate,
    #[error("corpus index schema is newer than this build")]
    SchemaAhead,
    #[error("corpus index rejected a row it cannot have derived")]
    UnderivableRow,
    #[error("corpus index refused a vector it cannot bind to a window")]
    VectorRejected,
    #[error("corpus index database error")]
    Database,
}

impl From<rusqlite::Error> for CorpusIndexError {
    fn from(_: rusqlite::Error) -> Self {
        // Deliberately content-free. A SQLite error string can carry a column
        // value, and the values here are transcript text.
        Self::Database
    }
}

/// What a sync changed. Counts only; never a meeting ID, never text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncOutcome {
    pub meetings: usize,
    pub turns: usize,
    pub windows: usize,
}

/// A filter over the corpus. Every field is optional and they conjoin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListRequest {
    pub folder: Option<String>,
    pub created_at_or_after: Option<u64>,
    pub created_at_or_before: Option<u64>,
    pub lifecycles: Vec<MeetingLifecycle>,
    /// Zero means "no cap"; a caller that wants a page states its size.
    pub limit: usize,
    pub offset: usize,
}

/// A page of results that states its own truncation.
///
/// `total` is the count matching the filter, `rows` is what this page carries.
/// The scan refuses rather than return a prefix; a store can page, but only if
/// the caller can tell a page from the whole answer. Two fields, not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    pub total: usize,
    pub rows: Vec<IndexedMeeting>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedMeeting {
    pub meeting_id: String,
    pub created_at_epoch_seconds: u64,
    pub lifecycle: MeetingLifecycle,
    pub meeting_record_sha256: String,
    pub attempt_sha256: String,
    pub transcript_sha256: Option<String>,
    pub transcript_relative_path: Option<String>,
    pub note_json_sha256: Option<String>,
    pub note_markdown_sha256: Option<String>,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub turn_count: usize,
}

pub struct CorpusIndex {
    connection: Connection,
    path: PathBuf,
}

impl CorpusIndex {
    /// Opens, creating the file if absent, after checking by hand what SQLite
    /// will not check for us.
    ///
    /// `rusqlite` opens a path; none of `storage.rs`'s descriptor-bound
    /// guards run on that path. So the parent directory's mode and the file's
    /// mode, type and link state are verified here, before the path is handed
    /// over, and the mode is re-asserted after creation.
    pub fn open(storage: &StorageRoot) -> Result<Self, CorpusIndexError> {
        let library = storage
            .resolve(Path::new("library"))
            .map_err(|_| CorpusIndexError::StorageUnavailable)?;
        create_private_dir(&library).map_err(|_| CorpusIndexError::StorageUnavailable)?;
        let path = library.join(DATABASE_NAME);
        Self::open_at(&path)
    }

    fn open_at(path: &Path) -> Result<Self, CorpusIndexError> {
        require_private_parent(path)?;
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            require_private_file(&metadata)?;
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // A fresh database is created with the process umask, which is not
        // ours to assume. Narrow it before anything is written into it.
        set_private_mode(path)?;

        // DELETE, not WAL: WAL's `-wal` and `-shm` sidecars would hold
        // transcript bytes under permissions SQLite chooses, and nothing here
        // needs a second concurrent reader yet. `secure_delete` overwrites
        // freed pages so a deleted meeting does not stay legible.
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "secure_delete", "ON")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        // `journal_mode` is the pragma where a set can silently not take, and a
        // clean close removes `-wal`/`-shm`, so no after-the-fact file listing
        // would notice WAL. Read it back and refuse.
        let journal: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !journal.eq_ignore_ascii_case("delete") {
            return Err(CorpusIndexError::NotPrivate);
        }

        let index = Self {
            connection,
            path: path.to_path_buf(),
        };
        index.migrate()?;
        Ok(index)
    }

    fn migrate(&self) -> Result<(), CorpusIndexError> {
        let applied: Option<i64> = self
            .connection
            .query_row(
                "SELECT applied_migration FROM index_identity WHERE id = 0",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        match applied {
            Some(version) if version == APPLIED_MIGRATION => return Ok(()),
            // Forward-only. An older binary must not read a file a newer one
            // wrote, because it would read columns whose meaning changed.
            Some(version) if version > APPLIED_MIGRATION => {
                return Err(CorpusIndexError::SchemaAhead);
            }
            // No stepwise migration exists, so the honest upgrade from any
            // earlier version is to drop and re-derive. The files are the
            // authority for everything except `window_vector`, which is
            // discarded here and recomputed — stated rather than implied,
            // because an upgrade silently throwing away every embedding is a
            // cost worth being able to predict.
            Some(_) => {
                self.connection.execute_batch(
                    "DROP TABLE IF EXISTS corpus_window_vector;
                     DROP TABLE IF EXISTS corpus_window_segment;
                     DROP TABLE IF EXISTS corpus_window;
                     DROP TABLE IF EXISTS turn;
                     DROP TABLE IF EXISTS meeting;
                     DROP TABLE IF EXISTS index_identity;",
                )?;
            }
            None => {}
        }
        self.connection.execute_batch(SCHEMA_2)?;
        self.connection.execute(
            "INSERT OR REPLACE INTO index_identity (id, schema, sqlite_version, applied_migration)
             VALUES (0, ?1, ?2, ?3)",
            params![CORPUS_INDEX_SCHEMA, rusqlite::version(), APPLIED_MIGRATION],
        )?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The SQLite build that wrote this file, recorded beside the schema so the
    /// index carries its own provenance rather than inheriting the caller's.
    pub fn identity(&self) -> Result<(String, String), CorpusIndexError> {
        let row = self.connection.query_row(
            "SELECT schema, sqlite_version FROM index_identity WHERE id = 0",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        Ok(row)
    }

    /// Replaces the whole index from one validated projection, in a single
    /// transaction. A failure leaves the previous contents intact.
    ///
    /// Full replace, not incremental: skipping unchanged meetings requires a
    /// per-meeting entry point into the validator, which is a change to an
    /// audited module and belongs in its own step. The scan still runs; what
    /// this buys is everything the scan cannot answer afterwards.
    pub fn replace_from_projection(
        &mut self,
        projection: &LibraryProjection,
    ) -> Result<SyncOutcome, CorpusIndexError> {
        self.replace_from_projection_excluding(projection, &HashSet::new())
    }

    /// Same as [`Self::replace_from_projection`], but a meeting named in
    /// `excluded_meeting_ids` is written nowhere in this index -- no
    /// `meeting` row, no `turn`, no `corpus_window`, no `corpus_window_segment`
    /// -- as if its directory were not present under `meetings/` at all.
    ///
    /// This is the roadmap-I5 corpus-exclusion hook: a locked meeting's caller
    /// (`sync_corpus_index` in the desktop crate, which is the layer that can
    /// read `meeting_lock`) passes its id here. Because the whole table set is
    /// deleted and re-inserted every call, excluding a meeting that was
    /// indexed on a previous sync also *removes* its existing rows -- there is
    /// no separate "remove" path to keep in sync with this one. The stale
    /// vector prune below runs unchanged: a locked meeting's windows disappear
    /// from `corpus_window`, so its vectors fail the `NOT EXISTS` check and are
    /// pruned with everyone else's.
    pub fn replace_from_projection_excluding(
        &mut self,
        projection: &LibraryProjection,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<SyncOutcome, CorpusIndexError> {
        let transaction = self.connection.transaction()?;
        // Explicit, in dependency order, rather than leaning on `ON DELETE
        // CASCADE`: the cascade only fires while `PRAGMA foreign_keys` is on,
        // and a table that quietly kept its rows because a pragma did not take
        // is the failure this is avoiding. `corpus_window_vector` is not
        // cleared — it survives a resync and is pruned below against the
        // windows this call rebuilds.
        transaction.execute("DELETE FROM corpus_window_segment", [])?;
        transaction.execute("DELETE FROM corpus_window", [])?;
        transaction.execute("DELETE FROM turn", [])?;
        transaction.execute("DELETE FROM meeting", [])?;
        let mut meetings = 0;
        let mut turns = 0;
        let mut windows = 0;
        {
            let mut insert_meeting = transaction.prepare(
                "INSERT INTO meeting (
                    meeting_id, created_at_epoch_seconds, lifecycle,
                    meeting_record_sha256, attempt_sha256,
                    transcript_sha256, transcript_relative_path,
                    note_json_sha256, note_markdown_sha256, title, folder
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            let mut insert_turn = transaction.prepare(
                "INSERT INTO turn (meeting_id, turn_index, visible_index, gated, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut insert_window = transaction.prepare(
                "INSERT INTO corpus_window (meeting_id, window_index, word_count, text_sha256)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut insert_segment = transaction.prepare(
                "INSERT INTO corpus_window_segment (
                    meeting_id, window_index, segment_index,
                    source_turn_index, original_scalar_start, original_scalar_end
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for row in projection.rows() {
                let derived: DerivedRow<'_> = row.derived();
                if excluded_meeting_ids.contains(derived.meeting_id) {
                    // A locked meeting (or whatever else this caller excludes)
                    // leaves nothing behind for this call to remove later --
                    // its rows, and any vector standing on its old windows,
                    // are what the delete-then-reinsert above and the prune
                    // below take care of by simple absence.
                    continue;
                }
                insert_meeting.execute(params![
                    derived.meeting_id,
                    derived.created_at_epoch_seconds,
                    lifecycle_name(derived.lifecycle)?,
                    derived.meeting_record_sha256,
                    derived.attempt_sha256,
                    derived.transcript_sha256,
                    derived.transcript_relative_path,
                    derived.note_json_sha256,
                    derived.note_markdown_sha256,
                    derived.title,
                    derived.folder,
                ])?;
                meetings += 1;
                for turn in &derived.turns {
                    insert_turn.execute(params![
                        derived.meeting_id,
                        turn.index,
                        turn.visible_index,
                        turn.gated,
                        turn.text,
                    ])?;
                    turns += 1;
                }
                for window in corpus_window::windows(
                    derived
                        .turns
                        .iter()
                        .map(|turn| (u64::from(turn.index), turn.text, turn.gated)),
                ) {
                    // Reconstructed here rather than stored: the digest is what
                    // a vector binds to, and deriving it from the same segments
                    // a reader will use is what makes the binding mean
                    // something. A window that cannot be rebuilt from its own
                    // provenance is a bug, not a row to write.
                    let text = corpus_window::window_text(&window, |index| {
                        derived
                            .turns
                            .iter()
                            .find(|turn| u64::from(turn.index) == index)
                            .map(|turn| turn.text)
                    })
                    .ok_or(CorpusIndexError::UnderivableRow)?;
                    insert_window.execute(params![
                        derived.meeting_id,
                        window.window_index,
                        window.word_count,
                        corpus_window::window_digest(&text),
                    ])?;
                    for (position, segment) in window.segments.iter().enumerate() {
                        insert_segment.execute(params![
                            derived.meeting_id,
                            window.window_index,
                            position as u64,
                            segment.source_turn_index,
                            segment.original_scalar_start,
                            segment.original_scalar_end,
                        ])?;
                    }
                    windows += 1;
                }
            }
        }
        // Every vector whose window no longer exists, or whose window now says
        // something else, in one statement. It covers a removed meeting, an
        // edited transcript and a re-segmented meeting identically, because
        // they are the same question: is this vector still a description of
        // what is there?
        transaction.execute(
            "DELETE FROM corpus_window_vector WHERE NOT EXISTS (
                 SELECT 1 FROM corpus_window
                 WHERE corpus_window.meeting_id = corpus_window_vector.meeting_id
                   AND corpus_window.window_index = corpus_window_vector.window_index
                   AND corpus_window.text_sha256 = corpus_window_vector.text_sha256
             )",
            [],
        )?;
        // The recorded corpus digest describes contents this call just
        // replaced, so it stops being true here. `sync_if_changed` rewrites it
        // afterwards; a direct caller leaves it null and pays a full sync next
        // time, which is the safe direction to fail.
        transaction.execute(
            "UPDATE index_identity SET corpus_digest = NULL WHERE id = 0",
            [],
        )?;
        transaction.commit()?;
        Ok(SyncOutcome {
            meetings,
            turns,
            windows,
        })
    }

    pub fn meeting_count(&self) -> Result<usize, CorpusIndexError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM meeting", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Filtered, ordered, paged read. Ordering matches the scan's: newest
    /// first, then by meeting ID, so a caller swapping one for the other sees
    /// the same sequence.
    pub fn list(&self, request: &ListRequest) -> Result<ListPage, CorpusIndexError> {
        let mut clauses: Vec<String> = Vec::new();
        let mut arguments: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(folder) = &request.folder {
            clauses.push(format!("folder = ?{}", arguments.len() + 1));
            arguments.push(Box::new(folder.clone()));
        }
        if let Some(after) = request.created_at_or_after {
            clauses.push(format!(
                "created_at_epoch_seconds >= ?{}",
                arguments.len() + 1
            ));
            arguments.push(Box::new(after as i64));
        }
        if let Some(before) = request.created_at_or_before {
            clauses.push(format!(
                "created_at_epoch_seconds <= ?{}",
                arguments.len() + 1
            ));
            arguments.push(Box::new(before as i64));
        }
        if !request.lifecycles.is_empty() {
            let mut placeholders = Vec::new();
            for lifecycle in &request.lifecycles {
                placeholders.push(format!("?{}", arguments.len() + 1));
                arguments.push(Box::new(lifecycle_name(*lifecycle)?));
            }
            clauses.push(format!("lifecycle IN ({})", placeholders.join(", ")));
        }
        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        let bound: Vec<&dyn rusqlite::ToSql> =
            arguments.iter().map(|value| value.as_ref()).collect();

        let total: i64 = self.connection.query_row(
            &format!("SELECT COUNT(*) FROM meeting{where_clause}"),
            bound.as_slice(),
            |row| row.get(0),
        )?;

        let paging = if request.limit == 0 {
            String::new()
        } else {
            format!(" LIMIT {} OFFSET {}", request.limit, request.offset)
        };
        let mut statement = self.connection.prepare(&format!(
            "SELECT m.meeting_id, m.created_at_epoch_seconds, m.lifecycle,
                    m.meeting_record_sha256, m.attempt_sha256,
                    m.transcript_sha256, m.transcript_relative_path,
                    m.note_json_sha256, m.note_markdown_sha256, m.title, m.folder,
                    (SELECT COUNT(*) FROM turn t WHERE t.meeting_id = m.meeting_id)
             FROM meeting m{where_clause}
             ORDER BY m.created_at_epoch_seconds DESC, m.meeting_id ASC{paging}"
        ))?;
        let rows = statement
            .query_map(bound.as_slice(), |row| {
                Ok(IndexedMeeting {
                    meeting_id: row.get(0)?,
                    created_at_epoch_seconds: row.get::<_, i64>(1)? as u64,
                    lifecycle: parse_lifecycle(&row.get::<_, String>(2)?)
                        .unwrap_or(MeetingLifecycle::Incomplete),
                    meeting_record_sha256: row.get(3)?,
                    attempt_sha256: row.get(4)?,
                    transcript_sha256: row.get(5)?,
                    transcript_relative_path: row.get(6)?,
                    note_json_sha256: row.get(7)?,
                    note_markdown_sha256: row.get(8)?,
                    title: row.get(9)?,
                    folder: row.get(10)?,
                    turn_count: row.get::<_, i64>(11)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListPage {
            total: total as usize,
            rows,
        })
    }

    /// A content digest over every stored row.
    ///
    /// Its only job is to make "this index is rebuildable" a comparison rather
    /// than an assertion: derive it twice, from a live index and from a fresh
    /// one built out of the files, and the digests match or a column is holding
    /// something no file produced.
    ///
    /// **`SELECT *`, and the column loop is driven by `column_count()`.** The
    /// first version named its columns and counted to eleven, which meant a
    /// column added by a later migration would not reach the digest — and a
    /// write-only column is exactly what the rebuild test exists to catch, so
    /// naming the columns quietly defeated it. Never reintroduce a column list
    /// here.
    pub fn fingerprint(&self) -> Result<String, CorpusIndexError> {
        let mut hasher = Sha256::new();
        hasher.update(CORPUS_INDEX_SCHEMA.as_bytes());
        // `corpus_window_vector` is absent on purpose and it is the one
        // exception this digest has. A vector is a function of the files *and*
        // a model, so a rebuild from files alone cannot reproduce it and the
        // equality this digest asserts would be false for an honest store. Any
        // other new table belongs here.
        for statement in [
            "SELECT * FROM meeting ORDER BY meeting_id ASC",
            "SELECT * FROM turn ORDER BY meeting_id ASC, turn_index ASC",
            "SELECT * FROM corpus_window ORDER BY meeting_id ASC, window_index ASC",
            "SELECT * FROM corpus_window_segment
             ORDER BY meeting_id ASC, window_index ASC, segment_index ASC",
        ] {
            let mut prepared = self.connection.prepare(statement)?;
            // Column names are hashed too, so renaming a column is a change.
            for name in prepared.column_names() {
                hasher.update(b"\x1d");
                hasher.update(name.as_bytes());
            }
            let columns = prepared.column_count();
            let mut rows = prepared.query([])?;
            while let Some(row) = rows.next()? {
                for column in 0..columns {
                    hasher.update(b"\x1f");
                    match row.get_ref(column)? {
                        rusqlite::types::ValueRef::Null => hasher.update(b"\x00"),
                        rusqlite::types::ValueRef::Integer(value) => {
                            hasher.update(value.to_be_bytes())
                        }
                        rusqlite::types::ValueRef::Real(value) => {
                            hasher.update(value.to_be_bytes())
                        }
                        rusqlite::types::ValueRef::Text(value) => hasher.update(value),
                        rusqlite::types::ValueRef::Blob(value) => hasher.update(value),
                    }
                }
                hasher.update(b"\x1e");
            }
            hasher.update(b"\x1c");
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// The digest of the canonical inputs the current contents were built from.
    ///
    /// Every column this index stores is a function of these values — turn text
    /// included, because a transcript digest determines its turns. So an equal
    /// corpus digest means an equal index, and a sync can be skipped without
    /// reading a single row.
    ///
    /// `excluded_meeting_ids` must be the exact set `replace_from_projection_excluding`
    /// would be given, and for the same reason: an excluded meeting is hashed as
    /// though it were not in the corpus at all, so a meeting crossing into or out
    /// of the set changes this digest and forces the real resync that actually
    /// adds or removes its rows. Folding the set in some other way (say, hashing
    /// a marker byte instead of skipping the row) would still change the digest
    /// today, but would stop doing so the day two different exclusion sets
    /// happened to produce the same marker sequence -- skipping the row entirely
    /// has no such coincidence to worry about.
    fn corpus_digest(projection: &LibraryProjection, excluded_meeting_ids: &HashSet<String>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(CORPUS_INDEX_SCHEMA.as_bytes());
        for row in projection.rows() {
            let derived = row.derived();
            if excluded_meeting_ids.contains(derived.meeting_id) {
                continue;
            }
            for part in [
                Some(derived.meeting_id),
                Some(derived.meeting_record_sha256),
                Some(derived.attempt_sha256),
                derived.transcript_sha256,
                derived.note_json_sha256,
                derived.note_markdown_sha256,
                derived.title,
                derived.folder,
            ] {
                hasher.update(b"\x1f");
                hasher.update(part.unwrap_or("\x00").as_bytes());
            }
            hasher.update(b"\x1e");
        }
        format!("{:x}", hasher.finalize())
    }

    /// Syncs only when the canonical inputs moved.
    ///
    /// This is what makes the index affordable on a path the app already walks:
    /// an unchanged corpus costs one hash over the projection's row identities
    /// and one `SELECT`, with no write. It is not incremental sync — a single
    /// changed meeting still replaces the whole index. US-13.6 carries that.
    pub fn sync_if_changed(
        &mut self,
        projection: &LibraryProjection,
    ) -> Result<Option<SyncOutcome>, CorpusIndexError> {
        self.sync_if_changed_excluding(projection, &HashSet::new())
    }

    /// Same as [`Self::sync_if_changed`], but a meeting named in
    /// `excluded_meeting_ids` is treated as absent from the corpus for both the
    /// short-circuit digest and the write it guards -- see
    /// [`Self::replace_from_projection_excluding`] for what "absent" removes.
    ///
    /// Passing a changed exclusion set on an otherwise-unchanged corpus is
    /// exactly the case this exists for: a meeting that just became locked has
    /// moved into the set with no file underneath it touched, and the digest
    /// still has to notice and force the write that drops its rows.
    pub fn sync_if_changed_excluding(
        &mut self,
        projection: &LibraryProjection,
        excluded_meeting_ids: &HashSet<String>,
    ) -> Result<Option<SyncOutcome>, CorpusIndexError> {
        let digest = Self::corpus_digest(projection, excluded_meeting_ids);
        // The column is nullable, so it must be read as an `Option` — reading a
        // NULL into `String` is a type error, not an absence.
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT corpus_digest FROM index_identity WHERE id = 0",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        if stored.as_deref() == Some(digest.as_str()) {
            return Ok(None);
        }
        let outcome = self.replace_from_projection_excluding(projection, excluded_meeting_ids)?;
        self.connection.execute(
            "UPDATE index_identity SET corpus_digest = ?1 WHERE id = 0",
            params![digest],
        )?;
        Ok(Some(outcome))
    }

    /// Windows this embedder still owes a vector for, with the text to embed.
    ///
    /// A `limit` of zero means all of them. The text is rebuilt from the same
    /// segments a hit will cite, and checked against the stored digest before
    /// it is handed out — so a caller cannot embed a string the store would
    /// then refuse to accept a vector for.
    ///
    /// Nothing in this crate can consume this yet: no text embedder ships. It
    /// is the seam one would fill, and it is a read rather than a promise.
    pub fn pending_windows(
        &self,
        identity: &EmbedderIdentity,
        limit: usize,
    ) -> Result<Vec<PendingWindow>, CorpusIndexError> {
        let embedder = identity.canonical();
        let mut prepared = self.connection.prepare(
            "SELECT meeting_id, window_index, word_count, text_sha256
             FROM corpus_window AS w
             WHERE NOT EXISTS (
                 SELECT 1 FROM corpus_window_vector AS v
                 WHERE v.meeting_id = w.meeting_id
                   AND v.window_index = w.window_index
                   AND v.embedder = ?1
                   AND v.text_sha256 = w.text_sha256
             )
             ORDER BY meeting_id ASC, window_index ASC
             LIMIT ?2",
        )?;
        let cap: i64 = if limit == 0 { -1 } else { limit as i64 };
        let rows: Vec<(String, u64, u64, String)> = prepared
            .query_map(params![embedder, cap], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut pending = Vec::with_capacity(rows.len());
        for (meeting_id, window_index, word_count, text_sha256) in rows {
            let window = self.window_provenance(&meeting_id, window_index, word_count)?;
            let text = self.window_text(&meeting_id, &window)?;
            if corpus_window::window_digest(&text) != text_sha256 {
                return Err(CorpusIndexError::UnderivableRow);
            }
            pending.push(PendingWindow {
                meeting_id,
                window_index,
                text_sha256,
                text,
            });
        }
        Ok(pending)
    }

    /// Writes vectors, refusing the whole batch rather than any part of it.
    ///
    /// A vector is accepted only if a window with that exact `text_sha256` is
    /// present. That is the check that makes a stale vector impossible to
    /// introduce: the digest a caller supplies is the one it embedded, and if
    /// the meeting has moved since, no window carries it any more.
    pub fn store_window_vectors(
        &mut self,
        identity: &EmbedderIdentity,
        vectors: &[WindowVector],
    ) -> Result<usize, CorpusIndexError> {
        let embedder = identity.canonical();
        let dimension = identity.dimension as usize;
        let transaction = self.connection.transaction()?;
        let mut written = 0;
        {
            let mut bound = transaction.prepare(
                "SELECT 1 FROM corpus_window
                 WHERE meeting_id = ?1 AND window_index = ?2 AND text_sha256 = ?3",
            )?;
            let mut insert = transaction.prepare(
                "INSERT OR REPLACE INTO corpus_window_vector
                    (meeting_id, window_index, embedder, text_sha256, vector)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for vector in vectors {
                if vector.values.len() != dimension {
                    return Err(CorpusIndexError::VectorRejected);
                }
                let exists: Option<i64> = bound
                    .query_row(
                        params![vector.meeting_id, vector.window_index, vector.text_sha256],
                        |row| row.get(0),
                    )
                    .optional()?;
                if exists.is_none() {
                    return Err(CorpusIndexError::VectorRejected);
                }
                let mut blob = Vec::with_capacity(dimension * 4);
                for value in &vector.values {
                    blob.extend_from_slice(&value.to_le_bytes());
                }
                insert.execute(params![
                    vector.meeting_id,
                    vector.window_index,
                    embedder,
                    vector.text_sha256,
                    blob
                ])?;
                written += 1;
            }
        }
        transaction.commit()?;
        Ok(written)
    }

    /// How much of the corpus this embedder has actually answered for.
    ///
    /// Two numbers rather than a boolean, because the useful sentence for a
    /// person is "searched 40 of 800 passages", and a store that cannot say
    /// that will be asked to pretend it searched all of them.
    pub fn vector_coverage(
        &self,
        identity: &EmbedderIdentity,
    ) -> Result<VectorCoverage, CorpusIndexError> {
        let windows: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM corpus_window", [], |row| row.get(0))?;
        let embedded: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM corpus_window AS w
             JOIN corpus_window_vector AS v
               ON v.meeting_id = w.meeting_id
              AND v.window_index = w.window_index
              AND v.text_sha256 = w.text_sha256
             WHERE v.embedder = ?1",
            params![identity.canonical()],
            |row| row.get(0),
        )?;
        Ok(VectorCoverage {
            windows: windows as usize,
            embedded: embedded as usize,
        })
    }

    /// Rank meetings against an already-embedded question.
    ///
    /// The aggregation is [`corpus_window::AGGREGATION`] — one score per
    /// meeting, the best of its windows — because that is the arm the
    /// 2026-08-08 measurement ran. Cosine is computed with both norms rather
    /// than assuming unit vectors: `notes/mlx_minilm.py` returns the raw
    /// mean-pooled hidden state and the probe normalises at comparison time.
    ///
    /// The result carries [`VectorCoverage`] with it. A caller that reports a
    /// ranking without reporting how much of the corpus it covered is making a
    /// claim the store did not.
    pub fn nearest_windows(
        &self,
        identity: &EmbedderIdentity,
        question: &[f32],
        limit: usize,
    ) -> Result<SemanticSearch, CorpusIndexError> {
        if question.len() != identity.dimension as usize {
            return Err(CorpusIndexError::VectorRejected);
        }
        let coverage = self.vector_coverage(identity)?;
        let question_norm = norm(question);

        let mut prepared = self.connection.prepare(
            "SELECT w.meeting_id, w.window_index, w.word_count, v.vector
             FROM corpus_window AS w
             JOIN corpus_window_vector AS v
               ON v.meeting_id = w.meeting_id
              AND v.window_index = w.window_index
              AND v.text_sha256 = w.text_sha256
             WHERE v.embedder = ?1
             ORDER BY w.meeting_id ASC, w.window_index ASC",
        )?;
        let mut rows = prepared.query(params![identity.canonical()])?;
        let mut best: Vec<(String, u64, u64, f32)> = Vec::new();
        while let Some(row) = rows.next()? {
            let meeting_id: String = row.get(0)?;
            let window_index = row.get::<_, i64>(1)? as u64;
            let word_count = row.get::<_, i64>(2)? as u64;
            let blob: Vec<u8> = row.get(3)?;
            if blob.len() != question.len() * 4 {
                return Err(CorpusIndexError::VectorRejected);
            }
            let values: Vec<f32> = blob
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .collect();
            let scale = question_norm * norm(&values);
            let similarity = if scale > 0.0 {
                question
                    .iter()
                    .zip(&values)
                    .map(|(left, right)| left * right)
                    .sum::<f32>()
                    / scale
            } else {
                0.0
            };
            match best
                .iter_mut()
                .find(|(existing, _, _, _)| existing == &meeting_id)
            {
                Some(entry) if similarity > entry.3 => {
                    *entry = (meeting_id, window_index, word_count, similarity);
                }
                Some(_) => {}
                None => best.push((meeting_id, window_index, word_count, similarity)),
            }
        }

        // Ties are the measured failure mode, not an edge case, so the order
        // among them is fixed rather than left to the sort's stability.
        best.sort_by(|left, right| {
            right
                .3
                .partial_cmp(&left.3)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
                .then_with(|| left.1.cmp(&right.1))
        });

        let top = best.first().map(|entry| entry.3);
        let near_ties = top.map_or(0, |top| {
            best.iter()
                .filter(|entry| top - entry.3 <= corpus_window::DENSITY_BAND)
                .count()
        });
        if limit > 0 {
            best.truncate(limit);
        }

        let mut hits = Vec::with_capacity(best.len());
        for (meeting_id, window_index, word_count, similarity) in best {
            let window = self.window_provenance(&meeting_id, window_index, word_count)?;
            hits.push(SemanticHit {
                meeting_id,
                window_index,
                similarity,
                segments: window.segments,
            });
        }
        Ok(SemanticSearch {
            coverage,
            near_ties,
            hits,
        })
    }

    /// The words a window holds, with the naming the last sync saw for its
    /// meeting.
    ///
    /// Derived on demand and never stored. The store keeps a window's digest
    /// and the turn ranges it came from — deliberately, per [`corpus_window`]'s
    /// own note — so a quote costs one read of the turns it names and no
    /// private sentence sits on disk twice.
    ///
    /// It rebuilds through [`corpus_window::window_text`], so what a surface
    /// shows is the exact string the vector was computed from rather than a
    /// slice that looks like it. A window whose turns no longer reconstruct it
    /// is an error, not an empty quote: a hit nobody can quote is a claim with
    /// no evidence behind it, and this repository does not ship those.
    pub fn quote_window(
        &self,
        meeting_id: &str,
        window_index: u64,
    ) -> Result<QuotedWindow, CorpusIndexError> {
        let (word_count, title, folder, created_at_epoch_seconds) = self
            .connection
            .query_row(
                "SELECT w.word_count, m.title, m.folder, m.created_at_epoch_seconds
                 FROM corpus_window AS w
                 JOIN meeting AS m ON m.meeting_id = w.meeting_id
                 WHERE w.meeting_id = ?1 AND w.window_index = ?2",
                params![meeting_id, window_index as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(CorpusIndexError::UnderivableRow)?;
        let window = self.window_provenance(meeting_id, window_index, word_count as u64)?;
        let quote = self.window_text(meeting_id, &window)?;
        // `window_provenance` already refuses an empty segment list, so these
        // are present. Asked for rather than indexed, because a panic here
        // would be a worse answer than a refusal.
        let first = window
            .segments
            .first()
            .ok_or(CorpusIndexError::UnderivableRow)?;
        let last = window
            .segments
            .last()
            .ok_or(CorpusIndexError::UnderivableRow)?;
        Ok(QuotedWindow {
            quote,
            derived_title: self.derived_title(meeting_id)?,
            title,
            folder,
            created_at_epoch_seconds: created_at_epoch_seconds as u64,
            first_turn_index: first.source_turn_index,
            last_turn_index: last.source_turn_index,
        })
    }

    /// This meeting's opening line, by exactly the rule the library list uses.
    ///
    /// Reads every turn including the gated ones, because
    /// [`crate::meeting_title::derived_title`] is what skips them and doing it
    /// twice would let the two rules drift apart.
    fn derived_title(&self, meeting_id: &str) -> Result<Option<String>, CorpusIndexError> {
        let mut prepared = self.connection.prepare(
            "SELECT text, gated FROM turn WHERE meeting_id = ?1 ORDER BY turn_index ASC",
        )?;
        let turns: Vec<(String, bool)> = prepared
            .query_map(params![meeting_id], |row| {
                Ok((row.get(0)?, row.get::<_, i64>(1)? != 0))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(crate::meeting_title::derived_title(
            turns.iter().map(|(text, gated)| (text.as_str(), *gated)),
        ))
    }

    fn window_provenance(
        &self,
        meeting_id: &str,
        window_index: u64,
        word_count: u64,
    ) -> Result<CorpusWindow, CorpusIndexError> {
        let mut prepared = self.connection.prepare(
            "SELECT source_turn_index, original_scalar_start, original_scalar_end
             FROM corpus_window_segment
             WHERE meeting_id = ?1 AND window_index = ?2
             ORDER BY segment_index ASC",
        )?;
        let segments = prepared
            .query_map(params![meeting_id, window_index], |row| {
                Ok(WindowSegment {
                    source_turn_index: row.get::<_, i64>(0)? as u64,
                    original_scalar_start: row.get::<_, i64>(1)? as u64,
                    original_scalar_end: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if segments.is_empty() {
            return Err(CorpusIndexError::UnderivableRow);
        }
        Ok(CorpusWindow {
            window_index,
            word_count,
            segments,
        })
    }

    fn window_text(
        &self,
        meeting_id: &str,
        window: &CorpusWindow,
    ) -> Result<String, CorpusIndexError> {
        let mut prepared = self
            .connection
            .prepare("SELECT text FROM turn WHERE meeting_id = ?1 AND turn_index = ?2")?;
        let mut turns: Vec<(u64, String)> = Vec::new();
        for segment in &window.segments {
            if turns
                .iter()
                .any(|(index, _)| *index == segment.source_turn_index)
            {
                continue;
            }
            let text: Option<String> = prepared
                .query_row(params![meeting_id, segment.source_turn_index], |row| {
                    row.get(0)
                })
                .optional()?;
            turns.push((
                segment.source_turn_index,
                text.ok_or(CorpusIndexError::UnderivableRow)?,
            ));
        }
        corpus_window::window_text(window, |index| {
            turns
                .iter()
                .find(|(stored, _)| *stored == index)
                .map(|(_, text)| text.as_str())
        })
        .ok_or(CorpusIndexError::UnderivableRow)
    }
}

fn norm(values: &[f32]) -> f32 {
    values.iter().map(|value| value * value).sum::<f32>().sqrt()
}

/// A window with no vector yet, and the exact text to embed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWindow {
    pub meeting_id: String,
    pub window_index: u64,
    pub text_sha256: String,
    pub text: String,
}

/// A vector offered to the store, carrying the digest it was computed from.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowVector {
    pub meeting_id: String,
    pub window_index: u64,
    pub text_sha256: String,
    pub values: Vec<f32>,
}

/// How much of the corpus an embedder has answered for. Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorCoverage {
    pub windows: usize,
    pub embedded: usize,
}

impl VectorCoverage {
    pub fn complete(&self) -> bool {
        self.windows == self.embedded
    }
}

/// One meeting's best window, with the provenance to quote it.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticHit {
    pub meeting_id: String,
    pub window_index: u64,
    pub similarity: f32,
    pub segments: Vec<WindowSegment>,
}

/// One window's words, and enough naming to say where they came from.
///
/// The turn indices are the store's own, counted from zero. A surface that
/// numbers turns for a person adds one; this type does not, because the
/// transcript reader and the exact-search path both index from zero and a type
/// that quietly disagreed with them would be found by a reader, not a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedWindow {
    pub quote: String,
    /// The operator's own title, which is the only one the index stores.
    pub title: Option<String>,
    /// Recomputed from this meeting's turns, never stored — see
    /// [`crate::meeting_title`] for why. Carried because a surface that showed
    /// only the operator title would label a meeting with its timestamp while
    /// the library list showed its opening sentence.
    pub derived_title: Option<String>,
    pub folder: Option<String>,
    /// Carried because an untitled meeting is named by when it happened, and a
    /// search result that cannot name its meeting is not a result.
    pub created_at_epoch_seconds: u64,
    pub first_turn_index: u64,
    pub last_turn_index: u64,
}

/// A ranking that states what it searched and how crowded the answer was.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSearch {
    pub coverage: VectorCoverage,
    /// Meetings within [`corpus_window::DENSITY_BAND`] of the top score,
    /// counted before `limit` truncates. The 2026-08-08 measurement reported
    /// this rather than accuracy alone, because both of its failures were ties
    /// at a margin of 0.0000 — a wrong answer arriving as a visible tie is
    /// something a surface can act on, and an accuracy figure hides it.
    pub near_ties: usize,
    pub hits: Vec<SemanticHit>,
}

const SCHEMA_2: &str = "
CREATE TABLE IF NOT EXISTS index_identity (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    schema TEXT NOT NULL,
    sqlite_version TEXT NOT NULL,
    applied_migration INTEGER NOT NULL,
    corpus_digest TEXT
);

CREATE TABLE IF NOT EXISTS meeting (
    meeting_id TEXT PRIMARY KEY,
    created_at_epoch_seconds INTEGER NOT NULL,
    lifecycle TEXT NOT NULL,
    meeting_record_sha256 TEXT NOT NULL,
    attempt_sha256 TEXT NOT NULL,
    transcript_sha256 TEXT,
    transcript_relative_path TEXT,
    note_json_sha256 TEXT,
    note_markdown_sha256 TEXT,
    title TEXT,
    folder TEXT
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS meeting_created_at
    ON meeting (created_at_epoch_seconds DESC, meeting_id ASC);
CREATE INDEX IF NOT EXISTS meeting_folder ON meeting (folder);

CREATE TABLE IF NOT EXISTS turn (
    meeting_id TEXT NOT NULL REFERENCES meeting (meeting_id) ON DELETE CASCADE,
    turn_index INTEGER NOT NULL,
    visible_index INTEGER,
    gated INTEGER NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (meeting_id, turn_index)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS corpus_window (
    meeting_id TEXT NOT NULL REFERENCES meeting (meeting_id) ON DELETE CASCADE,
    window_index INTEGER NOT NULL,
    word_count INTEGER NOT NULL,
    text_sha256 TEXT NOT NULL,
    PRIMARY KEY (meeting_id, window_index)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS corpus_window_segment (
    meeting_id TEXT NOT NULL,
    window_index INTEGER NOT NULL,
    segment_index INTEGER NOT NULL,
    source_turn_index INTEGER NOT NULL,
    original_scalar_start INTEGER NOT NULL,
    original_scalar_end INTEGER NOT NULL,
    PRIMARY KEY (meeting_id, window_index, segment_index)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS corpus_window_vector (
    meeting_id TEXT NOT NULL,
    window_index INTEGER NOT NULL,
    embedder TEXT NOT NULL,
    text_sha256 TEXT NOT NULL,
    vector BLOB NOT NULL,
    PRIMARY KEY (meeting_id, window_index, embedder)
) WITHOUT ROWID;
";

fn lifecycle_name(lifecycle: MeetingLifecycle) -> Result<String, CorpusIndexError> {
    // Through serde, so the stored name is the same string the canonical
    // record uses and cannot drift from it.
    match serde_json::to_value(lifecycle) {
        Ok(serde_json::Value::String(name)) => Ok(name),
        _ => Err(CorpusIndexError::UnderivableRow),
    }
}

fn parse_lifecycle(name: &str) -> Option<MeetingLifecycle> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
}

fn require_private_parent(path: &Path) -> Result<(), CorpusIndexError> {
    use std::os::unix::fs::PermissionsExt;
    let parent = path.parent().ok_or(CorpusIndexError::StorageUnavailable)?;
    let metadata =
        std::fs::symlink_metadata(parent).map_err(|_| CorpusIndexError::StorageUnavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_dir()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(CorpusIndexError::NotPrivate);
    }
    Ok(())
}

fn require_private_file(metadata: &std::fs::Metadata) -> Result<(), CorpusIndexError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(CorpusIndexError::NotPrivate);
    }
    Ok(())
}

fn set_private_mode(path: &Path) -> Result<(), CorpusIndexError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| CorpusIndexError::NotPrivate)
}

#[cfg(test)]
// `pub(crate)` so `corpus_embedding`'s tests can build a real corpus through the
// same fixture rather than growing a second one that drifts from it.
pub(crate) mod tests {
    use std::collections::HashSet;

    use tempfile::TempDir;

    use super::*;
    use crate::library_read::ReadLimits;
    use crate::meeting::{
        ArtifactRef, AudioRetention, AudioRetentionRule, AudioState, MeetingArtifacts,
        MeetingRecord, MeetingSchema, artifact_ref, retention_policy_sha256, write_meeting,
    };
    use crate::storage::durable_create_new;

    pub(crate) struct Fixture {
        _temp: TempDir,
        pub(crate) storage: StorageRoot,
    }

    impl Fixture {
        pub(crate) fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let repository = temp.path().join("repository");
            create_private_dir(&repository).unwrap();
            let storage = StorageRoot::create(&temp.path().join("data"), &repository).unwrap();
            Self {
                _temp: temp,
                storage,
            }
        }

        pub(crate) fn meeting(&self, id: &str, created: u64, turns: &[&str]) {
            let gated: Vec<(&str, bool)> = turns.iter().map(|text| (*text, false)).collect();
            self.meeting_with_gates(id, created, &gated);
        }

        pub(crate) fn meeting_with_gates(&self, id: &str, created: u64, turns: &[(&str, bool)]) {
            let directory = self.storage.path().join("meetings").join(id);
            for child in ["", "capture", "transcript", "deletion"] {
                create_private_dir(&directory.join(child)).unwrap();
            }
            let rule = AudioRetentionRule::DeleteAfter { seconds: 60 };
            let policy = retention_policy_sha256(&rule);
            let attempt = serde_json::json!({
                "schema": "capture-attempt/1", "meeting_id": id,
                "attempt_id": "11111111-1111-4111-8111-111111111111",
                "created_at_epoch_seconds": created,
                "application_build_sha256": "a".repeat(64),
                "participant_notice_version": "internal-transcript-alpha/1",
                "operator_attestation": {
                    "participantsConsented": true, "headphones": true, "operatorAlone": true,
                },
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
            self.transcribe(id, created, turns);
        }

        /// A new transcript for a meeting that already exists, through the same
        /// path that wrote the first one.
        fn transcribe(&self, id: &str, created: u64, turns: &[(&str, bool)]) {
            let directory = self.storage.path().join("meetings").join(id);
            let rule = AudioRetentionRule::DeleteAfter { seconds: 60 };
            let policy = retention_policy_sha256(&rule);
            let turn_values: Vec<_> = turns
                .iter()
                .enumerate()
                .map(|(index, (text, gated))| {
                    serde_json::json!({
                        "start": index as f64, "end": index as f64 + 0.5,
                        "speaker": "Me", "text": text, "gated": gated,
                    })
                })
                .collect();
            let transcript = serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "capture-transcript/1", "source": "synthetic",
                "attribution": "channel", "bleed": null, "voiceprint": null,
                "capture_health": {}, "turns": turn_values,
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
        }

        pub(crate) fn projection(&self) -> LibraryProjection {
            LibraryProjection::rebuild_excluding(
                &self.storage,
                ReadLimits::default(),
                &HashSet::new(),
            )
            .unwrap()
        }
    }

    pub(crate) fn synced(fixture: &Fixture) -> CorpusIndex {
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        index
            .replace_from_projection(&fixture.projection())
            .unwrap();
        index
    }

    #[test]
    fn a_synced_index_holds_one_row_per_validated_meeting() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta", "gamma"]);
        fixture.meeting("meeting-b", 200, &["delta"]);
        let index = synced(&fixture);
        assert_eq!(index.meeting_count().unwrap(), 2);
        let page = index.list(&ListRequest::default()).unwrap();
        assert_eq!(page.total, 2);
        // Newest first, matching the scan's order.
        assert_eq!(page.rows[0].meeting_id, "meeting-b");
        assert_eq!(page.rows[1].meeting_id, "meeting-a");
        assert_eq!(page.rows[1].turn_count, 2);
    }

    /// The load-bearing test. Drop the database, rebuild from the files alone,
    /// and require the same content digest. A column written from anything but
    /// a canonical file fails here, which is what keeps this a cache.
    #[test]
    fn rebuild_from_files_equals_the_live_index() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta", "gamma"]);
        fixture.meeting("meeting-b", 200, &["delta"]);
        let live = synced(&fixture).fingerprint().unwrap();

        let path = fixture.storage.path().join("library").join(DATABASE_NAME);
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());

        let rebuilt = synced(&fixture).fingerprint().unwrap();
        assert_eq!(live, rebuilt);
    }

    /// The corpus store answers a count the scan can only truncate.
    ///
    /// **Rewritten 2026-08-08.** This test used to assert that
    /// `projection.search` returns `CapacityExceeded` past `MAX_SEARCH_RESULTS`,
    /// and read that refusal as the reason the store exists. The refusal was a
    /// defect rather than a design — a common word refused at five meetings —
    /// and the scan now truncates and reports its total instead. What survives
    /// is the real difference: a scan answers a *page* of spans, and a filtered
    /// list answers a *count* of meetings without materialising anything.
    #[test]
    fn a_filter_counts_what_the_scan_can_only_page() {
        let fixture = Fixture::new();
        // Forty meetings of three turns each is 120 spans carrying the word,
        // past the hundred-hit cap and well inside a real corpus.
        for ordinal in 0..40 {
            fixture.meeting(
                &format!("meeting-{ordinal:03}"),
                100 + ordinal,
                &["common one", "common two", "common three"],
            );
        }
        let projection = fixture.projection();
        let scanned = projection.search("common").expect("the scan answers");
        assert_eq!(scanned.hits.len(), 100, "the page is capped");
        assert_eq!(scanned.total, 120, "and it says how many it cut from");

        let index = synced(&fixture);
        let page = index
            .list(&ListRequest {
                created_at_or_after: Some(120),
                limit: 5,
                ..Default::default()
            })
            .unwrap();
        // Total and returned are separate numbers, so a page is never mistaken
        // for the whole answer.
        assert_eq!(page.total, 20);
        assert_eq!(page.rows.len(), 5);
        assert_eq!(page.rows[0].created_at_epoch_seconds, 139);
    }

    #[test]
    fn lifecycle_round_trips_through_its_canonical_name() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let index = synced(&fixture);
        let page = index
            .list(&ListRequest {
                lifecycles: vec![MeetingLifecycle::TranscriptReady],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].lifecycle, MeetingLifecycle::TranscriptReady);
        assert_eq!(
            lifecycle_name(MeetingLifecycle::TranscriptReady).unwrap(),
            "transcript-ready"
        );

        let empty = index
            .list(&ListRequest {
                lifecycles: vec![MeetingLifecycle::Ready],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(empty.total, 0);
    }

    /// SQLite opens a path directly, so none of the descriptor-bound guards in
    /// `storage.rs` apply to it. These are the checks that replace them.
    #[test]
    fn every_file_the_index_leaves_behind_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let index = synced(&fixture);
        drop(index);

        let library = fixture.storage.path().join("library");
        let mut seen = 0;
        for entry in std::fs::read_dir(&library).unwrap() {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o600,
                "{:?} is not 0600",
                entry.file_name()
            );
            seen += 1;
        }
        // No `-wal` or `-shm`: rollback journalling leaves one file at rest.
        assert_eq!(seen, 1);
    }

    #[test]
    fn a_group_readable_database_is_refused_rather_than_opened() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let index = CorpusIndex::open(&fixture.storage).unwrap();
        let path = index.path().to_path_buf();
        drop(index);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            CorpusIndex::open(&fixture.storage),
            Err(CorpusIndexError::NotPrivate)
        ));
    }

    #[test]
    fn a_newer_schema_refuses_instead_of_reading_columns_it_does_not_know() {
        let fixture = Fixture::new();
        let index = CorpusIndex::open(&fixture.storage).unwrap();
        index
            .connection
            .execute(
                "UPDATE index_identity SET applied_migration = ?1 WHERE id = 0",
                params![APPLIED_MIGRATION + 1],
            )
            .unwrap();
        drop(index);
        assert!(matches!(
            CorpusIndex::open(&fixture.storage),
            Err(CorpusIndexError::SchemaAhead)
        ));
    }

    #[test]
    fn the_index_records_the_sqlite_build_that_wrote_it() {
        let fixture = Fixture::new();
        let index = CorpusIndex::open(&fixture.storage).unwrap();
        let (schema, sqlite_version) = index.identity().unwrap();
        assert_eq!(schema, CORPUS_INDEX_SCHEMA);
        assert_eq!(sqlite_version, rusqlite::version());
        assert!(!sqlite_version.is_empty());
    }

    /// The rebuild pin is only as wide as the fingerprint. A column the digest
    /// does not read is a column that can hold anything, so add one and require
    /// the equality to break.
    #[test]
    fn a_column_no_file_produced_breaks_the_rebuild_digest() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let index = synced(&fixture);
        let before = index.fingerprint().unwrap();

        index
            .connection
            .execute("ALTER TABLE meeting ADD COLUMN invented TEXT", [])
            .unwrap();
        index
            .connection
            .execute("UPDATE meeting SET invented = 'not from any file'", [])
            .unwrap();
        let after = index.fingerprint().unwrap();

        assert_ne!(
            before, after,
            "fingerprint ignored a column, so the rebuild test cannot catch a write-only field"
        );
    }

    /// The WAL rejection is the reason US-13.4 gives for a design choice, and a
    /// file listing cannot check it: SQLite removes `-wal` and `-shm` on a
    /// clean close, so the one-file assertion passes under WAL too.
    #[test]
    fn the_journal_mode_is_read_back_rather_than_assumed() {
        let fixture = Fixture::new();
        let index = CorpusIndex::open(&fixture.storage).unwrap();
        let journal: String = index
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_ascii_lowercase(), "delete");
    }

    /// `secure_delete` is claimed to keep a removed meeting from staying
    /// legible in free space. Read the raw file and require the words gone.
    #[test]
    fn a_deleted_meetings_words_do_not_survive_in_the_database_file() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["quarkbeetle luminance"]);
        fixture.meeting("meeting-b", 200, &["ordinary text"]);
        let mut index = synced(&fixture);
        let path = index.path().to_path_buf();

        let raw = std::fs::read(&path).unwrap();
        assert!(
            find_bytes(&raw, b"quarkbeetle").is_some(),
            "fixture is not proving anything if the word was never written"
        );

        std::fs::remove_dir_all(fixture.storage.path().join("meetings").join("meeting-a")).unwrap();
        index
            .replace_from_projection(&fixture.projection())
            .unwrap();
        drop(index);

        let raw = std::fs::read(&path).unwrap();
        assert!(
            find_bytes(&raw, b"quarkbeetle").is_none(),
            "a removed meeting's words are still legible in the database file"
        );
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// US-13.3 claims the store's order matches the scan's. Assert it against
    /// the scan itself rather than against a hand-written expectation.
    #[test]
    fn list_order_matches_the_scans_order() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-c", 100, &["alpha"]);
        fixture.meeting("meeting-a", 300, &["beta"]);
        fixture.meeting("meeting-b", 100, &["gamma"]);
        let projection = fixture.projection();
        let scanned: Vec<_> = projection
            .rows()
            .iter()
            .map(|row| row.derived().meeting_id.to_owned())
            .collect();

        let index = synced(&fixture);
        let listed: Vec<_> = index
            .list(&ListRequest::default())
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.meeting_id)
            .collect();

        assert_eq!(scanned, listed);
        // Same-timestamp rows are what make this a real check rather than a
        // restatement of "newest first".
        assert_eq!(listed, vec!["meeting-a", "meeting-b", "meeting-c"]);
    }

    #[test]
    fn an_unchanged_corpus_syncs_once_and_then_skips() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();

        let first = index.sync_if_changed(&fixture.projection()).unwrap();
        assert_eq!(
            first,
            Some(SyncOutcome {
                meetings: 1,
                turns: 1,
                windows: 1
            })
        );
        assert_eq!(index.sync_if_changed(&fixture.projection()).unwrap(), None);

        fixture.meeting("meeting-b", 200, &["beta"]);
        let third = index.sync_if_changed(&fixture.projection()).unwrap();
        assert_eq!(
            third,
            Some(SyncOutcome {
                meetings: 2,
                turns: 2,
                windows: 2
            })
        );
        assert_eq!(index.meeting_count().unwrap(), 2);
    }

    /// A direct replace must not leave a digest describing contents it just
    /// overwrote, or the next sync would skip a corpus that actually moved.
    #[test]
    fn a_direct_replace_clears_the_recorded_corpus_digest() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        index.sync_if_changed(&fixture.projection()).unwrap();

        index
            .replace_from_projection(&fixture.projection())
            .unwrap();
        let stored: Option<String> = index
            .connection
            .query_row(
                "SELECT corpus_digest FROM index_identity WHERE id = 0",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap();
        assert_eq!(stored, None);
        assert!(
            index
                .sync_if_changed(&fixture.projection())
                .unwrap()
                .is_some()
        );
    }

    /// The downgrade path: a file from a schema this build does not have a
    /// migration for is dropped and re-derived, because the files are the
    /// authority and nothing is lost by rebuilding.
    #[test]
    fn an_older_schema_is_dropped_and_re_derived_rather_than_read() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let index = synced(&fixture);
        assert_eq!(index.meeting_count().unwrap(), 1);
        index
            .connection
            .execute(
                "UPDATE index_identity SET applied_migration = 0 WHERE id = 0",
                [],
            )
            .unwrap();
        drop(index);

        let reopened = CorpusIndex::open(&fixture.storage).unwrap();
        assert_eq!(reopened.meeting_count().unwrap(), 0);
        assert_eq!(reopened.identity().unwrap().0, CORPUS_INDEX_SCHEMA);
    }

    #[test]
    fn a_removed_meeting_leaves_no_turn_behind() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha", "beta"]);
        fixture.meeting("meeting-b", 200, &["gamma"]);
        let mut index = synced(&fixture);
        let turns: i64 = index
            .connection
            .query_row("SELECT COUNT(*) FROM turn", [], |row| row.get(0))
            .unwrap();
        assert_eq!(turns, 3);

        std::fs::remove_dir_all(fixture.storage.path().join("meetings").join("meeting-a")).unwrap();
        let outcome = index
            .replace_from_projection(&fixture.projection())
            .unwrap();
        assert_eq!(outcome.meetings, 1);
        assert_eq!(outcome.turns, 1);
        let turns: i64 = index
            .connection
            .query_row("SELECT COUNT(*) FROM turn", [], |row| row.get(0))
            .unwrap();
        assert_eq!(turns, 1);
    }

    /// A rename must resync the cache, or the index serves a title the files no
    /// longer carry.
    ///
    /// `corpus_digest` already covers `title` and `folder`, so this passes today
    /// — which is exactly why it is worth pinning. `sync_if_changed` skips when
    /// the digest is unchanged, and a later narrowing of that digest to the
    /// meeting/transcript/attempt hashes would look like a harmless
    /// optimisation and would silently strand every rename.
    /// `rebuild_from_files_equals_the_live_index` would not catch it: both sides
    /// of that comparison are rebuilt, so both would agree on the stale value.
    #[test]
    fn renaming_a_meeting_moves_the_corpus_digest_and_resyncs_the_index() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 10, &["one two three"]);
        crate::library_metadata::set_meeting_title(
            &fixture.storage,
            0,
            "meeting-a",
            Some("Before"),
        )
        .unwrap();

        let projection =
            LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        assert!(index.sync_if_changed(&projection).unwrap().is_some());
        assert!(
            index.sync_if_changed(&projection).unwrap().is_none(),
            "an unchanged corpus wrote twice"
        );
        assert_eq!(
            index.list(&ListRequest::default()).unwrap().rows[0]
                .title
                .as_deref(),
            Some("Before")
        );

        crate::library_metadata::set_meeting_title(&fixture.storage, 1, "meeting-a", Some("After"))
            .unwrap();
        let renamed = LibraryProjection::rebuild(&fixture.storage, ReadLimits::default()).unwrap();
        assert!(
            index.sync_if_changed(&renamed).unwrap().is_some(),
            "a rename did not move the digest, so the index still serves the old title"
        );
        assert_eq!(
            index.list(&ListRequest::default()).unwrap().rows[0]
                .title
                .as_deref(),
            Some("After")
        );
    }

    // --- roadmap intake I5: corpus-index exclusion ---

    /// The direct write path: an excluded meeting leaves nothing behind, not
    /// even its words in the raw file, exactly like the deletion case above.
    #[test]
    fn excluding_a_meeting_writes_none_of_its_rows_or_words() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["quarkbeetle luminance"]);
        fixture.meeting("meeting-b", 200, &["ordinary text"]);
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        let excluded: HashSet<String> = ["meeting-a".to_owned()].into_iter().collect();

        let outcome = index
            .replace_from_projection_excluding(&fixture.projection(), &excluded)
            .unwrap();
        assert_eq!(outcome.meetings, 1, "only meeting-b was written");
        assert_eq!(index.meeting_count().unwrap(), 1);
        let listed: Vec<_> = index
            .list(&ListRequest::default())
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.meeting_id)
            .collect();
        assert_eq!(listed, vec!["meeting-b"]);

        let path = index.path().to_path_buf();
        drop(index);
        let raw = std::fs::read(&path).unwrap();
        assert!(
            find_bytes(&raw, b"quarkbeetle").is_none(),
            "an excluded meeting's words are still legible in the database file"
        );
    }

    /// Roadmap intake I5's other named requirement: locking a meeting removes
    /// whatever the index already holds for it, on the very next sync -- with
    /// no file underneath that meeting touched at all. `sync_if_changed`'s
    /// ordinary short-circuit exists exactly to skip a write when nothing
    /// moved; this proves the exclusion set is itself something that "moves"
    /// for that purpose.
    #[test]
    fn a_meeting_that_becomes_excluded_loses_its_already_synced_rows() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        fixture.meeting("meeting-b", 200, &["gamma"]);
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        let projection = fixture.projection();

        assert!(index.sync_if_changed(&projection).unwrap().is_some());
        assert_eq!(index.meeting_count().unwrap(), 2);

        // Nothing on disk changed -- meeting-a merely became locked, which
        // this test stands in for by handing the same meeting id as an
        // exclusion on an otherwise identical projection.
        let now_locked: HashSet<String> = ["meeting-a".to_owned()].into_iter().collect();
        let outcome = index
            .sync_if_changed_excluding(&projection, &now_locked)
            .unwrap();
        assert!(
            outcome.is_some(),
            "a newly locked meeting did not move the digest, so its rows were never removed"
        );
        assert_eq!(index.meeting_count().unwrap(), 1);
        let turns: i64 = index
            .connection
            .query_row("SELECT COUNT(*) FROM turn WHERE meeting_id = 'meeting-a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(turns, 0, "meeting-a's turns outlived its exclusion");

        // And skips again once the exclusion set is unchanged, same as any
        // other unchanged sync.
        assert_eq!(
            index
                .sync_if_changed_excluding(&projection, &now_locked)
                .unwrap(),
            None
        );
    }

    /// Unlocking re-admits the meeting on the next rebuild: lifting the
    /// exclusion is exactly as much a corpus change as imposing it was.
    #[test]
    fn lifting_a_meetings_exclusion_re_admits_it_on_the_next_sync() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let mut index = CorpusIndex::open(&fixture.storage).unwrap();
        let projection = fixture.projection();

        let locked: HashSet<String> = ["meeting-a".to_owned()].into_iter().collect();
        index
            .sync_if_changed_excluding(&projection, &locked)
            .unwrap();
        assert_eq!(index.meeting_count().unwrap(), 0);

        let outcome = index.sync_if_changed(&projection).unwrap();
        assert!(
            outcome.is_some(),
            "lifting the exclusion did not move the digest, so the meeting stayed absent"
        );
        assert_eq!(index.meeting_count().unwrap(), 1);
    }

    /// A vector standing on an excluded meeting's window is pruned the same
    /// way a vector standing on a deleted meeting's window already is
    /// (`a_removed_meeting_leaves_no_vector_behind`, below) -- exclusion goes
    /// through the identical delete-then-reinsert path, so this pins that the
    /// two really do share the mechanism rather than merely looking alike.
    #[test]
    fn excluding_a_meeting_prunes_its_vectors_too() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        fixture.meeting("meeting-b", 200, &["gamma delta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        let batch: Vec<_> = pending
            .iter()
            .enumerate()
            .map(|(position, window)| offer(window, axis(position)))
            .collect();
        index.store_window_vectors(&identity, &batch).unwrap();
        assert_eq!(index.vector_coverage(&identity).unwrap().embedded, 2);

        // Same shape as `a_meeting_that_becomes_excluded_loses_its_already_synced_rows`
        // above: nothing on disk moved, only the exclusion set did.
        let locked: HashSet<String> = ["meeting-a".to_owned()].into_iter().collect();
        index
            .sync_if_changed_excluding(&fixture.projection(), &locked)
            .unwrap();

        let coverage = index.vector_coverage(&identity).unwrap();
        assert_eq!((coverage.windows, coverage.embedded), (1, 1));
        let orphans: i64 = index
            .connection
            .query_row(
                "SELECT COUNT(*) FROM corpus_window_vector WHERE meeting_id = 'meeting-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0, "a locked meeting's vector outlived its window");
    }

    // --- windows and vectors ---

    fn words(count: usize, tag: &str) -> String {
        (0..count)
            .map(|index| format!("{tag}{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A unit vector along one axis. Cosine between two of these is 1 when the
    /// axes match and 0 when they do not, which makes a ranking assertion an
    /// assertion about the ranking rather than about floating point.
    fn axis(index: usize) -> Vec<f32> {
        let mut values = vec![0.0_f32; EmbedderIdentity::measured().dimension as usize];
        values[index] = 1.0;
        values
    }

    fn blend(first: usize, second: usize, weight: f32) -> Vec<f32> {
        let mut values = vec![0.0_f32; EmbedderIdentity::measured().dimension as usize];
        values[first] = 1.0 - weight;
        values[second] = weight;
        values
    }

    fn offer(pending: &PendingWindow, values: Vec<f32>) -> WindowVector {
        WindowVector {
            meeting_id: pending.meeting_id.clone(),
            window_index: pending.window_index,
            text_sha256: pending.text_sha256.clone(),
            values,
        }
    }

    #[test]
    fn a_sync_cuts_meetings_into_windows_that_cite_their_turns() {
        let fixture = Fixture::new();
        let long = words(crate::corpus_window::WINDOW_WORDS + 10, "w");
        fixture.meeting("meeting-a", 100, &[&long]);
        let index = synced(&fixture);

        let counts: Vec<(u64, u64)> = index
            .connection
            .prepare("SELECT window_index, word_count FROM corpus_window ORDER BY window_index")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            counts,
            vec![(0, crate::corpus_window::WINDOW_WORDS as u64), (1, 10)]
        );
        let segments: i64 = index
            .connection
            .query_row("SELECT COUNT(*) FROM corpus_window_segment", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(segments, 2, "each window cites the one turn it came from");
    }

    #[test]
    fn a_gated_turn_reaches_no_window_and_no_vector() {
        let fixture = Fixture::new();
        fixture.meeting_with_gates(
            "meeting-a",
            100,
            &[("alpha beta", false), ("withheld entirely", true)],
        );
        let index = synced(&fixture);

        let pending = index
            .pending_windows(&EmbedderIdentity::measured(), 0)
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].text, "alpha beta");
        let cited: Vec<i64> = index
            .connection
            .prepare("SELECT DISTINCT source_turn_index FROM corpus_window_segment")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(cited, vec![0], "a withheld turn was cited by a window");
    }

    /// The same invariant one layer out, where it becomes visible. A window
    /// cannot cite a withheld turn, so a quote built from its segments cannot
    /// contain one — but a quote is the first thing that puts transcript words
    /// on screen from a search, and the cost of being wrong is a voice check
    /// undone by the feature that reads around it.
    #[test]
    fn a_quote_cannot_contain_a_withheld_turn() {
        let fixture = Fixture::new();
        fixture.meeting_with_gates(
            "meeting-a",
            100,
            &[
                ("the lease renews in March", false),
                ("withheld entirely", true),
                ("and the landlord wants an answer", false),
            ],
        );
        let index = synced(&fixture);

        let quoted = index.quote_window("meeting-a", 0).unwrap();
        assert_eq!(
            quoted.quote,
            "the lease renews in March and the landlord wants an answer"
        );
        assert!(!quoted.quote.contains("withheld"));
        // The turn locators skip it too, so a surface pointing at the passage
        // does not point at the gap.
        assert_eq!((quoted.first_turn_index, quoted.last_turn_index), (0, 2));
    }

    #[test]
    fn a_quote_is_refused_for_a_window_the_index_does_not_hold() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let index = synced(&fixture);

        assert!(matches!(
            index.quote_window("meeting-a", 7),
            Err(CorpusIndexError::UnderivableRow)
        ));
        assert!(matches!(
            index.quote_window("meeting-missing", 0),
            Err(CorpusIndexError::UnderivableRow)
        ));
    }

    #[test]
    fn pending_text_is_exactly_what_its_digest_names() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta", "gamma"]);
        let index = synced(&fixture);
        let identity = EmbedderIdentity::measured();

        let pending = index.pending_windows(&identity, 0).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].text, "alpha beta gamma");
        assert_eq!(
            pending[0].text_sha256,
            crate::corpus_window::window_digest(&pending[0].text)
        );
    }

    #[test]
    fn storing_a_vector_empties_the_pending_list_for_that_embedder_only() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();

        let pending = index.pending_windows(&identity, 0).unwrap();
        let written = index
            .store_window_vectors(&identity, &[offer(&pending[0], axis(0))])
            .unwrap();
        assert_eq!(written, 1);
        assert!(index.pending_windows(&identity, 0).unwrap().is_empty());
        assert!(index.vector_coverage(&identity).unwrap().complete());

        let other = EmbedderIdentity {
            pooling: "cls".to_owned(),
            ..identity.clone()
        };
        assert_eq!(index.pending_windows(&other, 0).unwrap().len(), 1);
        assert_eq!(index.vector_coverage(&other).unwrap().embedded, 0);
    }

    #[test]
    fn a_vector_the_store_cannot_bind_is_refused_and_nothing_is_written() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();

        let wrong_digest = WindowVector {
            text_sha256: "0".repeat(64),
            ..offer(&pending[0], axis(0))
        };
        assert!(matches!(
            index.store_window_vectors(&identity, &[wrong_digest]),
            Err(CorpusIndexError::VectorRejected)
        ));

        let wrong_width = WindowVector {
            values: vec![1.0, 0.0],
            ..offer(&pending[0], axis(0))
        };
        assert!(matches!(
            index.store_window_vectors(&identity, &[wrong_width]),
            Err(CorpusIndexError::VectorRejected)
        ));

        let absent = WindowVector {
            meeting_id: "meeting-nowhere".to_owned(),
            ..offer(&pending[0], axis(0))
        };
        assert!(matches!(
            index.store_window_vectors(&identity, &[absent]),
            Err(CorpusIndexError::VectorRejected)
        ));

        assert_eq!(index.vector_coverage(&identity).unwrap().embedded, 0);
    }

    #[test]
    fn a_refused_batch_writes_none_of_its_good_vectors() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        fixture.meeting("meeting-b", 200, &["gamma delta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        assert_eq!(pending.len(), 2);

        let batch = vec![
            offer(&pending[0], axis(0)),
            WindowVector {
                text_sha256: "0".repeat(64),
                ..offer(&pending[1], axis(1))
            },
        ];
        assert!(index.store_window_vectors(&identity, &batch).is_err());
        assert_eq!(
            index.vector_coverage(&identity).unwrap().embedded,
            0,
            "a partial batch left the store half-answered"
        );
    }

    /// The property the whole binding exists for: after the words change, the
    /// old vector is gone rather than still answering for them.
    #[test]
    fn a_vector_does_not_survive_the_words_it_was_computed_from() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["the lease renews in March"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        index
            .store_window_vectors(&identity, &[offer(&pending[0], axis(0))])
            .unwrap();
        assert_eq!(index.vector_coverage(&identity).unwrap().embedded, 1);

        fixture.transcribe("meeting-a", 100, &[("the lease renews in April", false)]);
        index.sync_if_changed(&fixture.projection()).unwrap();

        assert_eq!(index.vector_coverage(&identity).unwrap().embedded, 0);
        assert_eq!(index.pending_windows(&identity, 0).unwrap().len(), 1);
        // Deliberately a row count and not a coverage figure. Coverage joins on
        // the digest, so it reads zero whether the stale row was deleted or
        // merely ignored — and a test that cannot tell those apart would pass
        // with the prune removed. Verified by removing it.
        let stranded: i64 = index
            .connection
            .query_row("SELECT COUNT(*) FROM corpus_window_vector", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            stranded, 0,
            "the stale vector was hidden rather than deleted"
        );
    }

    #[test]
    fn a_vector_survives_a_resync_that_leaves_its_window_alone() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        index
            .store_window_vectors(&identity, &[offer(&pending[0], axis(0))])
            .unwrap();

        fixture.meeting("meeting-b", 200, &["gamma delta"]);
        index.sync_if_changed(&fixture.projection()).unwrap();

        let coverage = index.vector_coverage(&identity).unwrap();
        assert_eq!(
            (coverage.windows, coverage.embedded),
            (2, 1),
            "adding one meeting cost the other meeting its vector"
        );
    }

    #[test]
    fn a_removed_meeting_leaves_no_vector_behind() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        fixture.meeting("meeting-b", 200, &["gamma delta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        let batch: Vec<_> = pending
            .iter()
            .enumerate()
            .map(|(position, window)| offer(window, axis(position)))
            .collect();
        index.store_window_vectors(&identity, &batch).unwrap();

        std::fs::remove_dir_all(fixture.storage.path().join("meetings").join("meeting-b")).unwrap();
        index.sync_if_changed(&fixture.projection()).unwrap();

        let coverage = index.vector_coverage(&identity).unwrap();
        assert_eq!((coverage.windows, coverage.embedded), (1, 1));
        let orphans: i64 = index
            .connection
            .query_row(
                "SELECT COUNT(*) FROM corpus_window_vector WHERE meeting_id = 'meeting-b'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    /// Live behaviour: the aggregation follows the constant that records it.
    #[test]
    fn a_meeting_is_ranked_by_its_best_window_not_its_first() {
        assert_eq!(
            crate::corpus_window::AGGREGATION,
            "max-chunk-similarity",
            "this test asserts the aggregation the store contracts to"
        );
        let fixture = Fixture::new();
        let long = words(crate::corpus_window::WINDOW_WORDS + 5, "w");
        fixture.meeting("meeting-a", 100, &[&long]);
        fixture.meeting("meeting-b", 200, &["gamma delta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();

        let pending = index.pending_windows(&identity, 0).unwrap();
        assert_eq!(pending.len(), 3);
        let batch = vec![
            // meeting-a's first window is orthogonal to the question and its
            // second is the answer. A first-window ranker puts it last.
            offer(&pending[0], axis(5)),
            offer(&pending[1], axis(1)),
            offer(&pending[2], blend(5, 1, 0.6)),
        ];
        index.store_window_vectors(&identity, &batch).unwrap();

        let found = index.nearest_windows(&identity, &axis(1), 0).unwrap();
        assert_eq!(found.coverage.windows, 3);
        assert_eq!(found.coverage.embedded, 3);
        assert_eq!(
            found
                .hits
                .iter()
                .map(|hit| hit.meeting_id.as_str())
                .collect::<Vec<_>>(),
            vec!["meeting-a", "meeting-b"]
        );
        assert_eq!(found.hits[0].window_index, 1);
        assert!(found.hits[0].similarity > 0.99);
        assert!(!found.hits[0].segments.is_empty(), "a hit must be quotable");
    }

    #[test]
    fn a_ranking_reports_how_crowded_the_top_is() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        fixture.meeting("meeting-b", 200, &["beta"]);
        fixture.meeting("meeting-c", 300, &["gamma"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();

        // Two meetings inside the registered band of the top, one far outside.
        let batch = vec![
            offer(&pending[0], axis(1)),
            offer(&pending[1], blend(1, 2, 0.05)),
            offer(&pending[2], axis(2)),
        ];
        index.store_window_vectors(&identity, &batch).unwrap();

        let found = index.nearest_windows(&identity, &axis(1), 1).unwrap();
        assert_eq!(found.hits.len(), 1, "limit truncates the hits");
        assert_eq!(
            found.near_ties, 2,
            "the density count is taken before the limit, or it always reads 1"
        );
    }

    /// The state this ships in, and will stay in until an embedder exists. It
    /// is asserted in three documents, so it gets the test that fails if it
    /// changes: an empty ranking that says it searched nothing, not an error
    /// and not an empty ranking that reads like an answer.
    #[test]
    fn with_no_embedder_a_ranking_is_empty_and_says_so() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        fixture.meeting("meeting-b", 200, &["beta"]);
        let index = synced(&fixture);

        let found = index
            .nearest_windows(&EmbedderIdentity::measured(), &axis(1), 0)
            .expect("an unembedded corpus is a state, not a failure");
        assert!(found.hits.is_empty());
        assert_eq!(found.near_ties, 0);
        assert_eq!(
            (found.coverage.windows, found.coverage.embedded),
            (2, 0),
            "the store must be able to say how much it did not search"
        );
        assert!(!found.coverage.complete());
    }

    #[test]
    fn a_ranking_says_what_it_did_not_search() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        fixture.meeting("meeting-b", 200, &["beta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let pending = index.pending_windows(&identity, 0).unwrap();
        index
            .store_window_vectors(&identity, &[offer(&pending[0], axis(1))])
            .unwrap();

        let found = index.nearest_windows(&identity, &axis(1), 0).unwrap();
        assert_eq!(found.coverage.windows, 2);
        assert_eq!(found.coverage.embedded, 1);
        assert!(!found.coverage.complete());
        assert_eq!(found.hits.len(), 1);
    }

    #[test]
    fn a_question_of_the_wrong_width_is_refused_rather_than_scored() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha"]);
        let index = synced(&fixture);
        assert!(matches!(
            index.nearest_windows(&EmbedderIdentity::measured(), &[1.0, 0.0], 0),
            Err(CorpusIndexError::VectorRejected)
        ));
    }

    /// Windows are derived from files, so they belong in the rebuild digest.
    /// Vectors are not, so they must not — and both halves need a test, because
    /// getting either backwards is silent.
    #[test]
    fn the_rebuild_digest_covers_windows_and_deliberately_not_vectors() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["alpha beta"]);
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let before = index.fingerprint().unwrap();

        let pending = index.pending_windows(&identity, 0).unwrap();
        index
            .store_window_vectors(&identity, &[offer(&pending[0], axis(0))])
            .unwrap();
        assert_eq!(
            index.fingerprint().unwrap(),
            before,
            "a vector moved the digest, so a rebuild from files can never match"
        );

        index
            .connection
            .execute("UPDATE corpus_window SET word_count = word_count + 1", [])
            .unwrap();
        assert_ne!(
            index.fingerprint().unwrap(),
            before,
            "a window column is outside the digest, so nothing would catch it drifting"
        );
    }
}
