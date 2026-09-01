use serde_json::Value;

pub fn merge_config(base: &mut Value, override_value: Value) {
    match (base, override_value) {
        (Value::Object(base), Value::Object(override_value)) => {
            for (key, value) in override_value {
                match base.get_mut(&key) {
                    Some(existing) => merge_config(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, override_value) => *base = override_value,
    }
}

pub const PREVIEW_IDENTIFIER: &str = "com.ninochavez.local-meeting-notes.preview";
pub const PREVIEW_WINDOW: &str = "preview";
pub const PREVIEW_CAPABILITY: &str = "preview-window";
pub const PREVIEW_FRONTEND: &str = "../ui";
pub const FIXTURE_IDENTIFIER: &str = "com.ninochavez.local-meeting-notes.fixture";
pub const PRODUCTION_IDENTIFIER: &str = "com.ninochavez.local-meeting-notes";
pub const PRODUCTION_WINDOW: &str = "main";
pub const PRODUCTION_CAPABILITY: &str = "main-window";
pub const SETTINGS_CAPABILITY: &str = "settings-window";
pub const PRODUCTION_FRONTEND: &str = "../ui";
pub const CAPTURE_ENTITLEMENTS: &str = "capture-entitlements.plist";

const PRODUCT_COMMANDS: &[&str] = &[
    "app_snapshot",
    "open_settings_window",
    "start_meeting",
    "pause_meeting",
    "resume_meeting",
    "stop_meeting",
    "dismiss_meeting",
    "retry_startup",
    "install_transcript_model",
    "transcript_model_settings",
    "remove_transcript_model",
    "note_model_settings",
    "install_note_model",
    "remove_note_model",
    "first_run_permissions",
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
    "library_export_meeting",
    "correct_speaker_name",
    "local_vocabulary_list",
    "local_vocabulary_add",
    "local_vocabulary_edit",
    "local_vocabulary_set_enabled",
    "local_vocabulary_delete",
    "library_save_operator_note",
    "preview_delete_meeting_audio",
    "preview_delete_meeting_transcript",
    "preview_delete_meeting",
    "preview_list_trash",
    "restore_meeting_from_trash_command",
    "operator_note",
    "save_operator_note",
    "open_current_transcript_file",
    "restore_withheld_turn",
    "regenerate_note",
    "transcript_retry_start",
    "transcript_retry_pending",
    "transcript_retry_decide",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildMode {
    Production,
    Preview,
}

pub struct BuildPlan {
    pub commands: &'static [&'static str],
    pub capabilities_path: &'static str,
    pub permissions_path: &'static str,
}

impl BuildMode {
    pub fn from_enabled_features(preview: bool) -> Self {
        if preview {
            Self::Preview
        } else {
            Self::Production
        }
    }
}

pub fn plan(_: BuildMode) -> BuildPlan {
    BuildPlan {
        commands: PRODUCT_COMMANDS,
        capabilities_path: "capabilities/product/*.json",
        permissions_path: "permissions/production/**/*",
    }
}

pub fn validate(mode: BuildMode, config: &Value) -> Result<(), &'static str> {
    match mode {
        BuildMode::Production if is_production_config(config) => Ok(()),
        BuildMode::Production => Err(
            "the bundled shell requires the production identifier, product frontend, main and Settings capabilities, ad-hoc signing, and local runtime resources",
        ),
        BuildMode::Preview if is_preview_config(config) || is_fixture_config(config) => Ok(()),
        BuildMode::Preview => Err(
            "preview-surface requires the preview identifier, product frontend, product capabilities, ad-hoc signing, and the local runtime resource contract",
        ),
    }
}

fn is_preview_config(config: &Value) -> bool {
    is_preview_surface_config(config, "Yawn Preview", PREVIEW_IDENTIFIER, "Yawn — Preview")
}

fn is_fixture_config(config: &Value) -> bool {
    is_preview_surface_config(config, "Yawn Fixture", FIXTURE_IDENTIFIER, "Yawn — Fixture")
}

fn is_preview_surface_config(
    config: &Value,
    product_name: &str,
    identifier: &str,
    title: &str,
) -> bool {
    config.get("productName").and_then(Value::as_str) == Some(product_name)
        && config.get("identifier").and_then(Value::as_str) == Some(identifier)
        && config
            .pointer("/build/frontendDist")
            .and_then(Value::as_str)
            == Some(PREVIEW_FRONTEND)
        && has_single_window(config.pointer("/app/windows"), PREVIEW_WINDOW, title)
        && has_exact_strings(
            config.pointer("/app/security/capabilities"),
            &[PREVIEW_CAPABILITY, SETTINGS_CAPABILITY],
        )
        && config.pointer("/bundle/active").and_then(Value::as_bool) == Some(true)
        && config.pointer("/bundle/resources") == Some(&production_resources())
        && config
            .pointer("/bundle/macOS/minimumSystemVersion")
            .and_then(Value::as_str)
            == Some("14.4")
        && config
            .pointer("/bundle/macOS/signingIdentity")
            .and_then(Value::as_str)
            == Some("-")
        && config
            .pointer("/bundle/macOS/entitlements")
            .and_then(Value::as_str)
            == Some(CAPTURE_ENTITLEMENTS)
}

fn is_production_config(config: &Value) -> bool {
    config.get("productName").and_then(Value::as_str) == Some("Yawn")
        && config.get("identifier").and_then(Value::as_str) == Some(PRODUCTION_IDENTIFIER)
        && config
            .pointer("/build/frontendDist")
            .and_then(Value::as_str)
            == Some(PRODUCTION_FRONTEND)
        && has_single_window(config.pointer("/app/windows"), PRODUCTION_WINDOW, "Yawn")
        && has_exact_strings(
            config.pointer("/app/security/capabilities"),
            &[PRODUCTION_CAPABILITY, SETTINGS_CAPABILITY],
        )
        && config.pointer("/bundle/active").and_then(Value::as_bool) == Some(true)
        && config.pointer("/bundle/resources") == Some(&production_resources())
        && config
            .pointer("/bundle/macOS/minimumSystemVersion")
            .and_then(Value::as_str)
            == Some("14.4")
        && config
            .pointer("/bundle/macOS/signingIdentity")
            .and_then(Value::as_str)
            == Some("-")
        && config
            .pointer("/bundle/macOS/entitlements")
            .and_then(Value::as_str)
            == Some(CAPTURE_ENTITLEMENTS)
}

fn has_single_window(value: Option<&Value>, expected_label: &str, expected_title: &str) -> bool {
    value.and_then(Value::as_array).is_some_and(|windows| {
        windows.len() == 1
            && windows[0].get("label").and_then(Value::as_str) == Some(expected_label)
            && windows[0].get("title").and_then(Value::as_str) == Some(expected_title)
    })
}

fn has_exact_strings(value: Option<&Value>, expected: &[&str]) -> bool {
    value.and_then(Value::as_array).is_some_and(|entries| {
        entries.len() == expected.len()
            && entries
                .iter()
                .zip(expected)
                .all(|(entry, expected)| entry.as_str() == Some(*expected))
    })
}

fn production_resources() -> Value {
    serde_json::json!({
        "../runtime/app-runtime.json": "app-runtime.json",
        "../runtime/model-catalog.json": "model-catalog.json",
        "../runtime/bin": "bin",
        "../runtime/encoder-unavailable.identity": "encoder-unavailable.identity",
        "../runtime/models": "models",
        "../runtime/note-bridge.py": "note-bridge.py",
        "../runtime/note-generator-mlx.py": "note-generator-mlx.py",
        "../runtime/note-runtime-generate.json": "note-runtime-generate.json",
        "../runtime/note-runtime-project.json": "note-runtime-project.json",
        "../runtime/note-validator.zip": "note-validator.zip",
        "../runtime/notes": "notes",
        "../runtime/python-runtime": "python-runtime",
        "../runtime/spike": "spike",
        "../runtime/worker": "worker",
    })
}
