const CAPTURE_COPY = Object.freeze({
  idle: {
    eyebrow: "Ready when you are",
    title: "Your next meeting starts here.",
    detail: "Nothing is recording.",
    tone: "ready",
  },
  arming: {
    eyebrow: "Preparing capture",
    title: "Getting your audio ready.",
    detail: "Yawn is not recording until both audio sources are ready.",
    tone: "working",
  },
  recording: {
    eyebrow: "Recording locally",
    title: "Stay in the conversation.",
    detail: "Add a short note whenever something matters.",
    tone: "recording",
  },
  paused: {
    eyebrow: "Paused",
    title: "Nothing is being recorded.",
    detail: "The meeting is still open. Your notes are still saved. Resume when you are ready.",
    tone: "attention",
  },
  stopping: {
    eyebrow: "Stopping capture",
    title: "Finishing the recording.",
    detail: "Keep this window open while Yawn closes the audio safely.",
    tone: "working",
  },
  captured: {
    eyebrow: "Audio saved locally",
    title: "Making your transcript.",
    detail: "Your recording is safe while Yawn prepares the written record.",
    tone: "working",
  },
  transcribing: {
    eyebrow: "Transcribing on this Mac",
    title: "Making your transcript.",
    detail: "This can take a moment. Your own notes remain available below.",
    tone: "working",
  },
  summarizing: {
    eyebrow: "Preparing your note on this Mac",
    title: "Finishing your meeting note.",
    detail: "Yawn is preparing a local note from the completed transcript.",
    tone: "working",
  },
  "transcript-ready": {
    eyebrow: "Transcript ready",
    title: "Your meeting is ready to read.",
    detail: "Your transcript is already saved on this Mac. Go back to Meetings or record another meeting.",
    tone: "complete",
  },
  "transcription-failed": {
    eyebrow: "Needs attention",
    title: "The transcript was not created.",
    detail: "Yawn will tell you what remains available instead of calling this meeting complete.",
    tone: "attention",
  },
  "summary-failed": {
    eyebrow: "Needs attention",
    title: "The meeting note was not created.",
    detail: "The transcript remains available. Yawn will not call a note complete when it was not created.",
    tone: "attention",
  },
  "recovered-interrupted": {
    eyebrow: "Interrupted",
    title: "This meeting did not finish.",
    detail: "Review what survived before you start another recording.",
    tone: "attention",
  },
});

const CAPTURE_ACTIVITY_COPY = Object.freeze({
  arming: {
    label: "Preparing capture",
    detail: "Checking that both audio sources are ready. Nothing is recording yet.",
    tone: "working",
  },
  recording: {
    label: "Recording locally",
    detail: "Both audio sources are being captured on this Mac.",
    tone: "recording",
  },
  paused: {
    label: "Paused",
    detail: "Both audio sources are released. Nothing is reaching this Mac while this lasts.",
    tone: "attention",
  },
  stopping: {
    label: "Stopping recording",
    detail: "Closing both audio streams safely before transcription starts.",
    tone: "working",
  },
  captured: {
    label: "Checking captured audio",
    detail: "Verifying the finalized audio before Yawn makes the transcript.",
    tone: "working",
  },
  transcribing: {
    label: "Transcribing on this Mac",
    detail: "The captured audio is saved. Yawn is making the transcript on this Mac.",
    tone: "working",
  },
  summarizing: {
    label: "Preparing your note on this Mac",
    detail: "The transcript is ready. Yawn is preparing the local meeting note.",
    tone: "working",
  },
});

export function capturePresentation(snapshot) {
  const capture = snapshot?.capture || "idle";
  const presentation = CAPTURE_COPY[capture] || {
    eyebrow: "Needs attention",
    title: "Yawn needs attention.",
    detail: "The current recording state could not be read.",
    tone: "attention",
  };
  return { capture, ...presentation };
}

export function captureActivity(snapshot) {
  const capture = snapshot?.capture || "idle";
  const activity = CAPTURE_ACTIVITY_COPY[capture];
  return activity ? { capture, ...activity } : null;
}

export function captureActivityElapsedSeconds(snapshot, nowEpochSeconds = Date.now() / 1000) {
  if (!captureActivity(snapshot)) return null;
  const startedAt = Number(snapshot?.capture_state_started_at_epoch_seconds);
  if (!Number.isFinite(startedAt) || !Number.isFinite(nowEpochSeconds)) return null;
  return Math.max(0, Math.floor(nowEpochSeconds - startedAt));
}

export function transcriptionWorkerHeartbeatAgeSeconds(snapshot, nowEpochSeconds = Date.now() / 1000) {
  if (snapshot?.capture !== "transcribing") return null;
  const observedAt = Number(snapshot?.transcription_last_worker_heartbeat_at_epoch_seconds);
  if (!Number.isFinite(observedAt) || !Number.isFinite(nowEpochSeconds)) return null;
  return Math.max(0, Math.floor(nowEpochSeconds - observedAt));
}

// The single source of truth for which top-level surface renders, extracted
// from render() so the router's honesty is testable without a DOM.
//
// Order matters and is deliberate: the startup screen preempts everything only
// while the app genuinely cannot function yet (still checking, choosing a
// model, or a real installation failure). A per-meeting or retention
// needs-attention condition is NOT one of those — it never sets a non-ready
// startup, so it can never hide the library. That is the D-LOCK Order-4 rule
// made structural: "Back to Meetings" (dismiss → capture idle, startup ready)
// always resolves to "home", from every needs-attention state, so the library
// and its per-meeting delete/trash actions are always reachable.
// Rethink phase 1 (docs/experience-brief-2026-09-02.md, docs/design-direction
// -decision.md): a terminal capture state (transcript-ready, transcription
// -failed, recovered-interrupted) is content -- the After or needs-attention
// moment -- not the live During canvas (DESIGN.md's "capture" is only the
// in-progress canvas). Cold review bfa0a80 finding 03/08: launching Yawn with
// a stale terminal snapshot left over from a previous run showed that special
// terminal screen instead of the library. The fix: once the library has
// indexed that meeting and main.js has auto-selected it (`hasSelected` true),
// route to the ordinary "meeting" pane. Until that lookup resolves -- the one
// tick right after a live recording just stopped, before the library reflects
// it -- this still falls back to "capture" so the reader is never shown
// nothing. `hasSelected` is checked before the terminal fallback (the only
// order change from the prior version of this function), so it is the one
// thing that can turn a terminal snapshot into "meeting".
export function contentView({ hasInvoke, snapshot, hasSelected = false, trashOpen = false } = {}) {
  if (!hasInvoke) return "browser-notice";
  const startup = snapshot?.startup;
  if (!snapshot || startup === "checking") return "startup-checking";
  if (startup === "model-required") return "model-setup";
  if (startup !== "ready") return "startup-attention";
  if (captureIsInProgress(snapshot)) return "capture";
  if (hasSelected) return "meeting";
  if (snapshot.capture !== "idle") return "capture";
  if (trashOpen) return "trash";
  return "home";
}

export function canStartMeeting(snapshot) {
  const backgroundJobs = Number(snapshot?.background_transcription_queued_count) || 0;
  return snapshot?.startup === "ready"
    && snapshot?.capture === "idle"
    && backgroundJobs < 2;
}

export function backgroundTranscriptionPresentation(snapshot) {
  const queued = Math.max(0, Number(snapshot?.background_transcription_queued_count) || 0);
  const active = snapshot?.background_transcription_active === true;
  if (!active && queued === 0) return null;
  if (queued >= 2) {
    return {
      state: "full",
      label: "Earlier meetings are processing on this Mac.",
      detail: "Yawn will accept the next recording after one finishes processing. Nothing has been discarded.",
      canStart: false,
    };
  }
  return {
    state: "active",
    label: "An earlier meeting is processing on this Mac.",
    detail: "You can start the next meeting when the recorder is ready.",
    canStart: true,
  };
}

export function canOpenStart(snapshot, permission) {
  return canStartMeeting(snapshot) && permissionSummary(permission).state === "ready";
}

export function mergePermissions(previous, received) {
  const preserveMeasured = (reported, earlier) => (
    reported === "unmeasured" && earlier !== undefined && earlier !== null ? earlier : reported
  );
  return {
    ...previous,
    ...received,
    microphone: preserveMeasured(received.microphone, previous?.microphone),
    systemAudio: preserveMeasured(received.systemAudio, previous?.systemAudio),
  };
}

export function captureIsInProgress(snapshot) {
  return ["arming", "recording", "paused", "stopping", "captured", "transcribing", "summarizing"].includes(snapshot?.capture);
}

export function shouldPollSnapshot(snapshot) {
  return snapshot?.startup !== "ready"
    || captureIsInProgress(snapshot)
    || snapshot?.capture === "transcript-ready"
    || snapshot?.background_transcription_active === true
    || (Number(snapshot?.background_transcription_queued_count) || 0) > 0;
}

// Roadmap intake W8-B: the one-week local usage probe for cross-meeting exact
// search. `library.searchProbeEnabled` is the only signal the frontend has for
// whether the operator's local marker file exists -- with it false (the
// default, and everything before this packet), this must return null so the
// Home screen renders with zero difference from before the probe existed. A
// non-empty title-search query is also required: the probe rides the existing
// title-search box rather than adding a second field, so there is nothing to
// search until the operator has already typed something there. Capture gates
// on `captureIsInProgress` -- the same state the exact-search decision memo
// names for "unavailable while it would compete with the transcription
// worker" -- rather than a separate check, so this affordance is never
// available in a state the memo did not intend it to be.
export function transcriptSearchAffordancePresentation({ library, query, snapshot }) {
  if (!library?.searchProbeEnabled) return null;
  const trimmed = (query || "").trim();
  if (!trimmed) return null;
  if (captureIsInProgress(snapshot)) {
    return {
      state: "unavailable",
      query: trimmed,
      message: "Search across meetings is unavailable while recording.",
    };
  }
  return { state: "available", query: trimmed };
}

// The honest per-kind label for one cross-meeting search hit. A withheld turn
// must never render as blank or as invented transcript text (product-brief
// rule), and a title/folder match has no turn text to show at all.
export function transcriptSearchResultSnippet(result) {
  if (result?.kind === "withheld") {
    return "A voice check withheld this matching turn. It is not shown as transcript text.";
  }
  if (result?.kind === "meeting") return "Matched this meeting's title or folder.";
  return result?.text || "";
}

export function permissionSummary(permission) {
  if (!permission) {
    return { state: "checking", title: "Checking audio access", detail: "Yawn checks access on this Mac before the first recording." };
  }
  if (permission.probeUnavailable) {
    return { state: "attention", title: "Audio access could not be checked", detail: "Open Settings to check microphone and system-audio access." };
  }
  if (permission.microphone === "authorized" && permission.systemAudio === "authorized") {
    return { state: "ready", title: "Audio access is ready", detail: "Microphone and system audio are available to Yawn." };
  }
  if (permission.microphone === "denied" || permission.microphone === "restricted") {
    return { state: "attention", title: "Microphone access is needed", detail: "Allow Yawn to use the microphone before recording." };
  }
  if (permission.microphone === "authorized" && permission.systemAudio === "unmeasured") {
    return { state: "setup", title: "Allow system audio", detail: "Microphone access is ready. Let Yawn verify its capture helper before recording." };
  }
  if (["unavailable", "unsupported", "unknown"].includes(permission.systemAudio)) {
    return { state: "attention", title: "System audio needs attention", detail: "Open Settings to check system-audio access before recording." };
  }
  return { state: "setup", title: "Set up audio access", detail: "Allow microphone and system audio before your first recording." };
}

export function retentionLabel(days) {
  return `${days} ${Number(days) === 1 ? "day" : "days"}`;
}

// Playback is available only for a freshly opened, retained recording on a
// normal completed detail view. Handles remain opaque values for the native
// command; this presentation deliberately contains no fallback selector.
export function retainedAudioPlaybackPresentation(note, recovery, playback = {}) {
  if (!note || recovery || note.audioRetention?.state !== "retained") return null;
  if (!["note", "summary-failed", "transcript-only"].includes(note.state)) return null;
  const availableControls = [
    { source: "microphone", handle: note.microphonePlaybackHandle, label: "Play microphone" },
    { source: "system", handle: note.systemPlaybackHandle, label: "Play system audio" },
  ].filter((control) => Boolean(control.handle));
  if (!availableControls.length) return null;
  const playingSource = playback?.state === "playing" ? playback.source : "";
  return {
    controls: playback?.state === "idle" || !playback?.state ? availableControls : [],
    playingSource,
    isPlaying: Boolean(playingSource),
    status: playback?.state || "idle",
    message: typeof playback?.message === "string" ? playback.message : "",
  };
}

export function humanize(value) {
  return String(value || "").replace(/[-_]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

// Design intake D1: a row's preview of the note's outcome. `notePreview` is
// already the finished, capped sentence the backend read from a
// digest-verified note -- this seam only decides whether the row has one to
// show. It must never invent copy when the field is absent or blank: the
// governing constraint is real generated content only, never a placeholder
// line, and a row with no admitted note simply has no preview line.
export function libraryRowPreview(row) {
  const preview = typeof row?.notePreview === "string" ? row.notePreview.trim() : "";
  return preview || null;
}

// Desktop-design audit (2026-09-01), fix 1: the library's "loading" line
// rendered identically at 200 ms and forever -- no honest signal that a
// wait had gone on too long. After a bounded wait with no snapshot, the
// copy escalates once, plus one Try-again action reusing the existing
// refresh idiom (`refresh-library` / `refreshLibraryFromRecovery`). Copy and
// one action, no spinner. `stalled` must come from `state` in main.js (see
// `armLibraryStallTimer`), never from how long a DOM node has existed --
// the DOM gets patched in place on every render tick and cannot be trusted
// to remember when a wait started.
export function libraryLoadingPresentation(library, stalled) {
  if (library) return null;
  if (stalled) {
    return {
      stalled: true,
      message: "Still loading meetings. This is taking longer than usual.",
      action: { action: "refresh-library", label: "Try again" },
    };
  }
  return {
    stalled: false,
    message: "Loading meetings saved on this Mac…",
    action: null,
  };
}

// Pure transition table for the library-stall timer. main.js is the only
// caller that touches a real setTimeout; this function decides what phase
// that timeout should produce, so the escalation logic itself is testable
// without faking a clock.
//
//   idle    -- no load in flight, nothing pending.
//   waiting -- a load started; the bounded wait has not elapsed yet.
//   stalled -- the bounded wait elapsed before the load settled.
//
// "stall-elapsed" only moves "waiting" -> "stalled"; it is a no-op from any
// other phase, so a timer that fires after its load already settled (a slow
// callback racing a fast response) cannot resurrect a stalled state.
export function libraryStallTransition(phase, event) {
  if (event === "load-start") return "waiting";
  if (event === "load-settled") return "idle";
  if (event === "stall-elapsed") return phase === "waiting" ? "stalled" : phase;
  return phase;
}

// The library owns its state and message. Do not turn an unavailable or stale
// snapshot into the same empty-state promise used for a genuinely empty list.
export function libraryRecoveryPresentation(library) {
  const state = library?.state || "";
  if (!["unavailable", "stale"].includes(state)) return null;
  return {
    state,
    title: "Meetings need another check.",
    detail: typeof library?.message === "string" && library.message.trim()
      ? library.message.trim()
      : "Yawn could not load your saved meetings.",
    action: { action: "refresh-library", label: "Check again" },
  };
}

// Roadmap packet W10 (product brief, "A first run must teach without
// counterfeiting," amended 2026-09-01). The teaching empty state replaces the
// old one-line "your meetings will appear here" copy whenever the library is
// genuinely empty -- it states what pressing Record does, what exists
// afterward, and where it appears, and carries the one quiet guided-recording
// invitation. A filtered-to-zero result (a title search, in the only filter
// the product surface exposes today) keeps the narrower "no matches" copy and
// never gets the invitation: that affordance is about a library with nothing
// in it yet, not a query that happened to miss.
//
// Genuinely empty is keyed on `library.total === 0` -- the same fact
// `firstRunSheetVisible` gates on -- rather than on "no rows and no search
// text," so the two share one definition of empty. `LibraryFilterArgs` also
// carries a folder id and a date range; if either ever reaches the product
// surface, a filter narrowing a non-empty library to zero rows must still
// read as "no matches," never as the teaching copy.
//
// Callers must only reach this once `library` is a loaded, non-recovery
// snapshot -- the same precondition `renderLibrary` already establishes by
// checking `libraryLoadingPresentation`/`libraryRecoveryPresentation` first.
export function libraryEmptyStatePresentation(library) {
  if (library?.rows?.length) return null;
  if (Number(library?.total) !== 0) {
    return {
      variant: "no-matches",
      title: "No matching meetings",
      message: typeof library?.message === "string" && library.message.trim()
        ? library.message.trim()
        : "No meeting matches that title.",
      showGuidedInvite: false,
    };
  }
  return {
    variant: "no-meetings",
    title: "No meetings yet",
    message: "Press Record to start a private meeting. Yawn saves your notes and the transcript on this Mac as it finishes, and you can generate a note from them. It all appears right here.",
    showGuidedInvite: true,
  };
}

// Roadmap packet W10: the once-only first-run sheet's entire show/hide
// truth table, kept as one pure function so it is testable without a render
// pass. `library` is the same flattened `library_snapshot` response object
// main.js already holds in `state.library` -- `total` is the meeting count
// before any filter (so a title search never affects this), and
// `firstRunSheetSeen` is the dismissal marker's raw value (see
// `onboarding.rs`). Both must be present and loaded; a null or not-yet-loaded
// library must never show the sheet, or it would flash on for one tick
// before the first real snapshot arrives.
export function firstRunSheetVisible(library) {
  if (!library || typeof library.total !== "number") return false;
  return library.total === 0 && !library.firstRunSheetSeen;
}

// Roadmap packet W10: the one-line hint shown above the start sheet's
// attestations only when it was opened from the empty state's guided
// invitation, never from the ordinary Record button or the ⌘R hotkey. The
// exact copy is fixed by the product brief's amendment: it names the fastest
// path to seeing what Yawn makes and states the real, unconditional recovery
// (Trash, 30 days) rather than inventing a special disposable-meeting
// concept -- there is no such thing; this is an ordinary meeting.
export function startSheetGuidedHint(guided) {
  if (!guided) return null;
  return "A short test note to yourself is the fastest way to see what Yawn makes. You can delete it afterward — deleted meetings sit in Trash for 30 days.";
}

// W7-B (2026-09-01 desktop audit). This used to key entirely on exact
// backend message strings: a rewording of a known message would silently
// downgrade its recovery action to a dismiss-only toast, with no
// compile-time or test-time signal. A migrated backend error now carries a
// stable machine `code` alongside its `message` -- either as a Tauri
// command's rejected `{code, message}` object (see the Rust
// `error_codes::CommandError` envelope), or attached to a JS-re-thrown Error
// (`main.js`'s retained-audio and preview-deletion handlers already read an
// Ok-shaped `{state, message, code}` response and display it; they attach
// `.code` when re-throwing it as an `Error` to reach this same path).
//
// The three code lists below are the current mapping's source of truth.
// `error-codes.test.mjs` asserts their union is exactly the code set in
// `../src-tauri/error-codes.json` -- the single list this file and
// `error_codes.rs` are both tested against, so a code dropped from either
// side fails a test instead of silently drifting.
//
// The two message arrays below are DEPRECATED: an exact-string fallback for
// callers that still pass a plain string (or an error with no `code`, e.g.
// one not yet migrated on the Rust side). Do not add a new message here --
// give it a code in `error-codes.json` and `error_codes.rs` instead, and add
// the code to one of the lists above.
export const VIEW_STALE_CODE = "view-stale";
export const LIBRARY_UNAVAILABLE_CODES = Object.freeze([
  "library-unavailable",
  "preview-library-unavailable",
  "meeting-library-unavailable",
]);
export const SELECTED_MEETING_RECOVERY_CODES = Object.freeze([
  "transcript-unavailable",
  "transcript-changed",
  "vocabulary-check-unsafe",
  "vocabulary-read-failed",
  "vocabulary-unavailable",
  "meeting-action-in-use",
  "transcript-changed-retry",
  "speaker-correction-unavailable",
  "speaker-group-unavailable",
  "retry-quality-evidence-changed",
  "retry-device-evidence-changed",
  "retry-pause-evidence-changed",
  "retry-candidate-changed",
  "retained-audio-unavailable",
  "recording-deletion-unavailable",
  "audio-retention-unavailable",
  "meeting-changed-unavailable",
  "meeting-action-in-progress",
  "recording-deletion-failed",
  "transcript-deletion-unavailable",
  "transcript-deletion-failed",
  "meeting-deletion-unavailable",
]);

function errorCode(error) {
  return error && typeof error === "object" && typeof error.code === "string" ? error.code : null;
}

// Backend commands currently return most user-facing errors as text, not
// tagged classes. The two arrays below remain the deprecated fallback for
// exactly those unmigrated, stable recovery responses. A generic "try
// again" can describe any operation, so it must remain a plain error.
export function errorRecoveryPresentation(error, { hasSelectedMeeting = false } = {}) {
  const rawMessage = error instanceof Error
    ? error.message
    : error && typeof error === "object" && typeof error.message === "string"
      ? error.message
      : error;
  const message = String(rawMessage || "")
    .replace(/^Error:\s*/, "")
    .trim() || "Yawn could not complete that action.";
  const code = errorCode(error);
  if (code === VIEW_STALE_CODE || message === "That view is no longer current. Reopen it and try again.") {
    return {
      message,
      action: hasSelectedMeeting
        ? { action: "refresh-selected-meeting", label: "Refresh this meeting" }
        : { action: "refresh-library", label: "Check again" },
    };
  }
  if (LIBRARY_UNAVAILABLE_CODES.includes(code) || [
    "The local library is unavailable. Reopen the app and try again.",
    "The local Preview library is unavailable. Reopen the app and try again.",
    "The local meeting library is unavailable. Reopen the app and try again.",
  ].includes(message)) {
    return { message, action: { action: "refresh-library", label: "Check again" } };
  }
  // Deprecated exact-string fallback. "Meeting deletion could not complete.
  // Reopen Library and try again." has no live backend origin -- the actual
  // message reads "Moving this meeting to Trash could not complete...". Left
  // here unmigrated (already a dead match before this packet) rather than
  // "fixed" by coding a different message; see error_codes.rs's module doc.
  const selectedMeetingRecoveryMessages = [
    "The local transcript is unavailable. Reopen the meeting and try again.",
    "That transcript changed. Reopen the meeting and try again.",
    "Yawn could not safely check this transcript against local vocabulary. Nothing changed. Reopen the meeting and try again.",
    "Your saved vocabulary could not be read. Nothing changed. Reopen the meeting and try again.",
    "Vocabulary is unavailable. Reopen the meeting and try again.",
    "Another action is using this meeting. Reopen it and try again.",
    "The transcript changed. Reopen the meeting and try again.",
    "Speaker correction is unavailable. Reopen the meeting and try again.",
    "That speaker group is no longer available. Reopen the meeting and try again.",
    "Recording-quality evidence changed while opening this retry. Reopen the meeting and try again.",
    "Recording-device evidence changed while opening this retry. Reopen the meeting and try again.",
    "Pause evidence changed while opening this retry. Reopen the meeting and try again.",
    "The retry candidate changed. Reopen the meeting and try again.",
    "Retained audio is unavailable. Reopen Library and try again.",
    "Recording deletion is unavailable. Reopen Library and try again.",
    "Audio retention details are unavailable. Reopen Library and try again.",
    "This meeting changed or is no longer available. Reopen it and try again.",
    "Another action for this meeting is in progress. Reopen Library and try again.",
    "Recording deletion could not complete. Reopen Library and try again.",
    "Transcript deletion is unavailable. Reopen Library and try again.",
    "Transcript deletion could not complete. Reopen Library and try again.",
    "Meeting deletion is unavailable. Reopen Library and try again.",
    "Meeting deletion could not complete. Reopen Library and try again.",
    "That transcript is no longer available. Reopen Library and try again.",
  ];
  if (
    hasSelectedMeeting &&
    (SELECTED_MEETING_RECOVERY_CODES.includes(code) || selectedMeetingRecoveryMessages.includes(message))
  ) {
    return { message, action: { action: "refresh-selected-meeting", label: "Refresh this meeting" } };
  }
  return { message, action: null };
}

const MEETING_NOTE_GROUPS = Object.freeze([
  ["decision", "Decisions"],
  ["action", "Follow-ups"],
  ["proposal", "Ideas discussed"],
  ["question", "Open questions"],
]);

// A finished note and a set of selected transcript excerpts are different
// products. Keep that distinction in one view-model seam so the renderer cannot
// turn a point-only backend response into an apparent summary through copy alone.
export function meetingNotePresentation(note) {
  const claims = Array.isArray(note?.claims) ? note.claims : [];
  const legacySummary = Array.isArray(note?.summary)
    ? note.summary.filter((item) => typeof item === "string" && item.trim()).map((item) => item.trim())
    : typeof note?.summary === "string" && note.summary.trim()
      ? [note.summary.trim()]
      : [];
  const summaryClaims = claims.filter((claim) => claim?.claimType === "summary");
  const summary = summaryClaims.length
    ? summaryClaims
    : legacySummary.map((claim) => ({ claimType: "summary", claim }));
  const groups = MEETING_NOTE_GROUPS.map(([claimType, title]) => ({
    claimType,
    title,
    claims: claims.filter((claim) => claim?.claimType === claimType),
  })).filter((group) => group.claims.length);
  const highlights = claims.filter((claim) => claim?.claimType === "point");
  const hasNote = summary.length > 0 || groups.length > 0;
  return {
    state: hasNote ? "note" : highlights.length ? "extracts-only" : "empty",
    summary,
    groups,
    highlights,
  };
}

const CLAIM_TYPE_CITATION_LABELS = Object.freeze({
  summary: "Overview",
  decision: "Decision",
  action: "Follow-up",
  proposal: "Idea",
  question: "Open question",
  point: "Highlight",
});

// A short, non-truncating-looking preview: whole words only, never a
// mid-word cut. Six words is enough to recognize which claim this is
// without turning the transcript into a second copy of the note.
function claimTextPreview(text, wordLimit = 6) {
  const words = String(text || "").trim().split(/\s+/).filter(Boolean);
  if (!words.length) return "";
  const preview = words.slice(0, wordLimit).join(" ");
  return words.length > wordLimit ? `${preview}…` : preview;
}

// Roadmap intake I4 / design D4's reverse half: what a transcript turn's
// quiet citation affordance shows, derived only from data the note response
// already carries (`turnsCited` and `claims`). No claim text is duplicated
// here beyond a short preview -- the full claim stays in the note itself,
// one navigation away.
//
// Returns null for an uncited turn, or when the citing claim ordinal no
// longer resolves against `claims` (the two are read from the same response,
// so this should not happen live, but a renderer must not invent a citation
// for a claim it cannot show).
export function turnCitationPresentation(turnsCited, claims, sourceTurnIndex) {
  const turn = Number(sourceTurnIndex);
  if (!Array.isArray(turnsCited) || !Number.isInteger(turn)) return null;
  const entry = turnsCited.find((candidate) => Number(candidate?.turn) === turn);
  const ordinals = Array.isArray(entry?.claimOrdinals) ? entry.claimOrdinals : [];
  if (!ordinals.length) return null;
  const claimList = Array.isArray(claims) ? claims : [];
  const citations = ordinals
    .map((ordinal) => claimList.find((claim) => Number(claim?.ordinal) === Number(ordinal)))
    .filter(Boolean)
    .map((claim) => ({
      ordinal: claim.ordinal,
      label: `${CLAIM_TYPE_CITATION_LABELS[claim.claimType] || "Note"}: ${claimTextPreview(claim.claim)}`,
    }));
  if (!citations.length) return null;
  return {
    summary: citations.length === 1 ? "Cited in the note" : `Cited by ${citations.length} note items`,
    citations,
  };
}

// The optional, plain-sentence transcript-header summary design D4's reverse
// half allows: "12 of 48 turns are cited by the note." Null whenever there is
// nothing to say -- no citations at all, or a turn count Yawn cannot state.
export function transcriptCitationSummary(turnsCited, totalTurns) {
  const total = Number(totalTurns);
  if (!Array.isArray(turnsCited) || !Number.isInteger(total) || total <= 0) return null;
  const cited = turnsCited.filter((entry) => Array.isArray(entry?.claimOrdinals) && entry.claimOrdinals.length).length;
  if (!cited) return null;
  return `${cited} of ${total} ${total === 1 ? "turn is" : "turns are"} cited by the note.`;
}

// Roadmap intake I3's sidecar, read-only on the completed-meeting surface:
// mirrors `operatorNote`'s own present/empty/unreadable split exactly, so the
// renderer needs no separate rule for "nothing readable" versus "nothing
// written."
// Roadmap intake I5. The governing constraint is verbatim: "State the honest
// claim -- a local-access deterrent -- unless encryption at rest actually
// ships." It has not shipped, so every sentence below stays on the honest side
// of that line, and `meetingLockCopyIsHonest` asserts it mechanically rather
// than trusting a reviewer to notice a word drifting in later.
//
// The words that must appear, and the words that must not, are both load
// bearing. "not encryption" is the promise this packet makes about itself.
const LOCK_SHEET_COPY = Object.freeze({
  lock: Object.freeze({
    eyebrow: "Local barrier",
    heading: "Lock this meeting?",
    detail: "Its note, transcript, and audio will require Touch ID to open on this Mac. This is a local barrier, not encryption — the files on disk are unchanged.",
    label: "Lock meeting",
  }),
  unlock: Object.freeze({
    eyebrow: "Local barrier",
    heading: "Remove the lock?",
    detail: "This meeting will open without Touch ID again. Removing the lock changes nothing on disk — it was never encrypted.",
    label: "Remove lock",
  }),
});

// A Mac with no Touch ID sensor and no login password cannot run the check at
// all, so locking there would create a meeting this app can never reopen. The
// lock sheet says so and does not offer the button. This is the one case the
// packet's decision 4 does not cover -- it names the machine that becomes
// unable later, not the one that never could -- and refusing up front is
// cheaper than the unopenable meeting it prevents.
const LOCK_UNAVAILABLE_DETAIL = "This Mac cannot confirm it's you (no Touch ID or password available), so a locked meeting could not be reopened here.";

export function meetingLockSheetCopy(kind, { canConfirm = true } = {}) {
  const copy = kind === "unlock" ? LOCK_SHEET_COPY.unlock : LOCK_SHEET_COPY.lock;
  if (kind !== "unlock" && !canConfirm) {
    return { ...copy, detail: LOCK_UNAVAILABLE_DETAIL, label: copy.label, blocked: true };
  }
  return { ...copy, blocked: false };
}

// The mechanical half of the honesty rule, kept beside the copy rather than
// only in the test file so the constraint travels with the sentences it
// governs.
//
// It is a claim check, not a word ban. "This is not encryption" and "it was
// never encrypted" are the sentences the packet requires, and they contain the
// very word an outright ban would reject -- so explicit denials are removed
// first, and only an unqualified claim left standing fails.
const LOCK_COPY_DENIALS = /\b(?:not|never|no|isn't|is not|does not|doesn't|without)\s+(?:been\s+)?(?:encryption|encrypted|encrypt|secure|secured|protected)\b/g;
const LOCK_COPY_CLAIMS = ["encrypt", "secure", "protected", "safe from"];

export function meetingLockCopyIsHonest(text) {
  const value = String(text || "").toLowerCase().replace(LOCK_COPY_DENIALS, " ");
  return !LOCK_COPY_CLAIMS.some((forbidden) => value.includes(forbidden));
}

// What the meeting detail shows for one meeting's lock. `state` drives which
// affordance renders; nothing here decides access, which lives in Rust.
//
//   locked      -- the gate refused this open: show the barrier, offer to
//                  confirm, and render no meeting content at all.
//   unreadable  -- locked, and Yawn could not read the lock file. Same
//                  refusal, but the sentence must match the actions actually
//                  offered here: Confirm-to-open (read-only, same button as
//                  the "locked" case) and Remove-lock, reachable from Manage
//                  even on this barrier screen -- not "removing the lock is
//                  the only way," which was never true once Confirm-to-open
//                  sat right beside it.
//   unlocked    -- read normally. `lockable` is what the Manage menu offers.
//   open        -- locked, and open for reading because a confirmation was
//                  spent. Says so, because a reader who does not know the
//                  meeting is still locked cannot understand why playing its
//                  audio asks again.
export function meetingLockPresentation(note) {
  if (!note) return null;
  const lock = note.lock || {};
  if (note.state === "locked") {
    return {
      state: lock.unreadable ? "unreadable" : "locked",
      heading: "This meeting is locked",
      detail: lock.unreadable
        ? "Yawn could not read this meeting's lock, so it stays locked. Confirm to open it for reading, or remove the lock from Manage."
        : "Its note, transcript, and audio stay closed until Touch ID confirms it's you on this Mac. This is a local barrier, not encryption — the files on disk are unchanged.",
      action: { action: "unlock-meeting-open", label: "Confirm to open" },
    };
  }
  if (lock.locked) {
    return {
      state: "open",
      heading: "Open for now",
      detail: "This meeting stays locked. Exporting it or playing its audio asks for Touch ID again.",
      action: null,
    };
  }
  return { state: "unlocked", heading: "", detail: "", action: null };
}

// Refit R10: a sidebar row's length, m:ss under an hour and h:mm:ss at or
// above it. Pure so it is testable without a DOM; called only when a row
// actually carries a finite `durationSeconds` (see `libraryRowMetaPresentation`)
// -- older or not-yet-populated rows never reach it, so absent fields render
// exactly as before this packet.
export function durationLabel(totalSeconds) {
  const total = Math.max(0, Math.floor(Number(totalSeconds) || 0));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

// A locked row shows its title and its date. It shows no note preview -- the
// backend already withholds one -- and no transcript or note-only line, which
// is the detail Bear's obscured previews also drop. "Locked" replaces it, so
// the row still says something true about itself rather than going blank.
//
// Refit R10 (all-surfaces-2765401-installed-cold.md finding 3): a parallel
// Rust change adds two optional `LibrarySnapshotRow` fields this reads
// defensively -- `durationSeconds` (a finite number or absent/null) and
// `recovery` (a string state or absent/null). Neither field existing yet
// leaves `duration` and `needsAttention` at their same off defaults, so a
// row from a build that hasn't shipped the Rust side renders byte-identical
// to before this packet.
export function libraryRowMetaPresentation(row) {
  if (row?.locked) return { locked: true, label: "Locked", preview: null, duration: null, needsAttention: false };
  const durationSeconds = row?.durationSeconds;
  return {
    locked: false,
    label: row?.transcriptAvailable ? "transcript available" : "note only",
    preview: libraryRowPreview(row),
    duration: typeof durationSeconds === "number" && Number.isFinite(durationSeconds) ? durationLabel(durationSeconds) : null,
    needsAttention: ["recovered-interrupted", "needs-attention"].includes(row?.recovery),
  };
}

// DESIGN.md's sidebar row title: an operator-named or transcript-derived
// title stays as-is; a meeting with neither reads "Meeting · date" (never a
// raw transcript fragment as a title, and never blank).
export function sidebarRowTitle(row, dateLabelText) {
  const label = typeof row?.label === "string" ? row.label.trim() : "";
  return label || `Meeting · ${dateLabelText}`;
}

// Newest first. The backend's own row order is not a contract this frontend
// can rely on (no sort is documented on `library_snapshot`), so the sidebar
// derives its own order from the one timestamp every row already carries.
// Both the day-grouped sidebar and "the most recent meeting" (launch
// selection) read this same order, so they can never disagree with each
// other about which meeting is newest.
export function sortLibraryRows(rows) {
  return [...(Array.isArray(rows) ? rows : [])]
    .sort((a, b) => Number(b?.createdAtEpochSeconds || 0) - Number(a?.createdAtEpochSeconds || 0));
}

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfLocalDay(epochSeconds) {
  const d = new Date(Number(epochSeconds) * 1000);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

// DESIGN.md's sidebar groups: Today, Yesterday, Previous 7 days, Previous 30
// days, then a month label ("August 2026") for anything older. `now` is
// injectable so a day-boundary test can pin it rather than racing the clock.
export function sidebarGroupLabel(createdAtEpochSeconds, nowEpochSeconds = Date.now() / 1000) {
  if (!Number.isFinite(Number(createdAtEpochSeconds))) return "Previous 30 days";
  const diffDays = Math.round((startOfLocalDay(nowEpochSeconds) - startOfLocalDay(createdAtEpochSeconds)) / DAY_MS);
  if (diffDays <= 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays <= 7) return "Previous 7 days";
  if (diffDays <= 30) return "Previous 30 days";
  return new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" })
    .format(new Date(Number(createdAtEpochSeconds) * 1000));
}

const GROUP_ORDER = ["Today", "Yesterday", "Previous 7 days", "Previous 30 days"];

// Buckets library rows into DESIGN.md's day groups, newest group first, rows
// within a group newest first. Month buckets (anything older than 30 days)
// keep insertion order, which is recency order because the source rows are
// already sorted -- see `sortLibraryRows`.
export function sidebarGroups(rows, nowEpochSeconds = Date.now() / 1000) {
  const sorted = sortLibraryRows(rows);
  const byLabel = new Map();
  for (const row of sorted) {
    const label = sidebarGroupLabel(row.createdAtEpochSeconds, nowEpochSeconds);
    if (!byLabel.has(label)) byLabel.set(label, []);
    byLabel.get(label).push(row);
  }
  const monthLabels = [...byLabel.keys()].filter((label) => !GROUP_ORDER.includes(label));
  const order = [...GROUP_ORDER.filter((label) => byLabel.has(label)), ...monthLabels];
  return order.map((label) => ({ label, rows: byLabel.get(label) }));
}

// The toolbar's title area (DESIGN.md: "Yawn" / the selected meeting's title
// / "New Recording"). Recording (in-progress capture) always wins -- the
// title must say what the window is doing right now, not what was last
// selected underneath it.
export function toolbarTitlePresentation({ capturing = false, selectedTitle = "" } = {}) {
  if (capturing) return "New Recording";
  const title = typeof selectedTitle === "string" ? selectedTitle.trim() : "";
  return title || "Yawn";
}

// Maps one `authorize_locked_action`, `lock_meeting`, or `unlock_meeting`
// response onto what the reader is told. The three failure states are kept
// apart on purpose: "you cancelled", "this Mac cannot ask", and "that view is
// out of date" lead to three different next moves.
export function lockedActionOutcome(response) {
  const state = response?.state || "";
  const message = typeof response?.message === "string" ? response.message.trim() : "";
  if (["authorized", "locked", "unlocked"].includes(state)) {
    return { ok: true, state, message };
  }
  return { ok: false, state: state || "unavailable", message };
}

export function meetingContextPresentation(meetingContext) {
  if (meetingContext?.unreadable) return { state: "unreadable", text: "" };
  const text = typeof meetingContext?.text === "string" ? meetingContext.text.trim() : "";
  return { state: text ? "present" : "empty", text };
}

// Clipboard text is a portable reading copy, not a claim that the transcript
// is complete. A turn the voice check withheld still occupies a visible line,
// so pasting this elsewhere cannot silently turn a known gap into a seamless
// record.
export function transcriptPlainText(turns) {
  const rows = Array.isArray(turns) ? turns : [];
  const lines = [];
  for (const turn of rows) {
    const seconds = Math.max(0, Math.floor(Number(turn?.start) || 0));
    const stamp = `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
    if (turn?.withheld) {
      lines.push(`[${stamp}] (withheld — a voice check set this turn aside)`);
      continue;
    }
    const speaker = typeof turn?.speaker === "string" && turn.speaker.trim()
      ? turn.speaker.trim()
      : "Unattributed";
    const text = typeof turn?.text === "string" ? turn.text.trim() : "";
    if (!text) continue;
    lines.push(`[${stamp}] ${speaker}: ${text}`);
  }
  return lines.join("\n");
}

// The retained turn owns the attribution. Missing attribution stays explicit,
// and a withheld turn carries no speaker claim at all.
export function transcriptSpeakerLabel(turn) {
  if (turn?.withheld) return null;
  const speaker = typeof turn?.speaker === "string" ? turn.speaker.trim() : "";
  return speaker || "Unattributed";
}

export function transcriptTurnsForSourceSpeaker(turns, sourceSpeaker) {
  if (!Array.isArray(turns)) return [];
  const source = typeof sourceSpeaker === "string" ? sourceSpeaker : null;
  return turns.filter((turn) => !turn?.withheld && (turn?.sourceSpeaker || null) === source);
}

// Search is intentionally local to the retained text. A withheld turn remains
// visible in the full transcript, but it has no text that can truthfully match.
export function transcriptTurnsMatching(turns, query) {
  const rows = Array.isArray(turns) ? turns : [];
  const needle = String(query || "").trim().toLocaleLowerCase();
  if (!needle) return rows;
  return rows.filter((turn) => (
    !turn?.withheld
    && String(turn?.text || "").toLocaleLowerCase().includes(needle)
  ));
}

// A restore control is valid only for the exact meeting view that supplied
// the digest and row index. The backend repeats these checks authoritatively;
// this gate keeps stale or incomplete browser state from creating a request.
export function withheldTurnPresentation(
  turn,
  {
    meetingId = "",
    meetingHandle = "",
    transcriptMeetingId = "",
    transcriptSha256 = "",
    capture = "idle",
  } = {},
) {
  const sourceTurnIndex = Number(turn?.sourceTurnIndex);
  const digest = typeof transcriptSha256 === "string" ? transcriptSha256.trim() : "";
  const valid = Boolean(turn?.withheld)
    && typeof meetingId === "string"
    && meetingId.length > 0
    && typeof meetingHandle === "string"
    && meetingHandle.length > 0
    && transcriptMeetingId === meetingId
    && /^[a-f0-9]{64}$/i.test(digest)
    && Number.isInteger(sourceTurnIndex)
    && sourceTurnIndex >= 0
    && capture === "idle";
  return valid
    ? { action: "restore-withheld-turn", label: "Restore this turn", sourceTurnIndex }
    : null;
}

// Vocabulary is a review-layer control, not an app-wide setting. The browser
// only opens it from a stable, retained meeting projection; the command repeats
// this check before it reads or changes the local store.
export function localVocabularyPresentation({
  meetingId = "",
  transcriptMeetingId = "",
  transcriptSha256 = "",
  capture = "idle",
} = {}) {
  const digest = typeof transcriptSha256 === "string" ? transcriptSha256.trim() : "";
  const valid = typeof meetingId === "string"
    && meetingId.length > 0
    && transcriptMeetingId === meetingId
    && /^[a-f0-9]{64}$/i.test(digest)
    && capture === "idle";
  return valid
    ? { action: "open-vocabulary", label: "Vocabulary", meetingId, sourceTranscriptSha256: digest }
    : null;
}

// Transcript retry is available only from the exact, retained transcript the
// reader is already reviewing. A pending candidate is still bound to that
// source; it is not an instruction to promote anything automatically.
export function transcriptRetryPresentation({
  meetingId = "",
  transcriptMeetingId = "",
  sourceTranscriptSha256 = "",
  audioRetentionState = "",
  capture = "",
  recovery = null,
  pending = null,
} = {}) {
  const digest = typeof sourceTranscriptSha256 === "string" ? sourceTranscriptSha256.trim() : "";
  const eligible = typeof meetingId === "string"
    && meetingId.length > 0
    && transcriptMeetingId === meetingId
    && /^[a-f0-9]{64}$/i.test(digest)
    && audioRetentionState === "retained"
    && capture === "idle"
    && !recovery;
  if (!eligible) return null;

  const resumesPending = pending
    && pending.meetingId === meetingId
    && pending.sourceTranscriptSha256 === digest
    && typeof pending.operationId === "string"
    && pending.operationId.length > 0
    && typeof pending.candidateTranscriptSha256 === "string"
    && /^[a-f0-9]{64}$/i.test(pending.candidateTranscriptSha256);
  return {
    action: "start-transcript-retry",
    label: resumesPending ? "Review retry" : "Retry transcript",
    meetingId,
    sourceTranscriptSha256: digest,
    pending: resumesPending ? pending : null,
  };
}

// The read projection uses an array so each observation carries its own
// human-readable kind. Keep map tolerance for an older local candidate while
// normalizing the renderer to one small, content-safe shape.
export function transcriptRetryQualityPresentation(quality = null) {
  const state = typeof quality?.state === "string" && quality.state ? quality.state : "unavailable";
  const message = typeof quality?.message === "string" && quality.message
    ? quality.message
    : "Capture-quality details are unavailable for this retry.";
  const observations = Array.isArray(quality?.observations)
    ? quality.observations.map((observation) => ({
      kind: transcriptRetryQualityKindLabel(observation?.kind),
      detail: observation?.message || observation?.detail || observation?.status || "Observed",
    }))
    : quality?.observations && typeof quality.observations === "object"
      ? Object.entries(quality.observations).map(([kind, observation]) => ({
        kind: transcriptRetryQualityKindLabel(kind),
        detail: observation?.message || observation?.detail || observation?.status || observation || "Observed",
      }))
      : [];
  return { state, message, observations };
}

// D6 (design intake): the retry comparison must show *what* differs between
// the two transcripts before the keep/promote choice, not just that a retry
// exists. The word-level spans themselves come from the Rust
// `transcript_retry_diff` module; these two functions turn that raw payload
// into what the two columns render.

const RETRY_DIFF_LEGEND = "Highlights show where the transcripts differ.";
const RETRY_DIFF_SKIPPED_MESSAGE = "These transcripts are too long to highlight word differences.";
const RETRY_DIFF_IDENTICAL_MESSAGE = "No word-level differences found.";

// Keyed by the turn's position in that side's `turns` array (not
// `sourceTurnIndex`), matching how the diff was built against that same
// array order. Every visible turn gets an entry — `wordCount` is the count
// Rust's tokenizer used to build `spans`, present even when spans is empty
// (identical turn), because retryTurnDiffSegments needs it to check its own
// tokenizer regardless of whether anything differs.
function retryDiffSpansByTurn(side) {
  const map = new Map();
  if (!Array.isArray(side)) return map;
  for (const entry of side) {
    if (!entry || typeof entry.turnIndex !== "number") continue;
    map.set(entry.turnIndex, {
      wordCount: typeof entry.wordCount === "number" ? entry.wordCount : null,
      spans: Array.isArray(entry.spans) ? entry.spans : [],
    });
  }
  return map;
}

// Anything other than an explicit "computed" state is treated the same as
// "skipped": a diff that never ran must never be read as "these transcripts
// are identical". A turn with no differences still gets a (spans-empty)
// entry, so "differences exist" is decided by scanning every entry's spans,
// not by whether the side has any entries at all.
export function transcriptRetryDiffPresentation(diff = null) {
  const computed = diff?.state === "computed";
  const current = retryDiffSpansByTurn(diff?.current);
  const candidate = retryDiffSpansByTurn(diff?.candidate);
  const hasSpans = [...current.values(), ...candidate.values()].some((entry) => entry.spans.length > 0);
  const legend = !computed
    ? RETRY_DIFF_SKIPPED_MESSAGE
    : hasSpans
      ? RETRY_DIFF_LEGEND
      : RETRY_DIFF_IDENTICAL_MESSAGE;
  return { computed, legend, current, candidate };
}

// Splits one turn's text into the same word/separator runs the Rust side
// tokenized against (a "word" is a maximal run of non-whitespace characters),
// then marks each word segment highlighted when its word index falls inside
// any of the given spans. Whitespace segments are never highlighted and are
// rendered exactly as they appeared, so the visible spacing is unchanged.
//
// Fail-safe: `entry.wordCount` is the word count Rust's tokenizer counted
// for this same text. If this function's own count disagrees (an unusual
// whitespace character splitting differently here than in Rust's
// `str::split_whitespace`, for example), every span's word indices are
// suspect — the turn renders with no highlights rather than risk marking
// the wrong words, which this packet's own honesty rule treats as worse
// than not highlighting at all.
export function retryTurnDiffSegments(text, entry = null) {
  const source = typeof text === "string" ? text : "";
  if (!source) return [];
  const spans = Array.isArray(entry?.spans) ? entry.spans : [];
  const expectedWordCount = typeof entry?.wordCount === "number" ? entry.wordCount : null;

  const parts = source.split(/(\s+)/);
  const segments = [];
  let wordCount = 0;
  for (let i = 0; i < parts.length; i += 1) {
    const part = parts[i];
    if (part === "") continue;
    const isSeparator = i % 2 === 1;
    if (isSeparator) {
      segments.push({ text: part, wordIndex: -1 });
      continue;
    }
    segments.push({ text: part, wordIndex: wordCount });
    wordCount += 1;
  }

  const tokenizationAgrees = expectedWordCount !== null && wordCount === expectedWordCount;
  return segments.map(({ text: segmentText, wordIndex }) => ({
    text: segmentText,
    highlighted: wordIndex >= 0
      && tokenizationAgrees
      && spans.some((span) => wordIndex >= span?.startWord && wordIndex < span?.endWord),
  }));
}

// The pause control the operator sees beside Stop.
//
// The reducer is only moved once the capture helper confirms the change, so a
// request in flight is its own label rather than a state claim: the surface
// never says "Paused" over a microphone that is still open, or "Recording" over
// one that has not reopened yet.
export function capturePauseControlPresentation(snapshot) {
  const capture = snapshot?.capture;
  if (capture !== "recording" && capture !== "paused") return null;
  const pending = snapshot?.capture_pause_change_pending === true;
  if (capture === "recording") {
    return {
      action: "pause-recording",
      label: pending ? "Pausing…" : "Pause",
      disabled: pending,
    };
  }
  return {
    action: "resume-recording",
    label: pending ? "Resuming…" : "Resume",
    disabled: pending,
  };
}

// Whether a finished meeting has a gap in its audio, for the surface that
// already shows capture evidence.
//
// A receipt written before pause existed describes a recording that could not
// have been paused, so it reads as uninterrupted rather than as unknown. The
// native projection authors the sentence; this keeps the mapping closed and
// never invents one from a state it does not recognize.
export function capturePausePresentation(pauses = null) {
  const state = typeof pauses?.state === "string" ? pauses.state : "unavailable";
  if (state === "not-paused") {
    return {
      state,
      title: "Recording was not paused",
      detail: "The retained audio for this meeting runs without a gap.",
    };
  }
  if (state === "paused") {
    return {
      state,
      title: "Recording was paused",
      detail: typeof pauses?.message === "string" && pauses.message
        ? pauses.message
        : "This recording was paused, so its audio has a gap.",
    };
  }
  return {
    state: "unavailable",
    title: "Pauses could not be checked",
    detail: "Yawn could not verify whether this recording was paused.",
  };
}

// The native projection deliberately withholds device names and metadata. Keep
// this mapping closed too: the UI accepts only the state and action token, then
// supplies its own copy instead of rendering receipt-derived text.
export function recordingDevicePresentation(device = null) {
  if (device?.state === "identified") {
    return {
      state: "identified",
      title: "Recording device recorded",
      detail: "Yawn verified that a microphone identity was recorded for this meeting. This does not confirm it was the audio input you intended to use.",
      action: null,
    };
  }
  return {
    state: "unknown",
    title: "Recording device not verified",
    detail: "Yawn could not verify which microphone identity was recorded for this meeting.",
    action: device?.nextAction === "check-audio-input"
      ? { action: "open-settings", label: "Check audio input" }
      : null,
  };
}

export function transcriptRetryQualityKindLabel(kind) {
  const labels = {
    silence: "Silence",
    clipping: "Clipping",
    "low-input": "Low input",
    "background-noise": "Background noise",
  };
  return labels[kind] || "Observation";
}

// The generate control renders only from the note response's own eligibility
// signal — the backend includes the source pin exactly when the facade would
// admit the operation, so the browser never re-derives lifecycle rules. The
// copy states the honest costs: it runs locally, and it takes minutes.
export function noteGenerationPresentation(note, generatingMeetingId) {
  if (!note?.regenerationSourceSha256 || !note?.meetingId) return null;
  const generating = generatingMeetingId === note.meetingId;
  const replacing = Array.isArray(note?.claims) && note.claims.length > 0;
  return {
    action: "generate-note",
    label: generating ? "Generating note…" : replacing ? "Regenerate note" : "Generate note",
    disabled: generating,
    help: generating
      ? "The note model is reading this transcript on your Mac. This can take several minutes — you can keep using Yawn."
      : replacing
        ? "Runs the downloaded note model again on this Mac. Your current note stays in place unless a replacement passes every check."
      : "Runs the downloaded note model on this Mac. It usually takes several minutes, longer for long meetings. Nothing leaves your computer.",
  };
}

// Keep recovery copy at the same evidence boundary as the library response.
// A source pin is the only browser-visible proof that note regeneration can
// run. Transcript and audio states are read-only facts: do not invent a retry
// control for either one.
export function meetingRecoveryPresentation(note, transcript, generatingMeetingId = "") {
  const noteState = note?.state || "";
  const meetingId = note?.meetingId || "";
  const hasSource = typeof note?.regenerationSourceSha256 === "string"
    && note.regenerationSourceSha256.trim().length > 0
    && Boolean(meetingId);
  const transcriptState = transcript?.state || "";
  const transcriptUnavailable = ["stale", "unavailable"].includes(transcriptState);
  const generating = Boolean(meetingId) && generatingMeetingId === meetingId;
  const hasUsableNote = Array.isArray(note?.claims) && note.claims.length > 0;

  if (generating) {
    return {
      state: "generating",
      tone: "working",
      title: "Preparing your meeting note.",
      detail: hasUsableNote
        ? "Yawn is trying again. Your current note stays in place until a replacement passes every check."
        : "Yawn is trying again. Your transcript stays available while the note is prepared.",
      action: null,
    };
  }

  if (transcriptUnavailable) {
    return {
      state: "transcript-unavailable",
      tone: "attention",
      title: "The transcript is unavailable.",
      // Refit R9 (all-surfaces-2765401-installed-cold.md finding 2): this
      // detail used to say "Reopen Meetings to try again" while the one
      // button on screen says "Back to meetings" -- name the control that
      // actually exists instead of an instruction with no matching control.
      detail: typeof transcript?.message === "string" && transcript.message.trim()
        ? transcript.message.trim()
        : "Yawn could not load this meeting’s transcript. Go back to meetings and open it again.",
      action: { action: "meetings", label: "Back to meetings" },
    };
  }

  if (noteState === "summary-failed") {
    if (hasSource) {
      return {
        state: "summary-failed",
        tone: "attention",
        title: "Your meeting note needs another try.",
        detail: "Yawn could not create a note. Your transcript and current note remain unchanged.",
        action: { action: "generate-note", label: "Regenerate note" },
      };
    }
    return {
      state: "summary-failed-no-source",
      tone: "attention",
      title: "Your meeting note could not be created.",
      detail: "No usable transcript source remains, so Yawn cannot retry. Any note or transcript already shown stays unchanged.",
      action: null,
    };
  }

  // Audio retention matters for retranscription, not for reading or
  // regenerating a note from its transcript. Keep this warning even when a
  // finished note is present; the renderer leaves its note action available.
  if (note?.audioRetention?.state === "released") {
    return {
      state: "audio-released",
      tone: "attention",
      title: "The recording is no longer available.",
      detail: "The audio was already deleted. The transcript and note remain available, but this meeting cannot be retranscribed.",
      action: null,
    };
  }

  if (noteState === "stale" || noteState === "unavailable") {
    return {
      state: "meeting-unavailable",
      tone: "attention",
      title: "This meeting is unavailable.",
      // Refit R9: same fix as transcript-unavailable above -- the detail
      // names the button that is actually on screen ("Back to meetings"),
      // not an instruction ("Reopen Meetings") that matches no control. Does
      // not name Move to Trash: that secondary action only appears when the
      // meeting has a deletion handle (renderMeetingPane), so this detail
      // stays true regardless of which buttons the pane actually offers.
      detail: "Yawn could not read this meeting. Nothing already saved here was replaced. Go back to meetings to try it again.",
      action: { action: "meetings", label: "Back to meetings" },
    };
  }

  return null;
}

// Refit R9 (all-surfaces-2765401-installed-cold.md finding 2): the toolbar
// title used to keep naming a meeting that had just failed to load, directly
// contradicting the needs-attention pane's own message underneath it. This
// is the one fact both `render()`'s title computation and `renderMeetingPane`
// need to agree on -- "is there nothing readable about this meeting" -- so it
// is factored out once here rather than kept as two copies that could drift.
// Same readability check `renderMeetingPane` used inline before this packet:
// a recovered-interrupted meeting with retained audio, or any meeting with
// transcript turns or the operator's own notes, is content plus a fact (the
// workspace renders with a caption), not a full-pane replacement.
export function meetingBlockingRecovery(note, transcript, generatingMeetingId = "") {
  const recovery = meetingRecoveryPresentation(note, transcript, generatingMeetingId);
  if (!recovery || recovery.state === "audio-released") return null;
  const readable = Boolean(
    transcript?.turns?.length
    || note?.microphonePlaybackHandle
    || note?.systemPlaybackHandle
    || note?.operatorNote?.text
    || note?.meetingDeletionHandle
    || note?.operatorNoteHandle,
  );
  return readable ? null : recovery;
}

// Roadmap intake I2: the global hotkey summons the operator-note editor and
// always places the caret at the end of whatever is already written, never
// mid-text. This is the one pure fact in that path — where the caret and
// selection land, given the current text — so it is the one part of the
// hotkey's frontend handling exercised by a headless test. The DOM focus
// call itself needs a live textarea and is not covered here.
export function noteCaptureFocusSelection(text) {
  const end = typeof text === "string" ? text.length : 0;
  return { start: end, end, direction: "forward" };
}

// Roadmap intake I9: local trash for whole-meeting deletion. Trash is a
// quiet secondary list, not a dashboard — it shows only as a "Trash (N)"
// link when non-empty, and disappears entirely once empty rather than
// leaving an empty-state row on the home screen.
export function trashLinkPresentation(trash) {
  const count = trash?.entries?.length || 0;
  if (!count) return null;
  return { count, label: `Trash (${count})` };
}

// The three states the quiet Trash view itself can be in. Loading and empty
// are both plain text — no empty-state artwork, per the governing
// constraint. `entries` carries a fallback label so a never-titled meeting
// still reads as something in the list, not a bare identifier.
export function trashListPresentation(trash) {
  if (!trash) {
    return { state: "loading", entries: [] };
  }
  const entries = (trash.entries || []).map((entry) => ({
    meetingId: entry.meetingId,
    label: entry.label && entry.label.trim() ? entry.label : `Meeting · ${String(entry.meetingId || "").slice(0, 8)}`,
    deletedAtEpochSeconds: entry.deletedAtEpochSeconds,
    purgeAfterEpochSeconds: entry.purgeAfterEpochSeconds,
  }));
  return { state: entries.length ? "populated" : "empty", entries };
}

// The exact confirmation copy for each of the three destructive-looking
// meeting actions. Only whole-meeting deletion changed: recording and
// transcript deletion stay immediate and permanent (roadmap intake I9's
// scope boundary), so their copy is unchanged; deleting a meeting now states
// the trash-and-recovery truth plainly instead of claiming permanence it no
// longer has at the moment of the click.
// Design intake D5: evidence disclosure's three depths (hover preview, split
// view, synced scroll). These are pure presentation/decision helpers only --
// DOM creation, geometry measurement, and scheduling stay in main.js, which
// hands each function already-measured input.

// Depth 1's hover/keyboard-focus preview. `claim.spans` (batched onto the
// note response -- see `library_reader.rs`'s `LibraryClaim.spans`) already
// carries every locator's quoted transcript text, so this never triggers a
// fetch. Only the first span previews: a claim citing several turns still
// shows one exact position, matching split view's own "first cited span"
// landing rule (decision 3) rather than a second, different behavior for
// hover.
export function evidencePopoverPresentation(claim, transcriptTurns) {
  const span = claim?.spans?.[0];
  if (!span || typeof span.text !== "string" || !span.text.trim()) return null;
  const turns = Array.isArray(transcriptTurns) ? transcriptTurns : [];
  const turn = turns.find((candidate) => Number(candidate?.sourceTurnIndex) === Number(span.sourceTurnIndex));
  return {
    text: span.text,
    speaker: transcriptSpeakerLabel(turn) || "Unattributed",
    start: Number.isFinite(Number(turn?.start)) ? Number(turn.start) : null,
    sourceTurnIndex: span.sourceTurnIndex,
  };
}

// Depth 2's width gate (design decision 3): below this, the split cramps two
// reading columns into too little room and the surface falls back to the
// existing below-the-note disclosure instead.
const EVIDENCE_SPLIT_MIN_WINDOW_WIDTH = 1100;

export function evidenceSplitAllowed(windowWidth) {
  return Number.isFinite(Number(windowWidth)) && Number(windowWidth) >= EVIDENCE_SPLIT_MIN_WINDOW_WIDTH;
}

// Depth 3's sync target: given the note column's currently visible claims
// (topmost first -- a geometry read the caller already performed) and the
// note's claims, the transcript column tracks the first cited span of the
// first visible claim that has one. A claim with no locatable span (no
// `spans`) is skipped rather than stalling the scan, so a run of source-free
// claims at the top of the viewport does not freeze the target on nothing.
export function evidenceSyncTarget(visibleOrdinals, claims) {
  const ordinals = Array.isArray(visibleOrdinals) ? visibleOrdinals : [];
  const claimList = Array.isArray(claims) ? claims : [];
  for (const ordinal of ordinals) {
    const claim = claimList.find((candidate) => Number(candidate?.ordinal) === Number(ordinal));
    const span = claim?.spans?.[0];
    if (span && Number.isInteger(span.sourceTurnIndex)) return span.sourceTurnIndex;
  }
  return null;
}

// Depth 3's suspension rule: "manual scroll in the transcript column
// suspends sync until the reader next scrolls the note." A programmatic
// sync-scroll (which may animate under `behavior: "smooth"`) fires several
// scroll events of its own; comparing each one's position against a
// continuously-updated "last known good" value can misfire mid-animation.
// A bounded time window sidesteps that: every sync-scroll call marks
// "ours" for a fixed duration that covers its own animation, and any
// transcript scroll event observed after that window is the reader's own.
export function isManualTranscriptScroll(nowMs, syncSuppressedUntilMs) {
  return !Number.isFinite(Number(syncSuppressedUntilMs)) || Number(nowMs) > Number(syncSuppressedUntilMs);
}

// Escape's precedence across the three depths plus the app's existing modal
// sheets: the innermost, most transient surface closes first. A popover
// closing under a reader's cursor while a sheet stays open behind it is the
// "no fighting the reader" rule read backwards.
export function nextEscapeTarget({ popoverOpen = false, modalOpen = false, splitOpen = false } = {}) {
  if (popoverOpen) return "popover";
  if (modalOpen) return "modal";
  if (splitOpen) return "split";
  return null;
}

export function meetingDeletionConfirmationCopy(kind) {
  if (kind === "delete-recording") {
    return {
      eyebrow: "Permanent deletion",
      heading: "Delete this recording?",
      detail: "This permanently removes the saved microphone and system audio from this Mac. The transcript and your personal notes stay.",
      label: "Delete recording",
    };
  }
  if (kind === "delete-transcript") {
    return {
      eyebrow: "Permanent deletion",
      heading: "Delete this transcript?",
      detail: "This permanently removes the transcript and generated points from this Mac. Any recording and your personal notes stay.",
      label: "Delete transcript",
    };
  }
  return {
    eyebrow: "Moves to Trash",
    heading: "Delete this meeting?",
    detail: "This meeting moves to Trash. You can restore it from there for 30 days. After that, Yawn removes it permanently — there is no server copy, so it cannot be recovered.",
    label: "Delete meeting",
  };
}
