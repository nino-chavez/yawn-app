//! Global hotkey lifecycle for operator-note capture during an active
//! recording (roadmap intake I2, `docs/roadmap.md` "Category-review intake —
//! 2026-08-31"). The governing constraint from that entry is verbatim:
//! "Notes land only in the operator canvas; no floating sticky-note surface,
//! no new data model." So this module does exactly one thing when the
//! hotkey fires: bring the existing main window forward and ask the
//! frontend to focus the operator-note editor that is already part of the
//! capture view. It never opens a new window, never renders an overlay, and
//! never stores anything of its own.
//!
//! Lifecycle: the hotkey is armed only for the lifetime of one capture
//! attempt. `activate` is called once a meeting starts recording;
//! `deactivate` is called the moment that attempt stops or fails. There is
//! deliberately no registration at application launch — an idle app has no
//! global hotkey at all.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::{ACTIVE_WINDOW_LABEL, ApplicationState, write_diagnostic};

/// Event name the frontend listens for. `main.js` focuses the operator-note
/// textarea and places the caret at the end; this module only decides when
/// to ask for that, never how the editor itself behaves.
pub const NOTE_CAPTURE_FOCUS_EVENT: &str = "note-capture-hotkey";

/// Ctrl+Option+Y (⌃⌥Y). Fixed for this packet — there is no settings surface
/// to change it yet. A later packet can make this configurable behind the
/// same event contract without touching the lifecycle below.
fn note_capture_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyY)
}

/// Registers the plugin's runtime handler. Call once, during `.setup()`,
/// before any meeting can start. This only teaches the plugin what to do
/// *if* the shortcut is registered — it does not arm the hotkey itself.
/// Arming is `activate`'s job, scoped to an active capture attempt.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, triggered, event| {
                if *triggered == note_capture_shortcut() && event.state() == ShortcutState::Pressed
                {
                    summon_note_capture(app);
                }
            })
            .build(),
    )
}

fn summon_note_capture(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ACTIVE_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(NOTE_CAPTURE_FOCUS_EVENT, ());
}

/// Arms the hotkey. Called once a capture attempt has started recording.
///
/// macOS can refuse this registration outright (most commonly a missing
/// Input Monitoring / Accessibility grant for the app). On failure this
/// writes a private diagnostic and returns — the meeting keeps recording
/// without the hotkey. It never claims the hotkey is armed when
/// registration failed, and this packet builds no permission-prompt UI.
pub fn activate(app: &AppHandle) {
    let shortcut = note_capture_shortcut();
    let global_shortcut = app.global_shortcut();
    if global_shortcut.is_registered(shortcut) {
        return;
    }
    if let Err(error) = global_shortcut.register(shortcut) {
        let state = app.state::<ApplicationState>();
        write_diagnostic(
            &state,
            "note_capture_hotkey_registration_failed",
            &error.to_string(),
        );
    }
}

/// Disarms the hotkey. Safe to call whether or not it is currently
/// registered. Called from every path that ends or fails a capture
/// attempt, not only the explicit-stop happy path.
pub fn deactivate(app: &AppHandle) {
    let shortcut = note_capture_shortcut();
    let global_shortcut = app.global_shortcut();
    if global_shortcut.is_registered(shortcut) {
        let _ = global_shortcut.unregister(shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These two are the only pure, headless-testable facts about this
    // module: the binding is stable and matches the documented default.
    // Everything else here (plugin install, OS-level register/unregister,
    // window focus, event emission) needs a live Tauri app/event loop and
    // is not exercised by `cargo test`.

    #[test]
    fn shortcut_is_stable_across_calls() {
        assert_eq!(note_capture_shortcut(), note_capture_shortcut());
    }

    #[test]
    fn shortcut_matches_the_documented_default_binding() {
        let expected = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyY);
        assert_eq!(note_capture_shortcut(), expected);
    }
}
