//! Narrow, local-only Apple Speech capability probe.
//!
//! The helper is verified by `RuntimeManifest` before this module receives its
//! path.  The JSON file is only a bounded hand-off from that owned child; it is
//! deleted after parsing and never becomes a meeting artifact.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use uuid::Uuid;

const MAX_CAPABILITY_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Ready,
    AssetsRequired,
    Unavailable,
    Installing,
    Failed,
}

impl State {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::AssetsRequired => "assets-required",
            Self::Unavailable => "unavailable",
            Self::Installing => "installing",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Capability {
    pub(crate) state: State,
    pub(crate) reason: Option<String>,
    pub(crate) locale: String,
    pub(crate) asset_identity: String,
    pub(crate) os_version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityDocument {
    schema: String,
    state: String,
    reason: Option<String>,
    locale: String,
    os_version: String,
    asset_identity: String,
}

pub(crate) fn unavailable(locale: &str, reason: impl Into<String>) -> Capability {
    Capability {
        state: State::Unavailable,
        reason: Some(reason.into()),
        locale: locale.into(),
        asset_identity: "os-managed".into(),
        os_version: String::new(),
    }
}

/// Probe a manifest-verified helper.  A malformed or failed response is an
/// unavailable capability, never a reason to assume native speech is ready.
pub(crate) fn probe(helper: &Path, private_root: &Path, locale: &str) -> Capability {
    let output = private_root.join(format!(".apple-speech-capability-{}.json", Uuid::new_v4()));
    let mut command = Command::new(helper);
    let result = command
        .args(["--capabilities", "--locale", locale, "--out"])
        .arg(&output);
    let result = run_bounded(result, Duration::from_secs(30));
    let capability = match result {
        Ok(true) => parse(&output, locale),
        Ok(_) => unavailable(locale, "Apple Speech is unavailable on this Mac."),
        Err(_) => unavailable(locale, "Apple Speech could not be checked on this Mac."),
    };
    let _ = fs::remove_file(output);
    capability
}

pub(crate) fn install_assets(helper: &Path, private_root: &Path, locale: &str) -> Capability {
    let output = private_root.join(format!(".apple-speech-install-{}.json", Uuid::new_v4()));
    let mut command = Command::new(helper);
    let result = command
        .args(["--install-assets", "--locale", locale, "--out"])
        .arg(&output);
    let result = run_bounded(result, Duration::from_secs(1800));
    let capability = match result {
        Ok(true) => parse(&output, locale),
        Ok(_) => Capability { state: State::Failed, reason: Some("Apple Speech assets could not be prepared.".into()), locale: locale.into(), asset_identity: "os-managed".into(), os_version: String::new() },
        Err(_) => Capability { state: State::Failed, reason: Some("Apple Speech assets could not be prepared.".into()), locale: locale.into(), asset_identity: "os-managed".into(), os_version: String::new() },
    };
    let _ = fs::remove_file(output);
    capability
}

fn run_bounded(command: &mut Command, timeout: Duration) -> std::io::Result<bool> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return match result { Err(error) => Err(error), _ => Ok(false) };
            }
        }
    }
}

fn parse(path: &Path, expected_locale: &str) -> Capability {
    let bytes = match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_CAPABILITY_BYTES
                && metadata.permissions().mode() & 0o077 == 0 => fs::read(path).ok(),
        _ => None,
    };
    let Some(bytes) = bytes else {
        return unavailable(expected_locale, "Apple Speech did not return a valid capability result.");
    };
    let Ok(document) = serde_json::from_slice::<CapabilityDocument>(&bytes) else {
        return unavailable(expected_locale, "Apple Speech did not return a valid capability result.");
    };
    if document.schema != "apple-speech-capability/1"
        || document.locale != expected_locale
        || document.os_version.is_empty()
        || document.asset_identity != "os-managed"
    {
        return unavailable(expected_locale, "Apple Speech returned an unsupported capability result.");
    }
    let state = match document.state.as_str() {
        "ready" => State::Ready,
        "assets-required" => State::AssetsRequired,
        "unavailable" => State::Unavailable,
        _ => return unavailable(expected_locale, "Apple Speech returned an unsupported capability result."),
    };
    if matches!(state, State::Ready) != document.reason.is_none() {
        return unavailable(expected_locale, "Apple Speech returned an unsupported capability result.");
    }
    Capability { state, reason: document.reason, locale: document.locale, asset_identity: document.asset_identity, os_version: document.os_version }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn capability_requires_exact_schema_locale_and_private_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("capability.json");
        fs::write(&path, serde_json::json!({
            "schema":"apple-speech-capability/1", "state":"ready", "reason":null,
            "locale":"en-US", "os_version":"26.0", "asset_identity":"os-managed"
        }).to_string()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(parse(&path, "en-US").state, State::Ready);
        fs::write(&path, b"{}").unwrap();
        assert_eq!(parse(&path, "en-US").state, State::Unavailable);
    }
}
