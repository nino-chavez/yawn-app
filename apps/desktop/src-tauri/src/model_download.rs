use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use local_meeting_notes_session_core::model_store::{
    DownloadableModel, INSTALL_RECEIPT_NAME, ModelStoreError,
};
use local_meeting_notes_session_core::storage::{
    StorageRoot, create_private_dir, durable_create_new, sync_directory,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum ModelDownloadError {
    #[error("model download could not be completed")]
    Network,
    #[error("model download size or digest did not match the signed catalog")]
    Mismatch,
    #[error("model download storage is unavailable")]
    Storage,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Model(#[from] ModelStoreError),
}

pub(crate) fn install(
    storage: &StorageRoot,
    model: &impl DownloadableModel,
    mut progress: impl FnMut(u64),
) -> Result<(), ModelDownloadError> {
    let models = storage
        .resolve(Path::new("models"))
        .map_err(|_| ModelDownloadError::Storage)?;
    create_private_dir(&models)?;
    let relative = model.relative_path();
    let relative_parent = relative.parent().ok_or(ModelDownloadError::Storage)?;
    let mut ancestor = PathBuf::new();
    for component in relative_parent.components() {
        ancestor.push(component);
        let directory = storage
            .resolve(&ancestor)
            .map_err(|_| ModelDownloadError::Storage)?;
        create_private_dir(&directory)?;
    }
    let model_parent = storage
        .resolve(relative_parent)
        .map_err(|_| ModelDownloadError::Storage)?;
    let target = storage
        .resolve(&relative)
        .map_err(|_| ModelDownloadError::Storage)?;
    if target.exists() {
        if model.verify_directory(&target, local_meeting_notes_session_core::model_store::ModelVerification::Contents).is_ok() {
            model.activate(storage)?;
            progress(model.download_bytes());
            return Ok(());
        }
        if target.is_symlink() || !target.is_dir() {
            return Err(ModelDownloadError::Storage);
        }
        fs::remove_dir_all(&target)?;
        sync_directory(&model_parent)?;
    }

    let staging_name = format!(".install-{}", Uuid::new_v4());
    let staging = models.join(&staging_name);
    create_private_dir(&staging)?;
    let result = (|| {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(2 * 60 * 60))
            .user_agent("Yawn model installer")
            .build()
            .map_err(|_| ModelDownloadError::Network)?;
        let mut downloaded = 0_u64;
        for expected in &model.files() {
            let mut response = client
                .get(expected.url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .map_err(|_| ModelDownloadError::Network)?;
            if response
                .content_length()
                .is_some_and(|length| length != expected.bytes)
            {
                return Err(ModelDownloadError::Mismatch);
            }
            let path = staging.join(expected.name);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            let mut digest = Sha256::new();
            let mut file_bytes = 0_u64;
            let mut buffer = [0_u8; 1024 * 1024];
            loop {
                let read = response
                    .read(&mut buffer)
                    .map_err(|_| ModelDownloadError::Network)?;
                if read == 0 {
                    break;
                }
                file_bytes = file_bytes
                    .checked_add(read as u64)
                    .ok_or(ModelDownloadError::Mismatch)?;
                if file_bytes > expected.bytes {
                    return Err(ModelDownloadError::Mismatch);
                }
                output.write_all(&buffer[..read])?;
                digest.update(&buffer[..read]);
                downloaded += read as u64;
                progress(downloaded);
            }
            output.sync_all()?;
            if file_bytes != expected.bytes || format!("{:x}", digest.finalize()) != expected.sha256
            {
                return Err(ModelDownloadError::Mismatch);
            }
        }
        durable_create_new(&staging.join(INSTALL_RECEIPT_NAME), &model.receipt_bytes())?;
        model.verify_directory(&staging, local_meeting_notes_session_core::model_store::ModelVerification::Contents)?;
        fs::rename(&staging, &target)?;
        sync_directory(&model_parent)?;
        sync_directory(&models)?;
        model.activate(storage)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
        let _ = sync_directory(&models);
    }
    result
}
