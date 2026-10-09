//! In-app updates: check the release feed, offer the newer version, and
//! replace this app with it only while nothing is recording or transcribing.
//!
//! The mechanism is the official `tauri-plugin-updater`. The webview never
//! calls the plugin: four named commands here are the whole surface, so the
//! shell contract's command and permission lists stay the authority. What the
//! plugin does:
//!
//! - **Check** fetches the feed (`plugins.updater.endpoints` in
//!   `tauri.conf.json`) and compares its version with this build's.
//! - **Download** fetches the release's `.app.tar.gz` and verifies its
//!   minisign signature against `plugins.updater.pubkey`, failing closed.
//! - **Install** unpacks the bundle and renames it over this one. macOS keeps
//!   the running process on the old files, so the app must relaunch at once:
//!   until it does, any helper it starts would come from the new bundle and
//!   fail the build-identity checks.
//!
//! # What an update check sends
//!
//! One HTTPS GET of the feed, carrying the plugin's user agent. No meeting,
//! transcript, note, or identifier leaves the Mac. Settings says so beside
//! the switch, and the operator can turn the automatic check off; a manual
//! "Check for Updates…" still works with it off.
//!
//! # Why the install holds the command lock
//!
//! `start_meeting` takes the same lock. Holding it from the final idle check
//! through the bundle swap and the exit means a recording cannot start in
//! between, and the swap never happens under a live capture.
//!
//! # Why the relaunch waits for this process to exit
//!
//! Tauri's `restart` spawns the new binary before the old one exits. Yawn's
//! data-directory writer lock and the single-instance plugin would both turn
//! that overlap into a failed or immediately exiting second instance. A
//! detached shell waits for this pid to disappear, then opens the bundle.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{
    ApplicationState, CaptureState, StartupState, has_pending_transcription_work,
    product_facade::ProductOperationFacade, show_settings_window, sitting_task_active,
    transcription_operation_active,
};

/// Presence turns the automatic check off; absence (the default) leaves it on.
/// Same marker-file idiom as `onboarding.rs`: read fresh, written best-effort.
pub(crate) const AUTOMATIC_OFF_FLAG: &str = "update-check-off.flag";
/// Emitted to the windows when a background check finds a newer version.
/// The payload is the version string.
pub(crate) const UPDATE_AVAILABLE_EVENT: &str = "update-available";
/// Release-test hook: points the check at another HTTPS feed (see
/// DEPLOYMENT.md, "Prove the first update end to end"). The download's
/// signature is still verified against the built-in public key, so this
/// cannot install anything the release key did not sign. It can offer an
/// older signed build under a higher version number, because standalone
/// `tauri signer sign` records no version and `requireSignedVersion` is
/// therefore off; the same holds for anyone who controls the real feed.
const FEED_OVERRIDE_ENV: &str = "YAWN_UPDATE_FEED_URL";
/// The first automatic check waits for startup to settle.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// Checked against a monotonic last-check time on a short tick, so a Mac that
/// slept through the day still checks soon after it wakes.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const TICK: Duration = Duration::from_secs(15 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    Failed(String),
}

impl Phase {
    fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::UpToDate => "up-to-date",
            Self::Available => "available",
            Self::Downloading => "downloading",
            Self::Installing => "installing",
            Self::Failed(_) => "failed",
        }
    }

    fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Downloading | Self::Installing)
    }
}

#[derive(Default)]
struct Inner {
    phase: Phase,
    pending: Option<Update>,
    last_check: Option<Instant>,
    /// The version the windows were last told about, so a daily check does
    /// not re-announce the same release.
    announced: Option<String>,
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Default)]
pub(crate) struct UpdaterState {
    inner: Mutex<Inner>,
    background_started: AtomicBool,
}

/// What Settings renders. Copy lives in the frontend; this carries facts.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateStatus {
    automatic: bool,
    current_version: String,
    state: &'static str,
    available_version: Option<String>,
    notes: Option<String>,
    message: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    /// Why "Update and restart" cannot run right now, when it cannot.
    install_blocked: Option<&'static str>,
}

/// The facts the install gate reads, gathered in one place so the decision is
/// a pure function the tests can drive.
#[derive(Clone, Copy, Debug, Default)]
struct BusyFacts {
    starting: bool,
    capturing: bool,
    transcribing: bool,
    downloading_model: bool,
    analyzing_speakers: bool,
    product_operation: bool,
}

fn install_blocker(facts: BusyFacts) -> Option<&'static str> {
    if facts.starting {
        Some("Wait for Yawn to finish starting.")
    } else if facts.capturing {
        Some("Finish the current recording first.")
    } else if facts.transcribing {
        Some("Wait for transcription to finish.")
    } else if facts.downloading_model {
        Some("Wait for the model download to finish.")
    } else if facts.analyzing_speakers {
        Some("Wait for speaker analysis to finish.")
    } else if facts.product_operation {
        Some("Wait for the current task to finish.")
    } else {
        None
    }
}

fn busy_facts(app: &AppHandle) -> BusyFacts {
    let state = app.state::<ApplicationState>();
    let (capture, startup) = {
        let model = state.model.lock().expect("application model lock");
        (model.reducer.capture(), model.reducer.startup())
    };
    let storage_ready = state
        .storage
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false);
    // Without storage nothing can be queued, and a failed startup is exactly
    // when an update may be the fix, so only the in-memory facts count then.
    // `has_pending_transcription_work` reads a missing workspace as busy.
    let transcribing = if storage_ready {
        has_pending_transcription_work(&state)
    } else {
        transcription_operation_active(&state)
    };
    BusyFacts {
        starting: matches!(startup, StartupState::Checking | StartupState::Retrying),
        // A finished, failed or interrupted meeting holds no microphone; an
        // enrollment sitting does, whatever the meeting capture says.
        capturing: sitting_task_active(&state)
            || matches!(
                capture,
                CaptureState::Arming
                    | CaptureState::Recording
                    | CaptureState::Paused
                    | CaptureState::Stopping
                    | CaptureState::Captured
                    | CaptureState::Transcribing
                    | CaptureState::Summarizing
            ),
        transcribing,
        downloading_model: state.model_install_active.load(Ordering::SeqCst)
            || state.note_model_install_active.load(Ordering::SeqCst)
            || state.nemotron_model_install_active.load(Ordering::SeqCst),
        analyzing_speakers: state.speaker_analysis_active.load(Ordering::SeqCst),
        product_operation: app.state::<ProductOperationFacade>().is_active(),
    }
}

fn automatic_flag_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(AUTOMATIC_OFF_FLAG))
}

fn automatic_enabled_at(flag: Option<&Path>) -> bool {
    flag.is_none_or(|flag| !flag.exists())
}

fn set_automatic_at(flag: &Path, enabled: bool) -> std::io::Result<()> {
    if enabled {
        match std::fs::remove_file(flag) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    } else {
        std::fs::write(flag, b"")
    }
}

fn status_of(
    inner: &Inner,
    automatic: bool,
    current_version: String,
    blocked: Option<&'static str>,
) -> UpdateStatus {
    let pending = inner.pending.as_ref();
    UpdateStatus {
        automatic,
        current_version,
        state: inner.phase.name(),
        available_version: pending.map(|update| update.version.clone()),
        notes: pending.and_then(|update| update.body.clone()),
        message: match &inner.phase {
            Phase::Failed(message) => Some(message.clone()),
            _ => None,
        },
        downloaded_bytes: inner.downloaded,
        total_bytes: inner.total,
        install_blocked: pending.and(blocked),
    }
}

fn current_status(app: &AppHandle) -> UpdateStatus {
    let automatic = automatic_enabled_at(automatic_flag_path(app).as_deref());
    let current_version = app.package_info().version.to_string();
    // Settings polls this every second during a download. The busy facts can
    // fall through to a transcription-queue scan, so they are gathered only
    // when there is an install to gate; `install_and_relaunch` re-checks them
    // in full regardless.
    let installable = {
        let updater = app.state::<UpdaterState>();
        let inner = updater.inner.lock().expect("updater state lock");
        inner.pending.is_some() && matches!(inner.phase, Phase::Available | Phase::Failed(_))
    };
    let blocked = if installable {
        install_blocker(busy_facts(app))
    } else {
        None
    };
    let updater = app.state::<UpdaterState>();
    let inner = updater.inner.lock().expect("updater state lock");
    status_of(&inner, automatic, current_version, blocked)
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder().timeout(REQUEST_TIMEOUT);
    if let Ok(feed) = std::env::var(FEED_OVERRIDE_ENV) {
        let url = feed
            .parse()
            .map_err(|_| format!("{FEED_OVERRIDE_ENV} is not a URL"))?;
        builder = builder
            .endpoints(vec![url])
            .map_err(|error| error.to_string())?;
    }
    builder.build().map_err(|error| error.to_string())
}

fn set_phase(app: &AppHandle, phase: Phase) {
    let updater = app.state::<UpdaterState>();
    let mut inner = updater.inner.lock().expect("updater state lock");
    inner.phase = phase;
}

/// One feed check. `announce` is the background path: it tells the windows
/// about a version once, and stays quiet about failures the operator did not
/// ask for.
async fn check_once(app: &AppHandle, announce: bool) {
    let previous = {
        let updater = app.state::<UpdaterState>();
        let mut inner = updater.inner.lock().expect("updater state lock");
        if inner.phase.busy() {
            return;
        }
        inner.last_check = Some(Instant::now());
        std::mem::replace(&mut inner.phase, Phase::Checking)
    };
    let result = match build_updater(app) {
        Ok(updater) => updater.check().await.map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    let updater = app.state::<UpdaterState>();
    let mut inner = updater.inner.lock().expect("updater state lock");
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            inner.pending = Some(update);
            inner.phase = Phase::Available;
            if announce && inner.announced.as_deref() != Some(version.as_str()) {
                inner.announced = Some(version.clone());
                drop(inner);
                let _ = app.emit(UPDATE_AVAILABLE_EVENT, version);
            }
        }
        Ok(None) => {
            inner.pending = None;
            inner.phase = Phase::UpToDate;
        }
        // A background failure is not news: whatever Settings showed before
        // stays, including an update an earlier check found.
        Err(error) => {
            inner.phase = if announce {
                previous
            } else {
                Phase::Failed(format!("Yawn could not check for updates: {error}"))
            };
        }
    }
}

/// Starts the once-a-day automatic check. Idempotent: setup calls it once,
/// and the guard keeps a second caller from starting a second loop.
pub(crate) fn start_background_checks(app: AppHandle) {
    let updater = app.state::<UpdaterState>();
    if updater.background_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let worker = app.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            let app = worker;
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                let due = {
                    let updater = app.state::<UpdaterState>();
                    let inner = updater.inner.lock().expect("updater state lock");
                    inner
                        .last_check
                        .is_none_or(|last| last.elapsed() >= CHECK_INTERVAL)
                };
                if due && automatic_enabled_at(automatic_flag_path(&app).as_deref()) {
                    tauri::async_runtime::block_on(check_once(&app, true));
                }
                std::thread::sleep(TICK);
            }
        });
    if spawned.is_err() {
        updater.background_started.store(false, Ordering::SeqCst);
    }
}

/// The app menu's "Check for Updates…": Settings shows the result.
pub(crate) fn check_from_menu(app: &AppHandle) {
    let _ = show_settings_window(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check_once(&app, false).await });
}

#[tauri::command]
pub(crate) fn update_status(app: AppHandle) -> UpdateStatus {
    current_status(&app)
}

#[tauri::command]
pub(crate) fn set_automatic_update_check(
    app: AppHandle,
    enabled: bool,
) -> Result<UpdateStatus, String> {
    let flag = automatic_flag_path(&app).ok_or("Yawn could not find its settings folder.")?;
    set_automatic_at(&flag, enabled)
        .map_err(|_| "Yawn could not save this setting.".to_string())?;
    Ok(current_status(&app))
}

#[tauri::command]
pub(crate) fn check_for_updates(app: AppHandle) -> UpdateStatus {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { check_once(&handle, false).await });
    let mut status = current_status(&app);
    if !matches!(status.state, "downloading" | "installing") {
        status.state = "checking";
    }
    status
}

#[tauri::command]
pub(crate) fn install_update(app: AppHandle) -> Result<UpdateStatus, String> {
    if let Some(reason) = install_blocker(busy_facts(&app)) {
        return Err(reason.into());
    }
    let update = {
        let updater = app.state::<UpdaterState>();
        let mut inner = updater.inner.lock().expect("updater state lock");
        if inner.phase.busy() {
            return Err("An update is already in progress.".into());
        }
        let update = inner.pending.clone().ok_or("Check for updates first.")?;
        inner.phase = Phase::Downloading;
        inner.downloaded = 0;
        inner.total = None;
        update
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let progress = handle.clone();
        let downloaded = update
            .download(
                move |chunk, total| {
                    let updater = progress.state::<UpdaterState>();
                    if let Ok(mut inner) = updater.inner.lock() {
                        inner.downloaded += chunk as u64;
                        inner.total = total;
                    }
                },
                || {},
            )
            .await;
        let bytes = match downloaded {
            Ok(bytes) => bytes,
            Err(error) => {
                set_phase(
                    &handle,
                    Phase::Failed(format!("The update could not be downloaded: {error}")),
                );
                return;
            }
        };
        let finisher = handle.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            install_and_relaunch(&finisher, &update, &bytes)
        })
        .await;
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(message)) => set_phase(&handle, Phase::Failed(message)),
            Err(_) => set_phase(
                &handle,
                Phase::Failed("The update stopped unexpectedly.".into()),
            ),
        }
    });
    Ok(current_status(&app))
}

/// The order the last gate must keep. The busy check reads the shared
/// operation slot, so it runs before this install claims that slot; checked
/// after, it would only ever see the install's own claim and refuse every
/// time. A claim that fails here means another operation took the slot since
/// the check, which is the same refusal.
fn gate_install<C>(
    blocker: Option<&'static str>,
    claim: impl FnOnce() -> Result<C, String>,
) -> Result<C, String> {
    if let Some(reason) = blocker {
        return Err(format!(
            "The update was downloaded but not installed. {reason}"
        ));
    }
    claim().map_err(|_| {
        "The update was downloaded but not installed. Wait for the current task to finish."
            .to_string()
    })
}

/// Holds the command lock from the last idle check to the exit, so a
/// recording cannot start between them. Returns only on failure.
fn install_and_relaunch(app: &AppHandle, update: &Update, bytes: &[u8]) -> Result<(), String> {
    let state = app.state::<ApplicationState>();
    let _command = state
        .command_lock
        .lock()
        .map_err(|_| "Yawn is busy. Try again in a moment.".to_string())?;
    let operations = app.state::<ProductOperationFacade>();
    let _operation = gate_install(install_blocker(busy_facts(app)), || {
        operations.claim_runtime_change()
    })?;
    set_phase(app, Phase::Installing);
    let bundle = std::env::current_exe()
        .ok()
        .and_then(|exe| tauri_plugin_updater::extract_path_from_executable(&exe).ok())
        .ok_or("Yawn could not find its own app bundle.")?;
    update
        .install(bytes)
        .map_err(|error| format!("The update could not be installed: {error}"))?;
    relaunch_command(std::process::id(), &bundle, Path::new(OPEN))
        .spawn()
        .map_err(|_| "The update is installed. Quit and reopen Yawn to finish.".to_string())?;
    app.exit(0);
    Ok(())
}

const OPEN: &str = "/usr/bin/open";

/// A detached shell that waits for `pid` to exit, then runs `opener` on
/// `bundle` (`/usr/bin/open` outside tests). Its own process group keeps it
/// out of anything that signals Yawn's group.
fn relaunch_command(pid: u32, bundle: &Path, opener: &Path) -> Command {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg("while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; exec \"$3\" \"$2\"")
        .arg("yawn-relaunch")
        .arg(pid.to_string())
        .arg(bundle)
        .arg(opener)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_refused_for_each_kind_of_work_in_progress() {
        assert_eq!(install_blocker(BusyFacts::default()), None);
        let cases = [
            BusyFacts {
                starting: true,
                ..Default::default()
            },
            BusyFacts {
                capturing: true,
                ..Default::default()
            },
            BusyFacts {
                transcribing: true,
                ..Default::default()
            },
            BusyFacts {
                downloading_model: true,
                ..Default::default()
            },
            BusyFacts {
                analyzing_speakers: true,
                ..Default::default()
            },
            BusyFacts {
                product_operation: true,
                ..Default::default()
            },
        ];
        for facts in cases {
            assert!(install_blocker(facts).is_some(), "{facts:?}");
        }
    }

    #[test]
    fn an_idle_install_gets_through_the_gate_and_a_second_one_does_not() {
        // A real slot stands in for the product-operation facade: the busy
        // check reads it, the claim fills it. Checking after claiming would
        // refuse the first install too.
        let slot = std::sync::Mutex::new(None::<()>);
        let busy = || {
            install_blocker(BusyFacts {
                product_operation: slot.lock().unwrap().is_some(),
                ..Default::default()
            })
        };
        let claim = || {
            let mut held = slot.lock().unwrap();
            if held.is_some() {
                return Err("taken".to_string());
            }
            *held = Some(());
            Ok(())
        };
        // The order this replaces: claim first, then check. The check then
        // sees the install's own claim and calls an idle Yawn busy.
        claim().unwrap();
        assert!(
            busy().is_some(),
            "claim-then-check reads its own claim as busy"
        );
        *slot.lock().unwrap() = None;
        assert!(
            gate_install(busy(), claim).is_ok(),
            "an idle Yawn must install"
        );
        let refused = gate_install(busy(), claim).unwrap_err();
        assert!(
            refused.contains("Wait for the current task to finish."),
            "{refused}"
        );
        // A slot taken between the check and the claim is refused as well.
        let refused = gate_install(None, || Err::<(), _>("taken".to_string())).unwrap_err();
        assert!(
            refused.contains("Wait for the current task to finish."),
            "{refused}"
        );
        // A busy reason stops the gate before it claims anything.
        let mut claimed = false;
        let refused = gate_install(Some("Finish the current recording first."), || {
            claimed = true;
            Ok(())
        })
        .unwrap_err();
        assert!(!claimed);
        assert!(refused.ends_with("Finish the current recording first."));
    }

    #[test]
    fn a_recording_outranks_every_other_reason() {
        let facts = BusyFacts {
            capturing: true,
            transcribing: true,
            downloading_model: true,
            ..Default::default()
        };
        assert_eq!(
            install_blocker(facts),
            Some("Finish the current recording first.")
        );
    }

    #[test]
    fn the_automatic_check_is_on_until_the_flag_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        let flag = dir.path().join(AUTOMATIC_OFF_FLAG);
        assert!(automatic_enabled_at(Some(&flag)));
        set_automatic_at(&flag, false).unwrap();
        assert!(!automatic_enabled_at(Some(&flag)));
        set_automatic_at(&flag, false).unwrap();
        set_automatic_at(&flag, true).unwrap();
        assert!(automatic_enabled_at(Some(&flag)));
        // Turning it on twice is not an error either.
        set_automatic_at(&flag, true).unwrap();
        // With no settings folder at all, the default holds.
        assert!(automatic_enabled_at(None));
    }

    #[test]
    fn status_reports_a_block_only_when_there_is_something_to_install() {
        let inner = Inner::default();
        let status = status_of(
            &inner,
            true,
            "0.6.15".into(),
            Some("Finish the current recording first."),
        );
        assert_eq!(status.state, "idle");
        assert_eq!(status.install_blocked, None);
        assert_eq!(status.available_version, None);

        let failed = Inner {
            phase: Phase::Failed("offline".into()),
            ..Default::default()
        };
        let status = status_of(&failed, false, "0.6.15".into(), None);
        assert_eq!(status.state, "failed");
        assert_eq!(status.message.as_deref(), Some("offline"));
        assert!(!status.automatic);
    }

    #[test]
    fn checking_downloading_and_installing_are_the_busy_phases() {
        for phase in [Phase::Checking, Phase::Downloading, Phase::Installing] {
            assert!(phase.busy(), "{phase:?}");
        }
        for phase in [
            Phase::Idle,
            Phase::UpToDate,
            Phase::Available,
            Phase::Failed(String::new()),
        ] {
            assert!(!phase.busy(), "{phase:?}");
        }
    }

    #[test]
    fn the_shipped_config_is_one_the_plugin_accepts() {
        // The plugin reads plugins.updater at launch; a block it rejects
        // would stop Yawn starting at all.
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater: tauri_plugin_updater::Config =
            serde_json::from_value(config["plugins"]["updater"].clone()).unwrap();
        assert!(!updater.pubkey.is_empty());
        assert_eq!(updater.endpoints.len(), 1);
        assert_eq!(updater.endpoints[0].scheme(), "https");
        assert!(
            updater.endpoints[0]
                .path()
                .ends_with("/updates/latest.json")
        );
        assert!(!updater.dangerous_insecure_transport_protocol);
        assert!(!updater.dangerous_accept_invalid_certs);
    }

    #[test]
    fn the_release_feed_script_writes_what_the_plugin_reads() {
        // Runs the real scripts/prepare-update-feed.py, then parses its output
        // with the plugin's own feed type and looks up this Mac's entry.
        let dir = tempfile::TempDir::new().unwrap();
        let package = dir.path().join("Yawn-0.6.99-macos-arm64.app.tar.gz");
        std::fs::write(&package, b"package").unwrap();
        let signature = "untrusted comment: signature from tauri secret key\nRUQ=\ntrusted comment: timestamp:1\tfile:x\nAA==\n";
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, signature);
        std::fs::write(
            dir.path().join("Yawn-0.6.99-macos-arm64.app.tar.gz.sig"),
            &encoded,
        )
        .unwrap();
        let feed = dir.path().join("latest.json");
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/prepare-update-feed.py");
        let status = Command::new("python3")
            .arg(&script)
            .arg("--package")
            .arg(&package)
            .arg("--base-url")
            .arg("https://releases.example/")
            .arg("--output")
            .arg(&feed)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let release: tauri_plugin_updater::RemoteRelease =
            serde_json::from_str(&std::fs::read_to_string(&feed).unwrap()).unwrap();
        assert_eq!(release.version.to_string(), "0.6.99");
        assert_eq!(
            release.download_url("darwin-aarch64").unwrap().as_str(),
            "https://releases.example/updates/Yawn-0.6.99-macos-arm64.app.tar.gz"
        );
        assert_eq!(release.signature("darwin-aarch64").unwrap(), &encoded);
    }

    #[test]
    fn relaunch_opens_the_bundle_with_the_system_opener() {
        let command = relaunch_command(4242, Path::new("/Applications/Yawn.app"), Path::new(OPEN));
        assert_eq!(command.get_program(), "/bin/sh");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &args[2..],
            [
                "yawn-relaunch",
                "4242",
                "/Applications/Yawn.app",
                "/usr/bin/open"
            ]
        );
    }

    #[test]
    fn relaunch_runs_the_opener_only_after_the_waited_for_process_is_gone() {
        // A real child stands in for Yawn, and `touch` stands in for `open`
        // so the opener's run is observable as a file. The marker must not
        // exist while the child lives, and must exist once it has exited.
        let dir = tempfile::TempDir::new().unwrap();
        let marker = dir.path().join("opened");
        let mut stand_in = Command::new("/bin/sleep").arg("1").spawn().unwrap();
        let mut relaunch = relaunch_command(stand_in.id(), &marker, Path::new("/usr/bin/touch"))
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            !marker.exists(),
            "opened while the old process was still running"
        );
        stand_in.wait().unwrap();
        assert!(relaunch.wait().unwrap().success());
        assert!(marker.exists(), "never opened after the old process exited");
    }
}
