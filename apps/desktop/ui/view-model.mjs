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
