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
  humanize,
  libraryRecoveryPresentation,
  libraryRowPreview,
  localVocabularyPresentation,
  meetingContextPresentation,
  meetingDeletionConfirmationCopy,
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
  transcriptCitationSummary,
  transcriptPlainText,
  transcriptRetryDiffPresentation,
  transcriptRetryQualityPresentation,
  transcriptRetryQualityKindLabel,
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
  assert.match(source, /renderMeetingNote\(note, claimEvidence\)\}\n\s*\$\{renderGenerateNote\(note, recovery\)\}\n\s*\$\{renderTranscriptRetryAction\(note, transcript, recovery\)\}\n\s*\$\{renderTranscriptDisclosure\(transcript, recovery, note\)\}/);
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
