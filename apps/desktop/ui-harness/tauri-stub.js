// Harness stub: stands in for the Tauri bridge so the real ui/main.js runs
// its real render loop (900 ms poll included) against fake backend state.
// mode=capture (default): an active recording, so the operator-note textarea
// is live and the snapshot poll re-renders every 900 ms.
// mode=library: idle capture with one finished meeting, so the transcript
// search input is live and every keystroke re-renders synchronously.
// mode=startup / mode=model-setup: the startup-check and no-model surfaces.
// mode=retry-sheet / mode=retry-sheet-no-note / mode=delete-sheet (R30): the
// two sheets no earlier mode could reach, so both are now capturable. Each is
// library state plus the one backend answer its sheet needs -- a retry
// comparison, or a deletion handle -- and the sheet is opened by clicking the
// same control a person clicks, not by setting `state.modal` from outside.
// The retry sheet gets three modes: it forks on whether a note exists (the
// warning) and on whether the diff was computed or skipped (the sentence
// above the columns), and each fork has a reading only a fixture can show.
(() => {
  const mode = new URLSearchParams(location.search).get("mode") || "capture";
  const captureSnapshot = {
    startup: "ready",
    capture: "recording",
    meeting_id: "harness-meeting-1",
    mic_state: "capturing",
    system_state: "capturing",
    capture_state_started_at_epoch_seconds: Math.floor(Date.now() / 1000) - 30,
    turns: [],
    warnings: [],
  };
  const idleSnapshot = { startup: "ready", capture: "idle", meeting_id: "", turns: [], warnings: [] };
  const libraryRow = {
    handle: "row-handle-1",
    meetingId: "harness-meeting-1",
    label: "Harness meeting",
    labelSource: "operator",
    createdAtEpochSeconds: Math.floor(Date.now() / 1000) - 3600,
    transcriptAvailable: true,
  };
  const sheetMode = mode === "retry-sheet" || mode === "retry-sheet-no-note"
    || mode === "retry-sheet-diff-skipped" || mode === "delete-sheet";
  // The retry sheet forks on whether a generated note exists, and picking one
  // side of a fork as "the" fixture is how a branch stays unreachable after
  // the surface is supposedly covered (R23/R24b: making a hidden state visible
  // exposed copy written for the other side). `retry-sheet` has a note, so the
  // "using this retry clears it" warning renders; `retry-sheet-no-note` is the
  // same meeting transcript-only, where that warning is suppressed -- and the
  // warning is the element carrying the 16px above the button row, so R32's
  // measured gap is a different number on each side.
  const hasNote = mode !== "retry-sheet-no-note";
  // `RetryDiffProjection`'s own comment: "skipped" (a bound was hit, nothing
  // was compared) is deliberately a different wire shape from "computed with
  // zero spans" (compared, identical), so the sentence above the columns can
  // tell the reader "not checked" from "checked, same". Both readings need a
  // fixture or the distinction is untestable.
  const diffSkipped = mode === "retry-sheet-diff-skipped";
  // Two turns whose wording differs, so the diff columns render marked words
  // rather than two identical transcripts. Word counts are stated exactly as
  // the Rust projection states them, because the browser tokenizes each turn
  // itself and refuses a span whose count disagrees with its own.
  const currentTurns = [
    { start: 1, end: 3, speaker: "Me", text: "alpha decision about the launch", sourceTurnIndex: 0 },
    { start: 4, end: 6, speaker: "Them", text: "bravo follow-up owed by Friday", sourceTurnIndex: 1 },
  ];
  const candidateTurns = [
    { start: 1, end: 3, speaker: "Me", text: "alpha decision about the launch date", sourceTurnIndex: 0 },
    { start: 4, end: 6, speaker: "Them", text: "bravo follow-up owed by Thursday", sourceTurnIndex: 1 },
  ];
  // A generated note, so the retry sheet renders its "this clears the current
  // note" warning (it is suppressed on a transcript-only meeting) and the
  // transcript column renders the cited-turn control.
  const sheetNote = {
    meetingId: "harness-meeting-1",
    state: hasNote ? "note" : "transcript-only",
    claims: hasNote ? [
      { ordinal: 1, claimType: "decision", claim: "Launch on Friday.", handle: "claim-handle-1" },
      { ordinal: 2, claimType: "action", claim: "Send the pricing note to finance.", handle: "claim-handle-2" },
    ] : [],
    turnsCited: hasNote ? [
      { turn: 0, claimOrdinals: [1] },
      { turn: 1, claimOrdinals: [2] },
    ] : [],
    noteGenerationAvailable: true,
    operatorNote: { text: "", unreadable: false },
    operatorNoteHandle: "note-handle-1",
    transcriptHandle: "transcript-handle-1",
    // Only mode=delete-sheet offers the deletion; `canDeleteMeeting` reads
    // exactly this handle, which is why Manage showed no Move to Trash before.
    meetingDeletionHandle: mode === "delete-sheet" ? "deletion-handle-1" : undefined,
    audioRetention: { state: "retained", message: "Audio retained on this Mac." },
    capturePauses: null,
  };
  // `RetryComparisonResponse` in src-tauri/src/main.rs, field for field.
  const retryComparison = {
    meetingId: "harness-meeting-1",
    operationId: "00000000-0000-4000-8000-000000000001",
    sourceTranscriptSha256: "0".repeat(64),
    candidateTranscriptSha256: "1".repeat(64),
    current: { turns: currentTurns, warnings: [] },
    candidate: { turns: candidateTurns, warnings: [] },
    quality: {
      state: "available",
      observations: [
        { kind: "silence", status: "not-observed", message: "No long silences." },
        { kind: "clipping", status: "not-observed", message: "No clipping." },
        { kind: "low-input", status: "observed", message: "Input level was low for part of this recording." },
        { kind: "background-noise", status: "unknown", message: "Not measured." },
      ],
      message: "One thing worth knowing about this recording.",
    },
    recordingDevice: { state: "identified", message: "MacBook Pro Microphone" },
    pauses: { state: "not-paused", count: 0, totalPausedSeconds: 0, message: "Nothing was paused." },
    diff: diffSkipped
      ? { state: "skipped", current: [], candidate: [] }
      : {
          state: "computed",
          current: [{ turnIndex: 1, wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] }],
          candidate: [
            { turnIndex: 0, wordCount: 6, spans: [{ startWord: 5, endWord: 6 }] },
            { turnIndex: 1, wordCount: 5, spans: [{ startWord: 4, endWord: 5 }] },
          ],
        },
  };
  const responses = {
    // mode=startup: the local startup check still running; mode=model-setup:
    // no speech model installed yet. Both render surfaces the other modes
    // never reach, so the harness can show them without a packaged build.
    app_snapshot: () => (mode === "capture" ? { ...captureSnapshot }
      : mode === "startup" ? { ...idleSnapshot, startup: "checking", startup_message: "Verifying on-device speech models." }
      : mode === "model-setup" ? { ...idleSnapshot, startup: "model-required", model_setup: { state: "idle", selectedModelId: "", options: [
          // The catalog's real two speech models, so the surface renders the
          // row count, titles, and sizes it renders in a packaged build.
          { id: "whisper-large-v3-turbo-q4", title: "Smaller download", detail: "A 4-bit local transcription model that uses about 464 MB.", downloadBytes: 463665005, installedBytes: 463665005 },
          { id: "whisper-large-v3-turbo", title: "Full model", detail: "The full local Turbo transcription model, using about 1.61 GB.", downloadBytes: 1613977880, installedBytes: 1613977880 },
        ] } }
      : { ...idleSnapshot }),
    first_run_permissions: () => ({ microphone: "authorized", systemAudio: "authorized", probeUnavailable: false }),
    library_snapshot: () => (mode === "library" || mode === "summary-failed" || sheetMode
      ? { rows: [{ ...libraryRow }], total: 1, metadataRevision: 1 }
      : { rows: [], total: 0, metadataRevision: 1 }),
    preview_list_trash: () => ({ entries: [] }),
    operator_note: () => ({ text: "", unreadable: false }),
    meeting_context: () => ({ text: "", unreadable: false }),
    save_operator_note: () => ({ unreadable: false }),
    save_meeting_context: () => ({ unreadable: false }),
    // mode=summary-failed: the same meeting after a rejected generation,
    // audio released, with a source pin so the retry control renders (R23).
    library_open_note: () => (sheetMode ? { ...sheetNote } : mode === "summary-failed" ? {
      meetingId: "harness-meeting-1",
      state: "summary-failed",
      claims: [],
      regenerationSourceSha256: "0000000000000000000000000000000000000000000000000000000000000000",
      noteGenerationAvailable: true,
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      audioRetention: { state: "released", message: "Audio released." },
      capturePauses: null,
    } : {
      meetingId: "harness-meeting-1",
      state: "transcript-only",
      claims: [],
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    }),
    library_open_transcript: () => ({
      meetingId: "harness-meeting-1",
      state: "available",
      currentTranscriptSha256: "0000000000000000000000000000000000000000000000000000000000000000",
      transcriptFileHandle: "transcript-file-handle-1",
      turns: currentTurns.map((turn) => ({ ...turn })),
    }),
    // R30. `transcript_retry_pending` answers "is a candidate already waiting";
    // null means no, which sends `openTranscriptRetry` down the start path and
    // makes the sheet appear from one click on Retry transcript.
    transcript_retry_pending: () => null,
    transcript_retry_start: () => ({ ...retryComparison }),
    library_save_operator_note: (args) => ({
      operatorNote: { text: args?.text || "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
    }),
    library_retained_audio_playback_status: () => ({ state: "idle", source: null, message: "" }),
  };
  window.__TAURI__ = {
    core: {
      invoke: (command, args) => {
        const handler = responses[command];
        if (!handler) return Promise.reject(new Error(`harness: unstubbed command ${command}`));
        return Promise.resolve(handler(args));
      },
    },
    event: { listen: () => Promise.resolve(() => {}) },
  };
})();
window.__errors = [];
window.addEventListener("error", (event) => window.__errors.push(String(event.message)));
window.addEventListener("unhandledrejection", (event) => window.__errors.push(String(event.reason)));
