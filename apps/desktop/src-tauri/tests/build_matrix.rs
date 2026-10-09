#[path = "../build_contract.rs"]
#[allow(dead_code)] // shared with build.rs; this test uses only part of it
mod build_contract;

use build_contract::{BuildMode, plan, validate};
use serde_json::{Value, json};

fn config(bytes: &str) -> Value {
    serde_json::from_str(bytes).unwrap()
}

#[test]
fn production_config_is_the_signed_product_lane() {
    let production = config(include_str!("../tauri.conf.json"));
    let plan = plan(BuildMode::Production);
    assert!(validate(BuildMode::Production, &production).is_ok());
    assert_eq!(plan.capabilities_path, "capabilities/product/*.json");
    assert_eq!(plan.permissions_path, "permissions/production/**/*");
    for command in [
        "app_snapshot",
        "start_meeting",
        "stop_meeting",
        "dismiss_meeting",
        "first_run_request_microphone",
        "first_run_request_system_audio",
        "library_snapshot",
        "library_set_meeting_title",
        "library_open_note",
        "library_play_retained_audio",
        "library_retained_audio_playback_status",
        "library_stop_retained_audio",
        "library_open_transcript",
        "library_open_transcript_file",
        "library_save_operator_note",
        "preview_delete_meeting_audio",
        "preview_delete_meeting_transcript",
        "preview_delete_meeting",
        "operator_note",
        "save_operator_note",
        "open_current_transcript_file",
    ] {
        assert!(
            plan.commands.contains(&command),
            "missing product command: {command}"
        );
    }
    assert_eq!(production["bundle"]["macOS"]["signingIdentity"], "-");
}

#[test]
fn preview_is_the_only_optional_build_lane() {
    let preview = config(include_str!("../tauri.preview.conf.json"));
    let fixture = config(include_str!("../tauri.fixture.conf.json"));
    assert!(validate(BuildMode::Preview, &preview).is_ok());
    assert!(validate(BuildMode::Preview, &fixture).is_ok());
    assert_eq!(
        BuildMode::from_enabled_features(false),
        BuildMode::Production
    );
    assert_eq!(BuildMode::from_enabled_features(true), BuildMode::Preview);
}

#[test]
fn product_builds_ship_the_complete_note_runtime() {
    // Flipped from its unadmitted-exclusion predecessor when the signed
    // catalog gained the note-model role: the bundle now ships the note
    // runtime, and shipping it *partially* is the failure this pins —
    // a bundle with a bridge but no generate manifest admits nothing and
    // reads as a build defect rather than an honest refusal.
    let production = config(include_str!("../tauri.conf.json"));
    let preview = config(include_str!("../tauri.preview.conf.json"));
    let fixture = config(include_str!("../tauri.fixture.conf.json"));
    for config in [&production, &preview, &fixture] {
        let resources = config["bundle"]["resources"].as_object().unwrap();
        for path in [
            "../runtime/note-bridge.py",
            "../runtime/note-generator-mlx.py",
            "../runtime/note-runtime-generate.json",
            "../runtime/note-runtime-project.json",
            "../runtime/note-validator.zip",
        ] {
            assert!(
                resources.contains_key(path),
                "missing note runtime resource: {path}"
            );
        }
    }
}

#[test]
fn product_and_preview_reject_each_others_identity() {
    let production = config(include_str!("../tauri.conf.json"));
    let preview = config(include_str!("../tauri.preview.conf.json"));
    let fixture = config(include_str!("../tauri.fixture.conf.json"));
    assert!(validate(BuildMode::Production, &preview).is_err());
    assert!(validate(BuildMode::Production, &fixture).is_err());
    assert!(validate(BuildMode::Preview, &production).is_err());
}

#[test]
fn fixture_rejects_preview_and_production_hybrids() {
    let production = config(include_str!("../tauri.conf.json"));
    let preview = config(include_str!("../tauri.preview.conf.json"));
    let fixture = config(include_str!("../tauri.fixture.conf.json"));

    let mut preview_identity = fixture.clone();
    preview_identity["identifier"] = preview["identifier"].clone();
    assert!(validate(BuildMode::Preview, &preview_identity).is_err());

    let mut production_window = fixture.clone();
    production_window["app"]["windows"][0]["label"] =
        production["app"]["windows"][0]["label"].clone();
    assert!(validate(BuildMode::Preview, &production_window).is_err());
}

#[test]
fn product_rejects_a_hybrid_or_non_adhoc_bundle() {
    let production = config(include_str!("../tauri.conf.json"));
    let mut hybrid = production.clone();
    hybrid["app"]["windows"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "label": "extra" }));
    assert!(validate(BuildMode::Production, &hybrid).is_err());

    let mut non_adhoc = production;
    non_adhoc["bundle"]["macOS"]["signingIdentity"] = Value::String("not-ad-hoc".into());
    assert!(validate(BuildMode::Production, &non_adhoc).is_err());
}
