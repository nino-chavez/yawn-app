import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  backgroundTranscriptionPresentation,
  canOpenStart,
  canStartMeeting,
  captureActivity,
  captureActivityElapsedSeconds,
  captureIsInProgress,
  capturePresentation,
  errorRecoveryPresentation,
  humanize,
  libraryRecoveryPresentation,
  localVocabularyPresentation,
  meetingRecoveryPresentation,
  meetingNotePresentation,
  mergePermissions,
  noteCaptureFocusSelection,
  noteGenerationPresentation,
  permissionSummary,
  recordingDevicePresentation,
  retainedAudioPlaybackPresentation,
  retentionLabel,
  shouldPollSnapshot,
  transcriptPlainText,
  transcriptRetryQualityPresentation,
  transcriptRetryQualityKindLabel,
  transcriptSpeakerLabel,
  transcriptRetryPresentation,
  transcriptTurnsForSourceSpeaker,
  transcriptTurnsMatching,
  transcriptionWorkerHeartbeatAgeSeconds,
  withheldTurnPresentation,
} from "./view-model.mjs";

test("capture states lead with the actual next condition", () => {
  assert.equal(capturePresentation({ capture: "recording" }).title, "Stay in the conversation.");
  const transcriptReady = capturePresentation({ capture: "transcript-ready" });
  assert.equal(transcriptReady.tone, "complete");
  assert.match(transcriptReady.detail, /already saved on this Mac/);
  assert.equal(capturePresentation({ capture: "not-a-state" }).tone, "attention");
  assert.equal(captureIsInProgress({ capture: "transcribing" }), true);
  assert.equal(captureIsInProgress({ capture: "transcript-ready" }), false);
});

test("activity reports a real phase duration without inventing progress", () => {
  const snapshot = {
    capture: "transcribing",
    capture_state_started_at_epoch_seconds: 100,
    transcription_last_worker_heartbeat_at_epoch_seconds: 160,
  };
  assert.equal(captureActivity(snapshot).label, "Transcribing locally");
  assert.equal(captureActivityElapsedSeconds(snapshot, 163), 63);
  assert.equal(transcriptionWorkerHeartbeatAgeSeconds(snapshot, 163), 3);
  assert.equal(captureActivityElapsedSeconds({ capture: "transcript-ready" }, 163), null);
  assert.equal(transcriptionWorkerHeartbeatAgeSeconds({ capture: "recording" }, 163), null);
});

test("recording starts only from the actual ready-and-idle state", () => {
  assert.equal(canStartMeeting({ startup: "ready", capture: "idle" }), true);
  assert.equal(canStartMeeting({ startup: "checking", capture: "idle" }), false);
  assert.equal(canStartMeeting({ startup: "ready", capture: "recording" }), false);
  assert.equal(canStartMeeting({ startup: "ready", capture: "idle", background_transcription_active: true, background_transcription_queued_count: 1 }), true);
  assert.equal(canStartMeeting({ startup: "ready", capture: "idle", background_transcription_active: true, background_transcription_queued_count: 2 }), false);
});

test("background transcription uses fixed private status and exposes capacity", () => {
  assert.deepEqual(backgroundTranscriptionPresentation({
    background_transcription_active: true,
    background_transcription_queued_count: 1,
  }), {
    state: "active",
    label: "An earlier meeting is processing locally.",
    detail: "You can start the next meeting when the recorder is ready.",
    canStart: true,
  });
  const full = backgroundTranscriptionPresentation({ background_transcription_queued_count: 2 });
  assert.equal(full.state, "full");
  assert.equal(full.canStart, false);
  assert.match(full.detail, /Nothing has been discarded/);
  assert.doesNotMatch(`${full.label} ${full.detail}`, /meeting[_ -]?id|transcript|audio text|model|path|receipt/i);
  assert.equal(backgroundTranscriptionPresentation({}), null);
});

test("the recording sheet waits for both audio sources", () => {
  const ready = { startup: "ready", capture: "idle" };
  assert.equal(canOpenStart(ready, { microphone: "authorized", systemAudio: "authorized" }), true);
  assert.equal(canOpenStart(ready, { microphone: "authorized", systemAudio: "unmeasured" }), false);
  assert.equal(canOpenStart({ startup: "checking", capture: "idle" }, { microphone: "authorized", systemAudio: "authorized" }), false);
});

test("permission checks retain an initial unmeasured system-audio state", () => {
  const firstCheck = mergePermissions(null, { microphone: "not-determined", systemAudio: "unmeasured" });
  assert.equal(firstCheck.systemAudio, "unmeasured");
  const microphoneApproved = mergePermissions(firstCheck, { microphone: "authorized", systemAudio: "unmeasured" });
  assert.equal(microphoneApproved.microphone, "authorized");
  assert.equal(microphoneApproved.systemAudio, "unmeasured");
  const audioApproved = mergePermissions(microphoneApproved, { microphone: "unmeasured", systemAudio: "authorized" });
  assert.equal(audioApproved.microphone, "authorized");
  assert.equal(audioApproved.systemAudio, "authorized");
});

test("audio setup does not confuse unknown with permission denied", () => {
  assert.equal(permissionSummary(null).state, "checking");
  assert.equal(permissionSummary({ probeUnavailable: true }).title, "Audio access could not be checked");
  assert.equal(permissionSummary({ microphone: "denied", systemAudio: "unmeasured" }).title, "Microphone access is needed");
  assert.equal(permissionSummary({ microphone: "authorized", systemAudio: "unmeasured" }).title, "Allow system audio");
  assert.equal(permissionSummary({ microphone: "authorized", systemAudio: "authorized" }).state, "ready");
});

test("small display helpers stay readable", () => {
  assert.equal(retentionLabel(1), "1 day");
  assert.equal(retentionLabel(7), "7 days");
  assert.equal(humanize("evidence_state"), "Evidence State");
});

test("retained audio playback keeps sources separate and disappears outside a normal retained detail", () => {
  const note = {
    state: "note",
    audioRetention: { state: "retained" },
    microphonePlaybackHandle: "opaque-mic",
    systemPlaybackHandle: "opaque-system",
  };
  const idle = retainedAudioPlaybackPresentation(note, null, { state: "idle" });
  assert.deepEqual(idle.controls.map((control) => control.label), ["Play microphone", "Play system audio"]);
  const playing = retainedAudioPlaybackPresentation(note, null, { state: "playing", source: "system" });
  assert.equal(playing.isPlaying, true);
  assert.equal(playing.playingSource, "system");
  assert.deepEqual(playing.controls, []);
  assert.equal(retainedAudioPlaybackPresentation({ ...note, audioRetention: { state: "released" } }, null), null);
  assert.equal(retainedAudioPlaybackPresentation(note, { state: "recovery" }), null);
});

test("a point-only artifact is not presented as a meeting summary", () => {
  const point = { ordinal: 0, claimType: "point", claim: "A selected excerpt." };
  assert.deepEqual(meetingNotePresentation({ claims: [point] }), {
    state: "extracts-only",
    summary: [],
    groups: [],
    highlights: [point],
  });
});

test("a generated meeting note leads with summary and outcome groups", () => {
  const decision = { ordinal: 0, claimType: "decision", claim: "Use the smaller battery." };
  const action = { ordinal: 1, claimType: "action", claim: "Send the cost table." };
  const presentation = meetingNotePresentation({
    claims: [
      { ordinal: 0, claimType: "summary", claim: "The group chose the smaller battery and assigned the cost follow-up." },
      decision,
      action,
    ],
  });
  assert.equal(presentation.state, "note");
  assert.deepEqual(presentation.summary, [
    { ordinal: 0, claimType: "summary", claim: "The group chose the smaller battery and assigned the cost follow-up." },
  ]);
  assert.deepEqual(presentation.groups.map((group) => group.title), ["Decisions", "Follow-ups"]);
  assert.deepEqual(presentation.highlights, []);
});

test("a copied transcript keeps known gaps visible", () => {
  const copied = transcriptPlainText([
    { start: 0, speaker: "Me", text: "we agreed to defer the migration" },
    { start: 63, withheld: true },
    { start: 125, speaker: "Them", text: "  send the numbers Friday  " },
    { start: 130, speaker: "Them", text: "   " },
    { start: 140, text: "no speaker recorded" },
  ]);
  assert.deepEqual(copied.split("\n"), [
    "[00:00] Me: we agreed to defer the migration",
    "[01:03] (withheld — a voice check set this turn aside)",
    "[02:05] Them: send the numbers Friday",
    "[02:20] Unattributed: no speaker recorded",
  ]);
  assert.equal(transcriptPlainText([]), "");
  assert.equal(transcriptPlainText(null), "");
});

test("the rendered transcript names known and missing attribution without guessing", () => {
  assert.equal(transcriptSpeakerLabel({ speaker: "Me" }), "Me");
  assert.equal(transcriptSpeakerLabel({ speaker: "  Them  " }), "Them");
  assert.equal(transcriptSpeakerLabel({ speaker: "Facilitator" }), "Facilitator");
  assert.equal(transcriptSpeakerLabel({}), "Unattributed");
  assert.equal(transcriptSpeakerLabel({ speaker: "   " }), "Unattributed");
  assert.equal(transcriptSpeakerLabel({ speaker: "Me", withheld: true }), null);
});

test("speaker correction targets only the matching retained source group", () => {
  const turns = [
    { sourceSpeaker: "Me", speaker: "Me" },
    { sourceSpeaker: "Them", speaker: "Alex", speakerCorrected: true },
    { sourceSpeaker: "Them", speaker: "Alex", speakerCorrected: true },
    { sourceSpeaker: null, speaker: null },
    { sourceSpeaker: "Them", speaker: null, withheld: true },
  ];
  assert.equal(transcriptTurnsForSourceSpeaker(turns, "Them").length, 2);
  assert.equal(transcriptTurnsForSourceSpeaker(turns, null).length, 1);
  assert.equal(transcriptTurnsForSourceSpeaker(turns, "Me").length, 1);
});

test("transcript search only matches retained text", () => {
  const turns = [
    { text: "Decide the release date" },
    { text: "This turn cannot be searched", withheld: true },
    { text: "Send the release recap" },
  ];
  assert.deepEqual(transcriptTurnsMatching(turns, "RELEASE"), [turns[0], turns[2]]);
  assert.deepEqual(transcriptTurnsMatching(turns, ""), turns);
  assert.deepEqual(transcriptTurnsMatching(null, "release"), []);
});

test("a withheld turn gets one restore action only in the current idle meeting", () => {
  const context = {
    meetingId: "m-1",
    meetingHandle: "meeting-handle",
    transcriptMeetingId: "m-1",
    transcriptSha256: "a".repeat(64),
    capture: "idle",
  };
  const turn = { sourceTurnIndex: 3, withheld: true };
  assert.deepEqual(withheldTurnPresentation(turn, context), {
    action: "restore-withheld-turn",
    label: "Restore this turn",
    sourceTurnIndex: 3,
  });
  for (const invalid of [
    { ...context, meetingHandle: "" },
    { ...context, transcriptMeetingId: "m-2" },
    { ...context, transcriptSha256: "stale" },
    { ...context, capture: "recording" },
  ]) {
    assert.equal(withheldTurnPresentation(turn, invalid), null);
  }
  assert.equal(withheldTurnPresentation({ ...turn, sourceTurnIndex: 3.5 }, context), null);
  assert.equal(withheldTurnPresentation({ sourceTurnIndex: 3 }, context), null);
});

test("vocabulary only opens for the exact idle transcript projection", () => {
  const eligible = {
    meetingId: "m-1",
    transcriptMeetingId: "m-1",
    transcriptSha256: "a".repeat(64),
    capture: "idle",
  };
  assert.deepEqual(localVocabularyPresentation(eligible), {
    action: "open-vocabulary",
    label: "Vocabulary",
    meetingId: "m-1",
    sourceTranscriptSha256: "a".repeat(64),
  });
  for (const invalid of [
    { ...eligible, transcriptMeetingId: "m-2" },
    { ...eligible, transcriptSha256: "stale" },
    { ...eligible, capture: "recording" },
    { meetingId: "m-1" },
  ]) {
    assert.equal(localVocabularyPresentation(invalid), null);
  }
});

test("transcript retry is offered only for a stable retained transcript", () => {
  const eligible = {
    meetingId: "m-1",
    transcriptMeetingId: "m-1",
    sourceTranscriptSha256: "a".repeat(64),
    audioRetentionState: "retained",
    capture: "idle",
  };
  assert.deepEqual(transcriptRetryPresentation(eligible), {
    action: "start-transcript-retry",
    label: "Retry transcript",
    meetingId: "m-1",
    sourceTranscriptSha256: "a".repeat(64),
    pending: null,
  });
  for (const invalid of [
    { ...eligible, transcriptMeetingId: "m-2" },
    { ...eligible, sourceTranscriptSha256: "stale" },
    { ...eligible, audioRetentionState: "released" },
    { ...eligible, capture: "recording" },
    { ...eligible, recovery: { state: "transcript-unavailable" } },
  ]) {
    assert.equal(transcriptRetryPresentation(invalid), null);
  }
});

test("a retry candidate resumes only when it is bound to the current transcript", () => {
  const context = {
    meetingId: "m-1",
    transcriptMeetingId: "m-1",
    sourceTranscriptSha256: "a".repeat(64),
    audioRetentionState: "retained",
    capture: "idle",
  };
  const pending = {
    meetingId: "m-1",
    operationId: "op-1",
    sourceTranscriptSha256: "a".repeat(64),
    candidateTranscriptSha256: "b".repeat(64),
  };
  const presentation = transcriptRetryPresentation({ ...context, pending });
  assert.equal(presentation.label, "Review retry");
  assert.equal(presentation.pending, pending);
  assert.equal(transcriptRetryPresentation({ ...context, pending: { ...pending, sourceTranscriptSha256: "c".repeat(64) } }).pending, null);
});

test("retry quality keeps canonical observation labels from the reader projection", () => {
  assert.deepEqual(transcriptRetryQualityPresentation({
    state: "available",
    message: "Capture checks are available.",
    observations: [
      { kind: "silence", status: "unknown", message: "No silence assessment was recorded." },
      { kind: "clipping", status: "ok", message: "No clipping was detected." },
    ],
  }), {
    state: "available",
    message: "Capture checks are available.",
    observations: [
      { kind: "Silence", detail: "No silence assessment was recorded." },
      { kind: "Clipping", detail: "No clipping was detected." },
    ],
  });
  assert.deepEqual(transcriptRetryQualityPresentation({
    observations: { silence: { status: "unknown" } },
  }).observations, [{ kind: "Silence", detail: "unknown" }]);
  assert.equal(transcriptRetryQualityPresentation().message, "Capture-quality details are unavailable for this retry.");
});

test("retry quality uses closed labels and keeps unknown kinds safe", () => {
  assert.equal(transcriptRetryQualityKindLabel("silence"), "Silence");
  assert.equal(transcriptRetryQualityKindLabel("clipping"), "Clipping");
  assert.equal(transcriptRetryQualityKindLabel("low-input"), "Low input");
  assert.equal(transcriptRetryQualityKindLabel("background-noise"), "Background noise");
  assert.equal(transcriptRetryQualityKindLabel("<img src=x onerror=alert(1)>"), "Observation");
});

test("recording device context never renders private backend fields or arbitrary copy", () => {
  assert.deepEqual(recordingDevicePresentation({
    state: "identified",
    message: "Private device name",
    name: "Private device name",
    index: 4,
  }), {
    state: "identified",
    title: "Recording device recorded",
    detail: "Yawn verified that a microphone identity was recorded for this meeting. This does not confirm it was the audio input you intended to use.",
    action: null,
  });
  assert.deepEqual(recordingDevicePresentation({
    state: "unknown",
    message: "Private receipt text",
    nextAction: "check-audio-input",
  }), {
    state: "unknown",
    title: "Recording device not verified",
    detail: "Yawn could not verify which microphone identity was recorded for this meeting.",
    action: { action: "open-settings", label: "Check audio input" },
  });
  assert.equal(recordingDevicePresentation({ state: "unknown", nextAction: "anything-else" }).action, null);
});

test("retry comparison UI keeps the decision explicit and uses exact backend commands", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /invoke\("transcript_retry_pending", \{\s*meetingId: retry\.meetingId,\s*sourceTranscriptSha256: retry\.sourceTranscriptSha256,/);
  assert.match(source, /invoke\("transcript_retry_start", \{\s*meetingId: retry\.meetingId,\s*sourceTranscriptSha256: retry\.sourceTranscriptSha256,/);
  assert.match(source, /invoke\("transcript_retry_decide", \{\s*meetingId: retry\.meetingId,\s*operationId: retry\.operationId,\s*sourceTranscriptSha256: retry\.sourceTranscriptSha256,\s*candidateTranscriptSha256: retry\.candidateTranscriptSha256,\s*decision,/);
  assert.match(source, /decideTranscriptRetry\("keep-current"\)/);
  assert.match(source, /decideTranscriptRetry\("use-retry"\)/);
  assert.match(source, /data-action="decide-retry-later"/);
  assert.match(source, /else if \(action === "decide-retry-later"\) \{\s*closeModal\(\);\s*render\(\);\s*\}/);
  assert.match(source, /The retained transcript stays as it is unless you explicitly use this retry\./);
  assert.match(source, /current generated note\.\s*<\/h3><p>You will need to regenerate the note/);
  // The "clears the current generated note" warning must not render on a
  // meeting that has no note. It is guarded by the same "transcript-only"
  // signal the note card reads, compared strictly so an unknown or loading
  // note state still shows the warning.
  assert.match(
    source,
    /const hasNoGeneratedNote = state\.selected\?\.note\?\.state === "transcript-only";/,
  );
  assert.match(source, /\$\{hasNoGeneratedNote \? "" : `<section class="retry-use-warning"/);
  assert.match(source, /state\.transcriptRetry = \{ \.\.\.retry, phase: "starting" \}/);
  assert.match(source, /catch \(error\) \{\s*if \(state\.selected === selection\) state\.transcriptRetry = null;\s*reportError\(error\);/);
});

test("retry comparison redacts withheld text and keeps the summary before personal notes", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /turn\.withheld \? "This turn was withheld by the voice check\." : escapeHtml\(turn\.text\)/);
  assert.match(source, /renderMeetingNote\(note, claimEvidence\)\}\n\s*\$\{renderGenerateNote\(note, recovery\)\}\n\s*\$\{renderTranscriptRetryAction\(note, transcript, recovery\)\}\n\s*\$\{renderTranscriptDisclosure\(transcript, recovery\)\}/);
  assert.match(source, /<aside class="meeting-notes-pane">\s*\$\{renderRetainedAudioPlayback\(playback\)\}\s*<section class="note-section your-notes-section"/);
  assert.match(source, /function renderRetryWarnings\(warnings, label\)/);
  assert.match(source, /renderRetryRecordingDevice\(retry\.recordingDevice\)/);
  assert.doesNotMatch(source, /retry\.recordingDevice\.(name|index|hostapi|message)/);
});

test("startup keeps polling until the app is ready", () => {
  assert.equal(shouldPollSnapshot({ startup: "checking", capture: "idle" }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle" }), false);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "recording" }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle", background_transcription_active: true }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle", background_transcription_queued_count: 1 }), true);
});

test("the generate control follows the backend's eligibility signal alone", () => {
  const eligible = { meetingId: "m-1", regenerationSourceSha256: "a".repeat(64) };
  const idle = noteGenerationPresentation(eligible, "");
  assert.equal(idle.action, "generate-note");
  assert.equal(idle.disabled, false);
  assert.equal(idle.label, "Generate note");
  assert.match(idle.help, /on this Mac/);
  assert.match(idle.help, /minutes/);

  const busy = noteGenerationPresentation(eligible, "m-1");
  assert.equal(busy.disabled, true);
  assert.equal(busy.label, "Generating note…");
  assert.match(busy.help, /keep using Yawn/);

  // Another meeting generating does not disable this one's control.
  assert.equal(noteGenerationPresentation(eligible, "m-2").disabled, false);

  const replacement = noteGenerationPresentation({ ...eligible, claims: [{ ordinal: 0 }] }, "");
  assert.equal(replacement.label, "Regenerate note");
  assert.match(replacement.help, /current note stays in place/);

  // No source pin — a ready note, a stale view, a deleted transcript — no control.
  assert.equal(noteGenerationPresentation({ meetingId: "m-1" }, ""), null);
  assert.equal(noteGenerationPresentation({ regenerationSourceSha256: "x" }, ""), null);
  assert.equal(noteGenerationPresentation(null, ""), null);
});

test("unavailable library keeps its backend message and offers a real refresh", () => {
  const recovery = libraryRecoveryPresentation({
    state: "unavailable",
    rows: [],
    message: "The local library is unavailable. Reopen the app and try again.",
  });
  assert.equal(recovery.title, "Meetings need another check.");
  assert.equal(recovery.detail, "The local library is unavailable. Reopen the app and try again.");
  assert.deepEqual(recovery.action, { action: "refresh-library", label: "Check again" });
  assert.equal(libraryRecoveryPresentation({ state: "empty", rows: [] }), null);
});

test("only exact backend recovery errors receive contextual actions", () => {
  assert.deepEqual(errorRecoveryPresentation("That view is no longer current. Reopen it and try again."), {
    message: "That view is no longer current. Reopen it and try again.",
    action: { action: "refresh-library", label: "Check again" },
  });
  assert.deepEqual(errorRecoveryPresentation("That view is no longer current. Reopen it and try again.", { hasSelectedMeeting: true }), {
    message: "That view is no longer current. Reopen it and try again.",
    action: { action: "refresh-selected-meeting", label: "Refresh this meeting" },
  });
  assert.deepEqual(errorRecoveryPresentation("The local meeting library is unavailable. Reopen the app and try again."), {
    message: "The local meeting library is unavailable. Reopen the app and try again.",
    action: { action: "refresh-library", label: "Check again" },
  });
  for (const message of [
    "Retained audio is unavailable. Reopen Library and try again.",
    "The transcript changed. Reopen the meeting and try again.",
    "That speaker group is no longer available. Reopen the meeting and try again.",
    "The retry candidate changed. Reopen the meeting and try again.",
  ]) {
    assert.deepEqual(errorRecoveryPresentation(message, { hasSelectedMeeting: true }), {
      message,
      action: { action: "refresh-selected-meeting", label: "Refresh this meeting" },
    });
    assert.deepEqual(errorRecoveryPresentation(message), { message, action: null });
  }
  assert.deepEqual(errorRecoveryPresentation("Refresh the library and try again."), {
    message: "Refresh the library and try again.",
    action: null,
  });
});

test("meeting refresh action reopens only the selected meeting", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /async function refreshSelectedMeetingFromRecovery\(\) \{\s*const meetingId = state\.selected\?\.row\?\.meetingId;[\s\S]*reopenSelectedMeeting\(meetingId\)/);
  assert.match(source, /else if \(action === "refresh-selected-meeting"\) void refreshSelectedMeetingFromRecovery\(\);/);
});

test("summary failure keeps the transcript and offers regeneration when its source is pinned", () => {
  const recovery = meetingRecoveryPresentation({
    state: "summary-failed",
    meetingId: "m-1",
    regenerationSourceSha256: "a".repeat(64),
    claims: [],
  }, { state: "transcript", turns: [{ text: "kept" }] });
  assert.equal(recovery.state, "summary-failed");
  assert.equal(recovery.action.action, "generate-note");
  assert.equal(recovery.action.label, "Regenerate note");
  assert.match(recovery.detail, /transcript/);
  assert.match(recovery.detail, /remain unchanged/);
});

test("summary failure without a source explains that retry is unavailable", () => {
  const recovery = meetingRecoveryPresentation({
    state: "summary-failed",
    meetingId: "m-1",
    claims: [],
  });
  assert.equal(recovery.state, "summary-failed-no-source");
  assert.equal(recovery.action, null);
  assert.match(recovery.detail, /cannot retry/);
  assert.match(recovery.detail, /stays unchanged/);
});

test("active generation says what remains without offering a second retry", () => {
  const recovery = meetingRecoveryPresentation({
    state: "summary-failed",
    meetingId: "m-1",
    regenerationSourceSha256: "a".repeat(64),
    claims: [{ claimType: "summary", claim: "The current note." }],
  }, { state: "transcript" }, "m-1");
  assert.equal(recovery.state, "generating");
  assert.equal(recovery.action, null);
  assert.match(recovery.detail, /current note stays in place/);
});

test("stale transcript routes back to meetings instead of inventing a transcript retry", () => {
  const recovery = meetingRecoveryPresentation(
    { state: "transcript-only", meetingId: "m-1" },
    { state: "stale", turns: [], message: "That transcript is no longer available." },
  );
  assert.equal(recovery.state, "transcript-unavailable");
  assert.deepEqual(recovery.action, { action: "meetings", label: "Back to meetings" });
  assert.equal(recovery.detail, "That transcript is no longer available.");
});

test("an empty transcript-only note remains explicit and only receives a generate control from its source pin", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /<h2 id="meeting-note-heading">No meeting note yet\.<\/h2>/);
  assert.match(source, /if \(note\?\.state !== "transcript-only"\) return "";/);
  assert.match(source, /!recovery && note\?\.state !== "transcript-only" && note\?\.message && !claims\.length/);
  assert.match(source, /\$\{renderMeetingNote\(note, claimEvidence\)\}\n\s*\$\{renderGenerateNote\(note, recovery\)\}/);
  assert.match(source, /function renderGenerateNote[\s\S]*noteGenerationPresentation\(note, state\.generatingMeetingId\)/);
});

test("released audio says retranscription is unavailable while preserving the note and transcript", () => {
  const recovery = meetingRecoveryPresentation({
    state: "transcript-only",
    meetingId: "m-1",
    audioRetention: { state: "released" },
  }, { state: "transcript", turns: [{ text: "kept" }] });
  assert.equal(recovery.state, "audio-released");
  assert.equal(recovery.action, null);
  assert.match(recovery.detail, /cannot be retranscribed/);
  assert.match(recovery.detail, /remain available/);
});

test("released audio remains visible even when a usable note is present", () => {
  const recovery = meetingRecoveryPresentation({
    state: "ready",
    meetingId: "m-1",
    claims: [{ claimType: "summary", claim: "The current note." }],
    audioRetention: { state: "released" },
  }, { state: "transcript", turns: [{ text: "kept" }] });
  assert.equal(recovery.state, "audio-released");
  assert.equal(recovery.action, null);
});

test("speaker corrections no longer create a recovery block before note generation", () => {
  const recovery = meetingRecoveryPresentation({
    state: "ready",
    meetingId: "m-1",
    regenerationSourceSha256: "a".repeat(64),
    claims: [{ claimType: "summary", claim: "The current note." }],
  }, { state: "transcript", turns: [{ speakerCorrected: true }] });
  assert.equal(recovery, null);
});

test("meeting detail exposes only explicit retained-audio controls and polls the owned player", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /Microphone and system audio are separate recordings\./);
  assert.match(source, /data-action="stop-retained-audio"/);
  assert.match(source, /invoke\("library_play_retained_audio", \{ handle \}\)/);
  assert.match(source, /invoke\("library_retained_audio_playback_status"\)/);
  assert.match(source, /invoke\("library_stop_retained_audio"\)/);
  assert.match(source, /async function stopRetainedAudio\(\)[\s\S]*reopenSelectedMeeting\(meetingId\)/);
  assert.match(source, /response\.state === "completed"[\s\S]*reopenSelectedMeeting\(meetingId\)/);
  assert.match(source, /response\.state !== "playing"[\s\S]*Retained audio is unavailable\. Reopen Library and try again\./);
  assert.doesNotMatch(source, /state\.error = String\(error\)/);
});

test("note-capture hotkey focus always lands at the end of the current draft", () => {
  assert.deepEqual(noteCaptureFocusSelection("already typed"), {
    start: "already typed".length,
    end: "already typed".length,
    direction: "forward",
  });
  assert.deepEqual(noteCaptureFocusSelection(""), { start: 0, end: 0, direction: "forward" });
  assert.deepEqual(noteCaptureFocusSelection(undefined), { start: 0, end: 0, direction: "forward" });
});

test("the roadmap I2 hotkey lands only in the existing operator canvas, never a new surface", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // Matches capture_shortcut::NOTE_CAPTURE_FOCUS_EVENT in main.rs exactly;
  // a drift here would silently break the whole feature.
  assert.match(source, /const NOTE_CAPTURE_FOCUS_EVENT = "note-capture-hotkey";/);
  assert.match(source, /tauriListen\(NOTE_CAPTURE_FOCUS_EVENT, \(\) => \{/);
  // No new window, no overlay: it must only touch the existing operator-note
  // field already rendered by renderCapture().
  assert.match(source, /root\.querySelector\('\[data-field="operator-note"\]'\)/);
  assert.doesNotMatch(source, /WebviewWindow|new Window\(/);
  // A missing or disabled editor must leave the request pending, never throw
  // or force a state the product brief forbids.
  assert.match(source, /if \(!target \|\| target\.disabled\) return;/);
});
