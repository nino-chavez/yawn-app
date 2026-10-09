//! Release check for `scripts/build-update-artifact.sh`: verifies an updater
//! package's minisign signature against the public key in `tauri.conf.json`,
//! the same check tauri-plugin-updater makes before it installs anything.
//!
//! Usage: verify_update_signature <package> <package.sig> <tauri.conf.json>
//! Exits non-zero, naming the reason, when the signature does not verify.

use std::process::ExitCode;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

fn decode(label: &str, encoded: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("{label} is not base64: {error}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{label} is not UTF-8 text"))
}

fn verify(package: &str, signature: &str, config: &str) -> Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(config)
            .map_err(|error| format!("cannot read {config}: {error}"))?,
    )
    .map_err(|error| format!("{config} is not JSON: {error}"))?;
    let public_key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("tauri.conf.json has no plugins.updater.pubkey")?;
    let public_key = PublicKey::decode(&decode("the public key", public_key)?)
        .map_err(|error| format!("the public key does not decode: {error}"))?;
    let signature = std::fs::read_to_string(signature)
        .map_err(|error| format!("cannot read {signature}: {error}"))?;
    let signature = Signature::decode(&decode("the signature", &signature)?)
        .map_err(|error| format!("the signature does not decode: {error}"))?;
    let data = std::fs::read(package).map_err(|error| format!("cannot read {package}: {error}"))?;
    public_key
        .verify(&data, &signature, true)
        .map_err(|error| format!("the signature does not verify: {error}"))?;
    Ok(signature.trusted_comment().to_owned())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [package, signature, config] = args.as_slice() else {
        eprintln!("usage: verify_update_signature <package> <package.sig> <tauri.conf.json>");
        return ExitCode::from(2);
    };
    match verify(package, signature, config) {
        Ok(comment) => {
            println!("updater signature: PASS ({comment})");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("updater signature: FAIL — {reason}");
            ExitCode::FAILURE
        }
    }
}
