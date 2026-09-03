use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::storage::{durable_replace, sync_directory, StorageRoot};

const MAX_CATALOG_BYTES: u64 = 128 * 1024;
const MAX_RECEIPT_BYTES: u64 = 32 * 1024;
const ACTIVE_RECEIPT: &str = "models/active-model.json";
/// Separate from `ACTIVE_RECEIPT` by filename (not just by directory) so
/// activating a note model can never write over, or be confused with, the
/// active transcript-model pointer. See `note_models` below.
const ACTIVE_NOTE_RECEIPT: &str = "models/active-note-model.json";
pub const INSTALL_RECEIPT_NAME: &str = "model-install.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCatalog {
    pub schema: ModelCatalogSchema,
    pub models: Vec<TranscriptModel>,
    /// Note-generation models (MLX-LM weight trees: config + one-or-more
    /// safetensors shards + tokenizer files), added alongside the existing
    /// whisper `models` list rather than replacing it.
    ///
    /// This is a backward-compatible extension of `yawn-model-catalog/1`,
    /// not a schema bump to `/2`: `model-catalog.json` is staged from the
    /// repo into the app bundle and pinned by its own sha256 inside the
    /// runtime manifest (`build_contract.rs`, `RuntimeManifest::model_catalog`
    /// in `runtime.rs`), so the binary and its catalog always ship, and are
    /// digest-bound, together — there is no scenario where an old binary
    /// reads a catalog file written for a newer one. Contrast with
    /// `RuntimeManifest`'s V1→V2 split, which exists because V2 makes a
    /// previously-optional verification step (the catalog resource) required
    /// for every input; adding an empty-by-default `note_models` list changes
    /// no verification step for any existing catalog or manifest. `#[serde(default)]`
    /// means a catalog written before this field existed — or one that simply
    /// ships no note models yet, as this branch does — still deserializes,
    /// with `note_models` empty. See `docs/note-runtime-decision.md` seam 6.
    #[serde(default)]
    pub note_models: Vec<NoteModel>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum ModelCatalogSchema {
    #[serde(rename = "yawn-model-catalog/1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptModel {
    pub id: String,
    pub revision: String,
    pub title: String,
    pub detail: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub files: Vec<TranscriptModelFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptModelFile {
    pub role: TranscriptModelFileRole,
    pub name: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptModelFileRole {
    Config,
    Weights,
}

/// A note-generation model: an MLX-LM weight tree, distinct in shape from a
/// whisper `TranscriptModel`. This is a separate struct (not an extension of
/// `TranscriptModel`) so the existing transcript-model type, its `Deserialize`
/// impl, and every existing struct literal that builds one are untouched.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct NoteModel {
    pub id: String,
    pub revision: String,
    pub title: String,
    pub detail: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub files: Vec<NoteModelFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct NoteModelFile {
    pub role: NoteModelFileRole,
    pub name: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

/// File roles inside one MLX-LM model tree (an mlx-community snapshot).
///
/// Unlike whisper's exactly-one-config, exactly-one-weights layout, an
/// MLX-LM tree ships weights as one-or-more safetensors shards (a single
/// `model.safetensors` for a small model, or `model-00001-of-00002.safetensors`
/// alongside a sibling shard when the quantized weights don't fit one file),
/// plus separate tokenizer artifacts. `Weights` may therefore appear more
/// than once per model; every other role appears at most once. `WeightsIndex`
/// (`model.safetensors.index.json`) is only present — and only valid —
/// alongside a sharded `Weights` set, but validation does not hard-require
/// it or the tokenizer roles: the measurement gate has not yet picked the
/// final model pin, and a fixed exact-file-count rule here would be a second
/// place that pin has to be re-litigated. `validate()` still requires
/// `Config` exactly once and `Weights` at least once — the minimum any MLX-LM
/// load needs — and pins every present file's name to its role.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum NoteModelFileRole {
    Config,
    Weights,
    WeightsIndex,
    Tokenizer,
    TokenizerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveNoteModelReceipt {
    schema: ActiveNoteModelSchema,
    id: String,
    revision: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum ActiveNoteModelSchema {
    #[serde(rename = "yawn-active-note-model/1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstalledNoteModelReceipt {
    schema: InstalledNoteModelSchema,
    id: String,
    revision: String,
    files: Vec<InstalledNoteModelFile>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum InstalledNoteModelSchema {
    #[serde(rename = "yawn-note-model-install/1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstalledNoteModelFile {
    role: NoteModelFileRole,
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone)]
pub struct InstalledNoteModel {
    pub entry: NoteModel,
    pub directory: PathBuf,
    pub receipt_path: PathBuf,
}

/// Borrowed, role-erased view of one catalog file. `model_download.rs::install`
/// only ever reads a file's name, url, byte count, and sha256 — never its
/// role — so this is the entire surface that function needs.
pub struct DownloadableFile<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub bytes: u64,
    pub sha256: &'a str,
}

/// What `model_download.rs::install` needs from a catalog entry, independent
/// of whether it is a `TranscriptModel` or a `NoteModel`.
///
/// `install()` itself is not generified by this trait on this branch — no
/// caller downloads a note model yet (that lands with the note-download
/// command, out of scope here). But every call `install()`'s body makes that
/// differs by model kind is covered by a method here, so the claim that it
/// is reusable is verified by the impls and `downloadable_model_reuse_is_verified_not_assumed`
/// (in tests) below rather than merely asserted:
///
/// - the download loop itself (streaming, byte-exactness, digest-per-file)
///   reads only `id`, `revision`, `download_bytes`, and `files` — already
///   kind-agnostic before this trait existed;
/// - `install_receipt_bytes(model)` → `model.receipt_bytes()`;
/// - `verify_model_directory(&staging, model)` → `model.verify_directory(&staging)`;
/// - `activate_model(storage, model)` → `model.activate(storage)`;
/// - the hardcoded `models/{id}/{revision}` staging path → `model.relative_path()`.
///
/// With those five substitutions, `install()`'s body is unchanged line for
/// line; only its parameter type moves from `&TranscriptModel` to
/// How much of an installed model directory a caller needs proved.
///
/// `Contents` is the original behaviour and the only one that may gate a run:
/// every file is re-hashed against the receipt. It costs a full read of the
/// model, which for the 8 GB note model measured about 5 s on this machine.
///
/// `Metadata` answers the cheaper question a UI asks -- is a usable model
/// installed here -- from the receipt, the exact file set, and each file's
/// size. It catches the failures that actually happen to an install (a
/// missing shard, a half-written directory, the wrong revision) without
/// reading a byte of weights. It is not a substitute for `Contents`: a
/// caller about to *run* the model must still prove the bytes, and
/// `admit_note_generator` does exactly that on the generate path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelVerification {
    Metadata,
    Contents,
}

/// `&impl DownloadableModel`.
pub trait DownloadableModel {
    fn id(&self) -> &str;
    fn revision(&self) -> &str;
    fn download_bytes(&self) -> u64;
    fn files(&self) -> Vec<DownloadableFile<'_>>;
    /// Where this model installs, relative to the storage root.
    fn relative_path(&self) -> PathBuf;
    /// The install-receipt bytes to write once every file's digest is confirmed.
    fn receipt_bytes(&self) -> Vec<u8>;
    /// Verifies an installed (or freshly staged) directory against this entry.
    fn verify_directory(
        &self,
        directory: &Path,
        verification: ModelVerification,
    ) -> Result<(), ModelStoreError>;
    /// Marks this model the active one of its own kind.
    fn activate(&self, storage: &StorageRoot) -> Result<(), ModelStoreError>;
}

impl DownloadableModel for TranscriptModel {
    fn id(&self) -> &str {
        &self.id
    }
    fn revision(&self) -> &str {
        &self.revision
    }
    fn download_bytes(&self) -> u64 {
        self.download_bytes
    }
    fn files(&self) -> Vec<DownloadableFile<'_>> {
        self.files
            .iter()
            .map(|file| DownloadableFile {
                name: &file.name,
                url: &file.url,
                bytes: file.bytes,
                sha256: &file.sha256,
            })
            .collect()
    }
    fn relative_path(&self) -> PathBuf {
        Path::new("models").join(&self.id).join(&self.revision)
    }
    fn receipt_bytes(&self) -> Vec<u8> {
        install_receipt_bytes(self)
    }
    fn verify_directory(
        &self,
        directory: &Path,
        verification: ModelVerification,
    ) -> Result<(), ModelStoreError> {
        verify_model_directory(directory, self, verification)
    }
    fn activate(&self, storage: &StorageRoot) -> Result<(), ModelStoreError> {
        activate_model(storage, self)
    }
}

impl DownloadableModel for NoteModel {
    fn id(&self) -> &str {
        &self.id
    }
    fn revision(&self) -> &str {
        &self.revision
    }
    fn download_bytes(&self) -> u64 {
        self.download_bytes
    }
    fn files(&self) -> Vec<DownloadableFile<'_>> {
        self.files
            .iter()
            .map(|file| DownloadableFile {
                name: &file.name,
                url: &file.url,
                bytes: file.bytes,
                sha256: &file.sha256,
            })
            .collect()
    }
    fn relative_path(&self) -> PathBuf {
        note_model_relative_path(&self.id, &self.revision)
    }
    fn receipt_bytes(&self) -> Vec<u8> {
        note_install_receipt_bytes(self)
    }
    fn verify_directory(
        &self,
        directory: &Path,
        verification: ModelVerification,
    ) -> Result<(), ModelStoreError> {
        verify_note_model_directory(directory, self, verification)
    }
    fn activate(&self, storage: &StorageRoot) -> Result<(), ModelStoreError> {
        activate_note_model(storage, self)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveModelReceipt {
    schema: ActiveModelSchema,
    id: String,
    revision: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum ActiveModelSchema {
    #[serde(rename = "yawn-active-model/1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstalledModelReceipt {
    schema: InstalledModelSchema,
    id: String,
    revision: String,
    files: Vec<InstalledModelFile>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum InstalledModelSchema {
    #[serde(rename = "yawn-model-install/1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstalledModelFile {
    role: TranscriptModelFileRole,
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone)]
pub struct InstalledTranscriptModel {
    pub entry: TranscriptModel,
    pub directory: PathBuf,
    pub receipt_path: PathBuf,
}

impl InstalledTranscriptModel {
    pub fn runtime_models(&self) -> Vec<(String, String)> {
        self.entry
            .files
            .iter()
            .map(|file| {
                let id = match file.role {
                    TranscriptModelFileRole::Config => "whisper-transcript-config",
                    TranscriptModelFileRole::Weights => "whisper-transcript-weights",
                };
                (id.to_string(), file.sha256.clone())
            })
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum ModelStoreError {
    #[error("model catalog is missing, malformed, or changed")]
    InvalidCatalog,
    #[error("model catalog contains an unsafe or unsupported entry")]
    InvalidCatalogEntry,
    #[error("selected model is not in the signed catalog")]
    UnknownModel,
    #[error("installed model receipt is missing, malformed, or changed")]
    InvalidReceipt,
    #[error("installed model is missing, unsafe, or changed")]
    InvalidModel,
    #[error("the active model cannot be removed")]
    ActiveModel,
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl ModelCatalog {
    pub fn load_and_verify(path: &Path, expected_sha256: &str) -> Result<Self, ModelStoreError> {
        if path.is_symlink() || !path.is_file() || !valid_sha256(expected_sha256) {
            return Err(ModelStoreError::InvalidCatalog);
        }
        let metadata = path.metadata()?;
        if metadata.len() > MAX_CATALOG_BYTES || file_sha256(path)? != expected_sha256 {
            return Err(ModelStoreError::InvalidCatalog);
        }
        let catalog: Self = serde_json::from_slice(&fs::read(path)?)
            .map_err(|_| ModelStoreError::InvalidCatalog)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), ModelStoreError> {
        if self.models.is_empty() || self.models.len() > 8 {
            return Err(ModelStoreError::InvalidCatalogEntry);
        }
        let mut ids = HashSet::new();
        for model in &self.models {
            if !safe_component(&model.id)
                || !safe_revision(&model.revision)
                || model.title.trim().is_empty()
                || model.title.len() > 80
                || model.detail.trim().is_empty()
                || model.detail.len() > 240
                || model.download_bytes == 0
                || model.installed_bytes == 0
                || !ids.insert(&model.id)
            {
                return Err(ModelStoreError::InvalidCatalogEntry);
            }
            let mut roles = HashSet::new();
            let mut installed_bytes = 0_u64;
            for file in &model.files {
                if !roles.insert(file.role)
                    || !matches!(
                        (file.role, file.name.as_str()),
                        (TranscriptModelFileRole::Config, "config.json")
                            | (TranscriptModelFileRole::Weights, "weights.safetensors")
                            | (TranscriptModelFileRole::Weights, "weights.npz")
                    )
                    || !file.url.starts_with("https://")
                    || file.url.len() > 2048
                    || !file.url.contains(&model.revision)
                    || !file.url.ends_with(&file.name)
                    || file.bytes == 0
                    || !valid_sha256(&file.sha256)
                {
                    return Err(ModelStoreError::InvalidCatalogEntry);
                }
                installed_bytes = installed_bytes
                    .checked_add(file.bytes)
                    .ok_or(ModelStoreError::InvalidCatalogEntry)?;
            }
            if roles
                != HashSet::from([
                    TranscriptModelFileRole::Config,
                    TranscriptModelFileRole::Weights,
                ])
                || installed_bytes != model.installed_bytes
                || model.download_bytes != model.installed_bytes
            {
                return Err(ModelStoreError::InvalidCatalogEntry);
            }
        }
        self.validate_note_models()?;
        Ok(())
    }

    /// Note-model counterpart of the loop above. Kept as its own pass rather
    /// than folded into the transcript loop, both so the transcript rules
    /// stay textually and behaviorally untouched and so an MLX file tree
    /// (sharded weights, tokenizer files) can use validation shaped for it
    /// instead of whisper's exactly-one-config-one-weights rule.
    fn validate_note_models(&self) -> Result<(), ModelStoreError> {
        if self.note_models.len() > 8 {
            return Err(ModelStoreError::InvalidCatalogEntry);
        }
        let mut ids = HashSet::new();
        for model in &self.note_models {
            if !safe_component(&model.id)
                || !safe_revision(&model.revision)
                || model.title.trim().is_empty()
                || model.title.len() > 80
                || model.detail.trim().is_empty()
                || model.detail.len() > 240
                || model.download_bytes == 0
                || model.installed_bytes == 0
                || !ids.insert(&model.id)
            {
                return Err(ModelStoreError::InvalidCatalogEntry);
            }
            let mut names = HashSet::new();
            let mut config_count = 0_u32;
            let mut weights_count = 0_u32;
            let mut weights_index_count = 0_u32;
            let mut tokenizer_count = 0_u32;
            let mut tokenizer_config_count = 0_u32;
            let mut installed_bytes = 0_u64;
            for file in &model.files {
                if !names.insert(file.name.as_str())
                    || !file.url.starts_with("https://")
                    || file.url.len() > 2048
                    || !file.url.contains(&model.revision)
                    || !file.url.ends_with(&file.name)
                    || file.bytes == 0
                    || !valid_sha256(&file.sha256)
                {
                    return Err(ModelStoreError::InvalidCatalogEntry);
                }
                let name_ok = match file.role {
                    NoteModelFileRole::Config => {
                        config_count += 1;
                        file.name == "config.json"
                    }
                    NoteModelFileRole::Weights => {
                        weights_count += 1;
                        safe_note_weights_name(&file.name)
                    }
                    NoteModelFileRole::WeightsIndex => {
                        weights_index_count += 1;
                        file.name == "model.safetensors.index.json"
                    }
                    NoteModelFileRole::Tokenizer => {
                        tokenizer_count += 1;
                        file.name == "tokenizer.json"
                    }
                    NoteModelFileRole::TokenizerConfig => {
                        tokenizer_config_count += 1;
                        file.name == "tokenizer_config.json"
                    }
                };
                if !name_ok {
                    return Err(ModelStoreError::InvalidCatalogEntry);
                }
                installed_bytes = installed_bytes
                    .checked_add(file.bytes)
                    .ok_or(ModelStoreError::InvalidCatalogEntry)?;
            }
            // A sharded weight set (more than one `Weights` file) requires
            // exactly one index, matching the doc comment on
            // `NoteModelFileRole`: `mlx_lm.load` needs the index to map
            // tensors across shards, and a single-file model has no shards
            // for an index to describe.
            let index_matches_sharding = if weights_count > 1 {
                weights_index_count == 1
            } else {
                weights_index_count == 0
            };
            if config_count != 1
                || weights_count == 0
                || !index_matches_sharding
                || tokenizer_count > 1
                || tokenizer_config_count > 1
                || installed_bytes != model.installed_bytes
                || model.download_bytes != model.installed_bytes
            {
                return Err(ModelStoreError::InvalidCatalogEntry);
            }
        }
        Ok(())
    }

    pub fn model(&self, id: &str) -> Result<&TranscriptModel, ModelStoreError> {
        self.models
            .iter()
            .find(|model| model.id == id)
            .ok_or(ModelStoreError::UnknownModel)
    }

    pub fn note_model(&self, id: &str) -> Result<&NoteModel, ModelStoreError> {
        self.note_models
            .iter()
            .find(|model| model.id == id)
            .ok_or(ModelStoreError::UnknownModel)
    }
}

pub fn installed_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
) -> Result<Option<InstalledTranscriptModel>, ModelStoreError> {
    let Some(entry) = active_model(storage, catalog)? else {
        return Ok(None);
    };
    let relative = Path::new("models").join(&entry.id).join(&entry.revision);
    let directory = storage.resolve(&relative).map_err(io::Error::other)?;
    verify_model_directory(&directory, &entry, ModelVerification::Contents)?;
    Ok(Some(InstalledTranscriptModel {
        receipt_path: directory.join(INSTALL_RECEIPT_NAME),
        entry,
        directory,
    }))
}

pub fn active_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
) -> Result<Option<TranscriptModel>, ModelStoreError> {
    let active_path = storage
        .resolve(Path::new(ACTIVE_RECEIPT))
        .map_err(io::Error::other)?;
    if !active_path.exists() {
        return Ok(None);
    }
    let active: ActiveModelReceipt = read_json(&active_path, MAX_RECEIPT_BYTES)?;
    let entry = catalog.model(&active.id)?;
    if active.revision != entry.revision {
        return Err(ModelStoreError::InvalidReceipt);
    }
    Ok(Some(entry.clone()))
}

pub fn model_is_stored(
    storage: &StorageRoot,
    entry: &TranscriptModel,
) -> Result<bool, ModelStoreError> {
    let directory = storage
        .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
        .map_err(io::Error::other)?;
    if !directory.exists() {
        return Ok(false);
    }
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    Ok(true)
}

pub fn remove_inactive_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
    id: &str,
) -> Result<bool, ModelStoreError> {
    let entry = catalog.model(id)?;
    if active_model(storage, catalog)?
        .as_ref()
        .is_some_and(|active| active.id == entry.id)
    {
        return Err(ModelStoreError::ActiveModel);
    }
    let models = storage
        .resolve(Path::new("models"))
        .map_err(io::Error::other)?;
    let parent = storage
        .resolve(&Path::new("models").join(&entry.id))
        .map_err(io::Error::other)?;
    let directory = storage
        .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
        .map_err(io::Error::other)?;
    if !directory.exists() {
        return Ok(false);
    }
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    fs::remove_dir_all(&directory)?;
    sync_directory(&parent)?;
    sync_directory(&models)?;
    Ok(true)
}

pub fn verify_model_directory(
    directory: &Path,
    entry: &TranscriptModel,
    verification: ModelVerification,
) -> Result<(), ModelStoreError> {
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    let receipt_path = directory.join(INSTALL_RECEIPT_NAME);
    let receipt: InstalledModelReceipt = read_json(&receipt_path, MAX_RECEIPT_BYTES)?;
    if receipt.id != entry.id || receipt.revision != entry.revision {
        return Err(ModelStoreError::InvalidReceipt);
    }
    let expected_files: HashSet<_> = entry.files.iter().map(|file| file.name.as_str()).collect();
    let actual_names: HashSet<_> = fs::read_dir(directory)?
        .map(|item| item.map(|item| item.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    let mut expected_names: HashSet<String> = expected_files
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected_names.insert(INSTALL_RECEIPT_NAME.to_string());
    if actual_names != expected_names || receipt.files.len() != entry.files.len() {
        return Err(ModelStoreError::InvalidModel);
    }
    for expected in &entry.files {
        let recorded = receipt
            .files
            .iter()
            .find(|file| file.role == expected.role)
            .ok_or(ModelStoreError::InvalidReceipt)?;
        if recorded.name != expected.name
            || recorded.bytes != expected.bytes
            || recorded.sha256 != expected.sha256
        {
            return Err(ModelStoreError::InvalidReceipt);
        }
        let path = directory.join(&expected.name);
        if path.is_symlink()
            || !path.is_file()
            || path.metadata()?.len() != expected.bytes
        {
            return Err(ModelStoreError::InvalidModel);
        }
        if verification == ModelVerification::Contents && file_sha256(&path)? != expected.sha256 {
            return Err(ModelStoreError::InvalidModel);
        }
    }
    Ok(())
}

pub fn install_receipt_bytes(entry: &TranscriptModel) -> Vec<u8> {
    let receipt = InstalledModelReceipt {
        schema: InstalledModelSchema::V1,
        id: entry.id.clone(),
        revision: entry.revision.clone(),
        files: entry
            .files
            .iter()
            .map(|file| InstalledModelFile {
                role: file.role,
                name: file.name.clone(),
                bytes: file.bytes,
                sha256: file.sha256.clone(),
            })
            .collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&receipt).expect("model receipt serializes");
    bytes.push(b'\n');
    bytes
}

pub fn activate_model(
    storage: &StorageRoot,
    entry: &TranscriptModel,
) -> Result<(), ModelStoreError> {
    let directory = storage
        .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
        .map_err(io::Error::other)?;
    verify_model_directory(&directory, entry, ModelVerification::Contents)?;
    let active = ActiveModelReceipt {
        schema: ActiveModelSchema::V1,
        id: entry.id.clone(),
        revision: entry.revision.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&active).expect("active receipt serializes");
    bytes.push(b'\n');
    let path = storage
        .resolve(Path::new(ACTIVE_RECEIPT))
        .map_err(io::Error::other)?;
    durable_replace(&path, &bytes)?;
    Ok(())
}

/// Storage segment under `models/` reserved for note-model directories,
/// disjoint from the transcript-model layout (`models/{id}/{revision}`).
/// `safe_component` — the same validator every catalog id must pass — allows
/// only `[A-Za-z0-9_-]`, so no valid transcript- or note-model id can ever
/// equal this segment (it contains `.`) and collide with it on disk.
const NOTE_MODEL_DIR: &str = "note.d";

fn note_model_relative_path(id: &str, revision: &str) -> PathBuf {
    Path::new("models")
        .join(NOTE_MODEL_DIR)
        .join(id)
        .join(revision)
}

pub fn installed_note_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
) -> Result<Option<InstalledNoteModel>, ModelStoreError> {
    let Some(entry) = active_note_model(storage, catalog)? else {
        return Ok(None);
    };
    let directory = storage
        .resolve(&note_model_relative_path(&entry.id, &entry.revision))
        .map_err(io::Error::other)?;
    verify_note_model_directory(&directory, &entry, ModelVerification::Contents)?;
    Ok(Some(InstalledNoteModel {
        receipt_path: directory.join(INSTALL_RECEIPT_NAME),
        entry,
        directory,
    }))
}

/// The active note-generation model, read from its own receipt
/// (`ACTIVE_NOTE_RECEIPT`). Deliberately independent of `active_model` above:
/// activating a note model never touches `ACTIVE_RECEIPT`, so the active
/// transcript model is never disturbed by a note-model activation, and vice
/// versa.
pub fn active_note_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
) -> Result<Option<NoteModel>, ModelStoreError> {
    let active_path = storage
        .resolve(Path::new(ACTIVE_NOTE_RECEIPT))
        .map_err(io::Error::other)?;
    if !active_path.exists() {
        return Ok(None);
    }
    let active: ActiveNoteModelReceipt = read_json(&active_path, MAX_RECEIPT_BYTES)?;
    let entry = catalog.note_model(&active.id)?;
    if active.revision != entry.revision {
        return Err(ModelStoreError::InvalidReceipt);
    }
    Ok(Some(entry.clone()))
}

pub fn note_model_is_stored(
    storage: &StorageRoot,
    entry: &NoteModel,
) -> Result<bool, ModelStoreError> {
    let directory = storage
        .resolve(&note_model_relative_path(&entry.id, &entry.revision))
        .map_err(io::Error::other)?;
    if !directory.exists() {
        return Ok(false);
    }
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    Ok(true)
}

pub fn remove_inactive_note_model(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
    id: &str,
) -> Result<bool, ModelStoreError> {
    let entry = catalog.note_model(id)?;
    if active_note_model(storage, catalog)?
        .as_ref()
        .is_some_and(|active| active.id == entry.id)
    {
        return Err(ModelStoreError::ActiveModel);
    }
    let namespace = storage
        .resolve(&Path::new("models").join(NOTE_MODEL_DIR))
        .map_err(io::Error::other)?;
    let parent = storage
        .resolve(&Path::new("models").join(NOTE_MODEL_DIR).join(&entry.id))
        .map_err(io::Error::other)?;
    let directory = storage
        .resolve(&note_model_relative_path(&entry.id, &entry.revision))
        .map_err(io::Error::other)?;
    if !directory.exists() {
        return Ok(false);
    }
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    fs::remove_dir_all(&directory)?;
    sync_directory(&parent)?;
    sync_directory(&namespace)?;
    Ok(true)
}

/// Note-model counterpart of `verify_model_directory`. The one structural
/// difference: a transcript model's receipt file is looked up by `role`
/// because each role appears at most once; a note model's `Weights` role can
/// appear on several shards, so the lookup key here is the (validated-unique)
/// file `name` instead.
pub fn verify_note_model_directory(
    directory: &Path,
    entry: &NoteModel,
    verification: ModelVerification,
) -> Result<(), ModelStoreError> {
    if directory.is_symlink() || !directory.is_dir() {
        return Err(ModelStoreError::InvalidModel);
    }
    let receipt_path = directory.join(INSTALL_RECEIPT_NAME);
    let receipt: InstalledNoteModelReceipt = read_json(&receipt_path, MAX_RECEIPT_BYTES)?;
    if receipt.id != entry.id || receipt.revision != entry.revision {
        return Err(ModelStoreError::InvalidReceipt);
    }
    let expected_files: HashSet<_> = entry.files.iter().map(|file| file.name.as_str()).collect();
    let actual_names: HashSet<_> = fs::read_dir(directory)?
        .map(|item| item.map(|item| item.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    let mut expected_names: HashSet<String> = expected_files
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected_names.insert(INSTALL_RECEIPT_NAME.to_string());
    if actual_names != expected_names || receipt.files.len() != entry.files.len() {
        return Err(ModelStoreError::InvalidModel);
    }
    for expected in &entry.files {
        let recorded = receipt
            .files
            .iter()
            .find(|file| file.name == expected.name)
            .ok_or(ModelStoreError::InvalidReceipt)?;
        if recorded.role != expected.role
            || recorded.bytes != expected.bytes
            || recorded.sha256 != expected.sha256
        {
            return Err(ModelStoreError::InvalidReceipt);
        }
        let path = directory.join(&expected.name);
        if path.is_symlink()
            || !path.is_file()
            || path.metadata()?.len() != expected.bytes
        {
            return Err(ModelStoreError::InvalidModel);
        }
        if verification == ModelVerification::Contents && file_sha256(&path)? != expected.sha256 {
            return Err(ModelStoreError::InvalidModel);
        }
    }
    Ok(())
}

pub fn note_install_receipt_bytes(entry: &NoteModel) -> Vec<u8> {
    let receipt = InstalledNoteModelReceipt {
        schema: InstalledNoteModelSchema::V1,
        id: entry.id.clone(),
        revision: entry.revision.clone(),
        files: entry
            .files
            .iter()
            .map(|file| InstalledNoteModelFile {
                role: file.role,
                name: file.name.clone(),
                bytes: file.bytes,
                sha256: file.sha256.clone(),
            })
            .collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&receipt).expect("note model receipt serializes");
    bytes.push(b'\n');
    bytes
}

/// Note-model counterpart of `activate_model`. Writes only
/// `ACTIVE_NOTE_RECEIPT`, so it can never disturb `ACTIVE_RECEIPT` — the two
/// pointers are separate files, not separate keys in one file, precisely so
/// this property holds without a runtime check.
pub fn activate_note_model(
    storage: &StorageRoot,
    entry: &NoteModel,
) -> Result<(), ModelStoreError> {
    let directory = storage
        .resolve(&note_model_relative_path(&entry.id, &entry.revision))
        .map_err(io::Error::other)?;
    verify_note_model_directory(&directory, entry, ModelVerification::Contents)?;
    let active = ActiveNoteModelReceipt {
        schema: ActiveNoteModelSchema::V1,
        id: entry.id.clone(),
        revision: entry.revision.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&active).expect("active note receipt serializes");
    bytes.push(b'\n');
    let path = storage
        .resolve(Path::new(ACTIVE_NOTE_RECEIPT))
        .map_err(io::Error::other)?;
    durable_replace(&path, &bytes)?;
    Ok(())
}

/// Clears the active-note-model pointer, leaving the installed tree intact.
///
/// Note generation is optional in a way transcription is not, so "no active
/// note model" is an ordinary state the reader already renders as an honest
/// refusal — this is what makes removing the only note model expressible:
/// deactivate, then `remove_inactive_note_model`. Touches only
/// `ACTIVE_NOTE_RECEIPT`; the transcript pointer is a separate file by design.
pub fn deactivate_note_model(storage: &StorageRoot) -> Result<(), ModelStoreError> {
    let path = storage
        .resolve(Path::new(ACTIVE_NOTE_RECEIPT))
        .map_err(io::Error::other)?;
    if path.is_symlink() || (path.exists() && !path.is_file()) {
        return Err(ModelStoreError::InvalidReceipt);
    }
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    max_bytes: u64,
) -> Result<T, ModelStoreError> {
    if path.is_symlink() || !path.is_file() || path.metadata()?.len() > max_bytes {
        return Err(ModelStoreError::InvalidReceipt);
    }
    serde_json::from_slice(&fs::read(path)?).map_err(|_| ModelStoreError::InvalidReceipt)
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn safe_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A sharded MLX weights filename: `model.safetensors`,
/// `model-00001-of-00002.safetensors`, and so on. Same character allowlist
/// as `safe_component` plus `.`, which the shard suffix and file extension
/// both need; still rejects path separators, `..`, and anything else that
/// would leave the model directory.
fn safe_note_weights_name(name: &str) -> bool {
    name.ends_with(".safetensors")
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn file_sha256(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{create_private_dir, durable_create_new};
    use tempfile::TempDir;

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn fixture() -> (TempDir, StorageRoot, ModelCatalog) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        create_private_dir(&repo).unwrap();
        let storage = StorageRoot::create(&temp.path().join("app"), &repo).unwrap();
        let config = b"config";
        let weights = b"weights";
        let catalog = ModelCatalog {
            schema: ModelCatalogSchema::V1,
            models: vec![TranscriptModel {
                id: "whisper-test-q4".into(),
                revision: "a".repeat(40),
                title: "Smaller download".into(),
                detail: "A smaller local model.".into(),
                download_bytes: (config.len() + weights.len()) as u64,
                installed_bytes: (config.len() + weights.len()) as u64,
                files: vec![
                    TranscriptModelFile {
                        role: TranscriptModelFileRole::Config,
                        name: "config.json".into(),
                        url: format!("https://example.test/{}/config.json", "a".repeat(40)),
                        bytes: config.len() as u64,
                        sha256: digest(config),
                    },
                    TranscriptModelFile {
                        role: TranscriptModelFileRole::Weights,
                        name: "weights.npz".into(),
                        url: format!("https://example.test/{}/weights.npz", "a".repeat(40)),
                        bytes: weights.len() as u64,
                        sha256: digest(weights),
                    },
                ],
            }],
            // Mechanical compile-fix for the new field, not a behavioral
            // change: every existing assertion in this module is about
            // `catalog.models`, and stays byte-for-byte as written.
            note_models: Vec::new(),
        };
        catalog.validate().unwrap();
        (temp, storage, catalog)
    }

    fn write_installed_model(storage: &StorageRoot, entry: &TranscriptModel) -> PathBuf {
        let directory = storage
            .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
            .unwrap();
        create_private_dir(&directory).unwrap();
        durable_create_new(&directory.join("config.json"), b"config").unwrap();
        durable_create_new(&directory.join(&entry.files[1].name), b"weights").unwrap();
        durable_create_new(
            &directory.join(INSTALL_RECEIPT_NAME),
            &install_receipt_bytes(entry),
        )
        .unwrap();
        directory
    }

    #[test]
    fn verifies_and_activates_only_a_complete_catalog_model() {
        let (_temp, storage, catalog) = fixture();
        let entry = &catalog.models[0];
        let directory = storage
            .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
            .unwrap();
        create_private_dir(&directory).unwrap();
        durable_create_new(&directory.join("config.json"), b"config").unwrap();
        durable_create_new(&directory.join("weights.npz"), b"weights").unwrap();
        durable_create_new(
            &directory.join(INSTALL_RECEIPT_NAME),
            &install_receipt_bytes(entry),
        )
        .unwrap();
        activate_model(&storage, entry).unwrap();
        let installed = installed_model(&storage, &catalog).unwrap().unwrap();
        assert_eq!(installed.entry.id, entry.id);
        assert_eq!(installed.directory, directory);
        assert_eq!(installed.runtime_models().len(), 2);
    }

    #[test]
    fn refuses_changed_weights_and_unexpected_files() {
        let (_temp, storage, catalog) = fixture();
        let entry = &catalog.models[0];
        let directory = storage
            .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
            .unwrap();
        create_private_dir(&directory).unwrap();
        durable_create_new(&directory.join("config.json"), b"config").unwrap();
        durable_create_new(&directory.join("weights.npz"), b"changed").unwrap();
        durable_create_new(
            &directory.join(INSTALL_RECEIPT_NAME),
            &install_receipt_bytes(entry),
        )
        .unwrap();
        assert!(matches!(
            verify_model_directory(&directory, entry, ModelVerification::Contents),
            Err(ModelStoreError::InvalidModel)
        ));
        fs::write(directory.join("weights.npz"), b"weights").unwrap();
        durable_create_new(&directory.join("extra"), b"unexpected").unwrap();
        assert!(matches!(
            verify_model_directory(&directory, entry, ModelVerification::Contents),
            Err(ModelStoreError::InvalidModel)
        ));
    }

    #[test]
    fn metadata_verification_skips_content_hashes_but_still_catches_a_broken_install() {
        // D-OPENFREEZE: the UI's availability check runs on every meeting
        // open, so it asks the cheap question. Wrong bytes at the right size
        // are admitted here on purpose -- only a run may claim the weights
        // are intact, and the generate path still passes Contents.
        let (_temp, storage, catalog) = fixture();
        let entry = &catalog.models[0];
        let directory = storage
            .resolve(&Path::new("models").join(&entry.id).join(&entry.revision))
            .unwrap();
        create_private_dir(&directory).unwrap();
        durable_create_new(&directory.join("config.json"), b"config").unwrap();
        durable_create_new(&directory.join("weights.npz"), b"changed").unwrap();
        durable_create_new(
            &directory.join(INSTALL_RECEIPT_NAME),
            &install_receipt_bytes(entry),
        )
        .unwrap();
        assert!(
            verify_model_directory(&directory, entry, ModelVerification::Metadata).is_ok(),
            "same-size wrong bytes pass the metadata check"
        );
        assert!(matches!(
            verify_model_directory(&directory, entry, ModelVerification::Contents),
            Err(ModelStoreError::InvalidModel)
        ));

        // The failures an install actually produces are still refused at
        // metadata depth: a missing file, and a truncated one.
        fs::remove_file(directory.join("weights.npz")).unwrap();
        assert!(matches!(
            verify_model_directory(&directory, entry, ModelVerification::Metadata),
            Err(ModelStoreError::InvalidModel)
        ));
        fs::write(directory.join("weights.npz"), b"short").unwrap();
        assert!(matches!(
            verify_model_directory(&directory, entry, ModelVerification::Metadata),
            Err(ModelStoreError::InvalidModel)
        ));
    }

    #[test]
    fn removes_only_an_inactive_catalog_model() {
        let (_temp, storage, mut catalog) = fixture();
        let mut other = catalog.models[0].clone();
        other.id = "whisper-test-full".into();
        other.revision = "b".repeat(40);
        for file in &mut other.files {
            file.url = format!("https://example.test/{}/{}", other.revision, file.name);
        }
        catalog.models.push(other.clone());
        catalog.validate().unwrap();

        let active = &catalog.models[0];
        let active_directory = write_installed_model(&storage, active);
        let other_directory = write_installed_model(&storage, &other);
        activate_model(&storage, active).unwrap();

        assert!(model_is_stored(&storage, active).unwrap());
        assert!(model_is_stored(&storage, &other).unwrap());
        assert!(matches!(
            remove_inactive_model(&storage, &catalog, &active.id),
            Err(ModelStoreError::ActiveModel)
        ));
        assert!(active_directory.exists());
        assert!(remove_inactive_model(&storage, &catalog, &other.id).unwrap());
        assert!(!other_directory.exists());
        assert!(!remove_inactive_model(&storage, &catalog, &other.id).unwrap());
    }

    // --- note-model fixtures -------------------------------------------------
    //
    // A note model is pushed onto the same `fixture()` catalog used above
    // rather than built standalone, because `ModelCatalog::validate` requires
    // `models` to be non-empty — matching the real shipped catalog, which
    // always carries the whisper entries and would carry note entries
    // alongside them, never instead of them.

    fn push_note_fixture(catalog: &mut ModelCatalog) -> NoteModel {
        let revision = "c".repeat(40);
        let files: [(NoteModelFileRole, &str, &[u8]); 6] = [
            (NoteModelFileRole::Config, "config.json", b"note-config"),
            (
                NoteModelFileRole::Weights,
                "model-00001-of-00002.safetensors",
                b"shard-0",
            ),
            (
                NoteModelFileRole::Weights,
                "model-00002-of-00002.safetensors",
                b"shard-1",
            ),
            (
                NoteModelFileRole::WeightsIndex,
                "model.safetensors.index.json",
                b"index",
            ),
            (NoteModelFileRole::Tokenizer, "tokenizer.json", b"tokenizer"),
            (
                NoteModelFileRole::TokenizerConfig,
                "tokenizer_config.json",
                b"tokenizer-config",
            ),
        ];
        let total: u64 = files.iter().map(|(_, _, bytes)| bytes.len() as u64).sum();
        let note = NoteModel {
            id: "note-test-mlx".into(),
            revision: revision.clone(),
            title: "Test note model".into(),
            detail: "A fixture MLX note-generation model.".into(),
            download_bytes: total,
            installed_bytes: total,
            files: files
                .iter()
                .map(|(role, name, bytes)| NoteModelFile {
                    role: *role,
                    name: (*name).to_string(),
                    url: format!("https://example.test/{revision}/{name}"),
                    bytes: bytes.len() as u64,
                    sha256: digest(bytes),
                })
                .collect(),
        };
        catalog.note_models.push(note.clone());
        catalog.validate().unwrap();
        note
    }

    fn write_installed_note_model(storage: &StorageRoot, entry: &NoteModel) -> PathBuf {
        let directory = storage
            .resolve(&note_model_relative_path(&entry.id, &entry.revision))
            .unwrap();
        create_private_dir(&directory).unwrap();
        let contents: [(&str, &[u8]); 6] = [
            ("config.json", b"note-config"),
            ("model-00001-of-00002.safetensors", b"shard-0"),
            ("model-00002-of-00002.safetensors", b"shard-1"),
            ("model.safetensors.index.json", b"index"),
            ("tokenizer.json", b"tokenizer"),
            ("tokenizer_config.json", b"tokenizer-config"),
        ];
        for (name, bytes) in contents {
            durable_create_new(&directory.join(name), bytes).unwrap();
        }
        durable_create_new(
            &directory.join(INSTALL_RECEIPT_NAME),
            &note_install_receipt_bytes(entry),
        )
        .unwrap();
        directory
    }

    #[test]
    fn verifies_and_activates_a_note_model_independently_of_the_transcript_model() {
        let (_temp, storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let transcript = catalog.models[0].clone();

        let transcript_directory = write_installed_model(&storage, &transcript);
        activate_model(&storage, &transcript).unwrap();
        let note_directory = write_installed_note_model(&storage, &note);
        activate_note_model(&storage, &note).unwrap();

        let installed_transcript = installed_model(&storage, &catalog).unwrap().unwrap();
        assert_eq!(installed_transcript.directory, transcript_directory);
        let installed_note = installed_note_model(&storage, &catalog).unwrap().unwrap();
        assert_eq!(installed_note.entry.id, note.id);
        assert_eq!(installed_note.directory, note_directory);
        assert_eq!(installed_note.entry.files.len(), 6);
        assert_ne!(installed_note.directory, installed_transcript.directory);

        // Two independent pointer files: activating one model never moves the
        // other's active receipt.
        assert_eq!(
            active_model(&storage, &catalog).unwrap().unwrap().id,
            transcript.id
        );
        assert_eq!(
            active_note_model(&storage, &catalog).unwrap().unwrap().id,
            note.id
        );
    }

    #[test]
    fn refuses_changed_note_weights_and_unexpected_files() {
        let (_temp, storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let directory = write_installed_note_model(&storage, &note);
        assert!(verify_note_model_directory(&directory, &note, ModelVerification::Contents).is_ok());

        // Tampering with one shard of a multi-shard weight set is caught,
        // exactly as a single-file whisper weight tamper is above.
        fs::write(
            directory.join("model-00002-of-00002.safetensors"),
            b"tampered",
        )
        .unwrap();
        assert!(matches!(
            verify_note_model_directory(&directory, &note, ModelVerification::Contents),
            Err(ModelStoreError::InvalidModel)
        ));
        fs::write(directory.join("model-00002-of-00002.safetensors"), b"shard-1").unwrap();
        durable_create_new(&directory.join("extra"), b"unexpected").unwrap();
        assert!(matches!(
            verify_note_model_directory(&directory, &note, ModelVerification::Contents),
            Err(ModelStoreError::InvalidModel)
        ));
    }

    #[test]
    fn removes_only_an_inactive_note_model() {
        let (_temp, storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let mut other = note.clone();
        other.id = "note-test-mlx-alt".into();
        other.revision = "d".repeat(40);
        for file in &mut other.files {
            file.url = format!("https://example.test/{}/{}", other.revision, file.name);
        }
        catalog.note_models.push(other.clone());
        catalog.validate().unwrap();

        let active_directory = write_installed_note_model(&storage, &note);
        let other_directory = write_installed_note_model(&storage, &other);
        activate_note_model(&storage, &note).unwrap();

        assert!(note_model_is_stored(&storage, &note).unwrap());
        assert!(note_model_is_stored(&storage, &other).unwrap());
        assert!(matches!(
            remove_inactive_note_model(&storage, &catalog, &note.id),
            Err(ModelStoreError::ActiveModel)
        ));
        assert!(active_directory.exists());
        assert!(remove_inactive_note_model(&storage, &catalog, &other.id).unwrap());
        assert!(!other_directory.exists());
        assert!(!remove_inactive_note_model(&storage, &catalog, &other.id).unwrap());
    }

    #[test]
    fn deactivation_makes_the_only_note_model_removable_and_is_idempotent() {
        let (_temp, storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let directory = write_installed_note_model(&storage, &note);
        activate_note_model(&storage, &note).unwrap();
        assert!(matches!(
            remove_inactive_note_model(&storage, &catalog, &note.id),
            Err(ModelStoreError::ActiveModel)
        ));

        deactivate_note_model(&storage).unwrap();
        assert!(active_note_model(&storage, &catalog).unwrap().is_none());
        deactivate_note_model(&storage).unwrap();
        assert!(remove_inactive_note_model(&storage, &catalog, &note.id).unwrap());
        assert!(!directory.exists());
    }

    #[test]
    fn a_catalog_without_the_note_models_key_still_deserializes_with_an_empty_list() {
        // The exact bytes `worker/build_manifest.py::model_catalog()` writes,
        // and `scripts/verify-release-bundle.py::verify_model_catalog` pins
        // by deep equality, today — no `noteModels` key at all. Proves the
        // additive field needs no change on either the Python builder or the
        // release verifier for a branch that ships no note models, per
        // `docs/note-runtime-decision.md` seam 6.
        let raw = r#"{
          "schema": "yawn-model-catalog/1",
          "models": [
            {
              "id": "whisper-large-v3-turbo-q4",
              "revision": "660c343bbf4e52ac257f0b7d952e5388e6f93bef",
              "title": "Smaller download",
              "detail": "A 4-bit local transcription model that uses about 464 MB.",
              "downloadBytes": 463665005,
              "installedBytes": 463665005,
              "files": [
                {"role": "config", "name": "config.json", "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo-q4/660c343bbf4e52ac257f0b7d952e5388e6f93bef/config.json", "bytes": 341, "sha256": "538e24557b8f9bc504700add5e7bbe32087c2353001ff563e64772ad4398671a"},
                {"role": "weights", "name": "weights.npz", "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo-q4/660c343bbf4e52ac257f0b7d952e5388e6f93bef/weights.npz", "bytes": 463664664, "sha256": "862bbc832b05f3f4ec19dd632b701d61a6d3f5c7906360a10d72a79870642a80"}
              ]
            },
            {
              "id": "whisper-large-v3-turbo",
              "revision": "a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb",
              "title": "Full model",
              "detail": "The full local Turbo transcription model, using about 1.61 GB.",
              "downloadBytes": 1613977880,
              "installedBytes": 1613977880,
              "files": [
                {"role": "config", "name": "config.json", "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo/a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb/config.json", "bytes": 268, "sha256": "b34fc29e4e11e0a25e812775dd67f4dd16fc2c8eb43d28ae25ff7d660ecb6379"},
                {"role": "weights", "name": "weights.safetensors", "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo/a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb/weights.safetensors", "bytes": 1613977612, "sha256": "951ed3fc1203e6a62467abb2144a96ce7eafca8fa77e3704fdb8635ff3e7f8a6"}
              ]
            }
          ]
        }"#;
        let catalog: ModelCatalog = serde_json::from_str(raw).unwrap();
        catalog.validate().unwrap();
        assert_eq!(catalog.models.len(), 2);
        assert!(catalog.note_models.is_empty());
    }

    #[test]
    fn downloadable_view_is_generic_over_transcript_and_note_models() {
        // Mechanical proof that `model_download.rs::install` can be reused
        // unchanged once a note-model download caller exists: both model
        // types satisfy `DownloadableModel` today, through the same trait,
        // with no special-casing.
        let (_temp, _storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let transcript = catalog.models[0].clone();

        fn total_declared_bytes(model: &impl DownloadableModel) -> u64 {
            model.files().iter().map(|file| file.bytes).sum()
        }

        assert_eq!(total_declared_bytes(&transcript), transcript.download_bytes());
        assert_eq!(total_declared_bytes(&note), note.download_bytes());
        assert_eq!(transcript.id(), transcript.id.as_str());
        assert_eq!(note.revision(), note.revision.as_str());
    }

    #[test]
    fn downloadable_model_reuse_is_verified_not_assumed() {
        // Drives one generic function through the exact sequence
        // `model_download.rs::install`'s body follows — resolve the staging
        // path, write every file, write the receipt, verify the directory,
        // activate — for a `TranscriptModel` and a `NoteModel` alike, using
        // only `DownloadableModel` methods. This is what makes the trait's
        // doc comment a verified claim rather than an asserted one: nothing
        // here special-cases either model kind.
        let (_temp, storage, mut catalog) = fixture();
        let note = push_note_fixture(&mut catalog);
        let transcript = catalog.models[0].clone();

        fn install_like<M: DownloadableModel>(
            storage: &StorageRoot,
            model: &M,
            files: &[(&str, &[u8])],
        ) {
            let directory = storage.resolve(&model.relative_path()).unwrap();
            create_private_dir(&directory).unwrap();
            for (name, bytes) in files {
                durable_create_new(&directory.join(name), bytes).unwrap();
            }
            durable_create_new(&directory.join(INSTALL_RECEIPT_NAME), &model.receipt_bytes())
                .unwrap();
            model.verify_directory(&directory, ModelVerification::Contents)
                .unwrap();
            model.activate(storage).unwrap();
        }

        install_like(
            &storage,
            &transcript,
            &[
                ("config.json", b"config"),
                ("weights.npz", b"weights"),
            ],
        );
        install_like(
            &storage,
            &note,
            &[
                ("config.json", b"note-config"),
                ("model-00001-of-00002.safetensors", b"shard-0"),
                ("model-00002-of-00002.safetensors", b"shard-1"),
                ("model.safetensors.index.json", b"index"),
                ("tokenizer.json", b"tokenizer"),
                ("tokenizer_config.json", b"tokenizer-config"),
            ],
        );

        assert_eq!(
            active_model(&storage, &catalog).unwrap().unwrap().id,
            transcript.id
        );
        assert_eq!(
            active_note_model(&storage, &catalog).unwrap().unwrap().id,
            note.id
        );
    }
}
