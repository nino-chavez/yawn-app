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
  capturePauseControlPresentation,
  capturePausePresentation,
  errorRecoveryPresentation,
  evidencePopoverPresentation,
  evidenceSplitAllowed,
  evidenceSyncTarget,
  firstRunSheetVisible,
  humanize,
  isManualTranscriptScroll,
  nextEscapeTarget,
  libraryEmptyStatePresentation,
  libraryLoadingPresentation,
  libraryRecoveryPresentation,
  libraryRowMetaPresentation,
  libraryRowPreview,
  libraryStallTransition,
  localVocabularyPresentation,
  lockedActionOutcome,
  meetingContextPresentation,
  meetingDeletionConfirmationCopy,
  meetingLockCopyIsHonest,
  meetingLockPresentation,
  meetingLockSheetCopy,
  meetingRecoveryPresentation,
  meetingNotePresentation,
  mergePermissions,
  noteCaptureFocusSelection,
  noteGenerationPresentation,
  permissionSummary,
  recordingDevicePresentation,
  retainedAudioPlaybackPresentation,
  retentionLabel,
  retryTurnDiffSegments,
  shouldPollSnapshot,
  startSheetGuidedHint,
  transcriptCitationSummary,
  transcriptPlainText,
  transcriptRetryDiffPresentation,
  transcriptRetryQualityPresentation,
  transcriptRetryQualityKindLabel,
  transcriptSearchAffordancePresentation,
  transcriptSearchResultSnippet,
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
  assert.equal(captureActivity(snapshot).label, "Transcribing on this Mac");
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
    label: "An earlier meeting is processing on this Mac.",
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

// Roadmap intake I4 / design D4's reverse half.
test("a turn's citation affordance names each citing claim's type and words", () => {
  const claims = [
    { ordinal: 0, claimType: "decision", claim: "Use the smaller battery for the pilot run." },
    { ordinal: 1, claimType: "action", claim: "Send the cost table by Friday." },
  ];
  const turnsCited = [{ turn: 5, claimOrdinals: [0, 1] }];
  const presentation = turnCitationPresentation(turnsCited, claims, 5);
  assert.equal(presentation.summary, "Cited by 2 note items");
  assert.deepEqual(presentation.citations, [
    { ordinal: 0, label: "Decision: Use the smaller battery for the…" },
    { ordinal: 1, label: "Follow-up: Send the cost table by Friday." },
  ]);
});

test("an uncited turn gets no citation affordance", () => {
  const claims = [{ ordinal: 0, claimType: "decision", claim: "Use the smaller battery." }];
  const turnsCited = [{ turn: 5, claimOrdinals: [0] }];
  assert.equal(turnCitationPresentation(turnsCited, claims, 6), null);
  assert.equal(turnCitationPresentation([], claims, 5), null);
  assert.equal(turnCitationPresentation(turnsCited, claims, undefined), null);
});

test("a citation naming a claim ordinal absent from claims resolves to nothing, not a broken entry", () => {
  const claims = [{ ordinal: 0, claimType: "decision", claim: "Use the smaller battery." }];
  const turnsCited = [{ turn: 5, claimOrdinals: [9] }];
  assert.equal(turnCitationPresentation(turnsCited, claims, 5), null);
});

test("a single citing claim reads as singular", () => {
  const claims = [{ ordinal: 2, claimType: "point", claim: "we agreed to defer the migration until Q3" }];
  const presentation = turnCitationPresentation([{ turn: 1, claimOrdinals: [2] }], claims, 1);
  assert.equal(presentation.summary, "Cited in the note");
  assert.deepEqual(presentation.citations, [
    { ordinal: 2, label: "Highlight: we agreed to defer the migration…" },
  ]);
});

test("the optional transcript-header citation summary states a plain count or nothing", () => {
  const turnsCited = [{ turn: 0, claimOrdinals: [0] }, { turn: 3, claimOrdinals: [1, 2] }];
  assert.equal(transcriptCitationSummary(turnsCited, 48), "2 of 48 turns are cited by the note.");
  assert.equal(transcriptCitationSummary([], 48), null);
  assert.equal(transcriptCitationSummary(turnsCited, 0), null);
  assert.equal(transcriptCitationSummary(null, 48), null);
});

// Roadmap intake I3's sidecar, shown read-only after the meeting.
test("meeting context presents present, empty, and unreadable as three distinct states", () => {
  assert.deepEqual(
    meetingContextPresentation({ text: "Decide the Q3 roadmap.", unreadable: false }),
    { state: "present", text: "Decide the Q3 roadmap." },
  );
  assert.deepEqual(meetingContextPresentation({ text: "", unreadable: false }), { state: "empty", text: "" });
  assert.deepEqual(meetingContextPresentation({ text: "   ", unreadable: false }), { state: "empty", text: "" });
  assert.deepEqual(meetingContextPresentation(undefined), { state: "empty", text: "" });
  // Unreadable wins even if a corrupt read somehow left text behind -- the
  // shell must never surface bytes from a file it could not parse.
  assert.deepEqual(
    meetingContextPresentation({ text: "should not surface", unreadable: true }),
    { state: "unreadable", text: "" },
  );
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

test("retry turn diff segments highlight only the word indices inside a span", () => {
  const entry = { wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] };
  const segments = retryTurnDiffSegments("the quarterly review starts monday", entry);
  assert.deepEqual(segments, [
    { text: "the", highlighted: false },
    { text: " ", highlighted: false },
    { text: "quarterly", highlighted: false },
    { text: " ", highlighted: false },
    { text: "review", highlighted: false },
    { text: " ", highlighted: false },
    { text: "starts", highlighted: false },
    { text: " ", highlighted: false },
    { text: "monday", highlighted: true },
  ]);
});

test("retry turn diff segments keep whitespace unhighlighted and support multiple spans", () => {
  const entry = {
    wordCount: 5,
    spans: [
      { startWord: 1, endWord: 2 },
      { startWord: 3, endWord: 4 },
    ],
  };
  const segments = retryTurnDiffSegments("alpha bravo charlie delta echo", entry);
  const highlightedWords = segments.filter((segment) => segment.highlighted).map((segment) => segment.text);
  assert.deepEqual(highlightedWords, ["bravo", "delta"]);
  assert.equal(segments.every((segment) => segment.text !== "" ), true);
});

test("retry turn diff segments are empty for empty text and default to no spans", () => {
  assert.deepEqual(retryTurnDiffSegments("", { wordCount: 1, spans: [{ startWord: 0, endWord: 1 }] }), []);
  assert.deepEqual(retryTurnDiffSegments("solo"), [{ text: "solo", highlighted: false }]);
});

test("retry turn diff segments render plain when the renderer's own word count disagrees with Rust's", () => {
  // The span says word 0 differs, but the entry's wordCount (3) does not
  // match what this renderer actually counts for this text (2 words) — a
  // stand-in for a tokenizer disagreement (e.g. an unusual whitespace
  // character). Every word must render unhighlighted rather than risk
  // marking the wrong one.
  const mismatched = { wordCount: 3, spans: [{ startWord: 0, endWord: 1 }] };
  const segments = retryTurnDiffSegments("alpha bravo", mismatched);
  assert.equal(segments.some((segment) => segment.highlighted), false);
});

test("retry turn diff segments render plain when no wordCount is present at all", () => {
  const segments = retryTurnDiffSegments("alpha bravo", { spans: [{ startWord: 0, endWord: 1 }] });
  assert.equal(segments.some((segment) => segment.highlighted), false);
});

test("retry diff presentation shows the legend when differences were computed", () => {
  const presentation = transcriptRetryDiffPresentation({
    state: "computed",
    current: [{ turnIndex: 1, wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] }],
    candidate: [{ turnIndex: 1, wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] }],
  });
  assert.equal(presentation.computed, true);
  assert.equal(presentation.legend, "Highlights show where the transcripts differ.");
  assert.deepEqual(presentation.current.get(1), { wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] });
  assert.deepEqual(presentation.candidate.get(1), { wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] });
});

test("retry diff presentation reports the identical sentence when every turn's spans are empty", () => {
  // Every visible turn still gets an entry (with its word count) even when
  // nothing differs — "identical" is decided by scanning for any non-empty
  // spans array, not by whether the side has entries at all.
  const presentation = transcriptRetryDiffPresentation({
    state: "computed",
    current: [{ turnIndex: 0, wordCount: 6, spans: [] }],
    candidate: [{ turnIndex: 0, wordCount: 6, spans: [] }],
  });
  assert.equal(presentation.computed, true);
  assert.equal(presentation.legend, "No word-level differences found.");
  assert.equal(presentation.current.size, 1);
  assert.equal(presentation.candidate.size, 1);
});

test("retry diff presentation reports the skipped sentence and never claims identical", () => {
  const skipped = transcriptRetryDiffPresentation({ state: "skipped", current: [], candidate: [] });
  assert.equal(skipped.computed, false);
  assert.equal(skipped.legend, "These transcripts are too long to highlight word differences.");

  // A missing or unrecognized diff payload fails the same safe way: it is
  // never presented as "these transcripts are identical".
  assert.equal(transcriptRetryDiffPresentation(null).legend, "These transcripts are too long to highlight word differences.");
  assert.equal(transcriptRetryDiffPresentation(undefined).computed, false);
});

test("word-level diff highlighting stays inside the retry comparison and never touches the main transcript render", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // Exactly one call site: renderRetryTurnBody, used only by the retry
  // comparison columns.
  assert.equal(source.split("retryTurnDiffSegments(").length - 1, 1);
  assert.match(source, /function renderRetryTurnBody\(turn, entry\) \{[\s\S]*?retryTurnDiffSegments\(turn\.text, entry\)/);
  // The main, non-retry transcript turn render (shared .transcript-line
  // class) is unchanged: still a plain escapeHtml of the turn text, with no
  // diff segmentation in its own render path.
  assert.match(source, /<p>\$\{turn\.withheld \? "This turn was withheld by the voice check\." : escapeHtml\(turn\.text\)\}<\/p>/);
  assert.match(source, /renderRetryDiffLegend\(\)/);
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
  // Design intake D5: the below-the-note transcript disclosure now renders
  // only when the evidence split is not showing the same workspace-mode
  // transcript elsewhere (see `renderMeetingWorkspace`) -- the two must never
  // render at once, since `renderTranscript`'s workspace branch hardcodes ids
  // the patcher would then have to choose between.
  assert.match(source, /renderMeetingNote\(note, claimEvidence\)\}\n\s*\$\{renderGenerateNote\(note, recovery\)\}\n\s*\$\{renderTranscriptRetryAction\(note, transcript, recovery\)\}\n\s*\$\{splitActive \? "" : renderTranscriptDisclosure\(transcript, recovery, note\)\}/);
  assert.match(source, /<aside class="meeting-notes-pane">\s*\$\{renderRetainedAudioPlayback\(playback\)\}\s*\$\{renderMeetingContextSection\(note\)\}\s*<section class="note-section your-notes-section"/);
  assert.match(source, /function renderRetryWarnings\(warnings, label\)/);
  assert.match(source, /renderRetryRecordingDevice\(retry\.recordingDevice\)/);
  assert.doesNotMatch(source, /retry\.recordingDevice\.(name|index|hostapi|message)/);
});

test("a paused recording states plainly that nothing is being captured", () => {
  const paused = capturePresentation({ capture: "paused" });
  assert.equal(paused.eyebrow, "Paused");
  assert.equal(paused.title, "Nothing is being recorded.");
  // A paused meeting is attention-adjacent, not an error and not recording.
  // The recording tone would assert that audio is still being captured.
  assert.equal(paused.tone, "attention");
  assert.notEqual(paused.tone, "recording");
  // The fallback presentation is the one that says Yawn could not read the
  // state. Reaching it would mean pause renders as a fault.
  assert.notEqual(paused.detail, "The current recording state could not be read.");

  const activity = captureActivity({ capture: "paused" });
  assert.equal(activity.label, "Paused");
  assert.equal(activity.tone, "attention");

  // Polling has to continue while paused, or the surface never learns that the
  // operator resumed.
  assert.equal(captureIsInProgress({ capture: "paused" }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "paused" }), true);
});

test("the pause control never claims a state the capture helper has not confirmed", () => {
  assert.equal(capturePauseControlPresentation({ capture: "idle" }), null);
  assert.equal(capturePauseControlPresentation({ capture: "arming" }), null);
  assert.equal(capturePauseControlPresentation({ capture: "transcript-ready" }), null);

  assert.deepEqual(capturePauseControlPresentation({ capture: "recording" }), {
    action: "pause-recording",
    label: "Pause",
    disabled: false,
  });
  assert.deepEqual(capturePauseControlPresentation({ capture: "paused" }), {
    action: "resume-recording",
    label: "Resume",
    disabled: false,
  });
  assert.deepEqual(
    capturePauseControlPresentation({ capture: "recording", capture_pause_change_pending: true }),
    { action: "pause-recording", label: "Pausing…", disabled: true },
  );
  assert.deepEqual(
    capturePauseControlPresentation({ capture: "paused", capture_pause_change_pending: true }),
    { action: "resume-recording", label: "Resuming…", disabled: true },
  );
});

test("a meeting's gaps read from the receipt, and a legacy receipt reads as uninterrupted", () => {
  assert.deepEqual(capturePausePresentation({ state: "not-paused", count: 0, totalPausedSeconds: 0 }), {
    state: "not-paused",
    title: "Recording was not paused",
    detail: "The retained audio for this meeting runs without a gap.",
  });
  // A receipt written before pause existed carries no field at all, and the
  // native projection reports that as not-paused. It must never surface as a
  // warning or as missing evidence.
  assert.deepEqual(
    capturePausePresentation({ state: "not-paused" }),
    capturePausePresentation({ state: "not-paused", count: 0, totalPausedSeconds: 0 }),
  );

  const paused = capturePausePresentation({
    state: "paused",
    count: 2,
    totalPausedSeconds: 90,
    message: "Recording was paused 2 times, for 1:30 in total. Nothing was captured during those gaps.",
  });
  assert.equal(paused.title, "Recording was paused");
  assert.match(paused.detail, /paused 2 times, for 1:30 in total/);

  // An unverifiable pause record says so; it never reads as no pauses.
  for (const shape of [null, {}, { state: "unavailable" }, { state: "not-a-state" }]) {
    const projection = capturePausePresentation(shape);
    assert.equal(projection.state, "unavailable");
    assert.equal(projection.title, "Pauses could not be checked");
  }
  // Backend copy is only used for a state this mapping recognizes.
  assert.equal(
    capturePausePresentation({ state: "nonsense", message: "Private receipt text" }).detail,
    "Yawn could not verify whether this recording was paused.",
  );
});

test("the recorder offers pause beside stop and stops straight from a pause", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /invoke\("pause_meeting"\)/);
  assert.match(source, /invoke\("resume_meeting"\)/);
  assert.match(source, /data-action="\$\{pauseControl\.action\}"/);
  // Stop stays reachable while paused: the operator must never have to resume
  // a meeting in order to end it.
  assert.match(source, /if \(!\["recording", "paused"\]\.includes\(state\.snapshot\?\.capture\)\) return;/);
  assert.match(source, /if \(state\.snapshot\?\.capture !== "paused"\) return;/);
  // The gap shows where capture evidence already shows, beside quality.
  assert.match(source, /renderRetryCapturePauses\(retry\.pauses\)/);
  // And on the ordinary path, which never opens a retry: the meeting the
  // operator reads after stopping.
  assert.match(source, /renderMeetingCapturePauses\(note\?\.capturePauses\)/);
  // A meeting still loading its note must not read as unverifiable.
  assert.match(source, /function renderMeetingCapturePauses\(pauses\) \{\s*if \(!pauses\) return "";/);
  // An uninterrupted recording stays quiet rather than announcing an absence.
  assert.match(source, /if \(presentation\.state === "not-paused"\) return "";/);
});

test("startup keeps polling until the app is ready", () => {
  assert.equal(shouldPollSnapshot({ startup: "checking", capture: "idle" }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle" }), false);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "recording" }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle", background_transcription_active: true }), true);
  assert.equal(shouldPollSnapshot({ startup: "ready", capture: "idle", background_transcription_queued_count: 1 }), true);
});

// Roadmap intake W8-B: the one-week local usage probe for cross-meeting exact
// search. The merge gate's flag-off requirement lives here: with the flag
// off, this must return null for every query and every capture state, which
// is what lets `renderTranscriptSearchAffordance` render nothing at all.
test("the transcript-search affordance renders nothing at all with the probe flag off", () => {
  assert.equal(
    transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: false },
      query: "budget",
      snapshot: { capture: "idle" },
    }),
    null,
  );
  assert.equal(
    transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: false },
      query: "budget",
      snapshot: { capture: "recording" },
    }),
    null,
    "flag off must win even during a state that would otherwise be unavailable",
  );
  assert.equal(transcriptSearchAffordancePresentation({ library: null, query: "budget", snapshot: null }), null);
});

test("the transcript-search affordance requires a non-empty query even with the flag on", () => {
  assert.equal(
    transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: true },
      query: "",
      snapshot: { capture: "idle" },
    }),
    null,
  );
  assert.equal(
    transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: true },
      query: "   ",
      snapshot: { capture: "idle" },
    }),
    null,
    "whitespace-only is the same as empty",
  );
});

test("the transcript-search affordance is available with the flag on, a query, and no capture in progress", () => {
  const presentation = transcriptSearchAffordancePresentation({
    library: { searchProbeEnabled: true },
    query: "  budget  ",
    snapshot: { capture: "idle" },
  });
  assert.deepEqual(presentation, { state: "available", query: "budget" });
});

test("the transcript-search affordance states unavailability with the brief's exact sentence while capture is in progress", () => {
  for (const capture of ["arming", "recording", "paused", "stopping", "captured", "transcribing", "summarizing"]) {
    const presentation = transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: true },
      query: "budget",
      snapshot: { capture },
    });
    assert.equal(presentation.state, "unavailable", `capture=${capture}`);
    assert.equal(presentation.message, "Search across meetings is unavailable while recording.");
  }
  // transcript-ready is not in captureIsInProgress's own list (recording and
  // transcribing are both finished by then), so the affordance stays
  // available rather than unavailable in that state.
  assert.equal(
    transcriptSearchAffordancePresentation({
      library: { searchProbeEnabled: true },
      query: "budget",
      snapshot: { capture: "transcript-ready" },
    }).state,
    "available",
  );
});

test("a cross-meeting search hit renders an honest per-kind snippet, never blank or invented text", () => {
  assert.equal(
    transcriptSearchResultSnippet({ kind: "withheld", text: null }),
    "A voice check withheld this matching turn. It is not shown as transcript text.",
  );
  assert.equal(transcriptSearchResultSnippet({ kind: "meeting", text: null }), "Matched this meeting's title or folder.");
  assert.equal(transcriptSearchResultSnippet({ kind: "transcript", text: "the exact matched words" }), "the exact matched words");
  assert.equal(transcriptSearchResultSnippet({ kind: "claim", text: null }), "");
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

test("a loaded library needs no loading presentation at all", () => {
  assert.equal(libraryLoadingPresentation({ rows: [] }, false), null);
  assert.equal(libraryLoadingPresentation({ rows: [] }, true), null);
});

test("the library loading line escalates once, with one Try-again action", () => {
  // Desktop-design audit (2026-09-01), fix 1: the loading line used to
  // render the same words at 200 ms and forever. Before a stall, plain
  // words and no action; after, honest stall copy plus one action that
  // reuses the existing refresh idiom -- no spinner, no second control.
  const early = libraryLoadingPresentation(null, false);
  assert.equal(early.stalled, false);
  assert.equal(early.message, "Loading meetings saved on this Mac…");
  assert.equal(early.action, null);

  const stalled = libraryLoadingPresentation(null, true);
  assert.equal(stalled.stalled, true);
  assert.equal(stalled.message, "Still loading meetings. This is taking longer than usual.");
  assert.deepEqual(stalled.action, { action: "refresh-library", label: "Try again" });
});

test("the library-stall timer's phases are a pure transition table", () => {
  // main.js only touches a real setTimeout; every decision about what phase
  // that timeout produces lives here, so it is tested without faking a clock.
  assert.equal(libraryStallTransition("idle", "load-start"), "waiting");
  assert.equal(libraryStallTransition("waiting", "stall-elapsed"), "stalled");
  assert.equal(libraryStallTransition("waiting", "load-settled"), "idle");
  assert.equal(libraryStallTransition("stalled", "load-settled"), "idle");
  // A retry after a stall re-arms the wait rather than staying stuck.
  assert.equal(libraryStallTransition("stalled", "load-start"), "waiting");
  // "stall-elapsed" only ever promotes "waiting" -> "stalled". A timer that
  // fires after its load already settled -- a slow callback racing a fast
  // response -- must not resurrect a stalled state from "idle", and an
  // already-stalled phase is unaffected by a second elapse.
  assert.equal(libraryStallTransition("idle", "stall-elapsed"), "idle");
  assert.equal(libraryStallTransition("stalled", "stall-elapsed"), "stalled");
});

test("main.js arms the stall timer only for a load that has not yet succeeded, and always disarms it", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /if \(!state\.library\) armLibraryStallTimer\(\);/);
  assert.match(source, /finally \{\s*disarmLibraryStallTimer\(\);\s*\}/);
  // The timer callback is the only writer of `state.libraryStalled` besides
  // the disarm path -- render() must run so the escalation actually shows.
  assert.match(source, /state\.libraryStalled = true;\s*render\(\);/);
  // renderLibrary reads the flag from state on every render, not from the DOM.
  assert.match(source, /renderLibrary\(library, state\.libraryStalled\)/);
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

// Roadmap packet W10: the teaching empty state and its guided invitation
// only appear at a genuinely empty library (`total === 0` -- the same fact
// `firstRunSheetVisible` gates on), never when a filter (today, only a title
// search reaches the product surface) simply matched nothing against an
// otherwise non-empty library.
test("the teaching empty state renders only when the library is genuinely empty, not merely filtered to nothing", () => {
  assert.equal(libraryEmptyStatePresentation({ total: 0, rows: [{ handle: "a" }] }), null);

  const genuinelyEmpty = libraryEmptyStatePresentation({ total: 0, rows: [] });
  assert.equal(genuinelyEmpty.variant, "no-meetings");
  assert.equal(genuinelyEmpty.title, "No meetings yet");
  assert.match(genuinelyEmpty.message, /Press Record to start a private meeting/);
  assert.match(genuinelyEmpty.message, /on this Mac/);
  // The note is generated on request (`generate-note`/`generateSelectedNote`
  // in main.js), not produced automatically the moment a meeting finishes --
  // the teaching copy must not claim otherwise.
  assert.match(genuinelyEmpty.message, /you can generate a note/);
  assert.equal(genuinelyEmpty.showGuidedInvite, true);

  // A non-empty library (total > 0) with zero rows is a filter that matched
  // nothing, never the teaching state, regardless of what filter caused it.
  const filteredToNothing = libraryEmptyStatePresentation({ total: 3, rows: [] });
  assert.equal(filteredToNothing.variant, "no-matches");
  assert.equal(filteredToNothing.title, "No matching meetings");
  assert.equal(filteredToNothing.message, "No meeting matches that title.");
  assert.equal(filteredToNothing.showGuidedInvite, false);

  const filteredToNothingWithBackendMessage = libraryEmptyStatePresentation({
    total: 3,
    rows: [],
    message: "No meeting from this folder matches that title.",
  });
  assert.equal(filteredToNothingWithBackendMessage.message, "No meeting from this folder matches that title.");
});

// Roadmap packet W10: the once-only first-run sheet's full show/hide truth
// table -- zero meetings and unseen shows it; anything else suppresses it,
// including a library that has not loaded yet (never a flash-on before the
// first real snapshot arrives).
test("the first-run sheet shows only when the library is loaded, empty, and undismissed", () => {
  assert.equal(firstRunSheetVisible(null), false, "an unloaded library must never show the sheet");
  assert.equal(firstRunSheetVisible({ rows: [] }), false, "a library with no total field is treated as not yet loaded");
  assert.equal(firstRunSheetVisible({ total: 0, firstRunSheetSeen: false }), true, "zero meetings, never dismissed: show it once");
  assert.equal(firstRunSheetVisible({ total: 0, firstRunSheetSeen: true }), false, "dismissal must suppress it");
  assert.equal(
    firstRunSheetVisible({ total: 4, firstRunSheetSeen: false }),
    false,
    "an operator with existing meetings is not a stranger, even if never dismissed",
  );
  assert.equal(firstRunSheetVisible({ total: 4, firstRunSheetSeen: true }), false);
});

// Roadmap packet W10: the guided start-sheet hint is the one visible
// difference between the guided invitation and every ordinary way to open
// the start sheet (the Home Record button, ⌘R).
test("the guided start-sheet hint appears only on the guided invocation, with the exact brief copy", () => {
  assert.equal(startSheetGuidedHint(false), null);
  assert.equal(startSheetGuidedHint(undefined), null);
  assert.equal(
    startSheetGuidedHint(true),
    "A short test note to yourself is the fastest way to see what Yawn makes. You can delete it afterward — deleted meetings sit in Trash for 30 days.",
  );
});

// Roadmap packet W10: proves the ordinary Record/⌘R path renders the start
// sheet exactly as it did before this packet. `renderStartSheet` itself has
// no rendering harness (main.js is not a testable module the way view-model
// functions are), so this checks the source shape instead: `openStart`
// defaults `guided` to `false`, the guided-hint markup is conditional on
// that state (so it is entirely absent -- not merely empty -- on the
// ordinary path), and the plain Record/⌘R call sites never pass `true`.
test("the ordinary start-sheet path never carries the guided hint", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /function openStart\(guided = false\)/);
  assert.match(source, /const guidedHint = startSheetGuidedHint\(state\.startSheetGuided\);/);
  assert.match(source, /\$\{guidedHint \? `<p class="quiet-copy guided-start-hint">/);
  // The Home Record button and the ⌘R hotkey both call the zero-argument
  // form; only the guided invitation's handler passes `true`.
  assert.match(source, /data-action="open-start"[^-]/);
  assert.match(source, /else if \(action === "open-start"\) openStart\(\);/);
  assert.match(source, /if \(event\.key\.toLowerCase\(\) === "r" && canOpenStart\(state\.snapshot, state\.permissions\)\) \{\s*event\.preventDefault\(\);/);
  assert.match(source, /else if \(action === "open-start-guided"\) openStart\(true\);/);
});

// Roadmap packet W10: ⌘R must not open the start sheet in the same tick the
// first-run sheet is still showing -- both share the one unkeyed
// `.modal-backdrop` slot dom-patch matches positionally, so swapping their
// content within one render would skip the entrance animation W9-B
// guarantees every sheet plays once (the same failure class the two toast
// kinds were given distinct ids to avoid). ⌘R dismisses the first-run sheet
// instead, mirroring the Escape branch, so only one sheet ever mounts per
// tick.
test("⌘R dismisses the first-run sheet rather than opening Start over it in the same tick", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  const rBranch = source.slice(
    source.indexOf('if (event.key.toLowerCase() === "r"'),
    source.indexOf('if (event.key.toLowerCase() === "k"'),
  );
  assert.match(rBranch, /if \(firstRunSheetShowing\) \{ void closeFirstRunSheet\(\); return; \}/);
  assert.match(rBranch, /openStart\(\);/);
});

test("a row's preview is the backend's own sentence, trimmed, or nothing at all", () => {
  // Design intake D1: real generated content only, never a placeholder line.
  assert.equal(libraryRowPreview({ notePreview: "We reviewed Q3 pricing." }), "We reviewed Q3 pricing.");
  assert.equal(libraryRowPreview({ notePreview: "  padded on both sides  " }), "padded on both sides");
  // No admitted note, a blank field, or the field simply missing all read the
  // same way: no preview line, never invented copy standing in for one.
  assert.equal(libraryRowPreview({ notePreview: null }), null);
  assert.equal(libraryRowPreview({ notePreview: "" }), null);
  assert.equal(libraryRowPreview({ notePreview: "   " }), null);
  assert.equal(libraryRowPreview({}), null);
  assert.equal(libraryRowPreview(null), null);
  assert.equal(libraryRowPreview(undefined), null);
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
  // Roadmap intake I5 added `lockToken`: absent for an unlocked meeting,
  // which is the ordinary case, and a fresh single-use confirmation for a
  // locked one.
  assert.match(source, /invoke\("library_play_retained_audio", \{ handle, lockToken \}\)/);
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

// Roadmap intake I9: local trash for whole-meeting deletion.

test("the Trash link is absent when empty and names the count when it is not", () => {
  assert.equal(trashLinkPresentation(null), null);
  assert.equal(trashLinkPresentation({ entries: [] }), null);
  const link = trashLinkPresentation({ entries: [{ meetingId: "a" }, { meetingId: "b" }] });
  assert.deepEqual(link, { count: 2, label: "Trash (2)" });
});

test("the quiet Trash list reports loading, empty, and populated states without artwork", () => {
  assert.deepEqual(trashListPresentation(null), { state: "loading", entries: [] });
  assert.deepEqual(trashListPresentation({ entries: [] }), { state: "empty", entries: [] });

  const populated = trashListPresentation({
    entries: [
      { meetingId: "titled", label: "Kickoff", deletedAtEpochSeconds: 100, purgeAfterEpochSeconds: 200 },
      { meetingId: "12345678-untitled", label: "", deletedAtEpochSeconds: 300, purgeAfterEpochSeconds: 400 },
    ],
  });
  assert.equal(populated.state, "populated");
  assert.equal(populated.entries[0].label, "Kickoff");
  assert.equal(populated.entries[1].label, "Meeting · 12345678");
  assert.equal(populated.entries[1].purgeAfterEpochSeconds, 400);
});

test("deleting a recording or a transcript keeps its permanent-deletion copy unchanged", () => {
  const recording = meetingDeletionConfirmationCopy("delete-recording");
  assert.equal(recording.eyebrow, "Permanent deletion");
  assert.match(recording.detail, /permanently removes the saved microphone and system audio/);

  const transcript = meetingDeletionConfirmationCopy("delete-transcript");
  assert.equal(transcript.eyebrow, "Permanent deletion");
  assert.match(transcript.detail, /permanently removes the transcript and generated points/);
});

test("deleting a whole meeting states the trash-and-recovery truth plainly", () => {
  const copy = meetingDeletionConfirmationCopy("delete-meeting");
  assert.equal(copy.eyebrow, "Moves to Trash");
  assert.equal(copy.heading, "Delete this meeting?");
  // The three load-bearing facts a reader must come away with: it is
  // recoverable, for how long, and that past that window there is no
  // server copy to fall back on — stated plainly, not in legalese.
  assert.match(copy.detail, /moves to Trash/);
  assert.match(copy.detail, /restore it from there for 30 days/);
  assert.match(copy.detail, /no server copy, so it cannot be recovered/);
});

test("main.js wires the Trash list, restore action, and confirmation copy through the pure view-model", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /invoke\("preview_list_trash"\)/);
  assert.match(source, /invoke\("restore_meeting_from_trash_command", \{ meetingId \}\)/);
  assert.match(source, /trashLinkPresentation\(state\.trash\)/);
  assert.match(source, /trashListPresentation\(state\.trash\)/);
  assert.match(source, /meetingDeletionConfirmationCopy\(state\.modal\)/);
  // The link must be able to disappear entirely, not render an empty row.
  assert.match(source, /data-action="open-trash"/);
  assert.match(source, /data-action="restore-trash-entry"/);
});

// --- Roadmap intake I5: per-meeting lock ---------------------------------
//
// The governing constraint, verbatim: "State the honest claim -- a
// local-access deterrent -- unless encryption at rest actually ships." It has
// not shipped. These tests are what keeps the copy on the honest side of that
// line as the file changes; a reviewer noticing a word is not a mechanism.

test("the lock sheet says exactly what the lock is, and what it is not", () => {
  const copy = meetingLockSheetCopy("lock");
  assert.equal(copy.eyebrow, "Local barrier");
  assert.equal(copy.heading, "Lock this meeting?");
  assert.equal(copy.label, "Lock meeting");
  assert.equal(copy.blocked, false);
  // The three facts a reader must come away with: what closes, what reopens
  // it, and that nothing on disk changed.
  assert.match(copy.detail, /note, transcript, and audio will require Touch ID to open on this Mac/);
  // The words the packet requires. Their absence is the failure this test
  // exists to catch.
  assert.match(copy.detail, /not encryption/);
  assert.match(copy.detail, /the files on disk are unchanged/);
});

test("no lock copy anywhere claims encryption or security", () => {
  const surfaces = [
    meetingLockSheetCopy("lock"),
    meetingLockSheetCopy("unlock"),
    meetingLockSheetCopy("lock", { canConfirm: false }),
    meetingLockPresentation({ state: "locked", lock: { locked: true, unreadable: false } }),
    meetingLockPresentation({ state: "locked", lock: { locked: true, unreadable: true } }),
    meetingLockPresentation({ state: "note", lock: { locked: true, unreadable: false } }),
  ];
  for (const surface of surfaces) {
    for (const value of Object.values(surface)) {
      if (typeof value !== "string") continue;
      assert.ok(
        meetingLockCopyIsHonest(value),
        `lock copy claims more than a local barrier: ${value}`,
      );
    }
  }
});

test("removing the lock restates that nothing was ever encrypted", () => {
  const copy = meetingLockSheetCopy("unlock");
  assert.equal(copy.heading, "Remove the lock?");
  assert.equal(copy.label, "Remove lock");
  assert.match(copy.detail, /never encrypted/);
});

test("a Mac that cannot confirm is not offered a lock, and is told why", () => {
  // Decision 4 covers the machine that becomes unable to confirm later. This
  // is the machine that never could: locking there would make a meeting this
  // app can never reopen, so the sheet refuses instead of creating one.
  const copy = meetingLockSheetCopy("lock", { canConfirm: false });
  assert.equal(copy.blocked, true);
  assert.match(copy.detail, /cannot confirm it's you \(no Touch ID or password available\)/);
  assert.match(copy.detail, /could not be reopened here/);
  // Removing an existing lock is never blocked by this: an unlock runs the
  // check itself and reports its own outcome.
  assert.equal(meetingLockSheetCopy("unlock", { canConfirm: false }).blocked, false);
});

test("a locked meeting detail shows the barrier and no meeting content", () => {
  const presentation = meetingLockPresentation({
    state: "locked",
    lock: { locked: true, unreadable: false },
  });
  assert.equal(presentation.state, "locked");
  assert.equal(presentation.heading, "This meeting is locked");
  assert.match(presentation.detail, /not encryption/);
  assert.deepEqual(presentation.action, {
    action: "unlock-meeting-open",
    label: "Confirm to open",
  });
});

test("an unreadable lock states the actions that actually exist here", () => {
  // Previously this detail said "removing the lock is the only way to open
  // it here" while the barrier's own button offers a read-only Confirm to
  // open -- a wording mismatch (2026-09-01 desktop-design audit, fix 4).
  // Changed: old "Removing the lock is the only way to open it here." ->
  // new "Confirm to open it for reading, or remove the lock from Manage."
  const presentation = meetingLockPresentation({
    state: "locked",
    lock: { locked: true, unreadable: true },
  });
  assert.equal(presentation.state, "unreadable");
  assert.match(presentation.detail, /could not read this meeting's lock/);
  assert.match(presentation.detail, /Confirm to open it for reading/);
  assert.match(presentation.detail, /remove the lock from Manage/);
  assert.doesNotMatch(presentation.detail, /is the only way to open it here/);
  // The action rendered on this barrier really is the read-only confirm --
  // the same one the plain "locked" state offers -- so the new sentence
  // matches the button beside it rather than describing a different one.
  assert.deepEqual(presentation.action, {
    action: "unlock-meeting-open",
    label: "Confirm to open",
  });
});

test("a meeting opened for reading still says it is locked", () => {
  // Without this the reader cannot understand why playing the audio asks
  // again a moment later.
  const presentation = meetingLockPresentation({
    state: "note",
    lock: { locked: true, unreadable: false },
  });
  assert.equal(presentation.state, "open");
  assert.match(presentation.detail, /stays locked/);
  assert.match(presentation.detail, /asks for Touch ID again/);
  assert.equal(presentation.action, null);
});

test("an unlocked meeting shows no lock surface at all", () => {
  const presentation = meetingLockPresentation({
    state: "note",
    lock: { locked: false, unreadable: false },
  });
  assert.equal(presentation.state, "unlocked");
  assert.equal(presentation.action, null);
  assert.equal(meetingLockPresentation(null), null);
});

test("a locked row keeps its title and date and drops preview and transcript detail", () => {
  const unlocked = libraryRowMetaPresentation({
    locked: false,
    transcriptAvailable: true,
    notePreview: "We agreed to ship on Friday.",
  });
  assert.deepEqual(unlocked, {
    locked: false,
    label: "transcript available",
    preview: "We agreed to ship on Friday.",
  });

  // Bear's obscured previews. Both the generated preview and the
  // transcript/note-only detail go; "Locked" replaces the detail rather than
  // leaving the row saying nothing about itself.
  const locked = libraryRowMetaPresentation({
    locked: true,
    transcriptAvailable: true,
    notePreview: "We agreed to ship on Friday.",
  });
  assert.deepEqual(locked, { locked: true, label: "Locked", preview: null });
});

test("a row with no transcript still reads honestly when unlocked", () => {
  assert.equal(
    libraryRowMetaPresentation({ locked: false, transcriptAvailable: false }).label,
    "note only",
  );
  assert.equal(libraryRowMetaPresentation(undefined).label, "note only");
});

test("the three confirmation failures stay three different answers", () => {
  // "you cancelled", "this Mac cannot ask", and "that view is out of date"
  // lead to three different next moves and must never collapse into one.
  assert.deepEqual(
    lockedActionOutcome({ state: "authorized", message: "Confirmed on this Mac." }),
    { ok: true, state: "authorized", message: "Confirmed on this Mac." },
  );
  assert.equal(lockedActionOutcome({ state: "declined", message: "no" }).ok, false);
  assert.equal(lockedActionOutcome({ state: "declined" }).state, "declined");
  assert.equal(lockedActionOutcome({ state: "unavailable" }).state, "unavailable");
  assert.equal(lockedActionOutcome({ state: "stale" }).state, "stale");
  assert.equal(lockedActionOutcome({ state: "not-locked" }).state, "not-locked");
  // An absent response is not a success.
  assert.equal(lockedActionOutcome(undefined).ok, false);
  assert.equal(lockedActionOutcome(null).state, "unavailable");
});

test("main.js gates every locked action through the Rust confirmation, not through hiding", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // The three commands, and the exact argument names the Rust side declares.
  assert.match(source, /invoke\("lock_meeting", \{ handle: selection\.row\.handle \}\)/);
  assert.match(source, /invoke\("unlock_meeting", \{ handle: selection\.row\.handle \}\)/);
  assert.match(source, /invoke\("authorize_locked_action", \{ handle, action \}\)/);
  // Every gated command carries a token field, so a locked meeting's refusal
  // is the Rust command's, not this file declining to draw a button.
  assert.match(source, /invoke\("library_open_note", \{ handle: row\.handle, lockToken \}\)/);
  assert.match(source, /invoke\("library_play_retained_audio", \{ handle, lockToken \}\)/);
  assert.match(source, /invoke\("library_export_meeting", \{ handle, lockToken \}\)/);
  assert.match(source, /invoke\("library_open_transcript_file", \{ handle, lockToken \}\)/);
  // Export and playback each ask for their own confirmation.
  assert.match(source, /confirmLockedAction\(selection\.row\.handle, "export"\)/);
  assert.match(source, /confirmLockedAction\(state\.selected\?\.row\?\.handle, "playback"\)/);
  // The reading confirmation is spent and re-issued, so an ordinary refresh
  // inside an open locked meeting does not ask again.
  assert.match(source, /state\.selected\?\.note\?\.lockToken \|\| null/);
  // The barrier and both sheet paths are reachable.
  assert.match(source, /data-action="lock-meeting"/);
  assert.match(source, /data-action="unlock-meeting"/);
  // The barrier's button is rendered from the view model's own action name,
  // so assert the wiring at both ends rather than a literal that does not
  // appear in this file.
  assert.equal(
    meetingLockPresentation({ state: "locked", lock: { locked: true } }).action.action,
    "unlock-meeting-open",
  );
  assert.match(source, /action === "unlock-meeting-open"/);
  assert.match(source, /data-action="\$\{escapeHtml\(lock\.action\.action\)\}"/);
  assert.match(source, /meetingLockSheetCopy\(unlock \? "unlock" : "lock"/);
  assert.match(source, /meetingLockPresentation\(note\)/);
  assert.match(source, /libraryRowMetaPresentation\(row\)/);
  // Deletion is never gated: no lock check stands in front of the three
  // deletion commands. Decision 7 -- blocking them would make a meeting
  // nobody can confirm for permanently undeletable, and audio retention
  // outranks the lock.
  for (const command of [
    "preview_delete_meeting_audio",
    "preview_delete_meeting_transcript",
    "preview_delete_meeting",
  ]) {
    const call = new RegExp(`invoke\\("${command}", \\{[^}]*\\}`);
    const found = source.match(call);
    assert.ok(found, `${command} is still invoked`);
    assert.ok(!found[0].includes("lockToken"), `${command} must not be gated by the lock`);
  }
});

// Design intake D5: evidence disclosure's three depths (hover preview, split
// view, synced scroll). See view-model.mjs's own comments on each function
// for why the shape is what it is; these tests pin the observable behavior.

test("the hover popover previews the first cited span, quoted, with a speaker", () => {
  const claim = {
    ordinal: 2,
    spans: [
      { sourceTurnIndex: 3, start: 0, end: 5, text: "alpha" },
      { sourceTurnIndex: 5, start: 0, end: 5, text: "delta" },
    ],
  };
  const turns = [
    { sourceTurnIndex: 3, start: 12, speaker: "Jamie" },
    { sourceTurnIndex: 5, start: 40, speaker: "Sam" },
  ];
  const presentation = evidencePopoverPresentation(claim, turns);
  // Only the first span previews, even though the claim cites two turns --
  // one exact position, matching the split's own "first cited span" landing
  // rule rather than a second, different behavior for hover.
  assert.deepEqual(presentation, {
    text: "alpha",
    speaker: "Jamie",
    start: 12,
    sourceTurnIndex: 3,
  });
});

test("the hover popover falls back to Unattributed and a null time when the turn cannot be matched", () => {
  const claim = { ordinal: 0, spans: [{ sourceTurnIndex: 9, start: 0, end: 3, text: "hey" }] };
  const presentation = evidencePopoverPresentation(claim, []);
  assert.equal(presentation.speaker, "Unattributed");
  assert.equal(presentation.start, null);
  assert.equal(presentation.text, "hey");
});

test("the hover popover offers nothing for a claim with no batched spans", () => {
  // A locator that could not currently be re-sliced (see
  // `library_reader.rs`'s `claim_locator_spans`) leaves `spans` empty even
  // when `locatorCount` is nonzero -- the popover must not invent a preview.
  assert.equal(evidencePopoverPresentation({ ordinal: 1, locatorCount: 1, spans: [] }, []), null);
  assert.equal(evidencePopoverPresentation(null, []), null);
  assert.equal(evidencePopoverPresentation({ ordinal: 1, spans: [{ text: "" }] }, []), null);
});

test("the split view's width gate matches the governing 1100px threshold", () => {
  assert.equal(evidenceSplitAllowed(1099), false);
  assert.equal(evidenceSplitAllowed(1100), true);
  assert.equal(evidenceSplitAllowed(1400), true);
  assert.equal(evidenceSplitAllowed(undefined), false);
  assert.equal(evidenceSplitAllowed(Number.NaN), false);
});

test("synced scroll targets the first visible claim's first cited span, skipping source-free claims", () => {
  const claims = [
    { ordinal: 0, spans: [] }, // no locatable span -- must not stall the scan
    { ordinal: 1, spans: [{ sourceTurnIndex: 7 }] },
    { ordinal: 2, spans: [{ sourceTurnIndex: 2 }] },
  ];
  // Ordinal 0 is topmost but has nothing to target; ordinal 1 is next and
  // does.
  assert.equal(evidenceSyncTarget([0, 1, 2], claims), 7);
  // No visible claim carries a span at all.
  assert.equal(evidenceSyncTarget([0], claims), null);
  // Nothing visible.
  assert.equal(evidenceSyncTarget([], claims), null);
});

test("a manual transcript scroll is only the reader's once the sync-suppression window elapses", () => {
  // No sync-scroll call has ever suppressed anything yet -- any scroll is
  // the reader's own.
  assert.equal(isManualTranscriptScroll(1_000, undefined), true);
  assert.equal(isManualTranscriptScroll(1_000, null), true);
  // Still inside the window a sync-scroll call opened.
  assert.equal(isManualTranscriptScroll(1_000, 1_500), false);
  // The window has elapsed -- a scroll observed now is the reader's.
  assert.equal(isManualTranscriptScroll(1_600, 1_500), true);
});

test("Escape closes the innermost surface first: popover, then modal, then split", () => {
  assert.equal(nextEscapeTarget({ popoverOpen: true, modalOpen: true, splitOpen: true }), "popover");
  assert.equal(nextEscapeTarget({ popoverOpen: false, modalOpen: true, splitOpen: true }), "modal");
  assert.equal(nextEscapeTarget({ popoverOpen: false, modalOpen: false, splitOpen: true }), "split");
  assert.equal(nextEscapeTarget({}), null);
});

test("Show source renders only when a claim has a batched span to show, and never re-fetches on click", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // The button's own action now opens the split directly from already-batched
  // data -- no network round trip, no single-use evidence handle spent per
  // click.
  assert.match(source, /data-action="open-evidence-split" data-ordinal="\$\{escapeHtml\(claim\.ordinal\)\}"/);
  assert.match(source, /if \(!claim\.spans\?\.length\) \{/);
  // `openClaimEvidence` (the pre-D5 fetch-and-store path) is left in place,
  // not deleted -- see the D5 report's decision-2 note -- but no control
  // wired through `renderClaimEvidence` invokes it any more.
  assert.match(source, /async function openClaimEvidence\(ordinal\)/);
  assert.doesNotMatch(source, /data-action="open-claim-evidence"[^>]*>\$\{state\.busyAction/);
});

test("the evidence split and the below-the-note disclosure never render at once", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // `renderTranscript`'s workspace branch hardcodes ids
  // (transcript-heading, transcript-search-input, ...); two live instances
  // would collide under the patcher's id-keying. `splitActive` gates both
  // branches from the same variable, so they cannot both be true.
  assert.match(source, /\$\{splitActive \? "" : renderTranscriptDisclosure\(transcript, recovery, note\)\}/);
  assert.match(source, /\$\{splitActive \? renderEvidenceSplitColumn\(transcript, citations, evidenceSplit\.turnIndex\) : ""\}/);
  assert.match(source, /const splitActive = Boolean\(/);
  assert.match(source, /evidenceSplitAllowed\(currentWindowWidth\(\)\)/);
});

test("the split column's width fallback reads the live window, gated by the shared 1100px threshold", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /function currentWindowWidth\(\) \{/);
  assert.match(source, /window\.addEventListener\("resize", handleEvidenceResize\)/);
  assert.match(source, /function handleEvidenceResize\(\) \{\s*const allowed = evidenceSplitAllowed\(currentWindowWidth\(\)\);/);
});

test("the sync-scroll handler is rAF-batched, not a per-scroll-event geometry read", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /function scheduleEvidenceSyncFromNote\(\) \{/);
  assert.match(source, /if \(evidenceSyncRafPending\) return;\s*evidenceSyncRafPending = true;\s*window\.requestAnimationFrame\(\(\) => \{/);
  // The geometry read (topmostVisibleClaimOrdinals) and the pure decision
  // (evidenceSyncTarget) both live inside the rAF callback's function,
  // performEvidenceSync -- never inside the scroll listener itself.
  assert.match(source, /function performEvidenceSync\(\) \{[\s\S]*?topmostVisibleClaimOrdinals\(notePane\)/);
  assert.doesNotMatch(source, /function handleEvidenceGlobalScroll\(event\) \{[\s\S]{0,400}getBoundingClientRect/);
});

test("popover state lives outside the patched tree, and the split's own state survives a render tick", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // The popover is a plain node appended to document.body -- never part of
  // the template string patchInto(root, ...) patches -- so an unrelated
  // render() call (a 900ms poll tick, a keystroke elsewhere) cannot disturb
  // it while it is showing. This is the same "leave it alone unless it
  // changed" contract dom-patch.mjs gives editors, achieved here by simply
  // never putting the popover inside the patched tree at all.
  assert.match(source, /document\.body\.appendChild\(el\)/);
  assert.doesNotMatch(source, /evidence-popover[\s\S]{0,200}patchInto/);
  // The split's own state (`state.selected.evidenceSplit`) is read at render
  // time and rendered back out (`data-evidence-split`), so it is what makes
  // the split survive a patch tick -- not a DOM node the patcher happens to
  // leave alone. `render()`'s tail reconciles the DOM to that state on every
  // call, the same way `restoreEditorFocus` and `syncActivityClock` already
  // do for their own concerns.
  assert.match(source, /syncEvidenceSplitScroll\(\);\s*dismissEvidencePopoverIfDetached\(\);/);
  assert.match(source, /evidenceSplit: \{ open: false, ordinal: null, turnIndex: null \}/);
});
