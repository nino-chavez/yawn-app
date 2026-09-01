import {
  backgroundTranscriptionPresentation,
  canOpenStart,
  captureActivity,
  captureActivityElapsedSeconds,
  capturePresentation,
  errorRecoveryPresentation,
  humanize,
  libraryRecoveryPresentation,
  libraryRowMetaPresentation,
  localVocabularyPresentation,
  lockedActionOutcome,
  meetingContextPresentation,
  meetingDeletionConfirmationCopy,
  meetingLockPresentation,
  meetingLockSheetCopy,
  meetingRecoveryPresentation,
  meetingNotePresentation,
  mergePermissions,
  noteCaptureFocusSelection,
  noteGenerationPresentation,
  permissionSummary,
  recordingDevicePresentation,
  capturePauseControlPresentation,
  capturePausePresentation,
  retainedAudioPlaybackPresentation,
  retentionLabel,
  retryTurnDiffSegments,
  shouldPollSnapshot,
  transcriptCitationSummary,
  transcriptPlainText,
  transcriptRetryDiffPresentation,
  transcriptRetryQualityPresentation,
  transcriptSpeakerLabel,
  transcriptRetryPresentation,
  transcriptTurnsForSourceSpeaker,
  transcriptTurnsMatching,
  transcriptionWorkerHeartbeatAgeSeconds,
  trashLinkPresentation,
  trashListPresentation,
  turnCitationPresentation,
  withheldTurnPresentation,
} from "./view-model.mjs";
import { patchInto } from "./dom-patch.mjs";

const root = document.querySelector("#app");
const invoke = window.__TAURI__?.core?.invoke;
const tauriListen = window.__TAURI__?.event?.listen;

// Roadmap intake I2: fired from the Rust side (capture_shortcut.rs) when the
// operator presses the global note-capture hotkey during an active
// recording. Must match capture_shortcut::NOTE_CAPTURE_FOCUS_EVENT exactly.
const NOTE_CAPTURE_FOCUS_EVENT = "note-capture-hotkey";

const state = {
  activeView: "home",
  audioPlayback: { state: "idle", source: null, message: "No recording is playing." },
  busyAction: "",
  consent: { participantsConsented: false, headphones: false, operatorAlone: false },
  contextDraft: "",
  contextLoadedFor: "",
  contextLoading: false,
  contextSaveQueue: Promise.resolve(),
  contextSaveState: "local",
  contextUnreadable: false,
  error: "",
  library: null,
  generatingMeetingId: "",
  meetingManagementOpen: false,
  modal: "",
  notice: "",
  noteDraft: "",
  noteLoadedFor: "",
  noteLoading: false,
  noteSaveQueue: Promise.resolve(),
  noteSaveState: "local",
  noteUnreadable: false,
  permissions: null,
  retentionDays: 7,
  renameDraft: "",
  search: "",
  searchTimer: null,
  selected: null,
  snapshot: null,
  speakerCorrection: null,
  speakerCorrectionDraft: "",
  transcriptRetry: null,
  vocabulary: null,
  transcriptActionStatus: {},
  transcriptQuery: "",
  noteCaptureFocusPending: false,
  // Roadmap intake I9: local trash for whole-meeting deletion. `trash` is
  // null until first loaded, then `{ entries: [...] }`; `trashOpen` selects
  // the quiet secondary list in place of Meetings.
  trash: null,
  trashOpen: false,
};

let noteSaveTimer;
let libraryNoteSaveTimer;
let contextSaveTimer;
let permissionsRefreshTask;
let activityTimer;
let audioPlaybackPollActive = false;

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function dateLabel(epochSeconds) {
  if (!Number.isFinite(Number(epochSeconds))) return "Saved locally";
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(Number(epochSeconds) * 1000));
}

function timeLabel(seconds) {
  const total = Math.max(0, Math.floor(Number(seconds) || 0));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

function elapsedAgoLabel(seconds) {
  const total = Math.max(0, Math.floor(Number(seconds) || 0));
  return total < 5 ? "just now" : `${timeLabel(total)} ago`;
}

function byteSizeLabel(bytes) {
  const value = Math.max(0, Number(bytes) || 0);
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  if (value < 1024 * 1024 * 1024) return `${Math.round(value / (1024 * 1024))} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function statusLabel(snapshot, permission) {
  const presentation = capturePresentation(snapshot);
  if (!snapshot) return { label: "Opening", tone: "working" };
  if (snapshot.startup !== "ready") {
    const modelWorking = snapshot.startup === "model-required"
      && ["downloading", "verifying"].includes(snapshot.model_setup?.state);
    return {
      label: snapshot.startup === "model-required"
        ? (modelWorking ? "Installing model" : "Choose a model")
        : snapshot.startup === "checking" ? "Opening" : "Needs attention",
      tone: snapshot.startup === "checking" || modelWorking ? "working" : "attention",
    };
  }
  if (presentation.capture === "idle" && permissionSummary(permission).state !== "ready") {
    return {
      label: permission ? "Audio setup" : "Checking audio",
      tone: permission ? "attention" : "working",
    };
  }
  if (presentation.capture === "idle") return { label: "Ready", tone: "ready" };
  if (presentation.capture === "recording") return { label: "Recording", tone: "recording" };
  if (presentation.capture === "transcript-ready") return { label: "Ready to read", tone: "complete" };
  return { label: presentation.eyebrow, tone: presentation.tone };
}

function noteSaveCopy() {
  if (state.noteUnreadable) return "Not editable";
  if (state.noteSaveState === "saving") return "Saving…";
  if (state.noteSaveState === "saved") return "Saved on this Mac";
  return "Stored on this Mac";
}

function contextSaveCopy() {
  if (state.contextUnreadable) return "Not editable";
  if (state.contextSaveState === "saving") return "Saving…";
  if (state.contextSaveState === "saved") return "Saved on this Mac";
  return "Stored on this Mac";
}

function permissionAction(permission) {
  if (!permission || permission.probeUnavailable) return { action: "open-settings", label: "Open Settings" };
  if (permission.microphone === "not-determined") return { action: "request-microphone", label: "Allow microphone" };
  if (permission.systemAudio === "unmeasured") return { action: "request-system-audio", label: "Allow system audio" };
  return { action: "open-settings", label: "Open Settings" };
}

function render() {
  const editorFocus = captureEditorFocus();
  const status = statusLabel(state.snapshot, state.permissions);
  const audioReady = canOpenStart(state.snapshot, state.permissions);
  let content;
  if (!invoke) content = renderBrowserNotice();
  else if (!state.snapshot || state.snapshot.startup === "checking") content = renderStartup(true);
  else if (state.snapshot.startup === "model-required") content = renderModelSetup();
  else if (state.snapshot.startup !== "ready") content = renderStartup(false);
  else if (state.snapshot.capture !== "idle") content = renderCapture();
  else if (state.selected) content = renderMeeting();
  else if (state.trashOpen) content = renderTrash();
  else content = renderHome();

  // Patched in place, never assigned wholesale: replacing root.innerHTML
  // destroyed every editor node per 900 ms poll tick, which reset WebKit's
  // per-element undo stack (and collapsed reader-opened <details>). See
  // dom-patch.mjs for the preservation rules.
  patchInto(root, `
    <div class="app-shell" data-capture="${escapeHtml(state.snapshot?.capture || "opening")}">
      <header class="topbar" data-tauri-drag-region>
        <button class="brand" type="button" data-action="home" data-tauri-drag-region="false">
          <span>Yawn</span>
        </button>
        <div class="topbar-actions" data-tauri-drag-region="false">
          <div class="topbar-status" aria-label="${escapeHtml(status.label)}">
            <span class="status-dot" data-tone="${escapeHtml(status.tone)}"></span>
            <span>${escapeHtml(status.label)}</span>
          </div>
          ${state.snapshot?.startup === "ready" && state.snapshot?.capture === "idle" ? `
            <button class="button button-quiet" type="button" data-action="meetings">Meetings</button>
            ${audioReady ? `<button class="button button-primary" type="button" data-action="open-start">Record</button>` : ""}
          ` : ""}
          <button class="icon-button" type="button" data-action="settings" aria-label="Open Settings">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 8.4a3.6 3.6 0 1 0 0 7.2 3.6 3.6 0 0 0 0-7.2Zm8.3 3.6a6.8 6.8 0 0 0-.1-1l2-1.5-2-3.4-2.4 1a8.2 8.2 0 0 0-1.7-1L15.8 3h-4l-.3 2.1a8.2 8.2 0 0 0-1.7 1l-2.4-1-2 3.4 2 1.5a6.8 6.8 0 0 0 0 2l-2 1.5 2 3.4 2.4-1a8.2 8.2 0 0 0 1.7 1l.3 2.1h4l.3-2.1a8.2 8.2 0 0 0 1.7-1l2.4 1 2-3.4-2-1.5a6.8 6.8 0 0 0 .1-1Z" /></svg>
          </button>
        </div>
      </header>
      <main class="stage">${content}</main>
      ${state.modal === "start" ? renderStartSheet() : ""}
      ${state.modal === "rename-meeting" ? renderRenameMeetingSheet() : ""}
      ${state.modal === "speaker-correction" ? renderSpeakerCorrectionSheet() : ""}
      ${state.modal === "transcript-retry" ? renderTranscriptRetrySheet() : ""}
      ${state.modal === "vocabulary" ? renderVocabularySheet() : ""}
      ${["delete-recording", "delete-transcript", "delete-meeting"].includes(state.modal) ? renderMeetingDeletionSheet() : ""}
      ${["lock-meeting", "unlock-meeting"].includes(state.modal) ? renderMeetingLockSheet() : ""}
      ${state.notice ? `<aside class="toast toast-notice" role="status"><button type="button" data-action="clear-notice" aria-label="Dismiss">×</button>${escapeHtml(state.notice)}</aside>` : ""}
      ${state.error ? `<aside class="toast" role="alert"><button type="button" data-action="clear-error" aria-label="Dismiss">×</button><span>${escapeHtml(state.error.message)}</span>${state.error.action ? `<button class="button button-quiet button-small" type="button" data-action="${escapeHtml(state.error.action.action)}">${escapeHtml(state.error.action.label)}</button>` : ""}</aside>` : ""}
    </div>
  `);
  restoreEditorFocus(editorFocus);
  if (state.noteCaptureFocusPending) focusOperatorNoteFromHotkey();
  syncActivityClock();
}

// Snapshot polling keeps recording and transcription honest, but rebuilding the
// whole document used to replace the active textarea every 900 ms. Preserve a
// real editor's focus and selection only when the same meeting still owns it.
function captureEditorFocus() {
  const active = document.activeElement;
  const field = active?.dataset?.field;
  if (!["operator-note", "meeting-context", "library-operator-note", "transcript-search", "vocabulary-before", "vocabulary-after", "library-search", "meeting-title", "speaker-name"].includes(field)) return null;
  if (!active.dataset.meetingId) return null;
  return {
    field,
    meetingId: active.dataset.meetingId,
    start: Number.isInteger(active.selectionStart) ? active.selectionStart : null,
    end: Number.isInteger(active.selectionEnd) ? active.selectionEnd : null,
    direction: active.selectionDirection || "none",
  };
}

function restoreEditorFocus(focus) {
  if (!focus) return;
  const target = Array.from(root.querySelectorAll(`[data-field="${focus.field}"]`))
    .find((candidate) => candidate.dataset.meetingId === focus.meetingId);
  if (!target || target.disabled) return;
  target.focus({ preventScroll: true });
  if (focus.start === null || focus.end === null || typeof target.setSelectionRange !== "function") return;
  const start = Math.min(focus.start, target.value.length);
  const end = Math.min(Math.max(start, focus.end), target.value.length);
  target.setSelectionRange(start, end, focus.direction);
}

// Roadmap intake I2: the Rust side already brought the window forward and
// requested focus (capture_shortcut.rs); this is the frontend half — find
// the operator-note editor already in the capture view and put the caret at
// the end. If the editor is not in the DOM yet (the poll-driven render()
// that owns it hasn't run since the hotkey fired) or is disabled, this
// leaves the request pending so the next render() retries it. No new
// window, no overlay: only the existing operator canvas.
function focusOperatorNoteFromHotkey() {
  const target = root.querySelector('[data-field="operator-note"]');
  if (!target || target.disabled) return;
  target.focus({ preventScroll: true });
  const selection = noteCaptureFocusSelection(target.value);
  if (typeof target.setSelectionRange === "function") {
    target.setSelectionRange(selection.start, selection.end, selection.direction);
  }
  state.noteCaptureFocusPending = false;
}

function listenForNoteCaptureHotkey() {
  if (!tauriListen) return;
  tauriListen(NOTE_CAPTURE_FOCUS_EVENT, () => {
    state.noteCaptureFocusPending = true;
    focusOperatorNoteFromHotkey();
  }).catch(() => {
    // No listener means no in-meeting hotkey focus-jump; the meeting itself
    // is unaffected, so this stays silent rather than raising a toast for a
    // convenience feature.
  });
}

function renderBrowserNotice() {
  return `
    <section class="startup-card attention">
      <div class="startup-orb" aria-hidden="true"></div>
      <p class="eyebrow">Desktop app required</p>
      <h1>Open Yawn from Applications.</h1>
      <p class="browser-note">This page has no access to your microphone, local meetings, or note storage in a browser. Run the desktop app to use Yawn.</p>
    </section>
  `;
}

function renderStartup(checking) {
  const problem = state.snapshot?.error || "Yawn could not complete its local startup check.";
  const progress = state.snapshot?.startup_message || "Yawn is checking its private workspace.";
  return `
    <section class="startup-card ${checking ? "" : "attention"}">
      <div class="startup-orb" aria-hidden="true"></div>
      <p class="eyebrow">${checking ? "Checking your local engine" : "Needs attention"}</p>
      <h1>${checking ? "Getting Yawn ready." : "Yawn cannot record yet."}</h1>
      <p class="lede">${checking ? `${escapeHtml(progress)} Recording stays off until that finishes.` : escapeHtml(problem)}</p>
      ${checking ? "" : `<button class="button button-primary" type="button" data-action="retry-startup">Check again</button>`}
    </section>
  `;
}

function renderModelSetup() {
  const setup = state.snapshot?.model_setup || {};
  const working = ["downloading", "verifying"].includes(setup.state);
  const selected = (setup.options || []).find((option) => option.id === setup.selectedModelId);
  const total = Math.max(1, Number(setup.totalBytes) || 1);
  const downloaded = Math.min(total, Math.max(0, Number(setup.downloadedBytes) || 0));
  const progressCopy = setup.state === "verifying"
    ? "Checking every downloaded file before Yawn uses it."
    : `${byteSizeLabel(downloaded)} of ${byteSizeLabel(total)} downloaded`;
  return `
    <section class="model-setup" aria-labelledby="model-setup-title">
      <div class="model-setup-copy">
        <p class="eyebrow">One-time setup</p>
        <h1 id="model-setup-title">Choose how much space Yawn uses.</h1>
        <p class="lede">Both models run on this Mac. The smaller model saves disk space. The full model keeps the original Turbo weights.</p>
      </div>
      ${working ? `
        <div class="model-download" role="status" aria-live="polite">
          <p class="eyebrow">${setup.state === "verifying" ? "Verifying" : "Downloading"}</p>
          <h2>${escapeHtml(selected?.title || "Speech model")}</h2>
          <progress max="${total}" value="${downloaded}"></progress>
          <p>${escapeHtml(progressCopy)}</p>
          <small>Keep Yawn open. Recording stays off until setup finishes.</small>
        </div>
      ` : `
        ${setup.error ? `<p class="model-setup-error" role="alert">${escapeHtml(setup.error)}</p>` : ""}
        <div class="model-options">
          ${(setup.options || []).map((option) => `
            <article class="model-option">
              <div>
                <h2>${escapeHtml(option.title)}</h2>
                <p>${escapeHtml(option.detail)}</p>
              </div>
              <dl>
                <div><dt>Download</dt><dd>${byteSizeLabel(option.downloadBytes)}</dd></div>
                <div><dt>On disk</dt><dd>${byteSizeLabel(option.installedBytes)}</dd></div>
              </dl>
              <button class="button ${option.id.includes("q4") ? "button-primary" : "button-secondary"}" type="button" data-action="install-model" data-model-id="${escapeHtml(option.id)}">Use this model</button>
            </article>
          `).join("")}
        </div>
        <p class="model-privacy">The model is downloaded directly to Yawn’s private folder. Meeting audio is not uploaded.</p>
      `}
    </section>
  `;
}

function renderHome() {
  const permission = permissionSummary(state.permissions);
  const audioReady = permission.state === "ready";
  const startAvailable = canOpenStart(state.snapshot, state.permissions);
  const setupAction = permissionAction(state.permissions);
  const library = state.library;
  return `
    <section class="home-workbench" aria-labelledby="home-title">
      <div class="home-copy">
        <p class="eyebrow">New meeting</p>
        <h1 id="home-title">Capture the conversation. Keep your own judgment.</h1>
        <p class="lede">A local recording, your private notes, and a transcript you can check when a detail matters.</p>
      </div>
      <div class="home-action">
        ${audioReady ? `
          <div class="inline-actions">
            <button class="button button-record" type="button" data-action="open-start" ${startAvailable ? "" : "disabled"}>Record</button>
            <span class="shortcut" aria-label="Keyboard shortcut">⌘ R</span>
          </div>
          <p>Everything stays on this Mac. No account, bot, or automatic sharing.</p>
        ` : `
          <button class="button button-primary" type="button" data-action="${setupAction.action}">${escapeHtml(setupAction.label)}</button>
          <p><strong>${escapeHtml(permission.title)}</strong><br />${escapeHtml(permission.detail)}</p>
        `}
      </div>
    </section>
    ${renderBackgroundTranscription(state.snapshot)}
    <section aria-labelledby="meetings-heading">
      <div class="section-heading">
        <h2 id="meetings-heading">Recent meetings</h2>
        ${library?.total ? `<input class="search-input" type="search" data-field="library-search" data-meeting-id="library" value="${escapeHtml(state.search)}" placeholder="Find a meeting by title" aria-label="Find a meeting by title" autocorrect="off" spellcheck="false" />` : ""}
      </div>
      ${renderLibrary(library)}
      ${(() => {
        const link = trashLinkPresentation(state.trash);
        return link
          ? `<button class="button button-quiet button-small trash-link" type="button" data-action="open-trash">${escapeHtml(link.label)}</button>`
          : "";
      })()}
    </section>
  `;
}

function renderTrash() {
  const presentation = trashListPresentation(state.trash);
  return `
    <section aria-labelledby="trash-heading">
      <div class="section-heading">
        <button class="icon-button" type="button" data-action="close-trash" aria-label="Back to Meetings">‹</button>
        <h2 id="trash-heading">Trash</h2>
      </div>
      <p class="quiet-copy">Deleted meetings stay here for 30 days, then Yawn removes them permanently. There is no server copy, so a meeting cannot be recovered after that.</p>
      ${presentation.state === "loading" ? `<p class="quiet-copy">Loading Trash…</p>` : ""}
      ${presentation.state === "empty" ? `<p class="quiet-copy">Trash is empty.</p>` : ""}
      ${presentation.state === "populated" ? `
        <div class="meeting-list" role="list">
          ${presentation.entries.map((entry) => {
            const busy = state.busyAction === `restore-${entry.meetingId}`;
            return `
            <div class="meeting-row trash-row" role="listitem">
              <span>
                <span class="meeting-row-title">${escapeHtml(entry.label)}</span>
                <span class="meeting-row-meta">Deleted ${escapeHtml(dateLabel(entry.deletedAtEpochSeconds))} · removed permanently ${escapeHtml(dateLabel(entry.purgeAfterEpochSeconds))}</span>
              </span>
              <button class="button button-secondary button-small" type="button" data-action="restore-trash-entry" data-meeting-id="${escapeHtml(entry.meetingId)}" ${busy ? "disabled" : ""}>${busy ? "Restoring…" : "Restore"}</button>
            </div>
          `;
          }).join("")}
        </div>
      ` : ""}
    </section>
  `;
}

function renderBackgroundTranscription(snapshot) {
  const processing = backgroundTranscriptionPresentation(snapshot);
  if (!processing) return "";
  return `
    <section class="message-card background-transcription" data-state="${escapeHtml(processing.state)}" aria-live="polite">
      <p class="eyebrow">Processing on this Mac</p>
      <h2>${escapeHtml(processing.label)}</h2>
      <p>${escapeHtml(processing.detail)}</p>
    </section>
  `;
}

function renderLibrary(library) {
  if (!library) return `<p class="quiet-copy">Loading meetings saved on this Mac…</p>`;
  const recovery = libraryRecoveryPresentation(library);
  if (recovery) {
    return `<section class="empty-library recovery-card attention" aria-labelledby="library-recovery-title"><h3 id="library-recovery-title">${escapeHtml(recovery.title)}</h3><p>${escapeHtml(recovery.detail)}</p><button class="button button-primary button-small" type="button" data-action="${recovery.action.action}">${escapeHtml(recovery.action.label)}</button></section>`;
  }
  if (!library.rows?.length) {
    const message = state.search.trim()
      ? library.message || "No meeting matches that title."
      : "Your finished meetings will appear here. Start with the next conversation you want to remember.";
    return `<div class="empty-library"><h3>${state.search.trim() ? "No matching meetings" : "No meetings yet"}</h3><p>${escapeHtml(message)}</p></div>`;
  }
  return `
    <div class="meeting-list" role="list">
      ${library.rows.map((row) => {
        // Roadmap intake I5 (a): a locked row keeps its title and date and
        // drops both the note preview and the transcript detail. The preview
        // is already absent from the backend response; this branch is the
        // rendered half of the same suppression.
        const meta = libraryRowMetaPresentation(row);
        return `
        <button class="meeting-row" type="button" role="listitem" data-action="open-meeting" data-handle="${escapeHtml(row.handle)}"${meta.locked ? ` data-locked="true"` : ""}>
          <span>
            <span class="meeting-row-title">${escapeHtml(row.label || `Meeting · ${dateLabel(row.createdAtEpochSeconds)}`)}</span>
            ${meta.preview ? `<span class="meeting-row-preview">${escapeHtml(meta.preview)}</span>` : ""}
            <span class="meeting-row-meta">${escapeHtml(dateLabel(row.createdAtEpochSeconds))} · ${escapeHtml(meta.label)}</span>
          </span>
          <span class="meeting-row-arrow" aria-hidden="true">›</span>
        </button>
      `;
      }).join("")}
    </div>
  `;
}

function renderCapture() {
  const snapshot = state.snapshot;
  const presentation = capturePresentation(snapshot);
  const terminal = ["transcript-ready", "transcription-failed", "recovered-interrupted"].includes(snapshot.capture);
  const disabled = state.noteUnreadable || !snapshot.meeting_id;
  const contextDisabled = state.contextUnreadable || !snapshot.meeting_id;
  const context = snapshot.error || snapshot.warnings?.[0] || "";
  return `
    <section class="session-workspace" aria-labelledby="capture-title">
      <header class="session-toolbar">
        <div class="session-status">
          <span class="record-dot" data-tone="${escapeHtml(presentation.tone)}" aria-hidden="true"></span>
          <div>
            <p class="eyebrow">${escapeHtml(presentation.eyebrow)}</p>
            <h1 id="capture-title">${escapeHtml(presentation.title)}</h1>
            <p>${escapeHtml(presentation.detail)}</p>
          </div>
        </div>
        ${captureAction(snapshot, terminal)}
      </header>
      <div class="capture-facts" aria-label="Recording state">
        <span class="status-pill" data-tone="${escapeHtml(presentation.tone)}">${escapeHtml(humanize(snapshot.capture))}</span>
        ${snapshot.mic_state ? `<span class="fact-pill">Mic: ${escapeHtml(snapshot.mic_state)}</span>` : ""}
        ${snapshot.system_state ? `<span class="fact-pill">System audio: ${escapeHtml(snapshot.system_state)}</span>` : ""}
      </div>
      ${renderActivityMonitor(snapshot)}
      ${context ? `<p class="message-card ${presentation.tone === "attention" ? "attention" : ""}">${escapeHtml(context)}</p>` : ""}
      <article class="note-workbench context-workbench">
        <div class="note-editor-head"><strong>What is this meeting for? (optional)</strong><span class="save-state" id="context-save-state">${escapeHtml(contextSaveCopy())}</span></div>
        <textarea class="note-editor" data-field="meeting-context" data-meeting-id="${escapeHtml(snapshot.meeting_id || "")}" aria-label="What is this meeting for? Optional." placeholder="What is this meeting for? What must get decided?" ${contextDisabled ? "disabled" : ""}>${escapeHtml(state.contextDraft)}</textarea>
        <div class="capture-foot">
          <p>${state.contextUnreadable ? "This context could not be read, so Yawn will not overwrite it." : "Your context, used to guide the generated note. Not a transcript."}</p>
        </div>
      </article>
      <article class="note-workbench">
        <div class="note-editor-head"><strong>Your notes</strong><span class="save-state" id="note-save-state">${escapeHtml(noteSaveCopy())}</span></div>
        <textarea class="note-editor" data-field="operator-note" data-meeting-id="${escapeHtml(snapshot.meeting_id || "")}" aria-label="Your meeting notes" placeholder="Write down the detail you will want to verify later." ${disabled ? "disabled" : ""}>${escapeHtml(state.noteDraft)}</textarea>
        <div class="capture-foot">
          <p>${state.noteUnreadable ? "This note could not be read, so Yawn will not overwrite it." : "Saved separately from the transcript. These are your notes, not generated claims."}</p>
        </div>
      </article>
      ${snapshot.turns?.length ? renderTranscript(snapshot.turns, terminal ? "Source transcript" : "Live transcript", terminal ? "The retained record for checking a detail that matters." : "Yawn adds local transcription here as it becomes available.", {
        copyAction: "copy-current-transcript",
        openFileAction: snapshot.capture === "transcript-ready" && snapshot.current_transcript_sha256 ? "open-current-transcript-file" : "",
      }) : ""}
    </section>
  `;
}

function renderActivityMonitor(snapshot) {
  const activity = captureActivity(snapshot);
  if (!activity) return "";
  const startedAt = Number(snapshot.capture_state_started_at_epoch_seconds);
  const elapsed = captureActivityElapsedSeconds(snapshot);
  const heartbeatAt = Number(snapshot.transcription_last_worker_heartbeat_at_epoch_seconds);
  const heartbeatAge = transcriptionWorkerHeartbeatAgeSeconds(snapshot);
  return `
    <section class="activity-monitor" data-tone="${escapeHtml(activity.tone)}" aria-labelledby="activity-title">
      <div>
        <p class="eyebrow">Activity</p>
        <h2 id="activity-title">${escapeHtml(activity.label)}</h2>
        <p>${escapeHtml(activity.detail)}</p>
      </div>
      <div class="activity-timing">
        <strong data-activity-elapsed data-activity-started-at="${Number.isFinite(startedAt) ? startedAt : ""}">${elapsed === null ? "Working" : timeLabel(elapsed)}</strong>
        <span>elapsed in this step</span>
        ${snapshot.capture === "transcribing" ? `<span class="activity-heartbeat" data-transcription-heartbeat data-transcription-heartbeat-at="${Number.isFinite(heartbeatAt) ? heartbeatAt : ""}" aria-live="polite">${heartbeatAge === null ? "Waiting for a confirmation on this Mac" : `Last confirmed on this Mac ${elapsedAgoLabel(heartbeatAge)}`}</span>` : ""}
      </div>
    </section>
  `;
}

function updateActivityClock() {
  for (const target of document.querySelectorAll("[data-activity-elapsed]")) {
    const rawStartedAt = target.dataset.activityStartedAt;
    if (!rawStartedAt) continue;
    const startedAt = Number(rawStartedAt);
    if (!Number.isFinite(startedAt)) continue;
    target.textContent = timeLabel(Math.max(0, Math.floor(Date.now() / 1000 - startedAt)));
  }
  for (const target of document.querySelectorAll("[data-transcription-heartbeat]")) {
    const rawHeartbeatAt = target.dataset.transcriptionHeartbeatAt;
    if (!rawHeartbeatAt) continue;
    const heartbeatAt = Number(rawHeartbeatAt);
    if (!Number.isFinite(heartbeatAt)) continue;
    target.textContent = `Last confirmed on this Mac ${elapsedAgoLabel(Date.now() / 1000 - heartbeatAt)}`;
  }
}

function syncActivityClock() {
  if (!captureActivity(state.snapshot)) {
    clearInterval(activityTimer);
    activityTimer = undefined;
    return;
  }
  updateActivityClock();
  if (activityTimer === undefined) activityTimer = setInterval(updateActivityClock, 1000);
}

function captureAction(snapshot, terminal) {
  const pauseControl = capturePauseControlPresentation(snapshot);
  if (pauseControl) {
    // Stop stays available while paused: ending the meeting from a pause is
    // ordinary, and the operator should never have to resume to stop.
    const stopping = state.busyAction === "stop";
    const changing = state.busyAction === "pause" || state.busyAction === "resume";
    return `<div class="inline-actions" aria-label="Recording controls">
      <button class="button button-quiet" type="button" data-action="${pauseControl.action}" ${pauseControl.disabled || changing || stopping ? "disabled" : ""}>${escapeHtml(pauseControl.label)}</button>
      <button class="button button-record" type="button" data-action="stop-recording" ${stopping ? "disabled" : ""}>${stopping ? "Stopping…" : "Stop recording"}</button>
    </div>`;
  }
  if (terminal) {
    const leaving = ["dismiss", "record-another"].includes(state.busyAction);
    if (snapshot.capture === "transcript-ready") {
      return `<div class="inline-actions" aria-label="Meeting actions">
        <button class="button button-quiet" type="button" data-action="dismiss-current" ${leaving ? "disabled" : ""}>${state.busyAction === "dismiss" ? "Opening Meetings…" : "Back to Meetings"}</button>
        <button class="button button-primary" type="button" data-action="record-another" ${leaving ? "disabled" : ""}>${state.busyAction === "record-another" ? "Preparing…" : "Record another meeting"}</button>
      </div>`;
    }
    return `<button class="button button-primary" type="button" data-action="dismiss-current" ${leaving ? "disabled" : ""}>${state.busyAction === "dismiss" ? "Opening Meetings…" : "Back to Meetings"}</button>`;
  }
  return "";
}

function transcriptActionStatus(scope) {
  return state.transcriptActionStatus?.[scope] || "";
}

function renderTranscript(turns, title, detail = "", { copyAction = "", openFileAction = "", exportAction = "", workspace = false, citations = null } = {}) {
  const scope = copyAction.includes("library") ? "library" : "current";
  const copyBusy = state.busyAction === copyAction;
  const fileBusy = state.busyAction === openFileAction;
  const exportBusy = state.busyAction === exportAction;
  const query = workspace ? state.transcriptQuery.trim() : "";
  const visibleTurns = workspace ? transcriptTurnsMatching(turns, query) : turns;
  // Roadmap intake I4 / design D4's reverse half. Only the library's finished
  // transcript passes `citations`, so the live capture view never grows the
  // affordance for a note that does not exist yet.
  const citationSummary = citations
    ? transcriptCitationSummary(citations.turnsCited, turns.length)
    : null;
  const vocabulary = workspace
    ? localVocabularyPresentation({
      meetingId: state.selected?.row?.meetingId,
      transcriptMeetingId: state.selected?.transcript?.meetingId,
      transcriptSha256: state.selected?.transcript?.currentTranscriptSha256,
      capture: state.snapshot?.capture,
    })
    : null;
  const actions = copyAction || openFileAction || exportAction ? `<div class="transcript-actions" aria-label="Transcript actions">
    ${copyAction ? `<button class="button button-quiet button-small" type="button" data-action="${copyAction}" ${copyBusy ? "disabled" : ""}>${copyBusy ? "Copying…" : "Copy transcript"}</button>` : ""}
    ${openFileAction ? `<button class="button button-quiet button-small" type="button" data-action="${openFileAction}" ${fileBusy ? "disabled" : ""}>${fileBusy ? "Opening…" : "Open transcript file"}</button>` : ""}
    ${exportAction ? `<button class="button button-quiet button-small" type="button" data-action="${exportAction}" ${exportBusy || !state.selected?.transcript?.transcriptFileHandle ? "disabled" : ""}>${exportBusy ? "Exporting…" : "Export meeting"}</button>` : ""}
    <span class="transcript-action-status" role="status" aria-live="polite">${escapeHtml(transcriptActionStatus(scope))}</span>
  </div>` : "";
  const transcriptLines = visibleTurns.map((turn) => {
    const speakerLabel = transcriptSpeakerLabel(turn);
    const correctionAvailable = workspace && !turn.withheld && Boolean(state.selected?.transcript?.currentTranscriptSha256);
    const restore = workspace
      ? withheldTurnPresentation(turn, {
        meetingId: state.selected?.row?.meetingId,
        meetingHandle: state.selected?.row?.handle,
        transcriptMeetingId: state.selected?.transcript?.meetingId,
        transcriptSha256: state.selected?.transcript?.currentTranscriptSha256,
        capture: state.snapshot?.capture,
      })
      : null;
    const correctionLabel = turn.speakerCorrected
      ? `Change speaker name. Currently ${speakerLabel}, corrected from ${turn.sourceSpeaker || "Unattributed"}.`
      : `Correct speaker name. Currently ${speakerLabel}.`;
    const citation = citations && !turn.withheld
      ? turnCitationPresentation(citations.turnsCited, citations.claims, turn.sourceTurnIndex)
      : null;
    return `
      <div class="transcript-line ${turn.withheld ? "withheld" : ""}">
        <div class="transcript-line-meta">
          <time>${escapeHtml(timeLabel(turn.start))}</time>
          ${speakerLabel ? correctionAvailable
            ? `<button class="speaker-label-button" type="button" data-action="open-speaker-correction" data-source-turn-index="${escapeHtml(turn.sourceTurnIndex)}" aria-label="${escapeHtml(correctionLabel)}"><span>${escapeHtml(speakerLabel)}</span>${turn.speakerCorrected ? `<small>Corrected</small>` : ""}</button>`
            : `<span>${escapeHtml(speakerLabel)}</span>` : ""}
        </div>
        <p>${turn.withheld ? "This turn was withheld by the voice check." : escapeHtml(turn.text)}</p>
        ${restore ? `<button class="button button-quiet button-small" type="button" data-action="restore-withheld-turn" data-source-turn-index="${escapeHtml(restore.sourceTurnIndex)}">${restore.label}</button>` : ""}
        ${citation ? `<div class="turn-citation" aria-label="${escapeHtml(citation.summary)}">
          ${citation.citations.map((cited) => `<button class="turn-citation-button" type="button" data-action="navigate-to-claim" data-ordinal="${escapeHtml(cited.ordinal)}">${escapeHtml(cited.label)}</button>`).join("")}
        </div>` : ""}
      </div>
    `;
  }).join("");
  if (workspace) {
    const queryStatus = query
      ? visibleTurns.length
        ? `${visibleTurns.length} matching ${visibleTurns.length === 1 ? "moment" : "moments"}.`
        : "No matching moments."
      : "Find a phrase, decision, or follow-up.";
    return `
      <section class="transcript-panel transcript-workspace" aria-labelledby="transcript-heading">
        <div class="transcript-workspace-toolbar">
          <div class="transcript-heading"><h3 id="transcript-heading">${escapeHtml(title)}</h3>${detail ? `<p>${escapeHtml(detail)}</p>` : ""}${citationSummary ? `<p class="transcript-citation-summary">${escapeHtml(citationSummary)}</p>` : ""}</div>
          <div class="transcript-workspace-actions">
            ${vocabulary ? `<button class="button button-quiet button-small" type="button" data-action="${vocabulary.action}">${vocabulary.label}</button>` : ""}
            ${actions}
          </div>
        </div>
        <div class="transcript-search-row">
          <label class="screen-reader-only" for="transcript-search-input">Find in transcript</label>
          <input class="transcript-search" id="transcript-search-input" data-field="transcript-search" autocorrect="off" spellcheck="false" data-meeting-id="${escapeHtml(state.selected?.row?.meetingId || "")}" type="search" value="${escapeHtml(state.transcriptQuery)}" placeholder="Find in transcript" autocomplete="off" aria-describedby="transcript-search-status">
          ${query ? `<button class="button button-quiet button-small" type="button" data-action="clear-transcript-search">Clear</button>` : ""}
          <span class="transcript-search-status" id="transcript-search-status" role="status" aria-live="polite">${escapeHtml(queryStatus)}</span>
        </div>
        <div class="transcript-scroll" tabindex="0" aria-label="Transcript turns">
          ${transcriptLines || `<p class="transcript-empty">${query ? "No retained transcript turn matches that search." : "No transcript turns are available."}</p>`}
        </div>
        ${turns.some((turn) => turn.speakerCorrected) ? `<p class="transcript-correction-note">Speaker names corrected here are a local review layer. The retained transcript file is unchanged.</p>` : ""}
      </section>
    `;
  }
  return `
    <section class="transcript-panel" aria-labelledby="transcript-heading">
      <div class="transcript-heading"><h3 id="transcript-heading">${escapeHtml(title)}</h3>${detail ? `<p>${escapeHtml(detail)}</p>` : ""}</div>
      ${actions}
      ${transcriptLines}
    </section>
  `;
}

function renderMeetingNoteItems(claims, claimEvidence) {
  return `
    <ul class="meeting-note-list">${claims.map((claim) => `<li class="meeting-note-item"${Number.isInteger(claim.ordinal) ? ` data-claim-item="${escapeHtml(claim.ordinal)}"` : ""}>
          <p>${escapeHtml(claim.claim)}</p>
          ${renderClaimEvidence(claim, claimEvidence[claim.ordinal])}
        </li>`).join("")}</ul>
  `;
}

function renderMeetingNote(note, claimEvidence) {
  const presentation = meetingNotePresentation(note);
  if (presentation.state === "empty") {
    if (note?.state !== "transcript-only") return "";
    return `
      <section class="meeting-note meeting-note-unavailable" aria-labelledby="meeting-note-heading">
        <header class="meeting-note-header">
          <p class="eyebrow">Meeting note</p>
          <h2 id="meeting-note-heading">No meeting note yet.</h2>
          <p>${escapeHtml(note?.message || "Yawn has no generated note for this meeting.")}</p>
        </header>
      </section>
    `;
  }
  if (presentation.state === "extracts-only") {
    const count = presentation.highlights.length;
    return `
      <section class="meeting-note meeting-note-unavailable" aria-labelledby="meeting-note-heading">
        <header class="meeting-note-header">
          <p class="eyebrow">Meeting note</p>
          <h2 id="meeting-note-heading">A summary wasn’t produced.</h2>
          <p>Yawn selected ${count} transcript ${count === 1 ? "excerpt" : "excerpts"}, but those excerpts are source material, not a meeting summary.</p>
        </header>
        <details class="transcript-highlights">
          <summary><span>Review selected excerpts</span><span>${count}</span></summary>
          <div class="transcript-highlights-content">${renderMeetingNoteItems(presentation.highlights, claimEvidence)}</div>
        </details>
      </section>
    `;
  }
  return `
    <section class="meeting-note" aria-labelledby="meeting-note-heading">
      <header class="meeting-note-header">
        <p class="eyebrow">Meeting note</p>
        <h2 id="meeting-note-heading">What happened and what comes next</h2>
        <p>Generated from the transcript. Use the source links to check anything that matters.</p>
      </header>
      ${presentation.summary.length ? `
        <section class="meeting-note-section meeting-note-overview" aria-labelledby="meeting-overview-heading">
          <h3 id="meeting-overview-heading">Overview</h3>
          ${presentation.summary.map((claim) => `<div class="meeting-note-summary-item"${Number.isInteger(claim.ordinal) ? ` data-claim-item="${escapeHtml(claim.ordinal)}"` : ""}>
            <p>${escapeHtml(claim.claim)}</p>
            ${claim.handle ? renderClaimEvidence(claim, claimEvidence[claim.ordinal]) : ""}
          </div>`).join("")}
        </section>
      ` : ""}
      ${presentation.groups.map((group, index) => `
        <section class="meeting-note-section" aria-labelledby="meeting-note-group-${index}">
          <h3 id="meeting-note-group-${index}">${escapeHtml(group.title)}</h3>
          ${renderMeetingNoteItems(group.claims, claimEvidence)}
        </section>
      `).join("")}
      ${presentation.highlights.length ? `
        <details class="transcript-highlights">
          <summary><span>Additional transcript highlights</span><span>${presentation.highlights.length}</span></summary>
          <div class="transcript-highlights-content">${renderMeetingNoteItems(presentation.highlights, claimEvidence)}</div>
        </details>
      ` : ""}
    </section>
  `;
}

function renderTranscriptDisclosure(transcript, recovery = null, note = null) {
  if (recovery?.state === "transcript-unavailable" && !transcript?.turns?.length) return "";
  if (!transcript?.turns?.length && !transcript?.message) return "";
  return `
    <details class="transcript-disclosure">
      <summary><span><strong>Full transcript</strong><small>The retained record for checking a decision, owner, or follow-up.</small></span><span class="transcript-disclosure-state">Open</span></summary>
      <div class="transcript-disclosure-content">
        ${transcript?.turns?.length ? renderTranscript(transcript.turns, "Source transcript", "Search or read the complete retained conversation.", {
          copyAction: "copy-library-transcript",
          openFileAction: "open-library-transcript-file",
          exportAction: "export-meeting",
          workspace: true,
          citations: note ? { turnsCited: note.turnsCited, claims: note.claims } : null,
        }) : `<section class="note-section transcript-unavailable"><p class="message-card">${escapeHtml(transcript.message)}</p></section>`}
      </div>
    </details>
  `;
}

function renderMeetingRecovery(recovery) {
  if (!recovery) return "";
  const headingId = `meeting-recovery-${recovery.state}`;
  return `
    <section class="message-card recovery-card ${recovery.tone === "working" ? "" : "attention"}" aria-labelledby="${headingId}">
      <h2 id="${headingId}">${escapeHtml(recovery.title)}</h2>
      <p>${escapeHtml(recovery.detail)}</p>
      ${recovery.action ? `<button class="button button-primary button-small" type="button" data-action="${escapeHtml(recovery.action.action)}">${escapeHtml(recovery.action.label)}</button>` : ""}
    </section>
  `;
}

function transcriptRetryContext(note, transcript, pending = null) {
  return transcriptRetryPresentation({
    meetingId: note?.meetingId || state.selected?.row?.meetingId || "",
    transcriptMeetingId: transcript?.meetingId || "",
    sourceTranscriptSha256: transcript?.currentTranscriptSha256 || "",
    audioRetentionState: note?.audioRetention?.state || "",
    capture: state.snapshot?.capture || "",
    recovery: meetingRecoveryPresentation(note, transcript, state.generatingMeetingId),
    pending,
  });
}

function renderTranscriptRetryAction(note, transcript, recovery) {
  const retry = transcriptRetryPresentation({
    meetingId: note?.meetingId || state.selected?.row?.meetingId || "",
    transcriptMeetingId: transcript?.meetingId || "",
    sourceTranscriptSha256: transcript?.currentTranscriptSha256 || "",
    audioRetentionState: note?.audioRetention?.state || "",
    capture: state.snapshot?.capture || "",
    recovery,
    pending: state.selected?.transcriptRetry || null,
  });
  if (!retry) return "";
  const starting = state.transcriptRetry?.phase === "starting";
  return `
    <section class="transcript-retry-action" aria-labelledby="transcript-retry-heading">
      <div>
        <h3 id="transcript-retry-heading">Transcript retry</h3>
        <p>${retry.pending
    ? "A retry is ready to review. Keep the retained transcript or explicitly use the retry."
    : "Run a local retry, then compare it with the retained transcript before deciding."}</p>
      </div>
      <button class="button button-quiet button-small" id="transcript-retry-action" type="button" data-action="${retry.action}" ${starting ? "disabled" : ""}>${starting ? "Preparing retry…" : escapeHtml(retry.label)}</button>
    </section>
  `;
}

function renderMeeting() {
  const { row, note, transcript } = state.selected;
  const title = row.label || `Meeting · ${dateLabel(row.createdAtEpochSeconds)}`;
  const claims = note?.claims || [];
  const operatorNote = note?.operatorNote;
  const selectedNoteState = state.selected?.operatorNoteSaveState || "local";
  const selectedNoteCopy = operatorNote?.unreadable
    ? "Not editable"
    : selectedNoteState === "saving"
      ? "Saving…"
      : selectedNoteState === "saved"
        ? "Saved on this Mac"
        : "Stored on this Mac";
  const noteEditable = !operatorNote?.unreadable && Boolean(note?.operatorNoteHandle);
  const claimEvidence = state.selected.claimEvidence || {};
  const metadataRevision = state.library?.metadataRevision;
  const canRename = metadataRevision != null && Number.isInteger(Number(metadataRevision));
  const canDeleteRecording = Boolean(note?.audioDeletionHandle);
  const canDeleteTranscript = Boolean(note?.transcriptDeletionHandle);
  const canDeleteMeeting = Boolean(note?.meetingDeletionHandle);
  const lock = meetingLockPresentation(note);
  // Roadmap intake I5. Locking is offered on any readable meeting; removing a
  // lock is offered wherever one exists, including on the barrier screen
  // itself, so an unreadable lock is never a dead end.
  const canLock = lock?.state === "unlocked";
  const canUnlock = Boolean(note?.lock?.locked);
  const canManage = canDeleteRecording || canDeleteTranscript || canDeleteMeeting || canLock || canUnlock;
  const retentionMessage = note?.audioRetention?.message || "Audio-retention details are unavailable for this meeting.";
  const recovery = meetingRecoveryPresentation(note, transcript, state.generatingMeetingId);
  const playback = retainedAudioPlaybackPresentation(note, recovery, state.audioPlayback);
  return `
    <article class="meeting-page meeting-workspace-page" aria-labelledby="meeting-title">
      <button class="text-button" type="button" data-action="meetings">Back to meetings</button>
      <header class="meeting-detail-header">
        <p class="eyebrow">Saved on this Mac</p>
        <div class="meeting-title-row">
          <h1 class="meeting-title" id="meeting-title">${escapeHtml(title)}</h1>
          <div class="meeting-header-actions">
            ${canRename ? `<button class="button button-quiet button-small" type="button" data-action="rename-meeting">Rename</button>` : ""}
            ${canManage ? `<div class="meeting-manage-control">
              <button class="button button-secondary button-small" type="button" data-action="toggle-meeting-management" aria-expanded="${state.meetingManagementOpen ? "true" : "false"}" aria-controls="meeting-manage-menu">Manage</button>
              ${state.meetingManagementOpen ? `<div class="meeting-manage-menu" id="meeting-manage-menu" role="group" aria-label="Manage this meeting">
                <p>These actions affect only this meeting on this Mac.</p>
                ${canLock ? `<button class="button button-secondary button-small" type="button" data-action="lock-meeting">Lock meeting…</button>` : ""}
                ${canUnlock ? `<button class="button button-secondary button-small" type="button" data-action="unlock-meeting">Remove lock…</button>` : ""}
                ${canDeleteRecording ? `<button class="button button-secondary button-small" type="button" data-action="delete-recording">Delete recording</button>` : ""}
                ${canDeleteTranscript ? `<button class="button button-secondary button-small" type="button" data-action="delete-transcript">Delete transcript</button>` : ""}
                ${canDeleteMeeting ? `<button class="button button-danger button-small" type="button" data-action="delete-meeting">Delete meeting…</button>` : ""}
              </div>` : ""}
            </div>` : ""}
          </div>
        </div>
        <div class="meeting-meta"><span>${escapeHtml(dateLabel(row.createdAtEpochSeconds))}</span><span>${escapeHtml(note?.state ? humanize(note.state) : "Loading note")}</span></div>
        <p class="meeting-storage-note">${escapeHtml(retentionMessage)}</p>
        ${renderMeetingCapturePauses(note?.capturePauses)}
      </header>
      ${renderMeetingLock(lock)}
      ${lock?.state === "locked" || lock?.state === "unreadable" ? "" : `
      ${renderMeetingRecovery(recovery)}
      ${!recovery && note?.state !== "transcript-only" && note?.message && !claims.length ? `<p class="message-card ${note.state === "summary-failed" ? "attention" : ""}">${escapeHtml(note.message)}</p>` : ""}
      <div class="meeting-workspace">
        <main class="meeting-source-pane">
          ${renderMeetingNote(note, claimEvidence)}
          ${renderGenerateNote(note, recovery)}
          ${renderTranscriptRetryAction(note, transcript, recovery)}
          ${renderTranscriptDisclosure(transcript, recovery, note)}
        </main>
        <aside class="meeting-notes-pane">
          ${renderRetainedAudioPlayback(playback)}
          ${renderMeetingContextSection(note)}
          <section class="note-section your-notes-section" aria-labelledby="operator-note-heading">
          <div class="note-editor-head"><h3 id="operator-note-heading">Your notes</h3><span class="save-state" id="library-note-save-state">${escapeHtml(selectedNoteCopy)}</span></div>
          ${operatorNote?.unreadable
            ? `<p class="message-card attention">Yawn could not read this meeting’s personal note, so it was left unchanged.</p>`
            : `<textarea class="note-editor meeting-notes-editor" data-field="library-operator-note" data-meeting-id="${escapeHtml(row.meetingId || "")}" aria-label="Your meeting notes" placeholder="Write down the detail you will want to verify later." ${noteEditable ? "" : "disabled"}>${escapeHtml(state.selected?.operatorNoteDraft || "")}</textarea>
              <p class="note-editor-help">${noteEditable ? "Saved separately from the transcript. These are your notes, not generated claims." : "Reopen this meeting to edit its notes."}</p>`}
          </section>
        </aside>
      </div>`}
    </article>
  `;
}

// Roadmap intake I5. The barrier itself, and the "still locked" note above a
// meeting that a confirmation opened for reading.
//
// A locked meeting renders this and nothing else -- there is no meeting
// content behind it to hide, because the response carried none. The sentence
// is the packet's honest claim, said where the reader meets the lock rather
// than only in a confirmation sheet they may never reopen.
function renderMeetingLock(lock) {
  if (!lock || lock.state === "unlocked") return "";
  if (lock.state === "open") {
    return `<p class="message-card meeting-lock-open" data-lock-state="open">${escapeHtml(lock.detail)}</p>`;
  }
  const busy = state.busyAction === "unlock-meeting-open";
  return `
    <section class="empty-library recovery-card meeting-lock-card" data-lock-state="${escapeHtml(lock.state)}" aria-labelledby="meeting-lock-title">
      <h3 id="meeting-lock-title">${escapeHtml(lock.heading)}</h3>
      <p>${escapeHtml(lock.detail)}</p>
      ${lock.action ? `<button class="button button-primary button-small" type="button" data-action="${escapeHtml(lock.action.action)}" ${busy ? "disabled" : ""}>${busy ? "Confirming…" : escapeHtml(lock.action.label)}</button>` : ""}
    </section>
  `;
}

// Roadmap intake I3's sidecar, shown read-only next to the operator's own
// note the same way `operatorNote` already is, so a meeting's stated purpose
// stays reachable after the meeting instead of disappearing once capture ends.
// No editing here -- `save_meeting_context` remains scoped to the meeting
// currently being captured.
function renderMeetingContextSection(note) {
  const presentation = meetingContextPresentation(note?.meetingContext);
  return `
    <section class="note-section meeting-context-section" aria-labelledby="meeting-context-heading">
      <div class="note-editor-head"><h3 id="meeting-context-heading">Meeting context</h3></div>
      ${presentation.state === "unreadable"
        ? `<p class="message-card attention">Yawn could not read this meeting’s pre-meeting context.</p>`
        : presentation.state === "present"
          ? `<p class="meeting-context-text">${escapeHtml(presentation.text)}</p>
             <p class="note-editor-help">What the operator said this meeting was for, used to guide the generated note. Not a transcript.</p>`
          : `<p class="note-editor-help">No pre-meeting context was written for this meeting.</p>`}
    </section>
  `;
}

// A gap in the audio changes how the transcript should be read, so it is stated
// next to where this meeting's record is described.
//
// An uninterrupted recording is the ordinary case and says nothing; a gap, or a
// gap Yawn could not check for, is what the reader needs told. Nothing renders
// until the note response has actually arrived, so a meeting still loading
// never reads as unverifiable.
function renderMeetingCapturePauses(pauses) {
  if (!pauses) return "";
  const presentation = capturePausePresentation(pauses);
  if (presentation.state === "not-paused") return "";
  return `<p class="message-card attention" data-capture-pauses="${escapeHtml(presentation.state)}">${escapeHtml(presentation.detail)}</p>`;
}

function renderRetainedAudioPlayback(playback) {
  if (!playback) return "";
  const playingLabel = playback.playingSource === "microphone"
    ? "Playing microphone recording"
    : playback.playingSource === "system"
      ? "Playing system audio recording"
      : playback.status === "completed"
        ? "Recording finished"
        : "No recording is playing";
  return `
    <section class="note-section retained-audio-section" aria-labelledby="retained-audio-heading">
      <h3 id="retained-audio-heading">Listen to saved audio</h3>
      <p class="note-editor-help">Microphone and system audio are separate recordings.</p>
      <div class="retained-audio-controls">
        ${playback.controls.map((control) => `<button class="button button-secondary button-small" type="button" data-action="play-retained-audio" data-source="${escapeHtml(control.source)}" ${playback.isPlaying ? "disabled" : ""}>${escapeHtml(control.label)}</button>`).join("")}
        ${playback.isPlaying ? `<button class="button button-quiet button-small" type="button" data-action="stop-retained-audio">Stop</button>` : ""}
      </div>
      <p class="save-state" aria-live="polite">${escapeHtml(playingLabel)}</p>
    </section>
  `;
}

function renderGenerateNote(note, recovery = meetingRecoveryPresentation(note, state.selected?.transcript, state.generatingMeetingId)) {
  if (recovery && recovery.state !== "audio-released") return "";
  const control = noteGenerationPresentation(note, state.generatingMeetingId);
  if (!control) return "";
  return `
    <section class="note-section generate-note-section" aria-label="Generate a meeting note">
      <button class="button button-primary" type="button" data-action="${control.action}" ${control.disabled ? "disabled" : ""}>${escapeHtml(control.label)}</button>
      <p class="note-editor-help">${escapeHtml(control.help)}</p>
    </section>
  `;
}

function renderClaimEvidence(claim, evidence) {
  if (!claim.locatorCount) return `<span class="claim-source-unavailable">No source passage is available for this point.</span>`;
  if (evidence?.state === "evidence" && evidence.text) {
    return `
      <div class="claim-evidence">
        <span>Transcript source · ${escapeHtml(timeLabel(evidence.start))}</span>
        <p>${escapeHtml(evidence.text)}</p>
      </div>
    `;
  }
  const action = `evidence-${claim.ordinal}`;
  return `<button class="claim-source-button" type="button" data-action="open-claim-evidence" data-ordinal="${escapeHtml(claim.ordinal)}" ${state.busyAction === action ? "disabled" : ""}>${state.busyAction === action ? "Opening source…" : "Show source"}</button>`;
}

function renderStartSheet() {
  const permission = permissionSummary(state.permissions);
  const audioReady = permission.state === "ready";
  const allConfirmed = Object.values(state.consent).every(Boolean);
  const action = permissionAction(state.permissions);
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet" role="dialog" aria-modal="true" aria-labelledby="start-sheet-title">
        <div class="sheet-head">
          <div><p class="eyebrow">Before recording</p><h2 id="start-sheet-title">Make the start explicit.</h2><p>Yawn records only after you confirm the meeting is ready to capture.</p></div>
          <button class="icon-button" type="button" data-action="close-start" aria-label="Close">×</button>
        </div>
        ${audioReady ? "" : `<div class="message-card attention"><strong>${escapeHtml(permission.title)}</strong><p>${escapeHtml(permission.detail)}</p><button class="text-button" type="button" data-action="${action.action}">${escapeHtml(action.label)}</button></div>`}
        <label class="field-label">Keep recording audio for
          <small>This choice covers the saved audio.</small>
          <select class="select" data-field="retention-days">${[1, 7, 30].map((days) => `<option value="${days}" ${Number(state.retentionDays) === days ? "selected" : ""}>${retentionLabel(days)}</option>`).join("")}</select>
        </label>
        <div class="attestation-list">
          ${attestation("participantsConsented", "Everyone in this meeting has agreed to be recorded.")}
          ${attestation("headphones", "I am using headphones for this recording.")}
          ${attestation("operatorAlone", "I am the only person near this microphone.")}
        </div>
        <p class="quiet-copy start-sheet-privacy-note">Recording, transcription, and storage all happen on this Mac — details in Settings.</p>
        <div class="sheet-actions">
          <button class="button button-quiet" type="button" data-action="close-start">Cancel</button>
          <button class="button button-record" type="button" data-action="start-recording" ${!audioReady || !allConfirmed || state.busyAction === "start" ? "disabled" : ""}>${state.busyAction === "start" ? "Starting…" : "Start recording"}</button>
        </div>
      </section>
    </div>
  `;
}

function renderRenameMeetingSheet() {
  const selection = state.selected;
  if (!selection) return "";
  const saving = state.busyAction === "rename-meeting";
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet" role="dialog" aria-modal="true" aria-labelledby="rename-meeting-title">
        <div class="sheet-head">
          <div><p class="eyebrow">Meeting name</p><h2 id="rename-meeting-title">Give this meeting a useful name.</h2><p>Only this local meeting’s label changes. The recording, transcript, and notes stay as they are.</p></div>
          <button class="icon-button" type="button" data-action="close-modal" aria-label="Close">×</button>
        </div>
        <form data-form="rename-meeting">
          <label class="field-label" for="meeting-title-input">Meeting name
            <input class="meeting-title-input" id="meeting-title-input" data-field="meeting-title" data-meeting-id="${escapeHtml(state.selected?.row?.meetingId || "")}" maxlength="120" value="${escapeHtml(state.renameDraft)}" placeholder="e.g. Q3 pricing review" autocomplete="off" />
            <small>Leave this empty to use the opening line from the transcript again.</small>
          </label>
          <div class="sheet-actions">
            <button class="button button-quiet" type="button" data-action="close-modal">Cancel</button>
            <button class="button button-primary" type="submit" ${saving ? "disabled" : ""}>${saving ? "Saving…" : "Save name"}</button>
          </div>
        </form>
      </section>
    </div>
  `;
}

function openSpeakerCorrection(sourceTurnIndex) {
  const transcript = state.selected?.transcript;
  const turn = transcript?.turns?.find((candidate) => Number(candidate.sourceTurnIndex) === sourceTurnIndex);
  if (!turn || turn.withheld || !transcript?.currentTranscriptSha256) return;
  const sourceSpeaker = turn.sourceSpeaker || null;
  const sourceLabel = sourceSpeaker || "Unattributed";
  state.speakerCorrection = {
    meetingId: transcript.meetingId,
    sourceTranscriptSha256: transcript.currentTranscriptSha256,
    sourceSpeaker,
    sourceLabel,
  };
  state.speakerCorrectionDraft = transcriptSpeakerLabel(turn) === "Unattributed"
    ? ""
    : transcriptSpeakerLabel(turn);
  state.modal = "speaker-correction";
  render();
  queueMicrotask(() => root.querySelector("#speaker-name-input")?.focus());
}

function renderSpeakerCorrectionSheet() {
  const correction = state.speakerCorrection;
  const turns = state.selected?.transcript?.turns || [];
  if (!correction) return "";
  const affected = transcriptTurnsForSourceSpeaker(turns, correction.sourceSpeaker).length;
  const saving = state.busyAction === "speaker-correction";
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet speaker-correction-sheet" role="dialog" aria-modal="true" aria-labelledby="speaker-correction-title">
        <div class="sheet-head">
          <div><p class="eyebrow">Transcript attribution</p><h2 id="speaker-correction-title">Name this speaker.</h2><p>This changes the review label for ${affected} matching transcript ${affected === 1 ? "turn" : "turns"}. The retained transcript file stays unchanged.</p></div>
          <button class="icon-button" type="button" data-action="close-modal" aria-label="Close">×</button>
        </div>
        <div class="speaker-correction-source"><span>Source label</span><strong>${escapeHtml(correction.sourceLabel)}</strong></div>
        <form data-form="speaker-correction">
          <label class="field-label" for="speaker-name-input">Speaker name
            <input class="meeting-title-input" id="speaker-name-input" data-field="speaker-name" data-meeting-id="${escapeHtml(state.speakerCorrection?.meetingId || "")}" maxlength="80" value="${escapeHtml(state.speakerCorrectionDraft)}" placeholder="e.g. Alex" autocomplete="off" />
            <small>Every turn tied to this source label will use the same name in this meeting.</small>
          </label>
          <div class="speaker-correction-provenance"><strong>What stays preserved</strong><p>Yawn keeps the original label and records this as a separate local correction. Reopen this control and use the source label to undo it.</p></div>
          <div class="sheet-actions">
            <button class="button button-quiet" type="button" data-action="use-source-speaker">Use source label</button>
            <button class="button button-quiet" type="button" data-action="close-modal">Cancel</button>
            <button class="button button-primary" type="submit" ${saving || !state.speakerCorrectionDraft.trim() ? "disabled" : ""}>${saving ? "Saving…" : "Save speaker name"}</button>
          </div>
        </form>
      </section>
    </div>
  `;
}

async function saveSpeakerCorrection() {
  const correction = state.speakerCorrection;
  const selection = state.selected;
  const replacement = state.speakerCorrectionDraft.trim();
  if (!correction || !selection?.row?.meetingId || !replacement) return;
  await flushSelectedNoteSave();
  await runBusy("speaker-correction", async () => {
    const response = await invoke("correct_speaker_name", {
      meetingId: correction.meetingId,
      sourceTranscriptSha256: correction.sourceTranscriptSha256,
      sourceSpeaker: correction.sourceSpeaker,
      replacement,
    });
    if (state.selected !== selection) return;
    state.modal = "";
    state.speakerCorrection = null;
    state.speakerCorrectionDraft = "";
    state.notice = response.message || "Speaker name saved on this Mac.";
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

function vocabularyContext() {
  return localVocabularyPresentation({
    meetingId: state.selected?.row?.meetingId,
    transcriptMeetingId: state.selected?.transcript?.meetingId,
    transcriptSha256: state.selected?.transcript?.currentTranscriptSha256,
    capture: state.snapshot?.capture,
  });
}

function resetVocabularyDraft() {
  if (!state.vocabulary) return;
  state.vocabulary = {
    ...state.vocabulary,
    editingId: "",
    pendingDeleteId: "",
    sourcePhrase: "",
    preferredReplacement: "",
  };
}

function closeModal() {
  const retryOpen = state.modal === "transcript-retry";
  state.modal = "";
  state.speakerCorrection = null;
  state.speakerCorrectionDraft = "";
  state.transcriptRetry = null;
  state.vocabulary = null;
  if (retryOpen) queueMicrotask(() => root.querySelector("#transcript-retry-action")?.focus());
}

async function openVocabulary() {
  const context = vocabularyContext();
  if (!context) return;
  state.vocabulary = {
    ...context,
    entries: [],
    editingId: "",
    sourcePhrase: "",
    preferredReplacement: "",
    loading: true,
  };
  state.modal = "vocabulary";
  render();
  queueMicrotask(() => root.querySelector("#vocabulary-before-input")?.focus());
  await refreshVocabulary();
}

async function refreshVocabulary() {
  const vocabulary = state.vocabulary;
  if (!vocabulary) return;
  state.vocabulary = { ...vocabulary, loading: true };
  render();
  try {
    const response = await invoke("local_vocabulary_list", {
      meetingId: vocabulary.meetingId,
      sourceTranscriptSha256: vocabulary.sourceTranscriptSha256,
    });
    if (state.vocabulary !== vocabulary && state.vocabulary?.meetingId !== vocabulary.meetingId) return;
    state.vocabulary = { ...state.vocabulary, entries: response.entries || [], loading: false };
  } catch (error) {
    if (state.vocabulary?.meetingId === vocabulary.meetingId) {
      state.vocabulary = { ...state.vocabulary, loading: false };
    }
    reportError(error);
    return;
  }
  render();
}

function renderVocabularySheet() {
  const vocabulary = state.vocabulary;
  if (!vocabulary) return "";
  const editing = vocabulary.entries?.find((entry) => entry.id === vocabulary.editingId);
  const saving = state.busyAction === "vocabulary-save";
  const rows = vocabulary.entries?.length
    ? vocabulary.entries.map((entry) => {
      const count = Number(entry.appliedTurnCount) || 0;
      const use = `${count} ${count === 1 ? "use" : "uses"} in this meeting`;
      return `
        <li class="vocabulary-ledger-row" data-enabled="${entry.enabled ? "true" : "false"}">
          <div class="vocabulary-ledger-terms"><span>${escapeHtml(entry.sourcePhrase)}</span><span aria-hidden="true">→</span><strong>${escapeHtml(entry.preferredReplacement)}</strong></div>
          <div class="vocabulary-ledger-meta"><span>${entry.enabled ? "Enabled" : "Disabled"}</span><span>${escapeHtml(use)}</span></div>
          <div class="vocabulary-ledger-actions">
            ${vocabulary.pendingDeleteId === entry.id
              ? `<span class="vocabulary-delete-confirmation" role="status">Delete this replacement?</span><button class="text-button" type="button" data-action="cancel-vocabulary-delete">Cancel</button><button class="text-button vocabulary-delete" type="button" data-action="confirm-vocabulary-delete" data-vocabulary-id="${escapeHtml(entry.id)}">Delete</button>`
              : `<button class="text-button" type="button" data-action="edit-vocabulary" data-vocabulary-id="${escapeHtml(entry.id)}">Edit</button><button class="text-button" type="button" data-action="toggle-vocabulary" data-vocabulary-id="${escapeHtml(entry.id)}">${entry.enabled ? "Disable" : "Enable"}</button><button class="text-button vocabulary-delete" type="button" data-action="request-vocabulary-delete" data-vocabulary-id="${escapeHtml(entry.id)}">Delete</button>`}
          </div>
        </li>
      `;
    }).join("")
    : `<li class="vocabulary-ledger-empty">${vocabulary.loading ? "Checking saved replacements…" : "No local replacements yet. Add an exact Before → After correction below."}</li>`;
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet vocabulary-sheet" role="dialog" aria-modal="true" aria-labelledby="vocabulary-sheet-title">
        <div class="sheet-head">
          <div><p class="eyebrow">Transcript vocabulary</p><h2 id="vocabulary-sheet-title">Keep exact words consistent.</h2><p>These local Before → After replacements apply to future note regenerations. They do not rewrite this transcript, change the words shown here, or regenerate a note.</p></div>
          <button class="icon-button" type="button" data-action="close-modal" aria-label="Close vocabulary">×</button>
        </div>
        <section class="vocabulary-ledger" aria-labelledby="vocabulary-ledger-title">
          <div class="vocabulary-ledger-head"><h3 id="vocabulary-ledger-title">Correction ledger</h3><span>${vocabulary.entries?.length || 0} saved</span></div>
          <ul>${rows}</ul>
        </section>
        <form data-form="vocabulary">
          <div class="vocabulary-form-head"><h3>${editing ? "Edit replacement" : "Add replacement"}</h3>${editing ? `<button class="text-button" type="button" data-action="cancel-vocabulary-edit">Cancel edit</button>` : ""}</div>
          <div class="vocabulary-fields">
            <label class="field-label" for="vocabulary-before-input">Before
              <input class="meeting-title-input" id="vocabulary-before-input" data-field="vocabulary-before" autocorrect="off" spellcheck="false" data-meeting-id="${escapeHtml(vocabulary.meetingId)}" maxlength="256" value="${escapeHtml(vocabulary.sourcePhrase)}" placeholder="Exact transcript spelling" autocomplete="off" />
            </label>
            <span class="vocabulary-arrow" aria-hidden="true">→</span>
            <label class="field-label" for="vocabulary-after-input">After
              <input class="meeting-title-input" id="vocabulary-after-input" data-field="vocabulary-after" autocorrect="off" spellcheck="false" data-meeting-id="${escapeHtml(vocabulary.meetingId)}" maxlength="256" value="${escapeHtml(vocabulary.preferredReplacement)}" placeholder="Preferred spelling" autocomplete="off" />
            </label>
          </div>
          <p class="vocabulary-help">Exact, case-sensitive matches only. Each phrase can be up to 256 characters.</p>
          <div class="sheet-actions">
            <button class="button button-quiet" type="button" data-action="close-modal">Close</button>
            <button class="button button-primary" type="submit" ${saving || !vocabulary.sourcePhrase.trim() || !vocabulary.preferredReplacement.trim() ? "disabled" : ""}>${saving ? "Saving…" : editing ? "Save replacement" : "Add replacement"}</button>
          </div>
        </form>
      </section>
    </div>
  `;
}

function retryWarningText(warning) {
  if (typeof warning === "string") return warning;
  if (warning && typeof warning.message === "string") return warning.message;
  return "Yawn reported a warning for this transcript.";
}

function renderRetryWarnings(warnings, label) {
  if (!Array.isArray(warnings) || !warnings.length) return "";
  return `<section class="retry-warnings" aria-label="${escapeHtml(label)} warnings"><h4>${escapeHtml(label)} warnings</h4><ul>${warnings.map((warning) => `<li>${escapeHtml(retryWarningText(warning))}</li>`).join("")}</ul></section>`;
}

function retryQualityObservation(observation) {
  return `<li><span>${escapeHtml(observation.kind)}</span><strong>${escapeHtml(observation.detail)}</strong></li>`;
}

function renderRetryQuality(quality) {
  const presentation = transcriptRetryQualityPresentation(quality);
  return `
    <section class="retry-quality" data-state="${escapeHtml(presentation.state)}" aria-labelledby="retry-quality-heading">
      <div><h3 id="retry-quality-heading">Capture quality</h3><p>${escapeHtml(presentation.message)}</p></div>
      ${presentation.observations.length ? `<ul>${presentation.observations.map(retryQualityObservation).join("")}</ul>` : ""}
    </section>
  `;
}

function renderRetryCapturePauses(pauses) {
  const presentation = capturePausePresentation(pauses);
  return `
    <section class="retry-quality" data-state="${escapeHtml(presentation.state)}" aria-labelledby="retry-pauses-heading">
      <div><h3 id="retry-pauses-heading">${escapeHtml(presentation.title)}</h3><p>${escapeHtml(presentation.detail)}</p></div>
    </section>
  `;
}

function renderRetryRecordingDevice(device) {
  const presentation = recordingDevicePresentation(device);
  return `
    <section class="retry-quality" data-state="${escapeHtml(presentation.state)}" aria-labelledby="retry-device-heading">
      <div><h3 id="retry-device-heading">${escapeHtml(presentation.title)}</h3><p>${escapeHtml(presentation.detail)}</p></div>
      ${presentation.action ? `<button class="button button-quiet button-small" type="button" data-action="${presentation.action.action}">${escapeHtml(presentation.action.label)}</button>` : ""}
    </section>
  `;
}

// D6: renders each turn's text with word-level diff highlighting so the
// reader sees what differs between the two sides before the keep/promote
// choice, instead of only being able to browse both transcripts side by
// side. `entry` is this turn's `{wordCount, spans}` from
// transcriptRetryDiffPresentation, keyed by the turn's position in this
// side's `turns` array — matching how the Rust diff built its per-side
// sequence. retryTurnDiffSegments does its own tokenizer-agreement check
// against `entry.wordCount`; an absent entry (turn not in the map) renders
// plain the same fail-safe way a mismatched count would.
function renderRetryTurnBody(turn, entry) {
  if (turn.withheld) return "This turn was withheld by the voice check.";
  return retryTurnDiffSegments(turn.text, entry)
    .map((segment) => segment.highlighted
      ? `<span class="retry-diff-word">${escapeHtml(segment.text)}</span>`
      : escapeHtml(segment.text))
    .join("");
}

function renderRetryComparisonTurns(turns, label) {
  const rows = Array.isArray(turns) ? turns : [];
  const diffPresentation = transcriptRetryDiffPresentation(state.transcriptRetry?.diff);
  const spansByTurn = label === "current" ? diffPresentation.current : diffPresentation.candidate;
  return `
    <section class="retry-transcript-column" data-side="${escapeHtml(label)}" aria-labelledby="retry-${label}-heading">
      <header><p class="eyebrow">${escapeHtml(label === "current" ? "Retained transcript" : "New local result")}</p><h3 id="retry-${label}-heading">${label === "current" ? "Current" : "Retry candidate"}</h3></header>
      ${renderRetryWarnings(label === "current" ? state.transcriptRetry?.current?.warnings : state.transcriptRetry?.candidate?.warnings, label === "current" ? "Current transcript" : "Retry candidate")}
      <div class="retry-transcript-turns" tabindex="0" aria-label="${escapeHtml(label === "current" ? "Current transcript turns" : "Retry candidate transcript turns")}">
        ${rows.length ? rows.map((turn, index) => {
    const speaker = transcriptSpeakerLabel(turn);
    const entry = spansByTurn.get(index) || null;
    return `<div class="transcript-line ${turn.withheld ? "withheld" : ""}">
              <div class="transcript-line-meta"><time>${escapeHtml(timeLabel(turn.start))}</time>${speaker ? `<span>${escapeHtml(speaker)}</span>` : ""}</div>
              <p>${renderRetryTurnBody(turn, entry)}</p>
            </div>`;
  }).join("") : `<p class="transcript-empty">No transcript turns are available for this comparison.</p>`}
      </div>
    </section>
  `;
}

function renderRetryDiffLegend() {
  const presentation = transcriptRetryDiffPresentation(state.transcriptRetry?.diff);
  return `<p class="retry-diff-legend">${escapeHtml(presentation.legend)}</p>`;
}

function renderTranscriptRetrySheet() {
  const retry = state.transcriptRetry;
  if (!retry?.operationId) return "";
  const deciding = state.busyAction === "decide-transcript-retry";
  // The warning is about losing a generated note, so it is only true when one
  // exists. "transcript-only" is the same signal the note card reads to say
  // "No meeting note yet." An unknown or still-loading note state keeps the
  // warning, because warning is the fail-safe direction.
  const hasNoGeneratedNote = state.selected?.note?.state === "transcript-only";
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet transcript-retry-sheet" role="dialog" aria-modal="true" aria-labelledby="transcript-retry-sheet-title">
        <div class="sheet-head">
          <div><p class="eyebrow">Transcript retry</p><h2 id="transcript-retry-sheet-title">Compare before changing the source.</h2><p>Nothing changes until you choose. The retained transcript stays as it is unless you explicitly use this retry.</p></div>
          <button class="icon-button" type="button" data-action="decide-retry-later" aria-label="Decide later">×</button>
        </div>
        ${renderRetryQuality(retry.quality)}
        ${renderRetryCapturePauses(retry.pauses)}
        ${renderRetryRecordingDevice(retry.recordingDevice)}
        ${renderRetryDiffLegend()}
        <div class="retry-transcript-comparison">
          ${renderRetryComparisonTurns(retry.current?.turns, "current")}
          ${renderRetryComparisonTurns(retry.candidate?.turns, "candidate")}
        </div>
        ${hasNoGeneratedNote ? "" : `<section class="retry-use-warning" aria-labelledby="retry-use-warning-heading"><h3 id="retry-use-warning-heading">Using this retry clears the current generated note.</h3><p>You will need to regenerate the note from the selected retry. Yawn will not regenerate it automatically.</p></section>`}
        <div class="sheet-actions retry-sheet-actions">
          <button class="button button-quiet" type="button" data-action="decide-retry-later" ${deciding ? "disabled" : ""}>Decide later</button>
          <button class="button button-secondary" type="button" data-action="keep-current-transcript" ${deciding ? "disabled" : ""}>${deciding ? "Saving decision…" : "Keep current"}</button>
          <button class="button button-danger" type="button" data-action="use-retry-transcript" ${deciding ? "disabled" : ""}>${deciding ? "Saving decision…" : "Use retry"}</button>
        </div>
      </section>
    </div>
  `;
}

async function saveVocabulary() {
  const vocabulary = state.vocabulary;
  if (!vocabulary || !vocabulary.sourcePhrase.trim() || !vocabulary.preferredReplacement.trim()) return;
  const payload = {
    meetingId: vocabulary.meetingId,
    sourceTranscriptSha256: vocabulary.sourceTranscriptSha256,
    sourcePhrase: vocabulary.sourcePhrase.trim(),
    preferredReplacement: vocabulary.preferredReplacement.trim(),
  };
  await runBusy("vocabulary-save", async () => {
    const response = vocabulary.editingId
      ? await invoke("local_vocabulary_edit", { ...payload, id: vocabulary.editingId })
      : await invoke("local_vocabulary_add", payload);
    if (state.vocabulary?.meetingId !== vocabulary.meetingId) return;
    state.vocabulary = { ...state.vocabulary, entries: response.entries || [], loading: false };
    resetVocabularyDraft();
    state.notice = "Local vocabulary saved. Future note regenerations will use it.";
  });
}

function editVocabulary(id) {
  const entry = state.vocabulary?.entries?.find((candidate) => candidate.id === id);
  if (!entry || !state.vocabulary) return;
  state.vocabulary = {
    ...state.vocabulary,
    editingId: id,
    pendingDeleteId: "",
    sourcePhrase: entry.sourcePhrase,
    preferredReplacement: entry.preferredReplacement,
  };
  render();
  queueMicrotask(() => root.querySelector("#vocabulary-before-input")?.focus());
}

async function setVocabularyEnabled(id, enabled) {
  const vocabulary = state.vocabulary;
  if (!vocabulary) return;
  await runBusy("vocabulary-save", async () => {
    const response = await invoke("local_vocabulary_set_enabled", {
      meetingId: vocabulary.meetingId,
      sourceTranscriptSha256: vocabulary.sourceTranscriptSha256,
      id,
      enabled,
    });
    if (state.vocabulary?.meetingId === vocabulary.meetingId) {
      state.vocabulary = { ...state.vocabulary, entries: response.entries || [], loading: false };
    }
  });
}

async function deleteVocabulary(id) {
  const vocabulary = state.vocabulary;
  if (!vocabulary) return;
  await runBusy("vocabulary-save", async () => {
    const response = await invoke("local_vocabulary_delete", {
      meetingId: vocabulary.meetingId,
      sourceTranscriptSha256: vocabulary.sourceTranscriptSha256,
      id,
    });
    if (state.vocabulary?.meetingId === vocabulary.meetingId) {
      state.vocabulary = { ...state.vocabulary, entries: response.entries || [], loading: false, pendingDeleteId: "" };
      if (state.vocabulary.editingId === id) resetVocabularyDraft();
    }
  });
}

function renderMeetingDeletionSheet() {
  const selection = state.selected;
  if (!selection) return "";
  const deleteRecording = state.modal === "delete-recording";
  const deleteTranscript = state.modal === "delete-transcript";
  const action = deleteRecording
    ? "confirm-delete-recording"
    : deleteTranscript
      ? "confirm-delete-transcript"
      : "confirm-delete-meeting";
  const busy = state.busyAction === action;
  const title = selection.row.label || `Meeting · ${dateLabel(selection.row.createdAtEpochSeconds)}`;
  const copy = meetingDeletionConfirmationCopy(state.modal);
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet destructive-sheet" role="dialog" aria-modal="true" aria-labelledby="delete-meeting-title">
        <div class="sheet-head">
          <div><p class="eyebrow">${escapeHtml(copy.eyebrow)}</p><h2 id="delete-meeting-title">${escapeHtml(copy.heading)}</h2><p>${escapeHtml(copy.detail)}</p></div>
          <button class="icon-button" type="button" data-action="close-modal" aria-label="Close">×</button>
        </div>
        <p class="destructive-target">${escapeHtml(title)}</p>
        <div class="sheet-actions">
          <button class="button button-quiet" type="button" data-action="close-modal">Cancel</button>
          <button class="button button-danger" type="button" data-action="${action}" ${busy ? "disabled" : ""}>${busy ? "Deleting…" : escapeHtml(copy.label)}</button>
        </div>
      </section>
    </div>
  `;
}

// Roadmap intake I5's confirmation sheet -- decision 5, the packet's soul.
//
// The detail sentence is the honest claim, and `meetingLockCopyIsHonest` in
// the view model is what keeps it honest as this file changes: "not
// encryption" stays, and no word from the forbidden list arrives.
//
// On a Mac that cannot run the device-owner check at all, the lock sheet says
// so and offers no Lock button. Locking there would produce a meeting this app
// could never reopen -- decision 4 covers the machine that becomes unable, and
// this covers the one that never could.
function renderMeetingLockSheet() {
  const selection = state.selected;
  if (!selection) return "";
  const unlock = state.modal === "unlock-meeting";
  const action = unlock ? "confirm-unlock-meeting" : "confirm-lock-meeting";
  const busy = state.busyAction === action;
  const title = selection.row.label || `Meeting · ${dateLabel(selection.row.createdAtEpochSeconds)}`;
  const copy = meetingLockSheetCopy(unlock ? "unlock" : "lock", {
    canConfirm: selection.note?.canConfirmOperator !== false,
  });
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet meeting-lock-sheet" role="dialog" aria-modal="true" aria-labelledby="meeting-lock-sheet-title">
        <div class="sheet-head">
          <div><p class="eyebrow">${escapeHtml(copy.eyebrow)}</p><h2 id="meeting-lock-sheet-title">${escapeHtml(copy.heading)}</h2><p>${escapeHtml(copy.detail)}</p></div>
          <button class="icon-button" type="button" data-action="close-modal" aria-label="Close">×</button>
        </div>
        <p class="destructive-target">${escapeHtml(title)}</p>
        <div class="sheet-actions">
          <button class="button button-quiet" type="button" data-action="close-modal">Cancel</button>
          ${copy.blocked ? "" : `<button class="button button-primary" type="button" data-action="${action}" ${busy ? "disabled" : ""}>${busy ? "Confirming…" : escapeHtml(copy.label)}</button>`}
        </div>
      </section>
    </div>
  `;
}

function attestation(name, label) {
  return `<label class="check-row"><input type="checkbox" data-field="attestation" data-attestation="${name}" ${state.consent[name] ? "checked" : ""} /><span>${escapeHtml(label)}</span></label>`;
}

function setNoteSaveCopy() {
  const target = document.querySelector("#note-save-state");
  if (target) target.textContent = noteSaveCopy();
}

function setContextSaveCopy() {
  const target = document.querySelector("#context-save-state");
  if (target) target.textContent = contextSaveCopy();
}

function setLibraryNoteSaveCopy() {
  const selection = state.selected;
  const target = document.querySelector("#library-note-save-state");
  if (!selection || !target) return;
  const unreadable = selection.note?.operatorNote?.unreadable;
  target.textContent = unreadable
    ? "Not editable"
    : selection.operatorNoteSaveState === "saving"
      ? "Saving…"
      : selection.operatorNoteSaveState === "saved"
        ? "Saved on this Mac"
        : "Stored on this Mac";
}

function reportError(error) {
  state.error = errorRecoveryPresentation(error, {
    hasSelectedMeeting: Boolean(state.selected?.row?.meetingId),
  });
  render();
}

function clearCurrentNote() {
  clearTimeout(noteSaveTimer);
  noteSaveTimer = undefined;
  state.noteDraft = "";
  state.noteLoadedFor = "";
  state.noteSaveState = "local";
  state.noteUnreadable = false;
  state.transcriptActionStatus = { ...state.transcriptActionStatus, current: "" };
}

function clearCurrentContext() {
  clearTimeout(contextSaveTimer);
  contextSaveTimer = undefined;
  state.contextDraft = "";
  state.contextLoadedFor = "";
  state.contextSaveState = "local";
  state.contextUnreadable = false;
}

function canReadCurrentNote(snapshot) {
  return ["recording", "stopping", "captured", "transcribing", "transcript-ready", "transcription-failed", "recovered-interrupted"].includes(snapshot?.capture);
}

async function refreshSnapshot({ shouldRender = true } = {}) {
  const oldMeetingId = state.snapshot?.meeting_id;
  state.snapshot = await invoke("app_snapshot");
  const meetingId = state.snapshot.meeting_id;
  if (!meetingId && oldMeetingId) { clearCurrentNote(); clearCurrentContext(); }
  if (meetingId && meetingId !== state.noteLoadedFor && canReadCurrentNote(state.snapshot)) void loadCurrentNote(meetingId);
  if (meetingId && meetingId !== state.contextLoadedFor && canReadCurrentNote(state.snapshot)) void loadCurrentContext(meetingId);
  if (state.snapshot.capture === "idle" && state.activeView === "capture") state.activeView = "home";
  if (shouldRender) render();
}

async function loadCurrentNote(meetingId) {
  if (state.noteLoading || state.noteLoadedFor === meetingId || state.noteDraft) return;
  state.noteLoading = true;
  try {
    const note = await invoke("operator_note");
    if (state.snapshot?.meeting_id !== meetingId || state.noteDraft) return;
    state.noteLoadedFor = meetingId;
    state.noteDraft = note.text || "";
    state.noteUnreadable = note.unreadable === true;
    state.noteSaveState = note.unreadable ? "unreadable" : note.text ? "saved" : "local";
    render();
  } catch {
    // The meeting directory can briefly be unavailable while capture arms.
  } finally {
    state.noteLoading = false;
  }
}

async function loadCurrentContext(meetingId) {
  if (state.contextLoading || state.contextLoadedFor === meetingId || state.contextDraft) return;
  state.contextLoading = true;
  try {
    const context = await invoke("meeting_context");
    if (state.snapshot?.meeting_id !== meetingId || state.contextDraft) return;
    state.contextLoadedFor = meetingId;
    state.contextDraft = context.text || "";
    state.contextUnreadable = context.unreadable === true;
    state.contextSaveState = context.unreadable ? "unreadable" : context.text ? "saved" : "local";
    render();
  } catch {
    // The meeting directory can briefly be unavailable while capture arms.
  } finally {
    state.contextLoading = false;
  }
}

async function refreshLibrary() {
  const title = state.search.trim();
  state.library = await invoke("library_snapshot", { filter: title ? { title } : null });
  await refreshTrash();
}

async function refreshTrash() {
  const response = await invoke("preview_list_trash");
  state.trash = { entries: response.entries || [] };
}

async function refreshPermissions() {
  if (permissionsRefreshTask) return permissionsRefreshTask;
  permissionsRefreshTask = (async () => {
    state.permissions = mergePermissions(state.permissions, await invoke("first_run_permissions"));
  })();
  try {
    await permissionsRefreshTask;
  } finally {
    permissionsRefreshTask = undefined;
  }
}

function refreshPermissionsOnReturn() {
  if (!invoke) return;
  void refreshPermissions()
    .then(render)
    .catch(reportError);
}

async function runBusy(action, operation) {
  state.busyAction = action;
  render();
  try {
    await operation();
  } catch (error) {
    reportError(error);
  } finally {
    state.busyAction = "";
    render();
  }
}

async function requestPermission(kind) {
  const command = kind === "microphone" ? "first_run_request_microphone" : "first_run_request_system_audio";
  await runBusy("permission", async () => {
    state.permissions = mergePermissions(state.permissions, await invoke(command));
  });
}

function openStart() {
  if (!canOpenStart(state.snapshot, state.permissions)) return;
  state.modal = "start";
  render();
}

async function startRecording() {
  if (!canOpenStart(state.snapshot, state.permissions) || !Object.values(state.consent).every(Boolean)) return;
  await runBusy("start", async () => {
    state.snapshot = await invoke("start_meeting", {
      retentionDays: Number(state.retentionDays),
      attestation: state.consent,
    });
    clearCurrentNote();
    clearCurrentContext();
    state.activeView = "capture";
    state.modal = "";
    state.selected = null;
  });
}

async function pauseRecording() {
  if (state.snapshot?.capture !== "recording") return;
  await runBusy("pause", async () => {
    state.snapshot = await invoke("pause_meeting");
  });
}

async function resumeRecording() {
  if (state.snapshot?.capture !== "paused") return;
  await runBusy("resume", async () => {
    state.snapshot = await invoke("resume_meeting");
  });
}

async function stopRecording() {
  if (!["recording", "paused"].includes(state.snapshot?.capture)) return;
  await runBusy("stop", async () => {
    state.snapshot = await invoke("stop_meeting");
    state.activeView = "capture";
  });
}

async function dismissCurrent() {
  await leaveCurrentCapture();
}

async function recordAnother() {
  await leaveCurrentCapture({ startAnother: true });
}

async function leaveCurrentCapture({ startAnother = false } = {}) {
  await runBusy(startAnother ? "record-another" : "dismiss", async () => {
    await flushPendingNoteSave();
    await flushPendingContextSave();
    state.snapshot = await invoke("dismiss_meeting");
    clearCurrentNote();
    clearCurrentContext();
    state.activeView = "home";
    state.selected = null;
    await refreshLibrary();
    if (startAnother && canOpenStart(state.snapshot, state.permissions)) state.modal = "start";
  });
}

// Roadmap intake I5. A locked row asks for the device-owner check before the
// meeting is opened, so nothing about it is read until the check passes. The
// Rust side would refuse the open anyway; asking first is what makes the
// refusal a door rather than an error.
async function confirmLockedAction(handle, action) {
  const response = await invoke("authorize_locked_action", { handle, action });
  const outcome = lockedActionOutcome(response);
  if (outcome.state === "not-locked") return { proceed: true, token: null };
  if (!outcome.ok) {
    state.notice = outcome.message;
    return { proceed: false, token: null };
  }
  return { proceed: true, token: response.token };
}

async function openMeeting(handle) {
  const row = state.library?.rows?.find((candidate) => candidate.handle === handle);
  if (!row) return;
  await flushSelectedNoteSave();
  await runBusy("meeting", async () => {
    let lockToken = null;
    if (row.locked) {
      const confirmed = await confirmLockedAction(handle, "open");
      if (!confirmed.proceed) {
        // Still show the meeting, so the reader lands on the barrier and its
        // sentence rather than on a list with a toast they may have missed.
        await loadSelectedMeeting(row, null);
        return;
      }
      lockToken = confirmed.token;
    }
    await loadSelectedMeeting(row, lockToken);
  });
}

// `lockToken` is roadmap intake I5's reading authority: a single-use
// confirmation for this exact meeting. `library_open_note` spends it and, when
// the read succeeds on a still-locked meeting, hands back a fresh one on
// `note.lockToken`. That is what `reopenSelectedMeeting` carries forward, so a
// refresh inside an open locked meeting does not ask again.
async function loadSelectedMeeting(row, lockToken = null) {
  state.transcriptActionStatus = { ...state.transcriptActionStatus, library: "" };
  const note = await invoke("library_open_note", { handle: row.handle, lockToken });
  const transcript = note.transcriptHandle
    ? await invoke("library_open_transcript", { handle: note.transcriptHandle })
    : null;
  const pendingRetry = await loadPendingTranscriptRetry(note, transcript);
  const operatorNote = note.operatorNote || { text: "", unreadable: false };
  state.meetingManagementOpen = false;
  state.transcriptQuery = "";
  state.selected = {
    row,
    note,
    transcript,
    transcriptRetry: pendingRetry,
    claimEvidence: {},
    operatorNoteDraft: operatorNote.text || "",
    operatorNoteSaveQueue: Promise.resolve(),
    operatorNoteSaveState: operatorNote.unreadable ? "unreadable" : operatorNote.text ? "saved" : "local",
  };
  state.audioPlayback = { state: "idle", source: null, message: "No recording is playing." };
  state.activeView = "meeting";
}

async function playRetainedAudio(source) {
  const note = state.selected?.note;
  const handle = source === "microphone"
    ? note?.microphonePlaybackHandle
    : source === "system"
      ? note?.systemPlaybackHandle
      : "";
  if (!handle) return;
  await runBusy(`play-${source}`, async () => {
    // Roadmap intake I5 (c): a locked meeting asks again for playback, even
    // though a confirmation is already holding it open to read. Standard
    // Notes' action-scoped re-auth -- one check, one action.
    let lockToken = null;
    if (note?.lock?.locked) {
      const confirmed = await confirmLockedAction(state.selected?.row?.handle, "playback");
      if (!confirmed.proceed) return;
      lockToken = confirmed.token;
    }
    const response = await invoke("library_play_retained_audio", { handle, lockToken });
    state.audioPlayback = response;
    if (response.state !== "playing") throw new Error(response.message || "Retained audio is unavailable. Reopen Library and try again.");
  });
}

async function stopRetainedAudio() {
  const meetingId = state.selected?.row?.meetingId;
  await runBusy("stop-retained-audio", async () => {
    const response = await invoke("library_stop_retained_audio");
    state.notice = response.message || "Playback stopped.";
    if (meetingId) await reopenSelectedMeeting(meetingId);
    else state.audioPlayback = response;
  });
}

async function refreshRetainedAudioPlayback() {
  if (!state.selected || state.audioPlayback?.state !== "playing" || audioPlaybackPollActive) return;
  audioPlaybackPollActive = true;
  try {
    const response = await invoke("library_retained_audio_playback_status");
    if (response.state === "completed") {
      const meetingId = state.selected?.row?.meetingId;
      state.notice = response.message || "The recording finished.";
      if (meetingId) await reopenSelectedMeeting(meetingId);
      else state.audioPlayback = response;
    } else {
      state.audioPlayback = response;
    }
    render();
  } catch (error) {
    state.audioPlayback = { state: "unavailable", source: null, message: "Retained audio is unavailable. Reopen Library and try again." };
    reportError(error);
  } finally {
    audioPlaybackPollActive = false;
  }
}

async function loadPendingTranscriptRetry(note, transcript) {
  const retry = transcriptRetryContext(note, transcript);
  if (!retry) return null;
  try {
    const pending = await invoke("transcript_retry_pending", {
      meetingId: retry.meetingId,
      sourceTranscriptSha256: retry.sourceTranscriptSha256,
    });
    return transcriptRetryPresentation({
      meetingId: retry.meetingId,
      transcriptMeetingId: transcript?.meetingId,
      sourceTranscriptSha256: retry.sourceTranscriptSha256,
      audioRetentionState: note?.audioRetention?.state,
      capture: state.snapshot?.capture,
      recovery: meetingRecoveryPresentation(note, transcript, state.generatingMeetingId),
      pending,
    })?.pending || null;
  } catch {
    // A pending candidate is a convenience. A failed check must not make the
    // retained meeting unreadable or claim that a retry exists.
    return null;
  }
}

async function openTranscriptRetry() {
  const selection = state.selected;
  const retry = transcriptRetryContext(selection?.note, selection?.transcript, selection?.transcriptRetry);
  if (!selection || !retry) return;
  if (retry.pending) {
    state.transcriptRetry = { ...retry.pending, phase: "ready" };
    state.modal = "transcript-retry";
    render();
    queueMicrotask(() => root.querySelector("[data-action='decide-retry-later']")?.focus());
    return;
  }
  state.transcriptRetry = { ...retry, phase: "starting" };
  render();
  try {
    const comparison = await invoke("transcript_retry_start", {
      meetingId: retry.meetingId,
      sourceTranscriptSha256: retry.sourceTranscriptSha256,
    });
    if (state.selected !== selection) return;
    selection.transcriptRetry = comparison;
    state.transcriptRetry = { ...comparison, phase: "ready" };
    state.modal = "transcript-retry";
    render();
    queueMicrotask(() => root.querySelector("[data-action='decide-retry-later']")?.focus());
  } catch (error) {
    if (state.selected === selection) state.transcriptRetry = null;
    reportError(error);
  }
}

async function decideTranscriptRetry(decision) {
  const selection = state.selected;
  const retry = state.transcriptRetry;
  if (!selection?.row?.meetingId || !retry?.operationId || !["keep-current", "use-retry"].includes(decision)) return;
  await flushSelectedNoteSave();
  await runBusy("decide-transcript-retry", async () => {
    const response = await invoke("transcript_retry_decide", {
      meetingId: retry.meetingId,
      operationId: retry.operationId,
      sourceTranscriptSha256: retry.sourceTranscriptSha256,
      candidateTranscriptSha256: retry.candidateTranscriptSha256,
      decision,
    });
    if (!response?.outcome) {
      throw new Error(response.message || "Yawn could not save that transcript decision.");
    }
    if (state.selected !== selection) return;
    state.modal = "";
    state.transcriptRetry = null;
    state.notice = response.message || (decision === "use-retry"
      ? "The retry is now the selected transcript. Regenerate the meeting note when you are ready."
      : "The retained transcript remains selected.");
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

async function reopenSelectedMeeting(meetingId) {
  // Held before `refreshLibrary` replaces `state.selected`'s row: the re-issued
  // reading confirmation belongs to the meeting being reopened, and losing it
  // here would drop the reader back at the barrier after an ordinary refresh.
  const carried = state.selected?.note?.meetingId === meetingId
    ? state.selected?.note?.lockToken || null
    : null;
  await refreshLibrary();
  const row = state.library?.rows?.find((candidate) => candidate.meetingId === meetingId);
  if (!row) {
    state.selected = null;
    state.activeView = "home";
    return;
  }
  await loadSelectedMeeting(row, carried);
}

async function generateSelectedNote() {
  const note = state.selected?.note;
  const control = noteGenerationPresentation(note, state.generatingMeetingId);
  if (!control || control.disabled || state.generatingMeetingId) return;
  const meetingId = note.meetingId;
  state.generatingMeetingId = meetingId;
  render();
  try {
    await invoke("regenerate_note", {
      meetingId,
      sourceTranscriptSha256: note.regenerationSourceSha256,
    });
  } catch (error) {
    reportError(error);
  } finally {
    state.generatingMeetingId = "";
    try {
      if (state.activeView === "meeting" && state.selected?.note?.meetingId === meetingId) {
        await reopenSelectedMeeting(meetingId);
      } else {
        await refreshLibrary();
      }
    } catch {
      // The refreshed view is a convenience; the durable receipt is not.
    }
    render();
  }
}

async function restoreSelectedWithheldTurn(sourceTurnIndex) {
  const selection = state.selected;
  const transcript = selection?.transcript;
  const action = withheldTurnPresentation(
    transcript?.turns?.find((turn) => Number(turn.sourceTurnIndex) === sourceTurnIndex),
    {
      meetingId: selection?.row?.meetingId,
      meetingHandle: selection?.row?.handle,
      transcriptMeetingId: transcript?.meetingId,
      transcriptSha256: transcript?.currentTranscriptSha256,
      capture: state.snapshot?.capture,
    },
  );
  if (!action || !selection?.row?.meetingId || !transcript?.currentTranscriptSha256) return;
  const meetingId = selection.row.meetingId;
  await runBusy("restore-withheld-turn", async () => {
    await invoke("restore_withheld_turn", {
      meetingId,
      sourceTranscriptSha256: transcript.currentTranscriptSha256,
      sourceTurnIndex: action.sourceTurnIndex,
    });
    await reopenSelectedMeeting(meetingId);
  });
}

function openMeetingRename() {
  const selection = state.selected;
  if (!selection) return;
  state.meetingManagementOpen = false;
  state.renameDraft = selection.row.labelSource === "operator" ? selection.row.label || "" : "";
  state.modal = "rename-meeting";
  render();
  queueMicrotask(() => root.querySelector("#meeting-title-input")?.focus());
}

async function saveMeetingTitle() {
  const selection = state.selected;
  const metadataRevision = state.library?.metadataRevision;
  const revision = Number(metadataRevision);
  if (!selection?.row?.meetingId || metadataRevision == null || !Number.isInteger(revision)) {
    reportError(new Error("Reopen Meetings before changing this name."));
    return;
  }
  await flushSelectedNoteSave();
  await runBusy("rename-meeting", async () => {
    const title = state.renameDraft.trim();
    const response = await invoke("library_set_meeting_title", {
      expectedRevision: revision,
      meetingId: selection.row.meetingId,
      title: title || null,
    });
    if (response.state !== "ok") throw new Error(response.message || "Yawn could not change this meeting name.");
    if (state.selected !== selection) return;
    state.modal = "";
    state.notice = title ? "Meeting name saved on this Mac." : "Meeting name reset to its opening line.";
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

function openMeetingDeletion(kind) {
  const selection = state.selected;
  if (!selection) return;
  const allowed = kind === "delete-recording"
    ? Boolean(selection.note?.audioDeletionHandle)
    : kind === "delete-transcript"
      ? Boolean(selection.note?.transcriptDeletionHandle)
      : Boolean(selection.note?.meetingDeletionHandle);
  if (!allowed) return;
  state.meetingManagementOpen = false;
  state.modal = kind;
  render();
}

async function deleteSelectedRecording() {
  const selection = state.selected;
  const handle = selection?.note?.audioDeletionHandle;
  if (!selection?.row?.meetingId || !handle) {
    reportError(new Error("Reopen this meeting before deleting its recording."));
    return;
  }
  await flushSelectedNoteSave();
  await runBusy("confirm-delete-recording", async () => {
    const response = await invoke("preview_delete_meeting_audio", { handle });
    if (!["released", "already-released"].includes(response.state)) {
      throw new Error(response.message || "Yawn could not delete this recording.");
    }
    if (state.selected !== selection) return;
    state.modal = "";
    state.notice = response.message || "The recording was permanently deleted from this Mac.";
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

async function deleteSelectedTranscript() {
  const selection = state.selected;
  const handle = selection?.note?.transcriptDeletionHandle;
  if (!selection?.row?.meetingId || !handle) {
    reportError(new Error("Reopen this meeting before deleting its transcript."));
    return;
  }
  await flushSelectedNoteSave();
  await runBusy("confirm-delete-transcript", async () => {
    const response = await invoke("preview_delete_meeting_transcript", { handle, confirmed: true });
    if (!["removed", "already-removed"].includes(response.state)) {
      throw new Error(response.message || "Yawn could not delete this transcript.");
    }
    if (state.selected !== selection) return;
    state.modal = "";
    state.notice = response.message || "The transcript and generated notes were permanently deleted from this Mac.";
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

async function deleteSelectedMeeting() {
  const selection = state.selected;
  const handle = selection?.note?.meetingDeletionHandle;
  if (!handle) {
    reportError(new Error("Reopen this meeting before deleting it."));
    return;
  }
  await flushSelectedNoteSave();
  await runBusy("confirm-delete-meeting", async () => {
    const response = await invoke("preview_delete_meeting", { handle, confirmed: true });
    if (!["trashed", "already-trashed"].includes(response.state)) {
      throw new Error(response.message || "Yawn could not delete this meeting.");
    }
    if (state.selected !== selection) return;
    state.selected = null;
    state.activeView = "home";
    state.modal = "";
    state.notice = response.message
      || "The meeting moved to Trash. It stays recoverable there for 30 days, then Yawn removes it permanently.";
    await refreshLibrary();
  });
}

async function openClaimEvidence(ordinal) {
  const selection = state.selected;
  if (!selection?.row?.meetingId || !Number.isFinite(ordinal)) return;
  await flushSelectedNoteSave();
  await runBusy(`evidence-${ordinal}`, async () => {
    // Claim handles are deliberately one-use capabilities. A fresh local
    // meeting handle keeps every source request scoped to the meeting the
    // operator already opened instead of keeping broad transcript access alive
    // in UI state.
    const library = await invoke("library_snapshot", { filter: null });
    const row = library.rows?.find((candidate) => candidate.meetingId === selection.row.meetingId);
    if (!row?.handle) {
      throw new Error("Yawn could not reopen this retained meeting.");
    }
    const note = await invoke("library_open_note", { handle: row.handle });
    const claim = note.claims?.find((candidate) => Number(candidate.ordinal) === ordinal);
    if (!claim?.handle || !claim.locatorCount) {
      throw new Error("Yawn could not open a retained source passage for this point.");
    }
    const evidence = await invoke("preview_library_open_evidence", {
      handle: claim.handle,
      locatorOrdinal: 0,
    });
    if (evidence.state !== "evidence" || !evidence.text) {
      throw new Error(evidence.message || "Yawn could not open that source passage.");
    }
    if (state.selected?.row?.handle !== selection.row.handle) return;
    state.selected = {
      ...state.selected,
      note,
      transcript: state.selected.transcript && evidence.transcriptHandle
        ? { ...state.selected.transcript, transcriptFileHandle: evidence.transcriptHandle }
        : state.selected.transcript,
      claimEvidence: {
        ...(state.selected.claimEvidence || {}),
        [ordinal]: evidence,
      },
    };
  });
}

// Roadmap intake I4 / design D4's reverse half: the mirror of "Show source"
// (claim -> transcript turn). Every claim is already rendered on this same
// page, so this is pure DOM navigation -- no native round trip, no new
// command. A cited claim can sit inside a closed <details> ("Additional
// transcript highlights"), so every ancestor gets opened before scrolling.
function navigateToClaim(ordinal) {
  if (!Number.isFinite(ordinal)) return;
  const target = root.querySelector(`[data-claim-item="${ordinal}"]`);
  if (!target) return;
  for (let ancestor = target.closest("details"); ancestor; ancestor = ancestor.parentElement?.closest("details")) {
    ancestor.open = true;
  }
  target.scrollIntoView({ behavior: "smooth", block: "center" });
  target.classList.add("claim-item-navigated");
  window.setTimeout(() => target.classList.remove("claim-item-navigated"), 1600);
}

function queueNoteSave(text = state.noteDraft, meetingId = state.snapshot?.meeting_id) {
  if (!meetingId || state.noteUnreadable) return Promise.resolve();
  state.noteSaveQueue = state.noteSaveQueue.catch(() => undefined).then(async () => {
    if (state.snapshot?.meeting_id !== meetingId) return;
    state.noteSaveState = "saving";
    setNoteSaveCopy();
    const saved = await invoke("save_operator_note", { text });
    if (state.snapshot?.meeting_id === meetingId && state.noteDraft === text) {
      state.noteUnreadable = saved.unreadable === true;
      state.noteSaveState = saved.unreadable ? "unreadable" : "saved";
      setNoteSaveCopy();
    }
  }).catch((error) => {
    state.noteSaveState = "local";
    state.error = errorRecoveryPresentation(error || "Yawn could not save this note.", {
      hasSelectedMeeting: Boolean(state.selected?.row?.meetingId),
    });
    render();
  });
  return state.noteSaveQueue;
}

function scheduleNoteSave() {
  clearTimeout(noteSaveTimer);
  noteSaveTimer = setTimeout(() => {
    noteSaveTimer = undefined;
    void queueNoteSave();
  }, 600);
}

function queueContextSave(text = state.contextDraft, meetingId = state.snapshot?.meeting_id) {
  if (!meetingId || state.contextUnreadable) return Promise.resolve();
  state.contextSaveQueue = state.contextSaveQueue.catch(() => undefined).then(async () => {
    if (state.snapshot?.meeting_id !== meetingId) return;
    state.contextSaveState = "saving";
    setContextSaveCopy();
    const saved = await invoke("save_meeting_context", { text });
    if (state.snapshot?.meeting_id === meetingId && state.contextDraft === text) {
      state.contextUnreadable = saved.unreadable === true;
      state.contextSaveState = saved.unreadable ? "unreadable" : "saved";
      setContextSaveCopy();
    }
  }).catch((error) => {
    state.contextSaveState = "local";
    state.error = errorRecoveryPresentation(error || "Yawn could not save this context.", {
      hasSelectedMeeting: Boolean(state.selected?.row?.meetingId),
    });
    render();
  });
  return state.contextSaveQueue;
}

function scheduleContextSave() {
  clearTimeout(contextSaveTimer);
  contextSaveTimer = setTimeout(() => {
    contextSaveTimer = undefined;
    void queueContextSave();
  }, 600);
}

function queueSelectedNoteSave(text = state.selected?.operatorNoteDraft, selection = state.selected) {
  if (!selection?.row?.meetingId || selection.note?.operatorNote?.unreadable) return Promise.resolve();
  selection.operatorNoteSaveQueue = (selection.operatorNoteSaveQueue || Promise.resolve())
    .catch(() => undefined)
    .then(async () => {
      if (state.selected !== selection) return;
      const handle = selection.note?.operatorNoteHandle;
      if (!handle) throw new Error("Reopen this meeting before saving its notes.");
      selection.operatorNoteSaveState = "saving";
      setLibraryNoteSaveCopy();
      const saved = await invoke("library_save_operator_note", { handle, text });
      if (state.selected !== selection) return;
      selection.note = {
        ...selection.note,
        operatorNote: saved.operatorNote,
        operatorNoteHandle: saved.operatorNoteHandle,
      };
      selection.operatorNoteSaveState = selection.operatorNoteDraft === text ? "saved" : "local";
      setLibraryNoteSaveCopy();
    })
    .catch((error) => {
      if (state.selected !== selection) return;
      selection.operatorNoteSaveState = "local";
      state.error = errorRecoveryPresentation(error || "Yawn could not save this note.", {
        hasSelectedMeeting: Boolean(state.selected?.row?.meetingId),
      });
      render();
    });
  return selection.operatorNoteSaveQueue;
}

function scheduleSelectedNoteSave() {
  clearTimeout(libraryNoteSaveTimer);
  libraryNoteSaveTimer = setTimeout(() => {
    libraryNoteSaveTimer = undefined;
    void queueSelectedNoteSave();
  }, 600);
}

async function flushPendingNoteSave() {
  if (noteSaveTimer !== undefined) {
    clearTimeout(noteSaveTimer);
    noteSaveTimer = undefined;
    await queueNoteSave();
  }
  await state.noteSaveQueue;
}

async function flushPendingContextSave() {
  if (contextSaveTimer !== undefined) {
    clearTimeout(contextSaveTimer);
    contextSaveTimer = undefined;
    await queueContextSave();
  }
  await state.contextSaveQueue;
}

async function flushSelectedNoteSave() {
  const selection = state.selected;
  if (!selection) return;
  if (libraryNoteSaveTimer !== undefined) {
    clearTimeout(libraryNoteSaveTimer);
    libraryNoteSaveTimer = undefined;
    await queueSelectedNoteSave(selection.operatorNoteDraft, selection);
  }
  await selection.operatorNoteSaveQueue;
}

async function openMeetings() {
  await flushSelectedNoteSave();
  if (invoke && state.audioPlayback?.state === "playing") {
    state.audioPlayback = await invoke("library_stop_retained_audio");
  }
  state.selected = null;
  state.meetingManagementOpen = false;
  state.transcriptQuery = "";
  state.activeView = "home";
  state.trashOpen = false;
  if (!invoke) {
    render();
    return;
  }
  await runBusy("library", refreshLibrary);
}

function openTrash() {
  state.selected = null;
  state.trashOpen = true;
  render();
  if (!invoke) return;
  void runBusy("open-trash", refreshTrash);
}

function closeTrash() {
  state.trashOpen = false;
  render();
}

async function restoreTrashEntry(meetingId) {
  if (!meetingId) return;
  await runBusy(`restore-${meetingId}`, async () => {
    const response = await invoke("restore_meeting_from_trash_command", { meetingId });
    if (!["restored", "not-found"].includes(response.state)) {
      throw new Error(response.message || "Yawn could not restore this meeting.");
    }
    state.notice = response.message || "The meeting was restored from Trash.";
    await refreshTrash();
    await refreshLibraryQuietly();
    if (!state.trash?.entries?.length) state.trashOpen = false;
  });
}

// A restore already refreshed Trash; Meetings needs the same treatment so the
// restored meeting shows up there without a second visible busy state.
async function refreshLibraryQuietly() {
  const title = state.search.trim();
  state.library = await invoke("library_snapshot", { filter: title ? { title } : null });
}

async function refreshLibraryFromRecovery() {
  if (!invoke) return;
  await runBusy("refresh-library", refreshLibrary);
}

async function refreshSelectedMeetingFromRecovery() {
  const meetingId = state.selected?.row?.meetingId;
  if (!meetingId || !invoke) return;
  await runBusy("refresh-selected-meeting", () => reopenSelectedMeeting(meetingId));
}

function transcriptTurns(scope) {
  return scope === "library"
    ? state.selected?.transcript?.turns || []
    : state.snapshot?.turns || [];
}

async function writeTranscriptToClipboard(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const fallback = document.createElement("textarea");
  fallback.value = text;
  fallback.setAttribute("readonly", "");
  fallback.style.cssText = "position:fixed;opacity:0;pointer-events:none;";
  document.body.append(fallback);
  fallback.select();
  const copied = document.execCommand("copy");
  fallback.remove();
  if (!copied) throw new Error("Copy failed. Select the transcript instead.");
}

async function copyTranscript(scope) {
  const text = transcriptPlainText(transcriptTurns(scope));
  if (!text) {
    state.transcriptActionStatus = { ...(state.transcriptActionStatus || {}), [scope]: "Nothing to copy." };
    render();
    return;
  }
  state.transcriptActionStatus = { ...(state.transcriptActionStatus || {}), [scope]: "Copying…" };
  render();
  try {
    await writeTranscriptToClipboard(text);
    state.transcriptActionStatus = { ...(state.transcriptActionStatus || {}), [scope]: "Copied." };
  } catch {
    state.transcriptActionStatus = { ...(state.transcriptActionStatus || {}), [scope]: "Copy failed. Select the transcript instead." };
  }
  render();
}

async function openTranscriptFile(scope) {
  if (scope === "library") await flushSelectedNoteSave();
  await runBusy(`open-${scope}-transcript-file`, async () => {
    if (scope === "current") {
      await invoke("open_current_transcript_file");
      return;
    }
    const selection = state.selected;
    const handle = selection?.transcript?.transcriptFileHandle;
    if (!handle) throw new Error("Reopen this meeting before opening its transcript file.");
    // Same `export` scope as `library_export_meeting`: both put this meeting's
    // transcript in front of something outside Yawn.
    let lockToken = null;
    if (selection?.note?.lock?.locked) {
      const confirmed = await confirmLockedAction(selection.row.handle, "export");
      if (!confirmed.proceed) return;
      lockToken = confirmed.token;
    }
    const opened = await invoke("library_open_transcript_file", { handle, lockToken });
    if (state.selected !== selection || !selection.transcript) return;
    selection.transcript = {
      ...selection.transcript,
      transcriptFileHandle: opened.transcriptFileHandle,
    };
  });
}

async function exportSelectedMeeting() {
  await flushSelectedNoteSave();
  await runBusy("export-meeting", async () => {
    const selection = state.selected;
    const handle = selection?.transcript?.transcriptFileHandle;
    if (!handle) throw new Error("Reopen this meeting before exporting it.");
    // Roadmap intake I5 (c). Export puts this meeting's contents somewhere the
    // lock does not reach, so it asks for its own confirmation.
    let lockToken = null;
    if (selection?.note?.lock?.locked) {
      const confirmed = await confirmLockedAction(selection.row.handle, "export");
      if (!confirmed.proceed) return;
      lockToken = confirmed.token;
    }
    const exported = await invoke("library_export_meeting", { handle, lockToken });
    if (state.selected !== selection || !selection.transcript) return;
    selection.transcript = {
      ...selection.transcript,
      transcriptFileHandle: exported.transcriptFileHandle,
    };
    state.transcriptActionStatus = {
      ...(state.transcriptActionStatus || {}),
      library: exported.withheld.length
        ? `Exported. Not included: ${exported.withheld.length === 1 ? "1 item" : `${exported.withheld.length} items`} (see README.txt).`
        : "Exported. Opened the export folder on this Mac.",
    };
  });
}

// Roadmap intake I5. Locking asks nothing -- it only takes access away -- and
// the confirmation sheet is the whole ceremony. Afterwards the meeting is
// reopened, which now lands on the barrier: the reader sees immediately what
// they just did, and every capability the previous view held is gone with the
// reader the Rust side dropped.
async function lockSelectedMeeting() {
  const selection = state.selected;
  if (!selection) return;
  await flushSelectedNoteSave();
  await runBusy("confirm-lock-meeting", async () => {
    const response = await invoke("lock_meeting", { handle: selection.row.handle });
    const outcome = lockedActionOutcome(response);
    closeModal();
    state.notice = outcome.message;
    await reopenSelectedMeeting(selection.row.meetingId);
  });
}

// The only path that clears the flag, and the reason the device-owner check
// exists. A declined or unavailable check leaves the meeting locked and says
// which of the two happened -- they are different facts and lead to different
// next moves.
async function unlockSelectedMeeting() {
  const selection = state.selected;
  if (!selection) return;
  await runBusy("confirm-unlock-meeting", async () => {
    const response = await invoke("unlock_meeting", { handle: selection.row.handle });
    const outcome = lockedActionOutcome(response);
    closeModal();
    state.notice = outcome.message;
    if (outcome.ok) await reopenSelectedMeeting(selection.row.meetingId);
  });
}

// From the barrier screen: confirm once and read the meeting, without removing
// its lock. Exporting it or playing its audio still asks again.
async function confirmOpenLockedMeeting() {
  const selection = state.selected;
  if (!selection) return;
  await runBusy("unlock-meeting-open", async () => {
    const confirmed = await confirmLockedAction(selection.row.handle, "open");
    if (!confirmed.proceed) return;
    await loadSelectedMeeting(selection.row, confirmed.token);
  });
}

async function retryStartup() {
  await runBusy("retry", async () => {
    state.snapshot = await invoke("retry_startup");
    await refreshPermissions();
  });
}

async function installModel(modelId) {
  state.snapshot = await invoke("install_transcript_model", { modelId });
  render();
}

function handleClick(event) {
  const control = event.target.closest("[data-action]");
  if (!control || control.disabled) return;
  const action = control.dataset.action;
  if (action === "home" || action === "meetings") void openMeetings();
  else if (action === "open-start") openStart();
  else if (action === "close-start" || action === "close-modal") {
    closeModal();
    render();
  }
  else if (action === "start-recording") void startRecording();
  else if (action === "stop-recording") void stopRecording();
  else if (action === "pause-recording") void pauseRecording();
  else if (action === "resume-recording") void resumeRecording();
  else if (action === "dismiss-current") void dismissCurrent();
  else if (action === "record-another") void recordAnother();
  else if (action === "open-meeting") void openMeeting(control.dataset.handle);
  else if (action === "open-speaker-correction") openSpeakerCorrection(Number(control.dataset.sourceTurnIndex));
  else if (action === "open-vocabulary") void openVocabulary();
  else if (action === "start-transcript-retry") void openTranscriptRetry();
  else if (action === "decide-retry-later") {
    closeModal();
    render();
  }
  else if (action === "keep-current-transcript") void decideTranscriptRetry("keep-current");
  else if (action === "use-retry-transcript") void decideTranscriptRetry("use-retry");
  else if (action === "edit-vocabulary") editVocabulary(control.dataset.vocabularyId);
  else if (action === "cancel-vocabulary-edit") {
    resetVocabularyDraft();
    render();
    queueMicrotask(() => root.querySelector("#vocabulary-before-input")?.focus());
  }
  else if (action === "request-vocabulary-delete" && state.vocabulary) {
    state.vocabulary = { ...state.vocabulary, pendingDeleteId: control.dataset.vocabularyId };
    render();
  }
  else if (action === "cancel-vocabulary-delete" && state.vocabulary) {
    state.vocabulary = { ...state.vocabulary, pendingDeleteId: "" };
    render();
  }
  else if (action === "toggle-vocabulary") {
    const entry = state.vocabulary?.entries?.find((candidate) => candidate.id === control.dataset.vocabularyId);
    if (entry) void setVocabularyEnabled(entry.id, !entry.enabled);
  }
  else if (action === "confirm-vocabulary-delete") void deleteVocabulary(control.dataset.vocabularyId);
  else if (action === "restore-withheld-turn") void restoreSelectedWithheldTurn(Number(control.dataset.sourceTurnIndex));
  else if (action === "use-source-speaker" && state.speakerCorrection) {
    state.speakerCorrectionDraft = state.speakerCorrection.sourceLabel;
    render();
    queueMicrotask(() => root.querySelector("#speaker-name-input")?.focus());
  }
  else if (action === "open-claim-evidence") void openClaimEvidence(Number(control.dataset.ordinal));
  else if (action === "navigate-to-claim") navigateToClaim(Number(control.dataset.ordinal));
  else if (action === "toggle-meeting-management") {
    state.meetingManagementOpen = !state.meetingManagementOpen;
    render();
  }
  else if (action === "generate-note") void generateSelectedNote();
  else if (action === "play-retained-audio") void playRetainedAudio(control.dataset.source);
  else if (action === "stop-retained-audio") void stopRetainedAudio();
  else if (action === "rename-meeting") openMeetingRename();
  else if (action === "lock-meeting" || action === "unlock-meeting") {
    state.meetingManagementOpen = false;
    state.modal = action;
    render();
  }
  else if (action === "confirm-lock-meeting") void lockSelectedMeeting();
  else if (action === "confirm-unlock-meeting") void unlockSelectedMeeting();
  else if (action === "unlock-meeting-open") void confirmOpenLockedMeeting();
  else if (action === "delete-recording") openMeetingDeletion("delete-recording");
  else if (action === "delete-transcript") openMeetingDeletion("delete-transcript");
  else if (action === "delete-meeting") openMeetingDeletion("delete-meeting");
  else if (action === "open-trash") openTrash();
  else if (action === "close-trash") closeTrash();
  else if (action === "restore-trash-entry") void restoreTrashEntry(control.dataset.meetingId);
  else if (action === "confirm-delete-recording") void deleteSelectedRecording();
  else if (action === "confirm-delete-transcript") void deleteSelectedTranscript();
  else if (action === "confirm-delete-meeting") void deleteSelectedMeeting();
  else if (action === "copy-current-transcript") void copyTranscript("current");
  else if (action === "copy-library-transcript") void copyTranscript("library");
  else if (action === "open-current-transcript-file") void openTranscriptFile("current");
  else if (action === "open-library-transcript-file") void openTranscriptFile("library");
  else if (action === "export-meeting") void exportSelectedMeeting();
  else if (action === "clear-transcript-search") {
    state.transcriptQuery = "";
    render();
    queueMicrotask(() => root.querySelector("[data-field='transcript-search']")?.focus());
  }
  else if (action === "retry-startup") void retryStartup();
  else if (action === "refresh-library") void refreshLibraryFromRecovery();
  else if (action === "refresh-selected-meeting") void refreshSelectedMeetingFromRecovery();
  else if (action === "install-model") void installModel(control.dataset.modelId).catch(reportError);
  else if (action === "request-microphone") void requestPermission("microphone");
  else if (action === "request-system-audio") void requestPermission("system-audio");
  else if (action === "settings" || action === "open-settings") {
    if (invoke) void invoke("open_settings_window").catch(reportError);
  }
  else if (action === "clear-error") { state.error = ""; render(); }
  else if (action === "clear-notice") { state.notice = ""; render(); }
}

function handleInput(event) {
  if (event.target.dataset.field === "library-search") {
    state.search = event.target.value;
    clearTimeout(state.searchTimer);
    state.searchTimer = setTimeout(() => void runBusy("search", refreshLibrary), 220);
  }
  if (event.target.dataset.field === "operator-note") {
    state.noteDraft = event.target.value;
    state.noteSaveState = "local";
    setNoteSaveCopy();
    scheduleNoteSave();
  }
  if (event.target.dataset.field === "meeting-context") {
    state.contextDraft = event.target.value;
    state.contextSaveState = "local";
    setContextSaveCopy();
    scheduleContextSave();
  }
  if (event.target.dataset.field === "library-operator-note") {
    if (!state.selected) return;
    state.selected.operatorNoteDraft = event.target.value;
    state.selected.operatorNoteSaveState = "local";
    setLibraryNoteSaveCopy();
    scheduleSelectedNoteSave();
  }
  if (event.target.dataset.field === "transcript-search") {
    state.transcriptQuery = event.target.value;
    render();
  }
  if (event.target.dataset.field === "meeting-title") {
    state.renameDraft = event.target.value;
  }
  if (event.target.dataset.field === "speaker-name") {
    state.speakerCorrectionDraft = event.target.value;
  }
  if (event.target.dataset.field === "vocabulary-before" && state.vocabulary) {
    state.vocabulary = { ...state.vocabulary, sourcePhrase: event.target.value };
  }
  if (event.target.dataset.field === "vocabulary-after" && state.vocabulary) {
    state.vocabulary = { ...state.vocabulary, preferredReplacement: event.target.value };
  }
}

function handleChange(event) {
  if (event.target.dataset.field === "retention-days") {
    state.retentionDays = Number(event.target.value);
    render();
  }
  if (event.target.dataset.field === "attestation") {
    state.consent[event.target.dataset.attestation] = event.target.checked;
    render();
  }
  if (event.target.dataset.field === "operator-note") void flushPendingNoteSave();
  if (event.target.dataset.field === "meeting-context") void flushPendingContextSave();
  if (event.target.dataset.field === "library-operator-note") void flushSelectedNoteSave();
}

function handleKeydown(event) {
  if (event.key === "Escape" && state.modal) {
    closeModal();
    render();
    return;
  }
  if (event.key === "Escape" && state.meetingManagementOpen) { state.meetingManagementOpen = false; render(); return; }
  if (!event.metaKey || event.altKey || event.ctrlKey) return;
  if (event.key.toLowerCase() === "r" && canOpenStart(state.snapshot, state.permissions)) {
    event.preventDefault();
    openStart();
  }
  if (event.key.toLowerCase() === "k" && state.snapshot?.capture === "idle") {
    event.preventDefault();
    void openMeetings().then(() => document.querySelector("[data-field='library-search']")?.focus());
  }
}

function handleSubmit(event) {
  const form = event.target.dataset.form;
  if (!["rename-meeting", "speaker-correction", "vocabulary"].includes(form)) return;
  event.preventDefault();
  if (form === "rename-meeting") void saveMeetingTitle();
  else if (form === "speaker-correction") void saveSpeakerCorrection();
  else void saveVocabulary();
}

async function initialize() {
  render();
  if (!invoke) return;
  listenForNoteCaptureHotkey();
  try {
    await Promise.all([
      refreshSnapshot({ shouldRender: false }),
      refreshLibrary(),
      refreshPermissions(),
    ]);
  } catch (error) {
    state.error = errorRecoveryPresentation(error || "Yawn could not open its local workspace.");
  }
  render();
  window.setInterval(() => {
    if (shouldPollSnapshot(state.snapshot)) {
      void refreshSnapshot().catch(reportError);
    }
    void refreshRetainedAudioPlayback();
  }, 900);
}

document.addEventListener("click", handleClick);
document.addEventListener("input", handleInput);
document.addEventListener("change", handleChange);
document.addEventListener("keydown", handleKeydown);
document.addEventListener("submit", handleSubmit);
window.addEventListener("focus", refreshPermissionsOnReturn);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) refreshPermissionsOnReturn();
});

void initialize();
