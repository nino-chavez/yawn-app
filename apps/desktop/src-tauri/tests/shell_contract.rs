use serde_json::Value;

#[path = "../build_contract.rs"]
mod build_contract;

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
    "preview_library_open_evidence",
    "library_open_transcript",
    "library_open_transcript_file",
    // Roadmap intake I7+I8: exports a reviewed meeting as plain per-item files
    // plus a compact archive, written only inside that meeting's own directory.
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
    // Roadmap intake I9: local trash for whole-meeting deletion. Listing is
    // the quiet secondary surface; restoring is the recovery path within the
    // 30-day window.
    "preview_list_trash",
    "restore_meeting_from_trash_command",
    "operator_note",
    "save_operator_note",
    // Roadmap intake I3: labeled pre-meeting context, mirroring the operator
    // note pair above.
    "meeting_context",
    "save_meeting_context",
    "open_current_transcript_file",
    "restore_withheld_turn",
    // Admitted with the generation invocation chain (docs/
    // note-runtime-decision.md, slice 4): the rendered control can reach a
    // terminal receipt now, so the command joins the pinned product set.
    "regenerate_note",
    // Retry creation, resume, and decision are all invoked by the comparison
    // sheet and therefore belong to the pinned product command set.
    "transcript_retry_start",
    "transcript_retry_pending",
    "transcript_retry_decide",
];

const NATIVE_ONLY_COMMANDS: &[&str] = &[];

const MAIN_PERMISSIONS: &[&str] = &[
    "core:window:allow-start-dragging",
    "allow-app-snapshot",
    "allow-open-settings-window",
    "allow-start-meeting",
    "allow-pause-meeting",
    "allow-resume-meeting",
    "allow-stop-meeting",
    "allow-dismiss-meeting",
    "allow-retry-startup",
    "allow-install-transcript-model",
    "allow-first-run-permissions",
    "allow-first-run-request-microphone",
    "allow-first-run-request-system-audio",
    "allow-library-snapshot",
    "allow-library-set-meeting-title",
    "allow-library-open-note",
    "allow-library-play-retained-audio",
    "allow-library-retained-audio-playback-status",
    "allow-library-stop-retained-audio",
    "allow-preview-library-open-evidence",
    "allow-library-open-transcript",
    "allow-library-open-transcript-file",
    "allow-library-export-meeting",
    "allow-correct-speaker-name",
    "allow-local-vocabulary-list",
    "allow-local-vocabulary-add",
    "allow-local-vocabulary-edit",
    "allow-local-vocabulary-set-enabled",
    "allow-local-vocabulary-delete",
    "allow-library-save-operator-note",
    "allow-preview-delete-meeting-audio",
    "allow-preview-delete-meeting-transcript",
    "allow-preview-delete-meeting",
    "allow-preview-list-trash",
    "allow-restore-meeting-from-trash-command",
    "allow-operator-note",
    "allow-save-operator-note",
    "allow-meeting-context",
    "allow-save-meeting-context",
    "allow-open-current-transcript-file",
    "allow-restore-withheld-turn",
    "allow-regenerate-note",
    "allow-transcript-retry-start",
    "allow-transcript-retry-pending",
    "allow-transcript-retry-decide",
    // Roadmap intake I2: lets the frontend listen for the note-capture
    // hotkey's focus event (capture_shortcut::NOTE_CAPTURE_FOCUS_EVENT).
    // Emitting from Rust needs no permission; only the frontend's listen
    // side is gated.
    "core:event:allow-listen",
    "core:event:allow-unlisten",
];

fn permissions(source: &str) -> Vec<String> {
    let capability: Value = serde_json::from_str(source).expect("valid capability JSON");
    capability["permissions"]
        .as_array()
        .expect("permission list")
        .iter()
        .map(|permission| permission.as_str().expect("permission name").to_owned())
        .collect()
}

#[test]
fn product_builds_keep_one_frontend_and_two_bounded_windows() {
    let production: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let preview: Value = serde_json::from_str(include_str!("../tauri.preview.conf.json")).unwrap();

    assert!(build_contract::validate(build_contract::BuildMode::Production, &production).is_ok());
    assert!(build_contract::validate(build_contract::BuildMode::Preview, &preview).is_ok());
    assert_eq!(production["build"]["frontendDist"], "../ui");
    assert_eq!(preview["build"]["frontendDist"], "../ui");
    assert_eq!(
        production["app"]["security"]["capabilities"],
        serde_json::json!(["main-window", "settings-window"])
    );
    assert_eq!(
        preview["app"]["security"]["capabilities"],
        serde_json::json!(["preview-window", "settings-window"])
    );
}

#[test]
fn product_windows_have_only_the_commands_the_new_surface_uses() {
    let main = permissions(include_str!("../capabilities/product/main.json"));
    let preview = permissions(include_str!("../capabilities/product/preview.json"));
    let expected: Vec<String> = MAIN_PERMISSIONS
        .iter()
        .map(|permission| (*permission).to_owned())
        .collect();

    assert_eq!(main, expected);
    assert_eq!(preview, expected);
    for forbidden in [
        "profile",
        "folder",
        "layout",
        "corpus",
        "reference",
        "shell",
        "fs",
    ] {
        assert!(
            main.iter()
                .all(|permission| !permission.contains(forbidden))
        );
    }
}

#[test]
fn retained_audio_playback_stays_on_the_fixed_native_standard_input_route() {
    let source = include_str!("../src/main.rs");
    assert!(source.contains("const RETAINED_AUDIO_PLAYER: &str = \"/usr/bin/afplay\""));
    assert!(source.contains("const RETAINED_AUDIO_STDIN: &str = \"/dev/stdin\""));
    assert!(source.contains("Command::new(RETAINED_AUDIO_PLAYER)"));
    assert!(source.contains(".stdin(Stdio::from(input))"));
    let playback_impl = &source[source.find("impl RetainedAudioPlayback").unwrap()
        ..source.find("struct RetainedAudioPlaybackResponse").unwrap()];
    assert!(!playback_impl.contains("pre_exec("));
    assert!(!source.contains("/dev/fd/3"));
    assert!(!source.contains("tauri_plugin_shell"));
    assert!(!source.contains("tauri_plugin_fs"));
}

#[test]
fn every_frontend_call_has_a_matching_product_permission() {
    fn invoked_commands(script: &str, dynamic_commands: &[&str]) -> Vec<String> {
        let mut invoked: Vec<String> = script
            .match_indices("invoke(\"")
            .map(|(at, _)| {
                let rest = &script[at + "invoke(\"".len()..];
                rest[..rest.find('"').expect("unterminated invoke name")].to_owned()
            })
            .collect();
        for command in dynamic_commands {
            assert!(script.contains(command));
            invoked.push((*command).to_owned());
        }
        invoked.sort();
        invoked.dedup();
        invoked
    }

    fn assert_permissions(source: &str, invoked: &[String]) {
        let permitted = permissions(source);
        for command in invoked {
            let needed = format!("allow-{}", command.replace('_', "-"));
            assert!(
                permitted.contains(&needed),
                "missing permission for {command}"
            );
        }
    }

    let main_invoked = invoked_commands(
        include_str!("../../ui/main.js"),
        &[
            "first_run_request_microphone",
            "first_run_request_system_audio",
        ],
    );
    let settings_invoked = invoked_commands(
        include_str!("../../ui/settings.js"),
        &[
            "first_run_request_microphone",
            "first_run_request_system_audio",
        ],
    );
    let mut invoked = main_invoked.clone();
    invoked.extend(settings_invoked.clone());
    invoked.sort();
    invoked.dedup();

    let mut expected: Vec<String> = PRODUCT_COMMANDS
        .iter()
        .filter(|command| !NATIVE_ONLY_COMMANDS.contains(command))
        .map(|command| (*command).to_owned())
        .collect();
    expected.sort();
    assert_eq!(invoked, expected);
    for source in [
        include_str!("../capabilities/product/main.json"),
        include_str!("../capabilities/product/preview.json"),
    ] {
        assert_permissions(source, &main_invoked);
    }
    assert_permissions(
        include_str!("../capabilities/product/settings-window.json"),
        &settings_invoked,
    );
}

#[test]
fn settings_can_only_manage_audio_access_and_local_speech_models() {
    let settings = permissions(include_str!("../capabilities/product/settings-window.json"));
    assert_eq!(
        settings,
        vec![
            "allow-first-run-permissions",
            "allow-first-run-request-microphone",
            "allow-first-run-request-system-audio",
            "allow-transcript-model-settings",
            "allow-install-transcript-model",
            "allow-remove-transcript-model",
            "allow-note-model-settings",
            "allow-install-note-model",
            "allow-remove-note-model",
        ]
    );

    let script = include_str!("../../ui/settings.js");
    for command in [
        "first_run_permissions",
        "first_run_request_microphone",
        "first_run_request_system_audio",
        "transcript_model_settings",
        "install_transcript_model",
        "remove_transcript_model",
        "note_model_settings",
        "install_note_model",
        "remove_note_model",
    ] {
        assert!(script.contains(command));
    }
    assert!(!script.contains("preview_"));
    assert!(!script.contains("operator_note"));
    assert!(!script.contains("meeting_context"));
}

#[test]
fn desktop_handler_exposes_the_same_small_product_command_set() {
    let source = include_str!("../src/main.rs");
    let handler_start = source
        .find(".invoke_handler(tauri::generate_handler![")
        .unwrap();
    let handler_end = source[handler_start..].find("])\n        .setup").unwrap() + handler_start;
    let handler = &source[handler_start..handler_end];

    for command in PRODUCT_COMMANDS {
        assert!(
            handler.contains(command),
            "handler does not register {command}"
        );
    }
    for retired in [
        "get_desktop_layout",
        "preview_profile",
        "library_create_folder",
        "corpus_search",
    ] {
        assert!(
            !handler.contains(retired),
            "retired command remains in the product handler: {retired}"
        );
    }
}
