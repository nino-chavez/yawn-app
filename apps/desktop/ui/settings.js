import { mergePermissions } from "./view-model.mjs";

// tokens.css keys dark-appearance values off `[data-theme="dark"]` (it has
// no `prefers-color-scheme` fallback of its own — see tokens.css's header),
// so this window has to set that attribute itself from the system setting;
// without it, Settings would always render the light palette regardless of
// macOS appearance.
if (typeof window.matchMedia === "function") {
  const dark = window.matchMedia("(prefers-color-scheme: dark)");
  const applyTheme = () => {
    document.documentElement.dataset.theme = dark.matches ? "dark" : "light";
  };
  applyTheme();
  dark.addEventListener?.("change", applyTheme);
}

const invoke = window.__TAURI__?.core?.invoke;
const permissionsRoot = document.querySelector("#permissions");
const modelsRoot = document.querySelector("#models");
const transcriptionEngineRoot = document.querySelector("#transcription-engine");
const modelMessage = document.querySelector("#model-message");
const noteModelsRoot = document.querySelector("#note-models");
const noteModelMessage = document.querySelector("#note-model-message");
const message = document.querySelector("#message");

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
let noteModels = null;
let noteModelLoadError = "";
let noteModelPoll = null;

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
    ? `<button class="allow-button" type="button" data-action="microphone">Allow microphone</button>`
    : "";
  const systemAction = permissions.systemAudio === "unmeasured"
    ? `<button class="allow-button" type="button" data-action="system-audio">Allow system audio</button>`
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
      : `<button class="${isPrimaryUseAction ? "allow-button" : "quiet-button"}" type="button" data-action="use-model" data-model-id="${escapeHtml(option.id)}" ${disabled ? "disabled" : ""}>${option.stored ? "Use this model" : "Download and use"}</button>`;
    const remove = option.stored && !option.active
      ? `<button class="quiet-button" type="button" data-action="remove-model" data-model-id="${escapeHtml(option.id)}" data-model-title="${escapeHtml(option.title)}" ${disabled ? "disabled" : ""}>Remove download</button>`
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
    : "You can download the other model at any time.");
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
    ? `<button class="quiet-button" type="button" data-action="use-apple-speech">Use Apple speech</button>`
    : !busy && ["assets-required", "failed"].includes(apple.state)
      ? `<button class="allow-button" type="button" data-action="prepare-apple-speech">Download Apple speech</button>`
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
  clearTimeout(transcriptionEnginePoll);
  if (transcriptionEngine?.operationActive) {
    transcriptionEnginePoll = setTimeout(() => void refreshTranscriptionEngine(), 500);
  } else if (!transcriptionEngine) {
    transcriptionEnginePoll = setTimeout(() => void refreshTranscriptionEngine(), 2000);
  }
}

function renderNoteModels() {
  if (!noteModels) {
    noteModelsRoot.innerHTML = noteModelLoadError
      ? row("Couldn't check note model", "unavailable", `${noteModelLoadError} Retrying…`)
      : row("Checking note model", "checking", "Yawn is checking what is stored on this Mac.");
    // Refit R11: same fix as renderModels above — the row already states the
    // one error, so the footnote does not restate it a second time.
    noteModelMessage.textContent = "";
    noteModelMessage.dataset.tone = "neutral";
    return;
  }
  const busy = noteModels.changeActive;
  noteModelsRoot.setAttribute("aria-busy", busy ? "true" : "false");
  noteModelsRoot.innerHTML = noteModels.options.map((option) => {
    const selected = noteModels.selectedModelId === option.id;
    const disabled = !noteModels.canChange || busy;
    const status = option.active
      ? `<span class="state" data-tone="ready">Selected</span>`
      : option.stored
        ? `<span class="state">On this Mac</span>`
        : `<span class="model-size">${byteSizeLabel(option.downloadBytes)}</span>`;
    const progress = selected && busy
      ? `<div class="model-progress"><progress max="${Math.max(1, noteModels.totalBytes)}" value="${Math.min(noteModels.totalBytes, noteModels.downloadedBytes)}"></progress><small>${noteModels.state === "verifying" ? "Checking the downloaded files…" : `${byteSizeLabel(noteModels.downloadedBytes)} of ${byteSizeLabel(noteModels.totalBytes)}`}</small></div>`
      : "";
    // Refit R18: one primary action per group. An unstored option is the primary
    // (it acquires a resource); a stored option is secondary (it switches to
    // what's already available).
    const isPrimaryUseAction = !option.stored;
    const use = option.active
      ? ""
      : `<button class="${isPrimaryUseAction ? "allow-button" : "quiet-button"}" type="button" data-action="use-note-model" data-model-id="${escapeHtml(option.id)}" ${disabled ? "disabled" : ""}>${option.stored ? "Use this model" : "Download and use"}</button>`;
    const remove = option.stored
      ? `<button class="quiet-button" type="button" data-action="remove-note-model" data-model-id="${escapeHtml(option.id)}" data-model-title="${escapeHtml(option.title)}" ${disabled ? "disabled" : ""}>Remove download</button>`
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
  const installed = noteModels.options.some((option) => option.active);
  noteModelMessage.textContent = noteModels.error || noteModels.unavailableReason || (installed
    ? "Removing the note model returns meetings to transcript-only."
    : "Without this model, meetings keep their transcript and no note is written.");
  noteModelMessage.dataset.tone = noteModels.error ? "attention" : "neutral";
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

function scheduleNoteModelPoll() {
  clearTimeout(noteModelPoll);
  if (noteModels?.changeActive) {
    noteModelPoll = setTimeout(() => void refreshNoteModels(), 500);
  } else if (!noteModels) {
    noteModelPoll = setTimeout(() => void refreshNoteModels(), 2000);
  }
}

async function refreshNoteModels() {
  if (!invoke) {
    noteModelMessage.textContent = "Open Settings from the Yawn desktop app.";
    return;
  }
  try {
    noteModels = await invoke("note_model_settings");
    noteModelLoadError = "";
  } catch {
    noteModels = null;
    noteModelLoadError = "Yawn could not check the saved note model.";
  }
  renderNoteModels();
  scheduleNoteModelPoll();
}

async function useNoteModel(modelId) {
  if (!invoke) return;
  noteModelMessage.textContent = "Starting the note model download…";
  try {
    noteModels = await invoke("install_note_model", { modelId });
    renderNoteModels();
    scheduleNoteModelPoll();
  } catch (error) {
    noteModelMessage.textContent = String(error || "Yawn could not install the note model.");
    noteModelMessage.dataset.tone = "attention";
  }
}

async function removeNoteModel(modelId, title) {
  if (!invoke) return;
  if (!window.confirm(`Remove ${title} from this Mac? Meetings keep their transcript, and you can download it again later.`)) return;
  noteModelMessage.textContent = `Removing ${title}…`;
  try {
    noteModels = await invoke("remove_note_model", { modelId });
    renderNoteModels();
  } catch (error) {
    noteModelMessage.textContent = String(error || "Yawn could not remove the note model.");
    noteModelMessage.dataset.tone = "attention";
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
      await Promise.all([refreshModels(), refreshNoteModels()]);
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

document.addEventListener("click", (event) => {
  const control = event.target.closest("[data-action]");
  const action = control?.dataset.action;
  if (action === "refresh") void refresh();
  if (action === "microphone" || action === "system-audio") void request(action);
  if (action === "use-model") void useModel(control.dataset.modelId);
  if (action === "prepare-apple-speech") void prepareAppleSpeech();
  if (action === "use-apple-speech") void useAppleSpeech();
  if (action === "remove-model") void removeModel(control.dataset.modelId, control.dataset.modelTitle);
  if (action === "use-note-model") void useNoteModel(control.dataset.modelId);
  if (action === "remove-note-model") void removeNoteModel(control.dataset.modelId, control.dataset.modelTitle);
});

void refresh();
void refreshModels();
void refreshTranscriptionEngine();
void refreshNoteModels();
