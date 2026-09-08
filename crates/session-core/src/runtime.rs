use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::protocol::{RuntimeKind, WorkerAdmission, WorkerReady};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub schema: RuntimeSchema,
    pub admission: RuntimeAdmission,
    pub runtime: RuntimeResource,
    pub worker: RuntimeResource,
    pub tap: RuntimeResource,
    pub encoder: RuntimeResource,
    /// The fallback first-run permission probe (`capture/permission-probe`).
    ///
    /// Added to `app-runtime/1` on 2026-08-06 rather than left out, because the
    /// non-recording admissions execute it. Every child this application runs is
    /// digest-verified through this manifest before it is spawned, and there is no
    /// precedent here for running an unverified one. Shipping the binary needs only
    /// staging — `production_resources()` copies `bin` wholesale — but *running* it
    /// to the same standard as the tap needs this entry.
    ///
    /// Widening the required set is a lockstep change: `worker/main.py` compares
    /// `set(document) != required` and this struct is `deny_unknown_fields`, so a
    /// manifest written by one version and read by the other fails closed rather
    /// than silently skipping a check. That is the intended behaviour and the
    /// reason both sides move in the same commit.
    pub permission_probe: RuntimeResource,
    /// The signed catalog of transcript models that may live in the private
    /// per-user model store. Version 1 manifests have no catalog and keep the
    /// transcript weights in `models`; version 2 requires this resource.
    pub model_catalog: Option<RuntimeResource>,
    /// The signed Apple Speech helper.  Runtime v3 makes this resource
    /// mandatory so the desktop can prove the helper it probes or launches is
    /// the helper selected by the transcription receipt.  Older runtimes keep
    /// this absent and truthfully report native speech as unavailable.
    pub apple_speech: Option<RuntimeResource>,
    pub models: Vec<ModelResource>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeAdmission {
    BoundaryTest,
    InternalAlpha,
    Product,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum RuntimeSchema {
    #[serde(rename = "app-runtime/1")]
    V1,
    #[serde(rename = "app-runtime/2")]
    V2,
    #[serde(rename = "app-runtime/3")]
    V3,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeResource {
    pub path: PathBuf,
    pub sha256: String,
}

/// The signed executable that must make a first-run permission request.
///
/// An internal-alpha meeting uses `meeting-capture`, so its preflight has to run
/// that same executable. Other admissions do not ship the meeting helper and use
/// the standalone probe only for their bounded first-run surface.
#[derive(Debug, PartialEq, Eq)]
pub enum PermissionRequester {
    CaptureHelper(PathBuf),
    StandaloneProbe(PathBuf),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelResource {
    pub id: String,
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime manifest is missing or malformed")]
    Malformed,
    #[error("runtime resource path is unsafe")]
    UnsafePath,
    #[error("runtime resource is missing or changed")]
    ResourceMismatch,
    #[error("runtime model identifiers are invalid or duplicated")]
    InvalidModels,
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl RuntimeManifest {
    pub fn load_and_verify(path: &Path) -> Result<Self, RuntimeError> {
        if path.is_symlink() || !path.is_file() {
            return Err(RuntimeError::Malformed);
        }
        let manifest: Self =
            serde_json::from_slice(&fs::read(path)?).map_err(|_| RuntimeError::Malformed)?;
        let root = path
            .parent()
            .ok_or(RuntimeError::UnsafePath)?
            .canonicalize()?;
        verify_resource(&root, &manifest.runtime)?;
        verify_resource(&root, &manifest.worker)?;
        verify_resource(&root, &manifest.tap)?;
        verify_resource(&root, &manifest.encoder)?;
        verify_resource(&root, &manifest.permission_probe)?;
        match (&manifest.schema, &manifest.model_catalog, &manifest.apple_speech) {
            (RuntimeSchema::V1, None, None) => {}
            (RuntimeSchema::V2, Some(catalog), None) => verify_resource(&root, catalog)?,
            (RuntimeSchema::V3, Some(catalog), Some(apple_speech)) => {
                if manifest.admission != RuntimeAdmission::InternalAlpha {
                    return Err(RuntimeError::Malformed);
                }
                verify_resource(&root, catalog)?;
                verify_resource(&root, apple_speech)?;
            }
            _ => return Err(RuntimeError::Malformed),
        }
        let mut model_ids = HashSet::new();
        for model in &manifest.models {
            if model.id.is_empty()
                || model.id.len() > 128
                || !model
                    .id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
                || !model_ids.insert(&model.id)
            {
                return Err(RuntimeError::InvalidModels);
            }
            verify_path_and_digest(&root, &model.path, &model.sha256)?;
        }
        Ok(manifest)
    }

    pub fn matches_ready(&self, ready: &WorkerReady) -> bool {
        self.matches_ready_with_external_models(ready, &[])
    }

    pub fn matches_ready_with_external_models(
        &self,
        ready: &WorkerReady,
        external_models: &[(String, String)],
    ) -> bool {
        let expected_count = self.models.len() + external_models.len();
        if !matches!(ready.runtime.kind, RuntimeKind::Bundled)
            || ready.admission != WorkerAdmission::from(&self.admission)
            || !ready
                .runtime
                .digest
                .eq_ignore_ascii_case(&self.runtime.sha256)
            || !ready.build.eq_ignore_ascii_case(&self.worker.sha256)
            || !ready.tap.available
            || !ready.tap.build.eq_ignore_ascii_case(&self.tap.sha256)
            || ready.models.len() != expected_count
        {
            return false;
        }
        self.models.iter().all(|expected| {
            ready.models.iter().any(|actual| {
                actual.available
                    && actual.id == expected.id
                    && actual.digest.eq_ignore_ascii_case(&expected.sha256)
            })
        }) && external_models
            .iter()
            .all(|(expected_id, expected_sha256)| {
                ready.models.iter().any(|actual| {
                    actual.available
                        && actual.id == *expected_id
                        && actual.digest.eq_ignore_ascii_case(expected_sha256)
                })
            })
    }

    /// The verified absolute path of the permission probe, or an error.
    ///
    /// First run asks about permissions interactively, so this resolves on a UI
    /// command rather than once during startup. `load_and_verify` is the right
    /// thing at startup and the wrong thing on a button press: it hashes every
    /// model, which is over a gigabyte of Whisper weights. This parses the
    /// manifest and verifies exactly the one binary that is about to be executed,
    /// which keeps the property that matters — no unverified child is ever
    /// spawned — at a cost proportional to the child rather than to the bundle.
    ///
    /// It deliberately does not check `admission`. A boundary build has first run
    /// too, and refusing to report permissions there would make the surface lie in
    /// exactly the builds most likely to be inspected.
    pub fn verified_permission_probe(manifest_path: &Path) -> Result<PathBuf, RuntimeError> {
        if manifest_path.is_symlink() || !manifest_path.is_file() {
            return Err(RuntimeError::Malformed);
        }
        let manifest: Self = serde_json::from_slice(&fs::read(manifest_path)?)
            .map_err(|_| RuntimeError::Malformed)?;
        let root = manifest_path
            .parent()
            .ok_or(RuntimeError::UnsafePath)?
            .canonicalize()?;
        verify_resource(&root, &manifest.permission_probe)?;
        Ok(root.join(&manifest.permission_probe.path))
    }

    /// Resolves and verifies the executable that is allowed to request first-run
    /// permissions without hashing every packaged model.
    ///
    /// The recorder itself owns access in an internal-alpha build. Asking a
    /// different helper can produce a prompt that does not clear the executable
    /// which later starts the meeting, so this selection intentionally follows the
    /// admission rather than treating every bundle as equivalent.
    pub fn verified_permission_requester(
        manifest_path: &Path,
    ) -> Result<PermissionRequester, RuntimeError> {
        if manifest_path.is_symlink() || !manifest_path.is_file() {
            return Err(RuntimeError::Malformed);
        }
        let manifest: Self = serde_json::from_slice(&fs::read(manifest_path)?)
            .map_err(|_| RuntimeError::Malformed)?;
        let root = manifest_path
            .parent()
            .ok_or(RuntimeError::UnsafePath)?
            .canonicalize()?;
        match manifest.admission {
            RuntimeAdmission::InternalAlpha => {
                verify_resource(&root, &manifest.tap)?;
                Ok(PermissionRequester::CaptureHelper(
                    root.join(&manifest.tap.path),
                ))
            }
            RuntimeAdmission::BoundaryTest | RuntimeAdmission::Product => {
                verify_resource(&root, &manifest.permission_probe)?;
                Ok(PermissionRequester::StandaloneProbe(
                    root.join(&manifest.permission_probe.path),
                ))
            }
        }
    }

    pub fn permits_application_start(&self) -> bool {
        matches!(
            self.admission,
            RuntimeAdmission::InternalAlpha | RuntimeAdmission::Product
        )
    }

    pub fn is_internal_alpha(&self) -> bool {
        self.admission == RuntimeAdmission::InternalAlpha
    }

    pub fn model_catalog_path(&self, manifest_path: &Path) -> Option<PathBuf> {
        self.model_catalog
            .as_ref()
            .and_then(|catalog| manifest_path.parent().map(|root| root.join(&catalog.path)))
    }

    /// Returns the verified helper path only for the v3 native-speech runtime.
    /// The selective verifier mirrors the permission probe path: a Settings
    /// readiness refresh must prove the executable it will run, without
    /// rehashing an installed Whisper model.
    pub fn verified_apple_speech_helper(manifest_path: &Path) -> Result<Option<PathBuf>, RuntimeError> {
        if manifest_path.is_symlink() || !manifest_path.is_file() {
            return Err(RuntimeError::Malformed);
        }
        let manifest: Self = serde_json::from_slice(&fs::read(manifest_path)?)
            .map_err(|_| RuntimeError::Malformed)?;
        let root = manifest_path
            .parent()
            .ok_or(RuntimeError::UnsafePath)?
            .canonicalize()?;
        match (&manifest.schema, &manifest.apple_speech) {
            (RuntimeSchema::V3, Some(helper)) => {
                verify_resource(&root, helper)?;
                Ok(Some(root.join(&helper.path)))
            }
            (RuntimeSchema::V1 | RuntimeSchema::V2, None) => Ok(None),
            _ => Err(RuntimeError::Malformed),
        }
    }
}

impl From<&RuntimeAdmission> for WorkerAdmission {
    fn from(admission: &RuntimeAdmission) -> Self {
        match admission {
            RuntimeAdmission::BoundaryTest => Self::BoundaryTest,
            RuntimeAdmission::InternalAlpha => Self::InternalAlpha,
            RuntimeAdmission::Product => Self::Product,
        }
    }
}

fn verify_resource(root: &Path, resource: &RuntimeResource) -> Result<(), RuntimeError> {
    verify_path_and_digest(root, &resource.path, &resource.sha256)
}

fn verify_path_and_digest(
    root: &Path,
    relative: &Path,
    expected_sha256: &str,
) -> Result<(), RuntimeError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || expected_sha256.len() != 64
        || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RuntimeError::UnsafePath);
    }
    let unresolved = root.join(relative);
    if unresolved.is_symlink() || !unresolved.is_file() {
        return Err(RuntimeError::ResourceMismatch);
    }
    let resolved = unresolved.canonicalize()?;
    if resolved.parent().is_none() || (resolved != root && !resolved.starts_with(root)) {
        return Err(RuntimeError::UnsafePath);
    }
    let actual = sha256_file(&resolved)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(RuntimeError::ResourceMismatch);
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, io::Error> {
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
    use tempfile::TempDir;

    fn write(path: &Path, bytes: &[u8]) -> String {
        fs::write(path, bytes).unwrap();
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn verifies_exact_resources_and_refuses_tamper() {
        let temp = TempDir::new().unwrap();
        let runtime = write(&temp.path().join("worker"), b"worker");
        let worker = write(&temp.path().join("main.py"), b"worker source");
        let tap = write(&temp.path().join("tap"), b"tap");
        let encoder = write(&temp.path().join("encoder"), b"encoder");
        let probe = write(&temp.path().join("permission-probe"), b"probe");
        let manifest = temp.path().join("manifest.json");
        fs::write(
            &manifest,
            serde_json::json!({
                "schema": "app-runtime/1",
                "admission": "product",
                "runtime": {"path": "worker", "sha256": runtime},
                "worker": {"path": "main.py", "sha256": worker},
                "tap": {"path": "tap", "sha256": tap},
                "encoder": {"path": "encoder", "sha256": encoder},
                "permission_probe": {"path": "permission-probe", "sha256": probe},
                "models": []
            })
            .to_string(),
        )
        .unwrap();
        let loaded = RuntimeManifest::load_and_verify(&manifest).unwrap();

        // The probe resolves and verifies on its own, without hashing the models.
        let probe_path = RuntimeManifest::verified_permission_probe(&manifest).unwrap();
        assert!(probe_path.ends_with("permission-probe"));
        assert!(matches!(
            RuntimeManifest::verified_permission_requester(&manifest),
            Ok(PermissionRequester::StandaloneProbe(path)) if path.ends_with("permission-probe")
        ));
        // And a tampered probe is refused, which is the whole reason it is in the
        // manifest rather than merely staged into the bundle.
        fs::write(temp.path().join("permission-probe"), b"swapped").unwrap();
        assert!(matches!(
            RuntimeManifest::verified_permission_probe(&manifest),
            Err(RuntimeError::ResourceMismatch)
        ));
        fs::write(temp.path().join("permission-probe"), b"probe").unwrap();
        let ready_frame = serde_json::json!({
            "schema": "worker-event/2",
            "event": "worker.ready",
            "protocol": 2,
            "admission": "product",
            "build": loaded.worker.sha256,
            "runtime": {"kind": "bundled", "digest": loaded.runtime.sha256},
            "tap": {"build": loaded.tap.sha256, "available": true},
            "models": [],
            "operations": []
        })
        .to_string();
        let mut ready =
            crate::protocol::parse_ready(ready_frame.as_bytes(), &std::collections::HashSet::new())
                .unwrap();
        assert!(loaded.matches_ready(&ready));
        assert!(loaded.permits_application_start());
        assert!(!loaded.is_internal_alpha());
        ready.admission = WorkerAdmission::BoundaryTest;
        assert!(!loaded.matches_ready(&ready));
        ready.admission = WorkerAdmission::Product;
        ready.tap.build = "0".repeat(64);
        assert!(!loaded.matches_ready(&ready));
        let mut boundary: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        boundary["admission"] = serde_json::json!("boundary-test");
        fs::write(&manifest, boundary.to_string()).unwrap();
        assert!(
            !RuntimeManifest::load_and_verify(&manifest)
                .unwrap()
                .permits_application_start()
        );
        boundary["admission"] = serde_json::json!("internal-alpha");
        fs::write(&manifest, boundary.to_string()).unwrap();
        let alpha = RuntimeManifest::load_and_verify(&manifest).unwrap();
        assert!(alpha.permits_application_start());
        assert!(alpha.is_internal_alpha());
        assert!(matches!(
            RuntimeManifest::verified_permission_requester(&manifest),
            Ok(PermissionRequester::CaptureHelper(path)) if path.ends_with("tap")
        ));
        fs::write(temp.path().join("tap"), b"changed").unwrap();
        assert!(matches!(
            RuntimeManifest::load_and_verify(&manifest),
            Err(RuntimeError::ResourceMismatch)
        ));
    }
}
