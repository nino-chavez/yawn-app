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
    eyebrow: "Transcribing locally",
    title: "Making your transcript.",
    detail: "This can take a moment. Your own notes remain available below.",
    tone: "working",
  },
  summarizing: {
    eyebrow: "Preparing notes",
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
    label: "Transcribing locally",
    detail: "The captured audio is saved. Yawn is making the transcript on this Mac.",
    tone: "working",
  },
  summarizing: {
    label: "Preparing meeting notes",
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
      label: "Earlier meetings are processing locally.",
      detail: "Yawn will accept the next recording after one finishes processing. Nothing has been discarded.",
      canStart: false,
    };
  }
  return {
    state: "active",
    label: "An earlier meeting is processing locally.",
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

// Backend commands currently return user-facing errors as text, not tagged
// classes. Map only their exact, stable recovery responses. A generic "try
// again" can describe any operation, so it must remain a plain error.
export function errorRecoveryPresentation(error, { hasSelectedMeeting = false } = {}) {
  const message = String(error instanceof Error ? error.message : error || "")
    .replace(/^Error:\s*/, "")
    .trim() || "Yawn could not complete that action.";
  if (message === "That view is no longer current. Reopen it and try again.") {
    return {
      message,
      action: hasSelectedMeeting
        ? { action: "refresh-selected-meeting", label: "Refresh this meeting" }
        : { action: "refresh-library", label: "Check again" },
    };
  }
  if ([
    "The local library is unavailable. Reopen the app and try again.",
    "The local Preview library is unavailable. Reopen the app and try again.",
    "The local meeting library is unavailable. Reopen the app and try again.",
  ].includes(message)) {
    return { message, action: { action: "refresh-library", label: "Check again" } };
  }
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
  if (hasSelectedMeeting && selectedMeetingRecoveryMessages.includes(message)) {
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

function retryDiffSpansByTurn(side) {
  const map = new Map();
  if (!Array.isArray(side)) return map;
  for (const entry of side) {
    if (!entry || typeof entry.turnIndex !== "number") continue;
    map.set(entry.turnIndex, Array.isArray(entry.spans) ? entry.spans : []);
  }
  return map;
}

// Anything other than an explicit "computed" state is treated the same as
// "skipped": a diff that never ran must never be read as "these transcripts
// are identical". `current`/`candidate` are Maps keyed by the turn's position
// in that side's `turns` array (not `sourceTurnIndex`), matching how the diff
// was built against that same array order.
export function transcriptRetryDiffPresentation(diff = null) {
  const computed = diff?.state === "computed";
  const current = retryDiffSpansByTurn(diff?.current);
  const candidate = retryDiffSpansByTurn(diff?.candidate);
  const hasSpans = current.size > 0 || candidate.size > 0;
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
export function retryTurnDiffSegments(text, spans = []) {
  const source = typeof text === "string" ? text : "";
  const ranges = Array.isArray(spans) ? spans : [];
  if (!source) return [];
  const parts = source.split(/(\s+)/);
  const segments = [];
  let wordIndex = 0;
  for (let i = 0; i < parts.length; i += 1) {
    const part = parts[i];
    if (part === "") continue;
    const isSeparator = i % 2 === 1;
    if (isSeparator) {
      segments.push({ text: part, highlighted: false });
      continue;
    }
    const thisWord = wordIndex;
    wordIndex += 1;
    const highlighted = ranges.some((span) => thisWord >= span?.startWord && thisWord < span?.endWord);
    segments.push({ text: part, highlighted });
  }
  return segments;
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
      detail: typeof transcript?.message === "string" && transcript.message.trim()
        ? transcript.message.trim()
        : "Yawn could not load this meeting’s transcript. Reopen Meetings to try this meeting again.",
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
      detail: "Yawn could not read this meeting. Nothing already saved here was replaced. Reopen Meetings to try again.",
      action: { action: "meetings", label: "Back to meetings" },
    };
  }

  return null;
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
