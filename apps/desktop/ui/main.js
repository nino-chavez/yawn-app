import {
  backgroundTranscriptionPresentation,
  canOpenStart,
  contentView,
  captureActivity,
  captureActivityElapsedSeconds,
  captureIsInProgress,
  capturePresentation,
  errorRecoveryPresentation,
  humanize,
  libraryEmptyStatePresentation,
  libraryLoadingPresentation,
  libraryRecoveryPresentation,
  libraryRowMetaPresentation,
  libraryStallTransition,
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
  evidencePopoverPresentation,
  firstRunSheetVisible,
  nextEscapeTarget,
  retainedAudioPlaybackPresentation,
  retentionLabel,
  retryTurnDiffSegments,
  shouldPollSnapshot,
  sidebarGroups,
  sidebarRowTitle,
  sortLibraryRows,
  startSheetGuidedHint,
  toolbarTitlePresentation,
  transcriptCitationSummary,
  transcriptPlainText,
  transcriptRetryDiffPresentation,
  transcriptRetryQualityPresentation,
  transcriptSearchAffordancePresentation,
  transcriptSearchResultSnippet,
  transcriptSpeakerLabel,
  transcriptRetryPresentation,
  transcriptTurnsForSourceSpeaker,
  transcriptTurnsMatching,
  transcriptionWorkerHeartbeatAgeSeconds,
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
  // Desktop-design audit (2026-09-01), fix 1: whether the library load in
  // flight has run past the bounded wait. Read by `renderLibrary` on every
  // render tick; only `armLibraryStallTimer`/`disarmLibraryStallTimer` write
  // it, so the escalation is state-driven, never derived from the DOM.
  libraryStalled: false,
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
  // Rethink phase 1 (DESIGN.md): the sidebar toggle's own state, plus the
  // one-shot launch-selection flags below. `sidebarCollapsed` is explicit
  // operator intent (the toolbar toggle or ⌘⇧S / menu:toggle-sidebar); the
  // width-based collapse below 900pt (DESIGN.md) is read live from the
  // window in `render()`, not mirrored into state.
  sidebarCollapsed: false,
  // Launch state (experience-brief-2026-09-02.md "Window state restoration"):
  // open to the library with the most recent meeting selected, attempted
  // exactly once so it never fights a later, deliberate deselection.
  launchSelectionAttempted: false,
  autoSelecting: false,
  snapshot: null,
  speakerCorrection: null,
  speakerCorrectionDraft: "",
  // Roadmap packet W10: true only when the start sheet was opened from the
  // empty state's guided invitation ("Try it: record a 30-second note to
  // yourself."), never from the ordinary Record button or ⌘R. It only
  // changes what `renderStartSheet` shows above the attestations -- the
  // consent flow, retention choice, and start path are identical either way.
  startSheetGuided: false,
  transcriptRetry: null,
  vocabulary: null,
  transcriptActionStatus: {},
  transcriptQuery: "",
  noteCaptureFocusPending: false,
  // Roadmap intake W8-B: the one-week local usage probe for cross-meeting
  // exact search. `transcriptSearchResults` is null until the operator
  // activates the affordance under the title-search box; `transcriptSearchRows`
  // is an unfiltered snapshot of every meeting, fetched alongside the search
  // so a hit outside the current title filter can still show a real title and
  // date rather than nothing. Both are cleared whenever the title-search query
  // changes or a meeting is opened, so a stale result list never survives past
  // the query it answered.
  transcriptSearchResults: null,
  transcriptSearchRows: null,
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

// Desktop-design audit (2026-09-01), fix 1: the bounded wait before the
// library's loading line escalates to honest stall copy. The phase and the
// real setTimeout handle are ordinary module state (same idiom as the
// timers above); `libraryStallTransition` (view-model.mjs) is the pure
// function that decides what the phase becomes, so only the plumbing lives
// here.
const LIBRARY_STALL_MS = 10_000;
let libraryStallPhase = "idle";
let libraryStallTimer;

function armLibraryStallTimer() {
  clearTimeout(libraryStallTimer);
  libraryStallPhase = libraryStallTransition(libraryStallPhase, "load-start");
  state.libraryStalled = false;
  libraryStallTimer = setTimeout(() => {
    libraryStallPhase = libraryStallTransition(libraryStallPhase, "stall-elapsed");
    if (libraryStallPhase === "stalled") {
      state.libraryStalled = true;
      render();
    }
  }, LIBRARY_STALL_MS);
}

function disarmLibraryStallTimer() {
  clearTimeout(libraryStallTimer);
  libraryStallTimer = undefined;
  libraryStallPhase = libraryStallTransition(libraryStallPhase, "load-settled");
  state.libraryStalled = false;
}

// Packet W7-C: local start-latency evidence for design intake D2's stated-
// but-never-measured seconds budget (docs/roadmap.md). Not part of `state` --
// same reasoning as the evidence-depth scheduling flags below: this is
// bookkeeping for one start attempt, not something any render depends on.
// Clock model: `t0EpochMs` anchors a single `Date.now()` read taken at the
// operator's click; every later mark in the same attempt is derived as that
// anchor plus a `performance.now()` delta, so the marks stay in the order
// they actually happened even though `performance.now()` and `Date.now()`
// are different clocks -- only one wall-clock read happens per attempt.
let startJourney = null;

function beginStartJourney() {
  startJourney = { t0EpochMs: Date.now(), t0Perf: performance.now(), t1EpochMs: null };
}

// t1: the start sheet rendered and interactive. Called immediately after the
// synchronous `render()` that shows it -- there is no virtual-DOM scheduling
// in this app, so the sheet's controls are already interactive the instant
// that call returns.
function markStartSheetInteractive() {
  if (!startJourney || startJourney.t1EpochMs != null) return;
  startJourney.t1EpochMs = startJourney.t0EpochMs + (performance.now() - startJourney.t0Perf);
}

// t2: consent confirmed. Reads whatever `openStart` began and clears it, so
// one start attempt cannot leak its marks into the next. A missing journey
// (should not happen -- `startRecording` is only reachable through the sheet
// `openStart` renders) falls back to a degenerate but still-ordered journey
// rather than sending Rust a partially-populated one.
function takeJourneyTimingForConfirm() {
  const journey = startJourney;
  startJourney = null;
  const t0EpochMs = journey ? journey.t0EpochMs : Date.now();
  const t0Perf = journey ? journey.t0Perf : performance.now();
  const t1EpochMs = journey?.t1EpochMs ?? t0EpochMs;
  const t2EpochMs = t0EpochMs + (performance.now() - t0Perf);
  return { t0EpochMs, t1EpochMs, t2EpochMs };
}

// Design intake D5's evidence-depth affordances, simplified by rethink phase
// 1 (see the Inspector section below). The popover is a transient, cursor-
// anchored overlay outside the patched tree entirely (see
// `showEvidencePopover`). `state.selected.evidenceSplit` is whether the
// inspector is open and which turn it targets; it lives there so it resets
// with the rest of `state.selected` on every meeting load -- see
// `loadSelectedMeeting`.
let evidenceHoverTimer = null;
let evidencePopoverEl = null;
let evidencePopoverAnchor = null;
const EVIDENCE_HOVER_DELAY_MS = 350;

// Roadmap packet W10: whether the first-run sheet is showing as of the most
// recent render. Not part of `state` -- it is fully derived from `state.library`
// and `state.modal` every tick (see `render()`), and this is only a cached copy
// so `handleKeydown`'s Escape handling can ask "is it showing right now"
// without recomputing the same derivation outside a render pass.
let firstRunSheetShowing = false;

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

// Design intake D5's split-view width gate reads the live window, not a
// cached value, so a resize crossing the threshold is seen the next time
// anything asks.
function currentWindowWidth() {
  return typeof window !== "undefined" && Number.isFinite(window.innerWidth) ? window.innerWidth : 0;
}

function prefersReducedMotion() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
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

// DESIGN.md's composition: below this width the sidebar collapses behind its
// toggle and nothing else reflows. Read live, like the evidence-split width
// gate this replaces -- never mirrored into state.
const SIDEBAR_COLLAPSE_WIDTH = 900;

function render() {
  const editorFocus = captureEditorFocus();
  const view = contentView({
    hasInvoke: Boolean(invoke),
    snapshot: state.snapshot,
    hasSelected: Boolean(state.selected),
    trashOpen: Boolean(state.trashOpen),
  });
  // These four states cannot function as a shell: there is no library to
  // show beside a document yet (still checking, choosing a model, or a real
  // startup failure), or there is no Tauri bridge to read one from at all.
  // DESIGN.md's toolbar/sidebar/document composition governs every ready
  // state; these render full-window, same as before this rethink.
  if (["browser-notice", "startup-checking", "model-setup", "startup-attention"].includes(view)) {
    patchInto(root, `<div class="app-shell app-shell-preshell">${
      view === "browser-notice" ? renderBrowserNotice()
        : view === "startup-checking" ? renderStartup(true)
          : view === "model-setup" ? renderModelSetup()
            : renderStartup(false)
    }</div>`);
    restoreEditorFocus(editorFocus);
    return;
  }

  // Roadmap packet W10: the first-run sheet shows only over the ready shell,
  // with nothing selected, capture idle, no other sheet already open, and
  // the library loaded with zero meetings still undismissed. See
  // `firstRunSheetVisible` for the truth table.
  const firstRunVisible = view === "home" && !state.modal && firstRunSheetVisible(state.library);
  firstRunSheetShowing = firstRunVisible;

  const capturing = captureIsInProgress(state.snapshot);
  const selectedTitle = view === "meeting" && state.selected
    ? sidebarRowTitle(state.selected.row, dateLabel(state.selected.row?.createdAtEpochSeconds))
    : "";
  const title = toolbarTitlePresentation({ capturing, selectedTitle });
  const collapsed = state.sidebarCollapsed || currentWindowWidth() < SIDEBAR_COLLAPSE_WIDTH;

  let paneContent;
  if (view === "capture") paneContent = renderCapturePane();
  else if (view === "meeting") paneContent = renderMeetingPane();
  else if (view === "trash") paneContent = renderTrashPane();
  else paneContent = renderEmptyPane();

  // Patched in place, never assigned wholesale: replacing root.innerHTML
  // destroyed every editor node per 900 ms poll tick, which reset WebKit's
  // per-element undo stack (and collapsed reader-opened <details>). See
  // dom-patch.mjs for the preservation rules.
  patchInto(root, `
    <div class="app-shell${collapsed ? " sidebar-collapsed" : ""}" data-capture="${escapeHtml(state.snapshot?.capture || "opening")}">
      <header class="toolbar" data-tauri-drag-region>
        <div class="toolbar-left" data-tauri-drag-region="false">
          <button class="icon-button sidebar-toggle" type="button" data-action="toggle-sidebar" title="Toggle Sidebar (⌘⇧S)" aria-label="Toggle Sidebar" aria-pressed="${collapsed ? "false" : "true"}">
            <svg width="16" height="14" viewBox="0 0 16 14" fill="none" aria-hidden="true">
              <rect x="0.5" y="0.5" width="15" height="13" rx="2.5" stroke="currentColor" stroke-opacity="0.85" />
              <line x1="5.5" y1="1" x2="5.5" y2="13" stroke="currentColor" stroke-opacity="0.85" />
            </svg>
          </button>
        </div>
        <div class="toolbar-title">${escapeHtml(title)}</div>
        <div class="toolbar-right" data-tauri-drag-region="false">
          <label class="search-field">
            <span class="visually-hidden">Search meetings</span>
            <input type="search" data-field="library-search" value="${escapeHtml(state.search)}" placeholder="Search meetings" aria-label="Search meetings" autocorrect="off" spellcheck="false" />
          </label>
          <div class="record-control">${renderToolbarRecordControl()}</div>
        </div>
      </header>
      <div class="body">
        <nav class="sidebar" id="sidebar" aria-label="Meetings">${renderSidebar()}</nav>
        <main class="content">${paneContent}</main>
      </div>
      ${firstRunVisible ? renderFirstRunSheet() : ""}
      ${state.modal === "start" ? renderStartSheet() : ""}
      ${state.modal === "rename-meeting" ? renderRenameMeetingSheet() : ""}
      ${state.modal === "speaker-correction" ? renderSpeakerCorrectionSheet() : ""}
      ${state.modal === "transcript-retry" ? renderTranscriptRetrySheet() : ""}
      ${state.modal === "vocabulary" ? renderVocabularySheet() : ""}
      ${["delete-recording", "delete-transcript", "delete-meeting"].includes(state.modal) ? renderMeetingDeletionSheet() : ""}
      ${["lock-meeting", "unlock-meeting"].includes(state.modal) ? renderMeetingLockSheet() : ""}
      ${state.notice ? `<aside id="toast-notice" class="toast toast-notice" role="status"><button type="button" data-action="clear-notice" aria-label="Dismiss">×</button>${escapeHtml(state.notice)}</aside>` : ""}
      ${state.error ? `<aside id="toast-error" class="toast" role="alert"><button type="button" data-action="clear-error" aria-label="Dismiss">×</button><span>${escapeHtml(state.error.message)}</span>${state.error.action ? `<button class="button button-quiet button-small" type="button" data-action="${escapeHtml(state.error.action.action)}">${escapeHtml(state.error.action.label)}</button>` : ""}</aside>` : ""}
    </div>
  `);
  restoreEditorFocus(editorFocus);
  if (state.noteCaptureFocusPending) focusOperatorNoteFromHotkey();
  syncActivityClock();
  dismissEvidencePopoverIfDetached();
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

// The main-window brief's app-menu contract: the shell (native menu bar)
// emits these three events, this frontend answers them. `menu:stop` reuses
// the exact same path as the toolbar Stop control -- `stopRecording` already
// guards on capture being recording/paused, so a Stop with nothing recording
// is a harmless no-op either way it's reached.
function listenForMenuEvents() {
  if (!tauriListen) return;
  tauriListen("menu:new-recording", () => openStart()).catch(() => {});
  tauriListen("menu:stop", () => { void stopRecording(); }).catch(() => {});
  tauriListen("menu:toggle-sidebar", () => toggleSidebar()).catch(() => {});
  tauriListen("menu:open-transcript", () => openFullTranscriptFromInspector()).catch(() => {});
}

// tokens.css keys dark mode on `html[data-theme="dark"]` only -- it carries
// no `prefers-color-scheme` fallback of its own (that's `settings.js`'s
// job for the Settings window; this is the same pattern for the main
// window). Set at load and kept in sync with the system appearance so
// "both appearances from tokens alone" (DESIGN.md) actually holds here.
function syncThemeFromSystem() {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    document.documentElement.dataset.theme = query.matches ? "dark" : "light";
  };
  apply();
  query.addEventListener("change", apply);
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

// -- Toolbar -------------------------------------------------------------

// DESIGN.md's Record control: idle ("Record"), live (elapsed + Pause/Resume
// + Stop). Every non-idle capture state shows the live badge -- only
// recording/paused get Pause or Resume (`capturePauseControlPresentation`
// returns null for arming/stopping/captured/transcribing/summarizing, which
// still show elapsed time and Stop, matching the toolbar title's "New
// Recording" for the same states).
function renderToolbarRecordControl() {
  const snapshot = state.snapshot;
  if (captureIsInProgress(snapshot)) {
    const capture = snapshot?.capture;
    const elapsed = captureActivityElapsedSeconds(snapshot);
    const elapsedLabel = elapsed === null ? humanize(capture) : timeLabel(elapsed);
    const badgeLabel = capture === "paused" ? `Paused ${elapsedLabel}` : elapsedLabel;
    const pauseControl = capturePauseControlPresentation(snapshot);
    const stopping = state.busyAction === "stop";
    const changing = state.busyAction === "pause" || state.busyAction === "resume";
    return `
      <span class="btn record live record-live-badge" aria-label="${escapeHtml(humanize(capture))}, ${escapeHtml(elapsedLabel)}">${escapeHtml(badgeLabel)}</span>
      ${pauseControl ? `<button class="btn" type="button" data-action="${pauseControl.action}" ${pauseControl.disabled || changing || stopping ? "disabled" : ""}>${escapeHtml(pauseControl.label)}</button>` : ""}
      <button class="btn" type="button" data-action="stop-recording" title="Stop (⌘.)" ${stopping ? "disabled" : ""}>${stopping ? "Stopping…" : "Stop"}</button>
    `;
  }
  const startAvailable = canOpenStart(state.snapshot, state.permissions);
  return `<button class="btn record record-idle" type="button" data-action="open-start" title="Record (⌘R)" ${startAvailable ? "" : "disabled"}>Record</button>`;
}

// -- Sidebar ---------------------------------------------------------------

function timeOfDayLabel(epochSeconds) {
  if (!Number.isFinite(Number(epochSeconds))) return "";
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" })
    .format(new Date(Number(epochSeconds) * 1000));
}

function renderSidebarRow(row, { selected = false } = {}) {
  const meta = libraryRowMetaPresentation(row);
  const title = sidebarRowTitle(row, dateLabel(row.createdAtEpochSeconds));
  return `
    <button class="row${selected ? " selected" : ""}" type="button" role="listitem" data-action="open-meeting" data-handle="${escapeHtml(row.handle)}"${meta.locked ? ` data-locked="true"` : ""}>
      <span class="row-title">${escapeHtml(title)}</span>
      <span class="row-caption caption">${escapeHtml(timeOfDayLabel(row.createdAtEpochSeconds))} · ${escapeHtml(meta.label)}</span>
      ${meta.preview ? `<span class="row-excerpt">${escapeHtml(meta.preview)}</span>` : ""}
    </button>`;
}

function renderSidebarGroups(rows, selectedHandle) {
  return sidebarGroups(rows).map((group) => `
    <div class="group-label">${escapeHtml(group.label)}</div>
    ${group.rows.map((row) => renderSidebarRow(row, { selected: row.handle === selectedHandle })).join("")}
  `).join("");
}

function renderRecordingNowRow() {
  const elapsed = captureActivityElapsedSeconds(state.snapshot);
  const label = elapsed === null ? humanize(state.snapshot?.capture) : timeLabel(elapsed);
  return `
    <div class="row recording-now" aria-current="true">
      <span class="row-title">Recording now</span>
      <span class="row-caption caption">${escapeHtml(label)}</span>
    </div>`;
}

function renderSidebar() {
  const library = state.library;
  const capturing = captureIsInProgress(state.snapshot);
  const selectedHandle = state.selected?.row?.handle || "";

  let body;
  const waiting = libraryLoadingPresentation(library, state.libraryStalled);
  const recovery = library ? libraryRecoveryPresentation(library) : null;
  if (waiting) {
    body = `
      <p class="quiet-copy sidebar-message">${escapeHtml(waiting.message)}</p>
      ${waiting.action ? `<button class="button button-quiet button-small" type="button" data-action="${escapeHtml(waiting.action.action)}">${escapeHtml(waiting.action.label)}</button>` : ""}`;
  } else if (recovery) {
    body = `
      <p class="quiet-copy sidebar-message">${escapeHtml(recovery.detail)}</p>
      <button class="button button-quiet button-small" type="button" data-action="${recovery.action.action}">${escapeHtml(recovery.action.label)}</button>`;
  } else {
    const empty = libraryEmptyStatePresentation(library);
    body = empty
      ? `
        <p class="quiet-copy sidebar-message">${escapeHtml(empty.message)}</p>
        ${empty.showGuidedInvite ? `<button class="button button-quiet button-small" type="button" data-action="open-start-guided" ${canOpenStart(state.snapshot, state.permissions) ? "" : "disabled"}>Try it: record a 30-second note to yourself.</button>` : ""}`
      : renderSidebarGroups(library.rows, selectedHandle);
    body += renderTranscriptSearchAffordance(library);
    body += renderTranscriptSearchResults();
  }

  return `
    <div class="sidebar-scroll">
      ${capturing ? renderRecordingNowRow() : ""}
      ${body}
    </div>
    <button class="trash-row${state.trashOpen ? " selected" : ""}" type="button" data-action="open-trash" aria-current="${state.trashOpen ? "true" : "false"}">
      <svg width="13" height="14" viewBox="0 0 13 14" fill="none" aria-hidden="true">
        <path d="M1 3.5h11M4.5 3.5V2a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5M2.5 3.5l.6 9a1 1 0 0 0 1 .95h4.8a1 1 0 0 0 1-.95l.6-9" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span>Trash</span>
    </button>`;
}

// -- Document pane -----------------------------------------------------------

// DESIGN.md's needs-attention surface: headline, detail, one primary and one
// secondary action, nothing else -- no banner, no icon. Shared by the empty
// pane's audio-permission block, a locked-meeting barrier, and a meeting's
// own recovery/failure state, so the shape stays the same wherever a
// condition actually blocks or endangers the record.
function renderNeedsAttentionPane({ headline, detail, action = null, secondaryAction = null }) {
  return `
    <div class="attention-pane">
      <h1 class="attention-headline">${escapeHtml(headline)}</h1>
      <p class="attention-detail">${escapeHtml(detail)}</p>
      ${action || secondaryAction ? `
      <div class="attention-actions">
        ${action ? `<button class="btn primary" type="button" data-action="${escapeHtml(action.action)}" ${action.disabled ? "disabled" : ""}>${escapeHtml(action.label)}</button>` : ""}
        ${secondaryAction ? `<button class="btn" type="button" data-action="${escapeHtml(secondaryAction.action)}">${escapeHtml(secondaryAction.label)}</button>` : ""}
      </div>` : ""}
    </div>`;
}

function renderEmptyPane() {
  const permission = permissionSummary(state.permissions);
  if (permission.state !== "ready" && permission.state !== "checking") {
    // DESIGN.md: a full-width surface is reserved for a condition that
    // blocks or endangers the record, and it always carries a next action.
    // Audio access not being ready blocks the one thing this window is for.
    const action = permissionAction(state.permissions);
    return renderNeedsAttentionPane({ headline: permission.title, detail: permission.detail, action });
  }
  const processing = backgroundTranscriptionPresentation(state.snapshot);
  return `
    <div class="empty-pane">
      <p class="caption">Select a meeting, or press Record</p>
      ${processing ? `<p class="caption">${escapeHtml(processing.detail)}</p>` : ""}
    </div>`;
}

function renderTrashPane() {
  const presentation = trashListPresentation(state.trash);
  return `
    <div class="doc-wrap">
      <article class="read trash-pane" aria-labelledby="trash-heading">
        <h1 id="trash-heading">Trash</h1>
        <p class="caption doc-caption">Deleted meetings stay here for 30 days, then Yawn removes them permanently. There is no server copy, so a meeting cannot be recovered after that.</p>
        ${presentation.state === "loading" ? `<p class="caption">Loading Trash…</p>` : ""}
        ${presentation.state === "empty" ? `<p class="caption">Trash is empty.</p>` : ""}
        ${presentation.state === "populated" ? `
          <ul class="trash-list">
            ${presentation.entries.map((entry) => {
              const busy = state.busyAction === `restore-${entry.meetingId}`;
              return `
              <li>
                <span>
                  <span class="row-title">${escapeHtml(entry.label)}</span>
                  <span class="row-caption caption">Deleted ${escapeHtml(dateLabel(entry.deletedAtEpochSeconds))} · removed permanently ${escapeHtml(dateLabel(entry.purgeAfterEpochSeconds))}</span>
                </span>
                <button class="btn" type="button" data-action="restore-trash-entry" data-meeting-id="${escapeHtml(entry.meetingId)}" ${busy ? "disabled" : ""}>${busy ? "Restoring…" : "Restore"}</button>
              </li>`;
            }).join("")}
          </ul>` : ""}
      </article>
    </div>`;
}

// Roadmap intake W8-B: the probe's entire UI. With the flag off (the
// product's default, and everything before this packet), `presentation` is
// null and this renders nothing -- not even an empty wrapper element -- so
// the Home screen is byte-identical to before the probe existed.
function renderTranscriptSearchAffordance(library) {
  const presentation = transcriptSearchAffordancePresentation({
    library,
    query: state.search,
    snapshot: state.snapshot,
  });
  if (!presentation) return "";
  if (presentation.state === "unavailable") {
    return `<p class="quiet-copy transcript-search-affordance" data-state="unavailable">${escapeHtml(presentation.message)}</p>`;
  }
  return `<button class="button button-quiet button-small transcript-search-affordance" type="button" data-action="search-transcripts" data-query="${escapeHtml(presentation.query)}">Search transcripts for “${escapeHtml(presentation.query)}”</button>`;
}

function renderTranscriptSearchResults() {
  const response = state.transcriptSearchResults;
  if (!response) return "";
  if (!response.results?.length) {
    return `<p class="quiet-copy transcript-search-results" data-state="${escapeHtml(response.state)}">${escapeHtml(response.message)}</p>`;
  }
  return `
    <div class="meeting-list transcript-search-results" role="list" aria-label="Transcript search results">
      ${response.results.map((result) => {
        const row = state.transcriptSearchRows?.find((candidate) => candidate.meetingId === result.meetingId);
        const title = row?.label || (row ? `Meeting · ${dateLabel(row.createdAtEpochSeconds)}` : "Meeting");
        return `
        <button class="meeting-row" type="button" role="listitem" data-action="open-search-result" data-search-handle="${escapeHtml(result.handle)}">
          <span>
            <span class="meeting-row-title">${escapeHtml(title)}</span>
            <span class="meeting-row-preview">${escapeHtml(transcriptSearchResultSnippet(result))}</span>
            ${row ? `<span class="meeting-row-meta">${escapeHtml(dateLabel(row.createdAtEpochSeconds))}</span>` : ""}
          </span>
          <span class="meeting-row-arrow" aria-hidden="true">›</span>
        </button>
      `;
      }).join("")}
    </div>
  `;
}

// DESIGN.md's During moment: the document pane becomes the note canvas --
// the operator's own notes at the reading measure, the pause/state fact as a
// caption, a collapsed live-transcript disclosure beneath. This also renders
// the one-tick terminal-capture fallback (see contentView): a terminal
// snapshot the library has not yet indexed into a `meeting` selection. That
// branch keeps a "Back to Meetings" / "Record another meeting" action so the
// reader is never stranded while the lookup resolves.
function renderCapturePane() {
  const snapshot = state.snapshot;
  const inProgress = captureIsInProgress(snapshot);
  const presentation = capturePresentation(snapshot);
  const disabled = state.noteUnreadable || !snapshot.meeting_id;
  const contextDisabled = state.contextUnreadable || !snapshot.meeting_id;
  const context = snapshot.error || snapshot.warnings?.[0] || "";
  return `
    <div class="canvas-wrap">
      <div class="canvas-caption caption">${escapeHtml(presentation.detail)}</div>
      ${context ? `<p class="caption canvas-context-note" data-tone="${escapeHtml(presentation.tone)}">${escapeHtml(context)}</p>` : ""}
      ${!inProgress ? `<div class="canvas-terminal-actions">${captureAction(snapshot, true)}</div>` : ""}
      ${renderActivityMonitor(snapshot)}
      <textarea class="notes-area" data-field="operator-note" data-meeting-id="${escapeHtml(snapshot.meeting_id || "")}" aria-label="Your meeting notes" placeholder="Write down the detail you will want to verify later." ${disabled ? "disabled" : ""}>${escapeHtml(state.noteDraft)}</textarea>
      <div class="canvas-field-foot caption">${escapeHtml(state.noteUnreadable ? "This note could not be read, so Yawn will not overwrite it." : noteSaveCopy())}</div>
      <details class="canvas-context-disclosure">
        <summary class="caption">What is this meeting for? (optional)</summary>
        <textarea class="notes-area canvas-context-area" data-field="meeting-context" data-meeting-id="${escapeHtml(snapshot.meeting_id || "")}" aria-label="What is this meeting for? Optional." placeholder="What is this meeting for? What must get decided?" ${contextDisabled ? "disabled" : ""}>${escapeHtml(state.contextDraft)}</textarea>
        <div class="canvas-field-foot caption">${escapeHtml(state.contextUnreadable ? "This context could not be read, so Yawn will not overwrite it." : contextSaveCopy())}</div>
      </details>
      ${snapshot.turns?.length ? `
        <details class="transcript-disclosure">
          <summary>Live transcript (${snapshot.turns.length} turns)</summary>
          <div class="transcript-disclosure-content">${renderTranscript(snapshot.turns, inProgress ? "Live transcript" : "Source transcript", inProgress ? "Yawn adds local transcription here as it becomes available." : "The retained record for checking a detail that matters.", {
            copyAction: "copy-current-transcript",
            openFileAction: snapshot.capture === "transcript-ready" && snapshot.current_transcript_sha256 ? "open-current-transcript-file" : "",
          })}</div>
        </details>` : ""}
    </div>
  `;
}

// Rethink phase 1: during ordinary recording or paused, the toolbar's own
// Record control already carries the live elapsed time, and the canvas
// caption above already states what's happening -- a third restatement here
// is exactly the "same status stated three times" the cold review flagged
// (finding 3). This line earns its place only for the transitional steps
// (arming, stopping, captured, transcribing, summarizing), where the elapsed-
// in-this-step timer and transcription heartbeat are information the reader
// cannot get anywhere else on screen.
function renderActivityMonitor(snapshot) {
  if (["recording", "paused"].includes(snapshot?.capture)) return "";
  const activity = captureActivity(snapshot);
  if (!activity) return "";
  const startedAt = Number(snapshot.capture_state_started_at_epoch_seconds);
  const elapsed = captureActivityElapsedSeconds(snapshot);
  const heartbeatAt = Number(snapshot.transcription_last_worker_heartbeat_at_epoch_seconds);
  const heartbeatAge = transcriptionWorkerHeartbeatAgeSeconds(snapshot);
  return `
    <div class="activity-monitor caption" data-tone="${escapeHtml(activity.tone)}" aria-label="${escapeHtml(activity.label)}">
      <span>${escapeHtml(activity.label)} · ${escapeHtml(activity.detail)}</span>
      <span class="activity-timing">
        <strong data-activity-elapsed data-activity-started-at="${Number.isFinite(startedAt) ? startedAt : ""}">${elapsed === null ? "Working" : timeLabel(elapsed)}</strong>
        ${snapshot.capture === "transcribing" ? `<span class="activity-heartbeat" data-transcription-heartbeat data-transcription-heartbeat-at="${Number.isFinite(heartbeatAt) ? heartbeatAt : ""}" aria-live="polite">${heartbeatAge === null ? "Waiting for a confirmation on this Mac" : `Last confirmed on this Mac ${elapsedAgoLabel(heartbeatAge)}`}</span>` : ""}
      </span>
    </div>
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

function renderTranscript(turns, title, detail = "", { copyAction = "", openFileAction = "", exportAction = "", workspace = false, citations = null, targetTurnIndex = null } = {}) {
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
    // Design intake D5, depth 3: the synced-scroll target is the one piece of
    // per-turn visual state the split's transcript column carries, and it is
    // rendered from `state` (via `targetTurnIndex`) rather than toggled as an
    // imperative class -- an unrelated re-render (a keystroke elsewhere on
    // the page) would otherwise wipe a class the template does not know
    // about the next time `dom-patch.mjs` syncs this node's attributes.
    const isSyncTarget = Number.isInteger(targetTurnIndex) && Number(turn.sourceTurnIndex) === targetTurnIndex;
    return `
      <div class="transcript-line ${turn.withheld ? "withheld" : ""}${isSyncTarget ? " transcript-line-target" : ""}" data-turn-index="${escapeHtml(turn.sourceTurnIndex)}">
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
          <p>${renderClaimText(claim)}</p>
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
          <h2 id="meeting-note-heading">No meeting note yet.</h2>
          <p class="caption">${escapeHtml(note?.message || "Yawn has no generated note for this meeting.")}</p>
        </header>
      </section>
    `;
  }
  if (presentation.state === "extracts-only") {
    const count = presentation.highlights.length;
    return `
      <section class="meeting-note meeting-note-unavailable" aria-labelledby="meeting-note-heading">
        <header class="meeting-note-header">
          <h2 id="meeting-note-heading">A summary wasn’t produced.</h2>
          <p class="caption">Yawn selected ${count} transcript ${count === 1 ? "excerpt" : "excerpts"}, but those excerpts are source material, not a meeting summary. Labeled <strong>Transcript highlights</strong> rather than a draft note.</p>
        </header>
        <details class="transcript-highlights">
          <summary><span>Transcript highlights</span><span>${count}</span></summary>
          <div class="transcript-highlights-content">${renderMeetingNoteItems(presentation.highlights, claimEvidence)}</div>
        </details>
      </section>
    `;
  }
  return `
    <section class="meeting-note" aria-label="Meeting note">
      ${presentation.summary.length ? `
        <section class="meeting-note-section meeting-note-overview" aria-labelledby="meeting-overview-heading">
          <h3 id="meeting-overview-heading">Overview</h3>
          ${presentation.summary.map((claim) => `<div class="meeting-note-summary-item"${Number.isInteger(claim.ordinal) ? ` data-claim-item="${escapeHtml(claim.ordinal)}"` : ""}>
            <p>${renderClaimText(claim)}</p>
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

// DESIGN.md's Inspector: the cited turn plus one neighbour on each side,
// target highlighted, neighbours dimmed, "Open full transcript" at the foot.
// Static once opened -- it never scrolls the note (that's the entire
// simplification from the prior depth-3 synced-scroll split).
function renderInspector(transcript, evidenceSplit) {
  const turnIndex = evidenceSplit.turnIndex;
  const turns = (transcript?.turns || []).filter((turn) => (
    Number.isInteger(Number(turn?.sourceTurnIndex))
    && Number(turn.sourceTurnIndex) >= turnIndex - 1
    && Number(turn.sourceTurnIndex) <= turnIndex + 1
  ));
  return `
    <aside class="inspector" id="evidence-split-column" aria-label="Cited transcript">
      <div class="inspector-head">
        <span class="inspector-label">Source</span>
        <button class="icon-button" type="button" data-action="close-evidence-split" aria-label="Close cited transcript">×</button>
      </div>
      ${turns.map((turn) => {
        const target = Number(turn.sourceTurnIndex) === turnIndex;
        const speakerLabel = transcriptSpeakerLabel(turn);
        return `
        <div class="transcript-turn${target ? " highlighted" : " dim"}" data-turn-index="${escapeHtml(turn.sourceTurnIndex)}">
          <div class="t-meta"><span class="t-who">${escapeHtml(speakerLabel || "Unattributed")}</span><span class="caption">${escapeHtml(timeLabel(turn.start))}</span></div>
          <p>${turn.withheld ? "This turn was withheld by the voice check." : escapeHtml(turn.text)}</p>
        </div>`;
      }).join("")}
      <button class="btn" type="button" data-action="open-full-transcript">Open full transcript</button>
    </aside>`;
}

function renderMeetingPane() {
  const { row, note, transcript } = state.selected;
  const title = sidebarRowTitle(row, dateLabel(row.createdAtEpochSeconds));

  // Roadmap intake I5. A locked meeting renders only the barrier -- there is
  // no meeting content behind it to hide, because the response carried none.
  const lock = meetingLockPresentation(note);
  if (lock && lock.state !== "unlocked" && lock.state !== "open") {
    const busy = state.busyAction === "unlock-meeting-open";
    return renderNeedsAttentionPane({
      headline: lock.heading,
      detail: lock.detail,
      action: lock.action ? { ...lock.action, disabled: busy } : null,
    });
  }

  // "audio-released" is not a blocking condition -- the transcript and note
  // stay fully readable; only retranscription is gone. It renders as a
  // caption inside the ordinary content below, the same way the prior
  // version kept the workspace visible under that one warning.
  const recovery = meetingRecoveryPresentation(note, transcript, state.generatingMeetingId);
  // A recovery presentation replaces the whole pane only when nothing about
  // the meeting is readable. A recovered-interrupted meeting with retained
  // audio, or any meeting with transcript turns or the operator's own notes,
  // is content plus a fact: it renders the workspace and the note card
  // carries the state (DESIGN.md, "a fact is a caption; a problem is a
  // needs-attention state" -- the needs-attention pane is for the case
  // where there is nothing else to show).
  const readable = Boolean(
    transcript?.turns?.length
    || note?.microphonePlaybackHandle
    || note?.systemPlaybackHandle
    || note?.operatorNote?.text,
  );
  const blockingRecovery = recovery && recovery.state !== "audio-released" && !readable ? recovery : null;
  if (blockingRecovery) {
    const canDeleteMeeting = Boolean(note?.meetingDeletionHandle);
    return renderNeedsAttentionPane({
      headline: blockingRecovery.title,
      detail: blockingRecovery.detail,
      action: blockingRecovery.action,
      secondaryAction: canDeleteMeeting ? { action: "delete-meeting", label: "Move to Trash…" } : null,
    });
  }

  if (lock?.state === "open") {
    return `
      <div class="doc-wrap"><article class="read">
        <h1>${escapeHtml(title)}</h1>
        <p class="caption doc-caption">${escapeHtml(dateLabel(row.createdAtEpochSeconds))}</p>
        <p class="caption">${escapeHtml(lock.detail)}</p>
      </article></div>`;
  }

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
  // Roadmap intake I5. Locking is offered on any readable meeting; removing a
  // lock is offered wherever one exists.
  const canLock = lock?.state === "unlocked";
  const canUnlock = Boolean(note?.lock?.locked);
  const canManage = canDeleteRecording || canDeleteTranscript || canDeleteMeeting || canLock || canUnlock;
  const playback = retainedAudioPlaybackPresentation(note, recovery, state.audioPlayback);
  const evidenceSplit = state.selected?.evidenceSplit || { open: false, ordinal: null, turnIndex: null };
  const inspectorOpen = Boolean(evidenceSplit.open && transcript?.turns?.length);

  // DESIGN.md: Retry, export, rename, lock, Manage, Move to Trash stay
  // available but demoted to the Manage menu / secondary controls next to
  // the title -- nothing removed from the command surface.
  const manageMenu = canManage ? `
    <div class="meeting-manage-control">
      <button class="btn" type="button" data-action="toggle-meeting-management" aria-expanded="${state.meetingManagementOpen ? "true" : "false"}" aria-controls="meeting-manage-menu">Manage</button>
      ${state.meetingManagementOpen ? `
      <div class="meeting-manage-menu" id="meeting-manage-menu" role="group" aria-label="Manage this meeting">
        <p class="caption">These actions affect only this meeting on this Mac.</p>
        ${canLock ? `<button class="btn" type="button" data-action="lock-meeting">Lock meeting…</button>` : ""}
        ${canUnlock ? `<button class="btn" type="button" data-action="unlock-meeting">Remove lock…</button>` : ""}
        ${canDeleteRecording ? `<button class="btn" type="button" data-action="delete-recording">Delete recording</button>` : ""}
        ${canDeleteTranscript ? `<button class="btn" type="button" data-action="delete-transcript">Delete transcript</button>` : ""}
        ${canDeleteMeeting ? `<button class="btn danger" type="button" data-action="delete-meeting">Move to Trash…</button>` : ""}
      </div>` : ""}
    </div>` : "";

  return `
    <div class="doc-wrap">
      <div class="doc-main">
      <article class="read" aria-labelledby="meeting-title">
        <div class="doc-head">
          <h1 id="meeting-title">${escapeHtml(title)}</h1>
          <div class="doc-head-actions">
            ${canRename ? `<button class="btn" type="button" data-action="rename-meeting">Rename</button>` : ""}
            ${manageMenu}
          </div>
        </div>
        <p class="caption doc-caption">${escapeHtml(dateLabel(row.createdAtEpochSeconds))} · ${escapeHtml(note?.state ? humanize(note.state) : "Loading note")}</p>
        ${renderMeetingCapturePauses(note?.capturePauses)}
        ${recovery?.state === "audio-released" ? `<p class="caption">${escapeHtml(recovery.detail)}</p>` : ""}
        ${note?.state !== "transcript-only" && note?.message && !claims.length ? `<p class="caption">${escapeHtml(note.message)}</p>` : ""}
        ${renderMeetingNote(note, claimEvidence)}
        ${renderGenerateNote(note, recovery)}
        ${renderTranscriptRetryAction(note, transcript, recovery)}
        ${renderRetainedAudioPlayback(playback)}
        ${renderMeetingContextSection(note)}
        <section class="note-section your-notes-section" aria-labelledby="operator-note-heading">
          <div class="note-editor-head"><h3 id="operator-note-heading">Your notes</h3><span class="caption" id="library-note-save-state">${escapeHtml(selectedNoteCopy)}</span></div>
          ${operatorNote?.unreadable
            ? `<p class="caption">Yawn could not read this meeting’s personal note, so it was left unchanged.</p>`
            : `<textarea class="notes-area meeting-notes-editor" data-field="library-operator-note" data-meeting-id="${escapeHtml(row.meetingId || "")}" aria-label="Your meeting notes" placeholder="Write down the detail you will want to verify later." ${noteEditable ? "" : "disabled"}>${escapeHtml(state.selected?.operatorNoteDraft || "")}</textarea>
              <p class="caption">${noteEditable ? "Saved separately from the transcript. These are your notes, not generated claims." : "Reopen this meeting to edit its notes."}</p>`}
        </section>
      </article>
      <div class="doc-transcript-disclosure-wrap">${renderTranscriptDisclosure(transcript, recovery, note)}</div>
      </div>
      ${inspectorOpen ? renderInspector(transcript, evidenceSplit) : ""}
    </div>
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
        ? `<p class="caption">Yawn could not read this meeting’s pre-meeting context.</p>`
        : presentation.state === "present"
          ? `<p class="meeting-context-text">${escapeHtml(presentation.text)}</p>
             <p class="caption">What the operator said this meeting was for, used to guide the generated note. Not a transcript.</p>`
          : `<p class="caption">No pre-meeting context was written for this meeting.</p>`}
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
// DESIGN.md: a fact is a caption, never a banner -- whether the fact is
// routine ("paused once, 0:03") or genuinely unverifiable. Neither carries an
// action, so neither qualifies for the needs-attention treatment either.
function renderMeetingCapturePauses(pauses) {
  if (!pauses) return "";
  const presentation = capturePausePresentation(pauses);
  if (presentation.state === "not-paused") return "";
  return `<p class="caption" data-capture-pauses="${escapeHtml(presentation.state)}">${escapeHtml(presentation.detail)}</p>`;
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
      <p class="caption">Microphone and system audio are separate recordings.</p>
      <div class="retained-audio-controls">
        ${playback.controls.map((control) => `<button class="btn" type="button" data-action="play-retained-audio" data-source="${escapeHtml(control.source)}" ${playback.isPlaying ? "disabled" : ""}>${escapeHtml(control.label)}</button>`).join("")}
        ${playback.isPlaying ? `<button class="btn" type="button" data-action="stop-retained-audio">Stop</button>` : ""}
      </div>
      <p class="caption" aria-live="polite">${escapeHtml(playingLabel)}</p>
    </section>
  `;
}

function renderGenerateNote(note, recovery = meetingRecoveryPresentation(note, state.selected?.transcript, state.generatingMeetingId)) {
  if (recovery && recovery.state !== "audio-released") return "";
  const control = noteGenerationPresentation(note, state.generatingMeetingId);
  if (!control) return "";
  return `
    <section class="note-section generate-note-section" aria-label="Generate a meeting note">
      <button class="btn primary" type="button" data-action="${control.action}" ${control.disabled ? "disabled" : ""}>${escapeHtml(control.label)}</button>
      <p class="caption">${escapeHtml(control.help)}</p>
    </section>
  `;
}

// DESIGN.md's claim row: the claim's own text is the affordance -- a dashed
// hairline closed, an evidence tint and accent underline open (`.claim` in
// styles.css) -- not a separate "Show source" control. Keeps the
// `.claim-source-button` class so the existing depth-1 hover popover still
// targets it (`evidenceSourceButtonFromEvent`), and the same
// `data-action="open-evidence-split"` so click handling is unchanged. `id`
// (not a positional match) is this element's identity across a render patch:
// the hover popover and the inspector-open action both need the same node
// reachable by ordinal regardless of where sibling claims shift it.
function renderClaimText(claim) {
  const text = escapeHtml(claim.claim);
  if (!claim.locatorCount || !claim.spans?.length) return `<span>${text}</span>`;
  const evidenceSplit = state.selected?.evidenceSplit;
  const open = Boolean(evidenceSplit?.open && Number(evidenceSplit.ordinal) === Number(claim.ordinal));
  return `<span class="claim claim-source-button${open ? " open" : ""}" id="claim-source-${escapeHtml(claim.ordinal)}" data-action="open-evidence-split" data-ordinal="${escapeHtml(claim.ordinal)}" role="button" tabindex="0">${text}</span>`;
}

// The honest note under a claim whose source cannot be shown -- never a
// placeholder, never invented wording. `evidence?.state === "evidence"` is
// the legacy `open-claim-evidence` fetch path (superseded by D5's
// `claim.spans`, kept only because reachable state can still carry it).
function renderClaimEvidence(claim, evidence) {
  if (evidence?.state === "evidence" && evidence.text) {
    return `
      <div class="claim-evidence">
        <span>Transcript source · ${escapeHtml(timeLabel(evidence.start))}</span>
        <p>${escapeHtml(evidence.text)}</p>
      </div>
    `;
  }
  if (!claim.locatorCount) return `<span class="claim-source-unavailable">No source passage is available for this point.</span>`;
  // Design intake D5, decision 2: `claim.spans` is already the digest-quoted
  // transcript text for every one of this claim's locators, batched onto the
  // note response -- see `library_reader.rs`'s `LibraryClaim.spans`. A claim
  // whose locators could not currently be re-sliced (`spans` empty despite a
  // nonzero `locatorCount`) says so rather than offering a control that would
  // only reach a stale-evidence error.
  if (!claim.spans?.length) {
    return `<span class="claim-source-unavailable">This point’s exact wording could not be verified against the transcript.</span>`;
  }
  return "";
}

// -- W9-B: the eight sheets below share one entrance-motion mechanism -------
//
// Each sheet's `.modal-backdrop` and `.start-sheet` (styles.css) carry the
// entrance animation as a plain, permanent CSS rule -- no class toggling, no
// JS-driven timing. That works because of how ui/dom-patch.mjs already
// treats these nodes: neither carries an `id` or `data-field`, so they are
// matched positionally by tag name and morphed in place across the 900 ms
// snapshot poll. The animation only plays when the browser first connects
// the node to the document, so the first render after `state.modal` opens is
// the only tick that ever fires it -- every later tick while the same sheet
// stays open reuses the same node and never replays it.
//
// Exit is intentionally instant: `closeModal()` (below) clears state.modal
// and `handleClick` calls render() in the same synchronous handler, and that
// render is also driven independently by the poll. Animating the close would
// mean holding the closing node alive across an async gap while an unrelated
// tick could re-render underneath it -- the "JS gymnastics" this packet was
// told to avoid. An honest instant close is the shipped behavior.
//
// Roadmap packet W10's first-run sheet (`renderFirstRunSheet`, below) is the
// eighth and reuses the same two classes, but is not tracked in `state.modal`
// at all -- its visibility is fully derived from `state.library` in `render()`
// (see `firstRunSheetVisible`), so there is nothing for `closeModal()` to
// clear. Its exit is instant for the same reason the others are: dismissal
// flips the derived condition false and calls `render()` synchronously.
//
// Do not add an `id` to a backdrop or dialog element for any reason other
// than a deliberate identity change (see ui/dom-patch.test.mjs) -- one would
// make the node re-insert, and therefore re-animate, on every poll tick.

function renderStartSheet() {
  const permission = permissionSummary(state.permissions);
  const audioReady = permission.state === "ready";
  const allConfirmed = Object.values(state.consent).every(Boolean);
  const action = permissionAction(state.permissions);
  // Roadmap packet W10: null on the ordinary Record/⌘R path, so this whole
  // block is absent and the sheet is byte-identical to before this packet.
  const guidedHint = startSheetGuidedHint(state.startSheetGuided);
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
        ${guidedHint ? `<p class="quiet-copy guided-start-hint">${escapeHtml(guidedHint)}</p>` : ""}
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

// Roadmap packet W10 (product brief, "A first run must teach without
// counterfeiting," amended 2026-09-01). Shown once, on the very first arrival
// at a ready Home with zero meetings -- see `firstRunSheetVisible` for the
// truth table this is gated on in `render()`. Mines the README's own
// "A meeting has three moments" table and the brief's before/during/after
// language rather than inventing new copy; states no capability the app does
// not have.
//
// Reuses `.modal-backdrop`/`.start-sheet` (styles.css) verbatim -- the same
// entrance-motion mechanism W9-B gave every other sheet, and the same
// dom-patch identity rules (no `id` on the backdrop or dialog) so it is
// never re-animated while it stays open across a poll tick. One primary
// action ("Got it") and nothing else in the action row: this is an
// acknowledgment, not a decision, so there is no Cancel beside it.
// Cold review bfa0a80 finding 01: "three-card layout is heavier than a
// one-time dismissible explainer needs." DESIGN.md drops eyebrow labels
// system-wide; the product brief's amendment (2026-09-01) permits "one short
// orientation," not seeded content -- so this is one paragraph naming the
// three moments, not a three-card dl.
function renderFirstRunSheet() {
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="start-sheet first-run-sheet" role="dialog" aria-modal="true" aria-labelledby="first-run-sheet-title">
        <div class="sheet-head">
          <div>
            <h2 id="first-run-sheet-title">Before, during, after.</h2>
            <p>Confirm consent and headphones, then record. Keep your own notes while it runs. Afterward, generate a readable note with decisions and follow-ups that point back to the transcript — all on this Mac.</p>
          </div>
          <button class="icon-button" type="button" data-action="dismiss-first-run" aria-label="Close">×</button>
        </div>
        <div class="sheet-actions">
          <button class="button button-primary" type="button" data-action="dismiss-first-run">Got it</button>
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
  // Roadmap packet W10: cleared here too, not only after a successful start,
  // so a cancelled guided invitation never leaks its hint into the next
  // ordinary Record click.
  state.startSheetGuided = false;
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

// W7-B (2026-09-01 desktop audit). A handful of commands answer `Ok` with a
// `{state, message, code}` shape rather than rejecting, and are displayed
// directly from that response -- but an unhappy `state` also re-throws the
// same `message` as an `Error` so it reaches `reportError` and the same
// recovery-action path a genuine command rejection would. This carries the
// response's `code` onto that re-thrown `Error` so `errorRecoveryPresentation`
// can key on it instead of the exact message text.
function throwRecoverableResponse(response, fallbackMessage) {
  const error = new Error(response?.message || fallbackMessage);
  if (response && typeof response.code === "string") error.code = response.code;
  throw error;
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
  // Only the load that hasn't yet succeeded once needs the stall timer --
  // once `state.library` is set it is never nulled out again, so a search
  // or a background refresh never re-arms it.
  if (!state.library) armLibraryStallTimer();
  try {
    state.library = await invoke("library_snapshot", { filter: title ? { title } : null });
  } finally {
    disarmLibraryStallTimer();
  }
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

// Roadmap packet W10: `guided` is true only for the empty state's "Try it"
// invitation. It changes nothing about the start journey itself -- the same
// t0 mark, the same sheet, the same consent gate -- only whether
// `renderStartSheet` shows the one-line guided hint above the attestations.
// DESIGN.md: the sidebar collapses behind its toggle (⌘⇧S, the View menu,
// or below 900pt window width -- read live in `render()`, not mirrored
// here). This is the operator's own explicit choice; it does not override
// the width-based collapse, which stays in effect either way.
function toggleSidebar() {
  state.sidebarCollapsed = !state.sidebarCollapsed;
  render();
}

function openStart(guided = false) {
  if (!canOpenStart(state.snapshot, state.permissions)) return;
  // t0: the operator's start intent -- this is the entry point every start
  // route (the topbar/Home Record button, the ⌘R hotkey, and the guided
  // invitation) shares.
  beginStartJourney();
  state.modal = "start";
  state.startSheetGuided = guided;
  render();
  markStartSheetInteractive();
}

// Roadmap packet W10: the once-only first-run sheet's dismissal. Flips the
// local copy of `firstRunSheetSeen` immediately so the sheet is gone on the
// very next render -- the operator should not wait on a round trip to see
// "Got it" take effect -- then persists it best-effort. See
// `dismiss_first_run_sheet` (main.rs) and `onboarding.rs` for why a write
// failure here stays silent: at most, the sheet reappears on a later cold
// boot, which is not worth an error toast.
async function closeFirstRunSheet() {
  if (state.library) state.library = { ...state.library, firstRunSheetSeen: true };
  render();
  if (!invoke) return;
  try {
    await invoke("dismiss_first_run_sheet");
  } catch {
    // Best-effort -- see the function doc above.
  }
}

async function startRecording() {
  if (!canOpenStart(state.snapshot, state.permissions) || !Object.values(state.consent).every(Boolean)) return;
  const journeyTiming = takeJourneyTimingForConfirm();
  await runBusy("start", async () => {
    state.snapshot = await invoke("start_meeting", {
      retentionDays: Number(state.retentionDays),
      attestation: state.consent,
      journeyTiming,
    });
    clearCurrentNote();
    clearCurrentContext();
    state.activeView = "capture";
    state.modal = "";
    state.startSheetGuided = false;
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
  state.trashOpen = false;
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

// Roadmap intake W8-B. Runs only when the affordance rendered, which already
// required a non-empty query and the local probe flag -- so this trusts the
// caller and simply asks the backend, which re-checks the flag itself as the
// first thing it does either way.
async function searchTranscripts(query) {
  await runBusy("transcript-search", async () => {
    const [results, unfiltered] = await Promise.all([
      invoke("preview_library_search", { query }),
      invoke("library_snapshot", { filter: null }),
    ]);
    state.transcriptSearchResults = results;
    state.transcriptSearchRows = unfiltered.rows || [];
  });
}

// Opens a cross-meeting search hit through the library's ordinary open path
// (`openMeeting`), rather than a second, search-specific reader -- the
// backend hands back only a meeting id and a transcript handle, never a
// filename or a general path, so this resolves that meeting id to the same
// row `openMeeting` already knows how to open (lock confirmation included).
async function openTranscriptSearchResult(handle) {
  let response;
  try {
    response = await invoke("preview_library_open_search_result", { handle });
  } catch (error) {
    reportError(error);
    return;
  }
  if (!response.meetingId) {
    state.notice = response.message || "That result is no longer available.";
    render();
    return;
  }
  let row = state.transcriptSearchRows?.find((candidate) => candidate.meetingId === response.meetingId)
    || state.library?.rows?.find((candidate) => candidate.meetingId === response.meetingId);
  if (!row) {
    try {
      const unfiltered = await invoke("library_snapshot", { filter: null });
      state.transcriptSearchRows = unfiltered.rows || [];
      row = state.transcriptSearchRows.find((candidate) => candidate.meetingId === response.meetingId);
    } catch (error) {
      reportError(error);
      return;
    }
  }
  if (!row) {
    state.notice = "That meeting could not be reopened.";
    render();
    return;
  }
  state.transcriptSearchResults = null;
  state.transcriptSearchRows = null;
  await openMeeting(row.handle);
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
  // Leaving Home for a meeting detail, however it was reached, must not leave
  // a stale cross-meeting search result list behind for the next Home render.
  state.transcriptSearchResults = null;
  state.transcriptSearchRows = null;
  state.selected = {
    row,
    note,
    transcript,
    transcriptRetry: pendingRetry,
    claimEvidence: {},
    // Design intake D5, depth 2/3: never persists across meetings, and reset
    // here rather than surviving a same-meeting refresh, the same lifecycle
    // `claimEvidence` already follows on this same object.
    evidenceSplit: { open: false, ordinal: null, turnIndex: null },
    operatorNoteDraft: operatorNote.text || "",
    operatorNoteSaveQueue: Promise.resolve(),
    operatorNoteSaveState: operatorNote.unreadable ? "unreadable" : operatorNote.text ? "saved" : "local",
  };
  state.audioPlayback = { state: "idle", source: null, message: "No recording is playing." };
  state.activeView = "meeting";
  dismissEvidencePopover();
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
    if (response.state !== "playing") {
      throwRecoverableResponse(response, "Retained audio is unavailable. Reopen Library and try again.");
    }
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
      throwRecoverableResponse(response, "Yawn could not delete this recording.");
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
      throwRecoverableResponse(response, "Yawn could not delete this transcript.");
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
      throwRecoverableResponse(response, "Yawn could not delete this meeting.");
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

// Design intake D5: evidence disclosure's three depths. Depth 1 (hover
// preview) and depths 2/3 (split view, synced scroll) are grouped together
// here because they share the same anchor -- a claim's "Show source"
// affordance -- and the same handle economy: every claim's locators already
// arrived quoted (`claim.spans`, batched onto the note response by
// `library_reader.rs`), so nothing below ever invokes a Tauri command.

// -- Depth 1: hover / keyboard-focus preview --------------------------------
//
// The popover is a plain DOM node appended to `document.body`, entirely
// outside the tree `patchInto` patches. That is deliberate, not an oversight:
// a render tick firing mid-hover (the 900 ms poll, an unrelated keystroke)
// must not disturb a transient overlay the reader is currently looking at,
// and nothing outside `#app` is ever touched by `render()`. The only seam
// between the two is `dismissEvidencePopoverIfDetached`, called from
// `render()`'s tail, which drops the popover if its anchor button itself was
// removed by that same patch (the claim it belonged to disappeared, not just
// moved).

function evidenceSourceButtonFromEvent(target) {
  return target instanceof Element ? target.closest(".claim-source-button") : null;
}

function handleEvidenceHoverIn(event) {
  const button = evidenceSourceButtonFromEvent(event.target);
  if (button) scheduleEvidencePopover(button);
}

function handleEvidenceHoverOut(event) {
  const button = evidenceSourceButtonFromEvent(event.target);
  if (!button) return;
  if (event.relatedTarget instanceof Node && button.contains(event.relatedTarget)) return;
  dismissEvidencePopover();
}

function handleEvidenceFocusIn(event) {
  const button = evidenceSourceButtonFromEvent(event.target);
  if (button) scheduleEvidencePopover(button);
}

function handleEvidenceFocusOut(event) {
  if (evidenceSourceButtonFromEvent(event.target)) dismissEvidencePopover();
}

function scheduleEvidencePopover(anchor) {
  window.clearTimeout(evidenceHoverTimer);
  const ordinal = Number(anchor.dataset.ordinal);
  evidenceHoverTimer = window.setTimeout(() => showEvidencePopover(anchor, ordinal), EVIDENCE_HOVER_DELAY_MS);
}

function showEvidencePopover(anchor, ordinal) {
  if (!document.contains(anchor)) return;
  const claim = state.selected?.note?.claims?.find((candidate) => Number(candidate?.ordinal) === ordinal);
  const presentation = evidencePopoverPresentation(claim, state.selected?.transcript?.turns);
  if (!presentation) return;
  dismissEvidencePopover();
  const el = document.createElement("div");
  el.className = "evidence-popover";
  el.setAttribute("role", "note");
  el.innerHTML = `
    <p class="evidence-popover-meta">${escapeHtml(presentation.speaker)}${presentation.start !== null ? ` · ${escapeHtml(timeLabel(presentation.start))}` : ""}</p>
    <p class="evidence-popover-text">${escapeHtml(presentation.text)}</p>
  `;
  document.body.appendChild(el);
  positionEvidencePopover(el, anchor);
  evidencePopoverEl = el;
  evidencePopoverAnchor = anchor;
}

function positionEvidencePopover(el, anchor) {
  const rect = anchor.getBoundingClientRect();
  const width = el.offsetWidth || 320;
  const left = Math.max(12, Math.min(rect.left, window.innerWidth - width - 12));
  el.style.position = "fixed";
  el.style.left = `${left}px`;
  el.style.top = `${rect.bottom + 8}px`;
}

function dismissEvidencePopover() {
  window.clearTimeout(evidenceHoverTimer);
  evidenceHoverTimer = null;
  evidencePopoverEl?.remove();
  evidencePopoverEl = null;
  evidencePopoverAnchor = null;
}

// `render()`'s tail calls this after every patch: the popover's anchor is
// looked up fresh each time it is shown (never cached across a render), so
// the only failure a patch can cause is the anchor button itself vanishing
// (the claim it belonged to no longer renders at all) -- everything else
// about `.claim-source-button` (see `renderClaimEvidence`) keeps the same
// `id` across a patch, so an ordinary re-render never disturbs it.
function dismissEvidencePopoverIfDetached() {
  if (evidencePopoverAnchor && !document.contains(evidencePopoverAnchor)) dismissEvidencePopover();
}

// -- Inspector: DESIGN.md's simplified depth 2 -------------------------------
//
// Rethink phase 1 replaces the prior split view's depth 3 (synced scroll
// tracking the note pane's topmost visible claim) with a fixed 320pt
// inspector: the cited turn plus one neighbour on each side, static once
// opened. DESIGN.md: "Inspector | closed, open | 320, panel background,
// never scrolls the note." There is nothing to scroll and no width gate --
// the prior `evidenceSplitAllowed(width)` fallback existed only for the two-
// reading-column split this replaces.

function openEvidenceSplit(ordinal) {
  if (!state.selected || !Number.isFinite(ordinal)) return;
  const claim = state.selected.note?.claims?.find((candidate) => Number(candidate?.ordinal) === ordinal);
  const span = claim?.spans?.[0];
  if (!span) return;
  dismissEvidencePopover();
  state.selected.evidenceSplit = { open: true, ordinal, turnIndex: span.sourceTurnIndex };
  render();
}

function closeEvidenceSplit() {
  if (!state.selected?.evidenceSplit?.open) return;
  state.selected.evidenceSplit = { open: false, ordinal: null, turnIndex: null };
  render();
}

// The inspector's own "Open full transcript" (DESIGN.md): expands the
// document's below-the-note transcript disclosure and brings it into view.
// The inspector itself stays open -- Esc or its own close control dismiss it.
function openFullTranscriptFromInspector() {
  const details = root.querySelector(".transcript-disclosure");
  if (!details) return;
  details.open = true;
  details.scrollIntoView({ behavior: prefersReducedMotion() ? "auto" : "smooth", block: "start" });
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
  state.transcriptSearchResults = null;
  state.transcriptSearchRows = null;
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
  else if (action === "toggle-sidebar") toggleSidebar();
  else if (action === "open-full-transcript") openFullTranscriptFromInspector();
  else if (action === "open-start") openStart();
  else if (action === "open-start-guided") openStart(true);
  else if (action === "dismiss-first-run") void closeFirstRunSheet();
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
  else if (action === "search-transcripts") void searchTranscripts(control.dataset.query);
  else if (action === "open-search-result") void openTranscriptSearchResult(control.dataset.searchHandle);
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
  else if (action === "open-evidence-split") openEvidenceSplit(Number(control.dataset.ordinal));
  else if (action === "close-evidence-split") closeEvidenceSplit();
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
    // A changed query invalidates whatever cross-meeting search results are
    // showing -- they answered the previous query, not this one.
    state.transcriptSearchResults = null;
    state.transcriptSearchRows = null;
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
  if (event.key === "Escape") {
    // Roadmap packet W10: the first-run sheet is not tracked in `state.modal`
    // (its visibility is fully derived -- see `render()`), so it needs its
    // own Escape check ahead of `nextEscapeTarget`, which knows nothing about
    // it. It has no nested popover or split behind it, so there is no
    // precedence to resolve: Escape simply dismisses it, the same as "Got it".
    if (firstRunSheetShowing) { void closeFirstRunSheet(); return; }
    // Design intake D5: the innermost, most transient surface closes first
    // -- a popover dismissing under the reader's cursor must not also close
    // a sheet or the split behind it.
    const target = nextEscapeTarget({
      popoverOpen: Boolean(evidencePopoverEl),
      modalOpen: Boolean(state.modal),
      splitOpen: Boolean(state.selected?.evidenceSplit?.open),
    });
    if (target === "popover") { dismissEvidencePopover(); return; }
    if (target === "modal") { closeModal(); render(); return; }
    if (target === "split") { closeEvidenceSplit(); return; }
    if (state.meetingManagementOpen) { state.meetingManagementOpen = false; render(); return; }
    return;
  }
  // DESIGN.md: "⌫ Move to Trash" -- scoped to a focused sidebar row so an
  // ordinary Backspace while editing a note or a search field never deletes
  // a meeting.
  if (event.key === "Backspace" && state.selected && document.activeElement?.closest?.(".sidebar .row")) {
    event.preventDefault();
    openMeetingDeletion("delete-meeting");
    render();
    return;
  }
  if (!event.metaKey || event.ctrlKey) return;
  const key = event.key.toLowerCase();
  // DESIGN.md: "⌘⌥T full transcript" is the one shortcut that uses Option,
  // so it is resolved before the general Option-key guard below rejects
  // every other combination.
  if (event.altKey && key === "t") {
    event.preventDefault();
    openFullTranscriptFromInspector();
    return;
  }
  if (event.altKey) return;
  if (key === "r" && !event.shiftKey && canOpenStart(state.snapshot, state.permissions)) {
    event.preventDefault();
    // Roadmap packet W10: the first-run sheet is not tracked in `state.modal`,
    // so ⌘R would otherwise set `state.modal = "start"` in the same tick the
    // sheet is still showing -- two sheets landing in the same unkeyed
    // `.modal-backdrop` slot within one render, which dom-patch would morph
    // in place rather than re-insert, silently skipping the entrance
    // animation W9-B guarantees every sheet plays once. Dismissing first and
    // returning (mirroring the Escape branch above) keeps every sheet
    // transition to one mount per tick; a second ⌘R then opens Start normally.
    if (firstRunSheetShowing) { void closeFirstRunSheet(); return; }
    openStart();
    return;
  }
  if (key === "." && !event.shiftKey && ["recording", "paused"].includes(state.snapshot?.capture)) {
    event.preventDefault();
    void stopRecording();
    return;
  }
  if (key === "p" && event.shiftKey) {
    event.preventDefault();
    if (state.snapshot?.capture === "recording") void pauseRecording();
    else if (state.snapshot?.capture === "paused") void resumeRecording();
    return;
  }
  if (key === "s" && event.shiftKey) {
    event.preventDefault();
    toggleSidebar();
    return;
  }
  if (key === "f" && !event.shiftKey && state.snapshot?.capture === "idle") {
    event.preventDefault();
    void openMeetings().then(() => document.querySelector("[data-field='library-search']")?.focus());
    return;
  }
  if (key === "," && !event.shiftKey) {
    event.preventDefault();
    if (invoke) void invoke("open_settings_window").catch(reportError);
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

// Rethink phase 1: launch/router fix (experience-brief-2026-09-02.md's
// "Window state restoration"; cold review bfa0a80 finding 03/08, "a stale
// terminal capture view" instead of the library). Attempted after every
// snapshot+library pairing, so it is safe to call repeatedly from the poll
// loop -- each branch is a no-op once it no longer applies.
async function maybeAutoSelectMeeting() {
  if (!invoke || state.autoSelecting || state.selected || state.modal || !state.library) return;

  // A terminal capture state (this session's just-finished recording, or a
  // stale one left over from a previous run) is content, not the live
  // canvas -- select it into the ordinary meeting pane the moment the
  // library has indexed it. Retried on every poll tick, not one-shot: a
  // capture that just finished may not be in `state.library` yet.
  const meetingId = state.snapshot?.meeting_id;
  if (meetingId && !captureIsInProgress(state.snapshot) && state.snapshot?.capture !== "idle") {
    const row = state.library.rows?.find((candidate) => candidate.meetingId === meetingId);
    if (!row) return;
    state.autoSelecting = true;
    try { await openMeeting(row.handle); } finally { state.autoSelecting = false; }
    return;
  }

  // Attempted exactly once, and only once capture is genuinely idle, so it
  // never fights a later, deliberate deselection (Trash, a needs-attention
  // "Back to Meetings").
  if (state.launchSelectionAttempted || state.snapshot?.capture !== "idle") return;
  state.launchSelectionAttempted = true;
  const top = sortLibraryRows(state.library.rows)[0];
  if (!top) return;
  state.autoSelecting = true;
  try { await openMeeting(top.handle); } finally { state.autoSelecting = false; }
}

async function initialize() {
  syncThemeFromSystem();
  render();
  if (!invoke) return;
  listenForNoteCaptureHotkey();
  listenForMenuEvents();
  try {
    await Promise.all([
      refreshSnapshot({ shouldRender: false }),
      refreshLibrary(),
      refreshPermissions(),
    ]);
  } catch (error) {
    state.error = errorRecoveryPresentation(error || "Yawn could not open its local workspace.");
  }
  await maybeAutoSelectMeeting();
  render();
  window.setInterval(() => {
    if (shouldPollSnapshot(state.snapshot)) {
      void refreshSnapshot().then(maybeAutoSelectMeeting).catch(reportError);
    } else {
      void maybeAutoSelectMeeting();
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
// Design intake D5's depth 1: hover/keyboard-focus preview of a claim's
// cited passage. `mouseover`/`mouseout`/`focusin`/`focusout` bubble normally.
document.addEventListener("mouseover", handleEvidenceHoverIn);
document.addEventListener("mouseout", handleEvidenceHoverOut);
document.addEventListener("focusin", handleEvidenceFocusIn);
document.addEventListener("focusout", handleEvidenceFocusOut);

void initialize();
