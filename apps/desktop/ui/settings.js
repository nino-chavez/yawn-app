import { mergePermissions, updatePresentation } from "./view-model.mjs";
import { APPEARANCES, getAppearance, setAppearance } from "./appearance.mjs";

// The Settings page (Rail, 2026-10-16): mounted inside the main window's
// document pane, opened from the sidebar gear, the Settings menu item and ⌘,.
// Every lookup and listener is scoped to `root`, so the page can live in the
// main window beside the library without reaching into it.
export const SETTINGS_MARKUP = `
      <section id="appearance" class="settings-group" tabindex="-1" aria-labelledby="appearance-title">
        <div class="settings-group-title">
          <h2 id="appearance-title">Appearance</h2>
        </div>
        <div class="settings-group-box">
          <div class="permission-line">
            <div class="permission-copy"><strong>Theme</strong><p>System follows the Mac's light or dark setting.</p></div>
            <div class="appearance-choice" role="group" aria-label="Theme" id="appearance-choice"></div>
          </div>
        </div>
      </section>
      <section id="recording" class="settings-group" tabindex="-1" aria-labelledby="audio-title">
        <div class="settings-group-title">
          <h2 id="audio-title">Recording</h2>
          <button class="quiet-button" type="button" data-settings-action="refresh">Check again</button>
        </div>
        <div class="settings-group-box">
          <div id="permissions" class="permission-list" aria-live="polite"></div>
        </div>
        <p id="message" class="message" role="status"></p>
      </section>
      <section id="transcription" class="settings-group" tabindex="-1" aria-labelledby="model-title">
        <div class="settings-group-title">
          <h2 id="model-title">Transcription</h2>
        </div>
        <div class="settings-group-box">
          <div id="transcription-engine" class="model-list" aria-live="polite"></div>
          <div id="models" class="model-list" aria-live="polite"></div>
          <p id="model-message" class="card-message" role="status"></p>
        </div>
      </section>
      <section id="speakers" class="settings-group" tabindex="-1" aria-labelledby="speaker-model-title">
        <div class="settings-group-title">
          <h2 id="speaker-model-title">Speakers</h2>
        </div>
        <div class="settings-group-box">
          <div id="speaker-model" class="model-list" aria-live="polite"></div>
          <p id="speaker-model-message" class="card-message" role="status"></p>
          <p class="card-message">Model: <a href="https://huggingface.co/nvidia/Nemotron-3-Diarization" target="_blank" rel="noreferrer">NVIDIA Nemotron-3-Diarization</a> (openmdw-1.1).</p>
        </div>
      </section>
      <section id="storage" class="settings-group" tabindex="-1" aria-labelledby="privacy-title">
        <div class="settings-group-title">
          <h2 id="privacy-title">Storage</h2>
        </div>
        <div class="settings-group-box">
          <dl class="privacy-rows">
            <div class="privacy-row">
              <dt>Meetings</dt>
              <dd>On this Mac, until you delete them. A deleted meeting stays in Trash for 30 days.</dd>
            </div>
            <div class="privacy-row">
              <dt>Audio</dt>
              <dd>Kept 1, 7 or 30 days, chosen at the start of each meeting.</dd>
            </div>
            <div class="privacy-row">
              <dt>Transcription</dt>
              <dd>Runs on this Mac, with Apple speech or the downloaded model selected above.</dd>
            </div>
          </dl>
        </div>
      </section>
      <section id="updates" class="settings-group" tabindex="-1" aria-labelledby="updates-title">
        <div class="settings-group-title">
          <h2 id="updates-title">Updates</h2>
          <button class="quiet-button" type="button" data-settings-action="check-updates">Check now</button>
        </div>
        <div class="settings-group-box">
          <div id="update-status" class="permission-list" aria-live="polite"></div>
          <label class="update-automatic">
            <span>Check for updates automatically</span>
            <input id="update-automatic" type="checkbox" checked />
          </label>
          <p id="update-disclosure" class="card-message">Once a day, Yawn downloads a small file from the release server that names the newest version. It sends no meeting data or account details; like any download, the server can see this Mac’s internet address.</p>
          <p id="update-message" class="card-message" role="status"></p>
        </div>
      </section>
      <p class="privacy-sentence">Nothing leaves this Mac. Yawn has no account and makes no network calls with meeting data; the optional update check above is its only connection.</p>
`;

export function mountSettings(root = document) {
  if (root !== document && !root.firstElementChild) root.innerHTML = SETTINGS_MARKUP;
  const invoke = window.__TAURI__?.core?.invoke;
  const permissionsRoot = root.querySelector("#permissions");
  const modelsRoot = root.querySelector("#models");
  const transcriptionEngineRoot = root.querySelector("#transcription-engine");
  const modelMessage = root.querySelector("#model-message");
  const speakerModelRoot = root.querySelector("#speaker-model");
  const speakerModelMessage = root.querySelector("#speaker-model-message");
  const message = root.querySelector("#message");
  const updateRoot = root.querySelector("#update-status");
  const updateAutomatic = root.querySelector("#update-automatic");
  const updateMessage = root.querySelector("#update-message");
  const updateCheckButton = root.querySelector('[data-settings-action="check-updates"]');
  const updateDisclosure = root.querySelector("#update-disclosure");

  let permissions = null;
  let models = null;
  let transcriptionEngine = null;
  let transcriptionEngineError = "";
  let transcriptionEnginePoll = null;
  let modelLoadError = "";
  // D-MODEL (roadmap): a runtime staged without downloadable speech models
  // rejects `transcript_model_settings` with this exact sentence -- a fact
  // about the build, not a check that failed. The bare `catch {}` this used to
  // have discarded the backend's message entirely, so that fact rendered
  // identically to "Yawn could not check the saved speech model. Retrying…"
  // forever, on a build that will never resolve it because there is nothing to
  // check. `modelBuiltIn` is true only for that one message; every other
  // rejection keeps the genuine-failure path.
  const MODEL_BUILT_IN_MARKER = "does not use downloadable speech models";
  let modelBuiltIn = false;
  let modelPoll = null;
  let speakerModel = null;
  let speakerModelPoll = null;
  let updateStatus = null;
  let updatePoll = null;
  // Set when the page unmounts: a refresh whose reply lands after that must
  // not arm another poll, or it would run for the rest of the session.
  let disposed = false;

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  function humanize(value) {
    return String(value || "unknown").replace(/[-_]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
  }

  function tone(value) {
    return ["authorized", "Selected", "Ready"].includes(value) ? "ready" : ["denied", "restricted", "unavailable", "unsupported", "unknown"].includes(value) ? "attention" : "neutral";
  }

  function byteSizeLabel(bytes) {
    const value = Math.max(0, Number(bytes) || 0);
    if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(2)} GB`;
    if (value >= 1_000_000) return `${Math.round(value / 1_000_000)} MB`;
    return `${Math.round(value / 1_000)} KB`;
  }

  function row(title, value, detail, action = "") {
    return `
      <div class="permission-line">
        <div class="permission-copy"><strong>${escapeHtml(title)}</strong><p>${escapeHtml(detail)}</p></div>
        ${action || `<span class="state" data-tone="${tone(value)}">${escapeHtml(humanize(value))}</span>`}
      </div>
    `;
  }

  function render() {
    if (!permissions) {
      permissionsRoot.innerHTML = row("Checking access", "checking", "Yawn is checking the permissions this Mac reports.");
      return;
    }
    const microphoneAction = permissions.microphone === "not-determined"
      ? `<button class="allow-button" type="button" data-settings-action="microphone">Allow microphone</button>`
      : "";
    const systemAction = permissions.systemAudio === "unmeasured"
      ? `<button class="allow-button" type="button" data-settings-action="system-audio">Allow system audio</button>`
      : "";
    permissionsRoot.innerHTML = [
      row("Microphone", permissions.microphone, "Yawn uses this to capture your voice.", microphoneAction),
      row("System audio", permissions.systemAudio, "Yawn checks access to audio from your meeting app. This check does not save audio.", systemAction),
    ].join("");
  }

  function renderModels() {
    // D-MODEL: a build fact, not a failure -- no attention tone, no "Retrying…",
    // and (see scheduleModelPoll) no retry poll, since there is nothing this
    // build will ever have to check. Plain row, no state pill: this isn't one
    // of "checking" / "unavailable" / "in use", so `row()`'s pill would only
    // invent a status this state doesn't have.
    if (modelBuiltIn) {
      modelsRoot.innerHTML = `
        <div class="permission-line">
          <div class="permission-copy"><strong>Speech model</strong><p>This build ships its speech model. There is nothing to choose here.</p></div>
        </div>
      `;
      modelMessage.textContent = "";
      modelMessage.dataset.tone = "neutral";
      return;
    }
    if (!models) {
      // A failed check must not read the same as a pending one — the row title
      // said "Checking speech model" either way, which is what made a genuine
      // failure look identical to (and get mistaken for) a check that was
      // simply still running. See scheduleModelPoll for the matching retry fix.
      modelsRoot.innerHTML = modelLoadError
        ? row("Couldn't check speech model", "unavailable", `${modelLoadError} Retrying…`)
        : row("Checking speech model", "checking", "Yawn is checking what is stored on this Mac.");
      // Refit R11 (all-surfaces-2765401-installed-cold.md finding 03/06): the
      // row above already states the one error once (title + "Retrying…"
      // detail); this footnote used to restate the exact same sentence a
      // second time. One error, one line — the footnote stays empty here.
      modelMessage.textContent = "";
      modelMessage.dataset.tone = "neutral";
      return;
    }
    const busy = models.changeActive;
    const whisperInUse = !transcriptionEngine || transcriptionEngine.selected === "whisper";
    modelsRoot.setAttribute("aria-busy", busy ? "true" : "false");
    modelsRoot.innerHTML = models.options.map((option) => {
      const selected = models.selectedModelId === option.id;
      const disabled = !models.canChange || busy || transcriptionEngine?.operationActive || transcriptionEngine?.canChange === false;
      const status = option.active && whisperInUse
        ? `<span class="state" data-tone="ready">Selected</span>`
        : option.active
          ? `<span class="state">Stored</span>`
        : option.stored
          ? `<span class="state">On this Mac</span>`
          : `<span class="model-size">${byteSizeLabel(option.downloadBytes)}</span>`;
      const progress = selected && busy
        ? `<div class="model-progress"><progress max="${Math.max(1, models.totalBytes)}" value="${Math.min(models.totalBytes, models.downloadedBytes)}"></progress><small>${models.state === "verifying" ? "Checking the downloaded files…" : `${byteSizeLabel(models.downloadedBytes)} of ${byteSizeLabel(models.totalBytes)}`}</small></div>`
        : "";
      // Refit R18: one primary action per group. An unstored option is the primary
      // (it acquires a resource); a stored option is secondary (it switches to
      // what's already available). Permission rows are always primary (allow access).
      const isPrimaryUseAction = !option.stored;
      const use = option.active && whisperInUse
        ? ""
        : `<button class="${isPrimaryUseAction ? "allow-button" : "quiet-button"}" type="button" data-settings-action="use-model" data-model-id="${escapeHtml(option.id)}" ${disabled ? "disabled" : ""}>${option.stored ? "Use this model" : "Download and use"}</button>`;
      const remove = option.stored && !option.active
        ? `<button class="quiet-button" type="button" data-settings-action="remove-model" data-model-id="${escapeHtml(option.id)}" data-model-title="${escapeHtml(option.title)}" ${disabled ? "disabled" : ""}>Remove download</button>`
        : "";
      return `
        <div class="model-line">
          <div class="model-copy">
            <div class="model-title-row"><strong>${escapeHtml(option.title)}</strong>${status}</div>
            <p>${escapeHtml(option.detail)}</p>
            ${progress}
          </div>
          <div class="model-actions">${use}${remove}</div>
        </div>
      `;
    }).join("");
    const hasInactiveDownload = models.options.some((option) => option.stored && !option.active);
    modelMessage.textContent = models.error || models.unavailableReason || (hasInactiveDownload
      ? "Inactive downloads can be removed to free space."
      : "");
    modelMessage.dataset.tone = models.error ? "attention" : "neutral";
  }

  function renderTranscriptionEngine() {
    if (!transcriptionEngineRoot) return;
    if (!transcriptionEngine) {
      transcriptionEngineRoot.innerHTML = row("Checking speech engine", "checking", transcriptionEngineError || "Yawn is checking what can transcribe on this Mac.");
      return;
    }
    const apple = transcriptionEngine.apple || {};
    const busy = transcriptionEngine.operationActive || !transcriptionEngine.canChange;
    const appleReady = apple.state === "ready";
    const action = !busy && appleReady && transcriptionEngine.selected !== "apple-native"
      ? `<button class="quiet-button" type="button" data-settings-action="use-apple-speech">Use Apple speech</button>`
      : !busy && ["assets-required", "failed"].includes(apple.state)
        ? `<button class="allow-button" type="button" data-settings-action="prepare-apple-speech">Download Apple speech</button>`
        : "";
    const detail = transcriptionEngineError || (apple.state === "assets-required"
      ? `${apple.reason || "Apple speech needs preparation."} Apple manages this download in macOS.`
      : apple.state === "installing"
        ? (apple.reason || "Preparing Apple speech on this Mac…")
        : apple.state === "failed" || apple.state === "unavailable"
          ? (apple.reason || "Apple speech is unavailable on this Mac.")
          : appleReady
            ? "Ready to transcribe with Apple’s on-device speech engine. No Yawn speech-model download is needed."
            : "The downloaded local speech model is selected.");
    const appleStatus = transcriptionEngine.selected === "apple-native" ? "Selected" : appleReady ? "Ready" : humanize(apple.state);
    transcriptionEngineRoot.innerHTML = row("Apple speech", appleStatus, detail, action);
    transcriptionEngineRoot.setAttribute("aria-busy", busy ? "true" : "false");
  }

  function scheduleTranscriptionEnginePoll() {
    if (disposed) return;
    clearTimeout(transcriptionEnginePoll);
    if (transcriptionEngine?.operationActive) {
      transcriptionEnginePoll = setTimeout(() => void refreshTranscriptionEngine(), 500);
    } else if (!transcriptionEngine) {
      transcriptionEnginePoll = setTimeout(() => void refreshTranscriptionEngine(), 2000);
    }
  }

  function renderSpeakerModel() {
    if (!speakerModel) {
      speakerModelRoot.innerHTML = row("Checking speaker analysis", "checking", "Yawn is checking the optional local model.");
      return;
    }
    const busy = speakerModel.changeActive;
    const unavailable = speakerModel.unavailableReason;
    speakerModelRoot.setAttribute("aria-busy", busy ? "true" : "false");
    const status = speakerModel.stored ? `<span class="state" data-tone="ready">Ready</span>` : `<span class="model-size">${byteSizeLabel(speakerModel.totalBytes)}</span>`;
    const action = speakerModel.stored ? "" : `<button class="allow-button" type="button" data-settings-action="install-speaker-model" ${!speakerModel.canChange || busy ? "disabled" : ""}>Download model</button>`;
    const progress = busy ? `<div class="model-progress"><progress max="${Math.max(1, speakerModel.totalBytes)}" value="${Math.min(speakerModel.totalBytes, speakerModel.downloadedBytes || 0)}"></progress><small>${byteSizeLabel(speakerModel.downloadedBytes || 0)} of ${byteSizeLabel(speakerModel.totalBytes)}</small></div>` : "";
    speakerModelRoot.innerHTML = `<div class="model-line"><div class="model-copy"><div class="model-title-row"><strong>Anonymous speaker analysis</strong>${status}</div><p>Finds anonymous microphone speaker clusters. It never names people or changes your transcript.</p>${progress}</div><div class="model-actions">${action}</div></div>`;
    speakerModelMessage.textContent = unavailable || (busy ? "Downloading and checking the speaker-analysis model…" : speakerModel.stored ? "Ready when you choose Analyze speakers for a finished meeting." : speakerModel.state === "download-failed" ? "Download failed. Check your connection and free space, then try again." : "Download this model before analyzing speakers.");
    speakerModelMessage.dataset.tone = unavailable || speakerModel.state === "download-failed" ? "attention" : "neutral";
  }

  function scheduleSpeakerModelPoll() {
    if (disposed) return;
    clearTimeout(speakerModelPoll);
    if (speakerModel?.changeActive) speakerModelPoll = setTimeout(() => void refreshSpeakerModel(), 500);
    else if (!speakerModel) speakerModelPoll = setTimeout(() => void refreshSpeakerModel(), 2000);
  }

  async function refreshSpeakerModel() {
    if (!invoke) return;
    try { speakerModel = await invoke("nemotron_model_settings"); }
    catch (error) { speakerModel = { unavailableReason: String(error || "Yawn could not check speaker analysis."), changeActive: false, canChange: false, stored: false, totalBytes: 107012128 }; }
    renderSpeakerModel();
    scheduleSpeakerModelPoll();
  }

  async function installSpeakerModel() {
    if (!invoke) return;
    try { speakerModel = await invoke("install_nemotron_model"); }
    catch (error) { speakerModelMessage.textContent = String(error || "Yawn could not download the speaker-analysis model."); speakerModelMessage.dataset.tone = "attention"; return; }
    renderSpeakerModel();
    scheduleSpeakerModelPoll();
  }

  // Root cause of the row freezing at "Checking speech model" forever (cold
  // review, all-surfaces-bfa0a80-installed-cold.md): this only rescheduled
  // while a model change was actively downloading. A `transcript_model_settings`
  // call that failed even once set `models` to null and stopped here — with no
  // snapshot, `models?.changeActive` reads undefined, so the row never asked
  // again. The `!models` arm below is the fix: keep asking (slower than the
  // active-download cadence) for as long as there is no snapshot yet, whether
  // that is the first check still pending or a check that just failed.
  function scheduleModelPoll() {
    if (disposed) return;
    clearTimeout(modelPoll);
    // D-MODEL: `modelBuiltIn` is a permanent fact about this build, not a
    // transient failure -- nothing will change on a retry, so this stops
    // polling instead of re-asking a question this build has already answered.
    if (modelBuiltIn) return;
    if (models?.changeActive) {
      modelPoll = setTimeout(() => void refreshModels(), 500);
    } else if (!models) {
      modelPoll = setTimeout(() => void refreshModels(), 2000);
    }
  }

  async function refreshModels() {
    if (!invoke) {
      modelMessage.textContent = "Open Settings from the Yawn desktop app.";
      return;
    }
    try {
      models = await invoke("transcript_model_settings");
      modelLoadError = "";
      modelBuiltIn = false;
    } catch (error) {
      models = null;
      // D-MODEL: read the rejection instead of discarding it (the bare
      // `catch {}` this used to be), so the one message that means "this build
      // ships a fixed speech model" can be told apart from every other
      // rejection, which stays a genuine, retried failure.
      const rejection = String(error || "");
      modelBuiltIn = rejection.includes(MODEL_BUILT_IN_MARKER);
      modelLoadError = modelBuiltIn ? "" : "Yawn could not check the saved speech model.";
    }
    renderModels();
    scheduleModelPoll();
  }

  async function useModel(modelId) {
    if (!invoke) return;
    modelMessage.textContent = "Starting the model change…";
    try {
      await invoke("install_transcript_model", { modelId });
      await refreshModels();
      await refreshTranscriptionEngine();
    } catch (error) {
      modelMessage.textContent = String(error || "Yawn could not change the speech model.");
      modelMessage.dataset.tone = "attention";
    }
  }

  async function refreshTranscriptionEngine() {
    if (!invoke) return;
    const wasChanging = transcriptionEngine?.operationActive;
    try {
      transcriptionEngine = await invoke("get_transcription_engine_settings");
      transcriptionEngineError = "";
      if (wasChanging && !transcriptionEngine.operationActive) {
        await refreshModels();
      }
    } catch {
      transcriptionEngine = null;
      transcriptionEngineError = "Yawn could not check the speech engine.";
    }
    renderTranscriptionEngine();
    renderModels();
    scheduleTranscriptionEnginePoll();
  }

  async function prepareAppleSpeech() {
    if (!invoke) return;
    try {
      await invoke("install_apple_speech_assets");
      await refreshTranscriptionEngine();
    } catch (error) {
      transcriptionEngineError = String(error || "Yawn could not prepare Apple speech.");
      renderTranscriptionEngine();
    }
  }

  async function useAppleSpeech() {
    if (!invoke || transcriptionEngine?.canChange === false) return;
    try {
      await invoke("select_transcription_engine", { engine: "apple-native" });
      await refreshTranscriptionEngine();
      await refreshModels();
    } catch (error) {
      transcriptionEngineError = String(error || "Yawn could not change the speech engine.");
      renderTranscriptionEngine();
    }
  }

  async function removeModel(modelId, title) {
    if (!invoke) return;
    if (!window.confirm(`Remove ${title} from this Mac? You can download it again later.`)) return;
    modelMessage.textContent = `Removing ${title}…`;
    try {
      models = await invoke("remove_transcript_model", { modelId });
      renderModels();
    } catch (error) {
      modelMessage.textContent = String(error || "Yawn could not remove that speech model.");
      modelMessage.dataset.tone = "attention";
    }
  }

  async function refresh() {
    if (!invoke) {
      message.textContent = "Open Settings from the Yawn desktop app.";
      return;
    }
    message.textContent = "Checking audio access…";
    try {
      permissions = mergePermissions(permissions, await invoke("first_run_permissions"));
      message.textContent = permissions.probeUnavailable ? "Yawn could not check audio access. Reopen the app and try again." : "";
    } catch {
      message.textContent = "Yawn could not check audio access.";
    }
    render();
  }

  async function request(kind) {
    if (!invoke) return;
    const command = kind === "microphone" ? "first_run_request_microphone" : "first_run_request_system_audio";
    message.textContent = kind === "microphone" ? "Waiting for macOS…" : "Checking system audio…";
    try {
      permissions = mergePermissions(permissions, await invoke(command));
      message.textContent = "";
    } catch {
      message.textContent = "Yawn could not update this permission.";
    }
    render();
  }

  // Updates (updater.rs). The row's state can move on its own -- a background
  // check, a download, an install -- so this window re-reads it: every second
  // while something is moving, every half minute otherwise. The read is a
  // local command; only Check now and the daily check go to the network.
  function renderUpdates() {
    if (!updateRoot) return;
    const view = updatePresentation(updateStatus);
    const action = view.action
      ? `<button class="allow-button" type="button" data-settings-action="${escapeHtml(view.action.action)}"${view.action.disabled ? " disabled" : ""}>${escapeHtml(view.action.label)}</button>`
      : `<span class="state" data-tone="${escapeHtml(view.tone)}">${escapeHtml(view.statusLabel)}</span>`;
    updateRoot.innerHTML = `
      <div class="permission-line">
        <div class="permission-copy"><strong>${escapeHtml(view.title)}</strong><p>${escapeHtml(view.detail)}</p></div>
        ${action}
      </div>
    `;
    updateRoot.setAttribute("aria-busy", view.poll ? "true" : "false");
    if (updateAutomatic) updateAutomatic.checked = view.automatic;
    if (updateDisclosure) updateDisclosure.textContent = view.disclosure;
    if (updateCheckButton) updateCheckButton.disabled = !view.canCheck;
  }

  function scheduleUpdatePoll() {
    if (disposed) return;
    clearTimeout(updatePoll);
    updatePoll = setTimeout(() => void refreshUpdates(), updatePresentation(updateStatus).poll ? 1000 : 30000);
  }

  async function refreshUpdates() {
    if (!invoke) return;
    try {
      updateStatus = await invoke("update_status");
    } catch {
      updateMessage.textContent = "Yawn could not read its update status.";
    }
    renderUpdates();
    scheduleUpdatePoll();
  }

  async function checkForUpdates() {
    if (!invoke) return;
    updateMessage.textContent = "";
    try {
      updateStatus = await invoke("check_for_updates");
    } catch (error) {
      updateMessage.textContent = String(error || "Yawn could not check for updates.");
    }
    renderUpdates();
    scheduleUpdatePoll();
  }

  async function installUpdate() {
    if (!invoke) return;
    updateMessage.textContent = "";
    try {
      updateStatus = await invoke("install_update");
    } catch (error) {
      updateMessage.textContent = String(error || "Yawn could not start the update.");
    }
    renderUpdates();
    scheduleUpdatePoll();
  }

  async function setAutomaticUpdateCheck(enabled) {
    if (!invoke) return;
    updateMessage.textContent = "";
    try {
      updateStatus = await invoke("set_automatic_update_check", { enabled });
    } catch (error) {
      updateMessage.textContent = String(error || "Yawn could not save this setting.");
    }
    renderUpdates();
  }

  root.addEventListener("change", (event) => {
    if (event.target?.id === "update-automatic") void setAutomaticUpdateCheck(event.target.checked);
  });

  root.addEventListener("click", (event) => {
    const control = event.target.closest("[data-settings-action]");
    const action = control?.dataset.settingsAction;
    if (action === "refresh") void refresh();
    if (action === "microphone" || action === "system-audio") void request(action);
    if (action === "use-model") void useModel(control.dataset.modelId);
    if (action === "prepare-apple-speech") void prepareAppleSpeech();
    if (action === "use-apple-speech") void useAppleSpeech();
    if (action === "remove-model") void removeModel(control.dataset.modelId, control.dataset.modelTitle);
    if (action === "install-speaker-model") void installSpeakerModel();
    if (action === "check-updates") void checkForUpdates();
    if (action === "install-update") void installUpdate();
    if (action === "appearance") { setAppearance(control.dataset.value); renderAppearance(); }
    // This page owns its controls: keep the main window's document-level
    // handler from also acting on a click that landed here.
    if (action) event.stopPropagation?.();
  });

  // Appearance (2026-10-16, operator decision): System, Light or Dark. System
  // follows macOS; the choice is a per-Mac display preference, not meeting data.
  function renderAppearance() {
    const choiceRoot = root.querySelector("#appearance-choice");
    if (!choiceRoot) return;
    const current = getAppearance();
    const labels = { system: "System", light: "Light", dark: "Dark" };
    choiceRoot.innerHTML = APPEARANCES.map((value) => `<button class="appearance-option" type="button" data-settings-action="appearance" data-value="${value}" aria-pressed="${value === current ? "true" : "false"}">${labels[value]}</button>`).join("");
  }

  renderAppearance();
  void refresh();
  void refreshModels();
  void refreshTranscriptionEngine();
  void refreshSpeakerModel();
  void refreshUpdates();

  return () => {
    disposed = true;
    for (const timer of [transcriptionEnginePoll, modelPoll, speakerModelPoll, updatePoll]) {
      clearTimeout(timer);
      clearInterval(timer);
    }
  };
}
