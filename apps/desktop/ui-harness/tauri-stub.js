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
  // A fixed time of day for the fixture rows, so two captures of one state
  // minutes apart (light, then dark) show the same meta line.
  const fixtureEpoch = (daysAgo, hour, minute) => {
    const at = new Date();
    at.setDate(at.getDate() - daysAgo);
    at.setHours(hour, minute, 0, 0);
    return Math.floor(at.getTime() / 1000);
  };
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
  let stopStatusSnapshot = { ...captureSnapshot };
  // mode=stop-lands: a normal Stop as the backend does it now. The take is
  // queued for background transcription, the capture steps Captured -> Idle
  // and clears the meeting projection (main.rs, after enqueue), so nothing in
  // the snapshot names the meeting any more. `library_snapshot` leaves the
  // meeting out while its capture holds it, as the real reader excludes active
  // meetings, and the library is otherwise empty -- the window was opened on
  // an empty library before the recording began.
  const stopLandsMode = mode === "stop-lands";
  // `&selected=1`: the library also holds an earlier meeting, and the scenario
  // opens it during the recording, so Stop lands with another meeting open.
  const stopLandsSelected = stopLandsMode && new URLSearchParams(location.search).has("selected");
  window.__harnessLibraryReads = 0;
  // mode=mic-change-stop: the microphone setup changes mid-recording (a call
  // ending), and the backend stops, saves and queues the take by itself — no
  // Stop click. Like any finalized take, it then steps Captured -> Idle and
  // clears the meeting projection (main.rs, after enqueue), so the notice
  // rides on an idle snapshot, not a "transcribing" one.
  // mode=mic-change-failed: the same change while arming, which still fails.
  // mode=mic-change-resume-failed: the change while resuming a paused take,
  // which also fails; the meeting is kept as interrupted.
  // Both failure modes show the capture pane's first moment. The meeting
  // record is created RecoveredInterrupted, so once the library lists it the
  // UI opens its "did not finish" page instead (see the cold review record).
  // Both modes carry a library, as a real install would. Copy is the
  // backend's own, verbatim.
  const MIC_CHANGE_SAVED = "Recording stopped because the microphone setup changed. Your audio was saved and queued for transcription.";
  const MIC_CHANGE_FAILED = "Recording did not start because the microphone setup changed. No audio was captured. Check your audio input, then press Record.";
  const MIC_CHANGE_RESUME_FAILED = "Recording could not resume because the microphone setup changed. This meeting is kept as interrupted. Check your audio input before starting another recording.";
  let micChangeSnapshot = { ...captureSnapshot };
  if (mode === "mic-change-stop") {
    setTimeout(() => {
      micChangeSnapshot = {
        ...idleSnapshot,
        capture_notice: MIC_CHANGE_SAVED,
        background_transcription_active: true,
        background_transcription_queued_count: 1,
      };
    }, 1200);
  }
  const idleSnapshot = { startup: "ready", capture: "idle", meeting_id: "", turns: [], warnings: [] };
  const libraryRow = {
    handle: "row-handle-1",
    meetingId: "harness-meeting-1",
    label: "Harness meeting",
    labelSource: "operator",
    createdAtEpochSeconds: fixtureEpoch(1, 14, 10),
    transcriptAvailable: true,
  };
  // mode=transcribing-meeting / transcribing-meeting-lands: the state right
  // after a normal Stop. The backend has queued the take, capture is idle, and
  // the newest meeting is listed (and auto-opened) with no transcript yet --
  // `library_reader.rs` answers "transcribing" for its note and
  // `transcriptPending` on its row while the queue still holds the request. The
  // scenario calls `window.__harnessLandTranscript()` to make the transcript
  // arrive, after which the same meeting reads as an ordinary transcript-ready
  // one and the queue is empty. The personal note a person typed is kept, as
  // the real `library_save_operator_note` keeps it.
  const transcribingMode = mode === "transcribing-meeting" || mode === "transcribing-meeting-lands";
  // `&legacy=1` answers with the backend contract from before the fix: the
  // just-stopped meeting opens as "transcript-only" with the retained-meeting
  // message, and its row carries no pending flag. It is the negative control --
  // the scenario is expected to FAIL against it, and the "before" frames come
  // from it.
  const legacyContract = new URLSearchParams(location.search).has("legacy");
  let transcriptLanded = false;
  let savedOperatorNote = "";
  window.__harnessLandTranscript = () => { transcriptLanded = true; };
  const sheetMode = mode === "retry-sheet" || mode === "retry-sheet-no-note"
    || mode === "retry-sheet-diff-skipped" || mode === "delete-sheet";
  const searchMode = mode === "search-results" || mode === "search-capture";
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
  // mode=ledger (Rail, 2026-10-15): a transcript-ready meeting whose operator
  // note carries a moment per line, a transcript spread across the meeting,
  // one pause, and retained audio with a deadline -- the state the ledger
  // (rail | note | nearest turn) and the transport are judged on. Invented.
  const ledgerTurns = [
    { start: 4, end: 20, speaker: "You", text: "Okay, let's go through the launch checklist from the top.", sourceTurnIndex: 0 },
    { start: 31, end: 40, speaker: "Them", text: "The retry review is the only thing still open on my side.", sourceTurnIndex: 1 },
    { start: 44, end: 50, speaker: "You", text: "Can that ship before Friday? Sam, is that yours?", sourceTurnIndex: 2 },
    { start: 52, end: 70, speaker: "Them", text: "Yes. I'll own it. Friday is fine if nothing else lands on me.", sourceTurnIndex: 3 },
    { start: 118, end: 126, speaker: "Them", text: "One thing on the retry flow. People pick keep or promote too fast.", sourceTurnIndex: 4 },
    { start: 127, end: 140, speaker: "You", text: "Then the diff has to be on screen before either button is.", sourceTurnIndex: 5 },
    { start: 141, end: 150, speaker: "Them", text: "Agreed. We'll block the buttons until the diff renders.", sourceTurnIndex: 6 },
    { start: 236, end: 248, speaker: "You", text: "Export. Does the thirty-day trash window apply to exported bundles?", sourceTurnIndex: 7 },
    { start: 249, end: 260, speaker: "Them", text: "I don't think we ever decided that. Let's park it as an open question.", sourceTurnIndex: 8 },
    { start: 431, end: 437, speaker: "You", text: "Release note. Who's drafting it?", sourceTurnIndex: 9 },
    { start: 438, end: 445, speaker: "Them", text: "Dana said she'd have a draft by Thursday.", sourceTurnIndex: 10 },
    { start: 672, end: 685, speaker: "Them", text: "What's the most we can spend on the installer work without going back to the board?", sourceTurnIndex: 11 },
    { start: 686, end: 700, speaker: "You", text: "If it stays under twelve hundred, nobody has to vote on it.", sourceTurnIndex: 12 },
    { start: 895, end: 903, speaker: "You", text: "Speaking of QA, when does the installer pass happen?", sourceTurnIndex: 13 },
    { start: 904, end: 915, speaker: "Them", text: "Monday. Full pass on a clean machine.", sourceTurnIndex: 14 },
    { start: 1171, end: 1182, speaker: "You", text: "Notarization has been slow all week. Has anyone talked to Priya?", sourceTurnIndex: 15 },
    { start: 1183, end: 1190, speaker: "Them", text: "Not yet. I'll ask her after this.", sourceTurnIndex: 16 },
    { start: 1366, end: 1380, speaker: "You", text: "Last thing. Same time on the twenty-eighth?", sourceTurnIndex: 17 },
    { start: 1381, end: 1385, speaker: "Them", text: "Works for me.", sourceTurnIndex: 18 },
  ];
  const ledgerNote = {
    text: "retry review ships before Friday — Sam owns\ndiff must be on screen before keep / promote\nexport: does the 30-day window cover bundles? (open)\nDana → release note Thursday\nbudget stays under 1,200 → no board vote\nQA pass on the installer Monday\nask Priya about the notarization delay\n\nfollow up with Sam on Monday",
    moments: [{ line: 0, seconds: 48 }, { line: 1, seconds: 130 }, { line: 2, seconds: 245 }, { line: 3, seconds: 440 }, { line: 4, seconds: 735 }, { line: 5, seconds: 947 }, { line: 6, seconds: 1225 }],
    unreadable: false,
  };
  const currentTurns = [
    { start: 1, end: 3, speaker: "Me", text: "alpha decision about the launch", sourceTurnIndex: 0 },
    { start: 4, end: 6, speaker: "Them", text: "bravo follow-up owed by Friday", sourceTurnIndex: 1 },
  ];
  const candidateTurns = [
    { start: 1, end: 3, speaker: "Me", text: "alpha decision about the launch date", sourceTurnIndex: 0 },
    { start: 4, end: 6, speaker: "Them", text: "bravo follow-up owed by Thursday", sourceTurnIndex: 1 },
  ];
  const searchTurns = [
    { start: 1, end: 3, sourceSpeaker: "Me", speaker: "Me", text: "Opening context from the earlier meeting.", sourceTurnIndex: 0 },
    { start: 4, end: 7, sourceSpeaker: "Them", speaker: "Them", text: "The exact budget decision is recorded here.", sourceTurnIndex: 1 },
    { start: 8, end: 10, sourceSpeaker: "Them", speaker: "Them", text: "", withheld: true, sourceTurnIndex: 2 },
  ];
  let openedSearchMeeting = false;
  let searchResultHandle = "";
  let searchTranscriptReadCount = 0;
  // A generated note, so the retry sheet renders its "this clears the current
  // note" warning (it is suppressed on a transcript-only meeting) and the
  // transcript column renders the cited-turn control.
  const sheetNote = {
    meetingId: "harness-meeting-1",
    state: hasNote ? "note" : "transcript-only",
    claims: hasNote ? [
      { ordinal: 1, claimType: "decision", claim: "Launch on Friday.", handle: "claim-handle-1", locatorCount: 1, spans: [{ sourceTurnIndex: 0, start: 0, end: 30, text: currentTurns[0].text }] },
      { ordinal: 2, claimType: "action", claim: "Send the pricing note to finance.", handle: "claim-handle-2", locatorCount: 1, spans: [{ sourceTurnIndex: 1, start: 30, end: 60, text: currentTurns[1].text }] },
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
    first_run_permissions: () => ({ microphone: "authorized", systemAudio: "authorized", probeUnavailable: false }),
    transcript_model_settings: () => ({ state: "idle", options: [], activeModelId: null, selectedModelId: null, downloadedBytes: 0, totalBytes: 0, error: null, changeActive: false, canChange: true, unavailableReason: null }),
    get_transcription_engine_settings: () => ({ selected: "apple-native", canChange: true, operationActive: false, apple: { state: "ready", reason: null, locale: "en-US" }, whisper: { state: "ready", reason: null } }),
    nemotron_model_settings: () => ({ state: "ready", stored: true, totalBytes: 107012128, downloadedBytes: 107012128, changeActive: false, canChange: true, unavailableReason: null }),
    // mode=startup: the local startup check still running; mode=model-setup:
    // no speech model installed yet. Both render surfaces the other modes
    // never reach, so the harness can show them without a packaged build.
    app_snapshot: () => (mode === "capture" || mode === "search-capture" ? { ...captureSnapshot }
      : transcribingMode ? { ...idleSnapshot, background_transcription_active: !transcriptLanded, background_transcription_queued_count: transcriptLanded ? 0 : 1 }
      : mode === "stop-status" || stopLandsMode ? { ...stopStatusSnapshot }
      : mode === "mic-change-stop" ? { ...micChangeSnapshot }
      : mode === "mic-change-failed" ? { ...captureSnapshot, meeting_id: "harness-meeting-new", capture: "recovered-interrupted", mic_state: "failed", system_state: "stopped", error: MIC_CHANGE_FAILED }
      : mode === "mic-change-resume-failed" ? { ...captureSnapshot, meeting_id: "harness-meeting-new", capture: "recovered-interrupted", mic_state: "failed", system_state: "stopped", error: MIC_CHANGE_RESUME_FAILED }
      : mode === "startup" ? { ...idleSnapshot, startup: "checking", startup_message: "Verifying on-device speech models." }
      // first-run: idle, permissions granted, and no meetings (the library
      // branch below returns no rows for it), so the window shows the
      // first-run page from DESIGN.md.
      : mode === "first-run" ? { ...idleSnapshot }
      : mode === "native-ready" ? { ...idleSnapshot, transcriptionEngine: { selected: "apple-native", canChange: true, operationActive: false, apple: { state: "ready", reason: null, locale: "en-US" }, whisper: { state: "ready", reason: null } } }
      : mode === "apple-assets-required" ? { ...idleSnapshot, startup: "model-required", transcriptionEngine: { selected: null, canChange: true, operationActive: false, apple: { state: "assets-required", reason: "Apple speech needs a one-time preparation.", locale: "en-US" }, whisper: { state: "download-required", reason: null } }, model_setup: { state: "idle", selectedModelId: "", options: [
          { id: "whisper-large-v3-turbo-q4", title: "Smaller download", detail: "A 4-bit local transcription model that uses about 464 MB.", downloadBytes: 463665005, installedBytes: 463665005 },
          { id: "whisper-large-v3-turbo", title: "Full model", detail: "The full local Turbo transcription model, using about 1.61 GB.", downloadBytes: 1613977880, installedBytes: 1613977880 },
        ] } }
      : mode === "apple-installing" ? { ...idleSnapshot, startup: "model-required", transcriptionEngine: { selected: null, canChange: false, operationActive: true, apple: { state: "installing", reason: "Preparing Apple speech on this Mac…", locale: "en-US" }, whisper: { state: "download-required", reason: null } }, model_setup: { state: "downloading", selectedModelId: "", options: [] } }
      : mode === "apple-failed" ? { ...idleSnapshot, startup: "model-required", transcriptionEngine: { selected: null, canChange: true, operationActive: false, apple: { state: "failed", reason: "Apple speech could not be prepared.", locale: "en-US" }, whisper: { state: "download-required", reason: null } }, model_setup: { state: "idle", selectedModelId: "", options: [
          { id: "whisper-large-v3-turbo-q4", title: "Smaller download", detail: "A 4-bit local transcription model that uses about 464 MB.", downloadBytes: 463665005, installedBytes: 463665005 },
          { id: "whisper-large-v3-turbo", title: "Full model", detail: "The full local Turbo transcription model, using about 1.61 GB.", downloadBytes: 1613977880, installedBytes: 1613977880 },
        ] } }
      : mode === "model-setup" ? { ...idleSnapshot, startup: "model-required", model_setup: { state: "idle", selectedModelId: "", options: [
          // The catalog's real two speech models, so the surface renders the
          // row count, titles, and sizes it renders in a packaged build.
          { id: "whisper-large-v3-turbo-q4", title: "Smaller download", detail: "A 4-bit local transcription model that uses about 464 MB.", downloadBytes: 463665005, installedBytes: 463665005 },
          { id: "whisper-large-v3-turbo", title: "Full model", detail: "The full local Turbo transcription model, using about 1.61 GB.", downloadBytes: 1613977880, installedBytes: 1613977880 },
        ] } }
      : { ...idleSnapshot }),
    first_run_permissions: () => {
      const scene = new URLSearchParams(location.search).get("scene") || "";
      const systemAudioUnmeasured = scene === "system-audio-unmeasured" || scene === "system-audio-unmeasured-empty";
      return {
        microphone: "authorized",
        systemAudio: systemAudioUnmeasured ? "unmeasured" : "authorized",
        probeUnavailable: false,
      };
    },
    first_run_request_microphone: () => ({ microphone: "authorized", systemAudio: "unmeasured", probeUnavailable: false, prompted: true }),
    first_run_request_system_audio: () => {
      const scene = new URLSearchParams(location.search).get("scene") || "";
      const systemAudioUnmeasured = scene === "system-audio-unmeasured" || scene === "system-audio-unmeasured-empty";
      // Layout scenes that pin the unmeasured Record gate must keep the probe
      // honest without flipping the fixture to authorized mid-capture.
      return {
        microphone: "unmeasured",
        systemAudio: systemAudioUnmeasured ? "unmeasured" : "authorized",
        probeUnavailable: false,
        prompted: true,
      };
    },
    library_snapshot: () => (++window.__harnessLibraryReads, stopLandsMode ? (() => {
        const capturing = ["arming", "recording", "paused", "stopping", "captured", "transcribing"].includes(stopStatusSnapshot.capture);
        const rows = [
          ...(capturing ? [] : [
            { ...libraryRow, label: "", labelSource: "date", createdAtEpochSeconds: Math.floor(Date.now() / 1000) - 60,
              transcriptAvailable: false, transcriptPending: true, recovery: null, durationSeconds: 1079 },
          ]),
          ...(stopLandsSelected ? [{ ...libraryRow, handle: "row-handle-2", meetingId: "harness-meeting-2", label: "Earlier harness meeting",
            createdAtEpochSeconds: fixtureEpoch(9, 9, 30), transcriptPending: false, recovery: null }] : []),
        ];
        return { rows, total: rows.length, metadataRevision: 1, searchProbeEnabled: false };
      })()
      : mode === "mic-change-stop" ? {
        // The just-stopped take, listed with its transcript still pending.
        rows: [
          { ...libraryRow, createdAtEpochSeconds: Math.floor(Date.now() / 1000) - 60, transcriptAvailable: false,
            transcriptPending: true, recovery: null },
          { ...libraryRow, handle: "row-handle-2", meetingId: "harness-meeting-2", label: "Earlier harness meeting",
            createdAtEpochSeconds: fixtureEpoch(9, 9, 30), transcriptPending: false, recovery: null },
        ], total: 2, metadataRevision: 1, searchProbeEnabled: false }
      : transcribingMode ? {
        rows: [
          // Just recorded and not yet named: no label, so the sidebar and the
          // page both fall back to "Meeting · <date>", as they do for real.
          { ...libraryRow, label: "", labelSource: "date", createdAtEpochSeconds: Math.floor(Date.now() / 1000) - 90,
            transcriptAvailable: transcriptLanded, transcriptPending: legacyContract ? undefined : !transcriptLanded, recovery: null, durationSeconds: 1832 },
          { ...libraryRow, handle: "row-handle-2", meetingId: "harness-meeting-2", label: "Earlier harness meeting",
            createdAtEpochSeconds: fixtureEpoch(9, 9, 30), transcriptPending: false, recovery: null },
        ], total: 2, metadataRevision: 1, searchProbeEnabled: false }
      : mode === "library" || mode === "ledger" || mode === "interrupted" || mode === "fidelity" || mode === "mic-change-failed" || mode === "mic-change-resume-failed" || mode === "transcript-retirement" || mode === "summary-failed" || mode === "saved-draft" || mode === "saved-draft-unreadable" || sheetMode || searchMode
      ? { rows: [{ ...libraryRow, ...(mode === "ledger" ? { durationSeconds: 1458 } : mode === "interrupted" ? { durationSeconds: 401, recovery: "recovered-interrupted", transcriptAvailable: false } : {}) }, {
          ...libraryRow,
          handle: "row-handle-2",
          meetingId: "harness-meeting-2",
          label: "Earlier harness meeting",
          createdAtEpochSeconds: fixtureEpoch(9, 9, 30),
        }], total: 2, metadataRevision: 1, searchProbeEnabled: searchMode }
      : { rows: [], total: 0, metadataRevision: 1 }),
    preview_list_trash: () => ({ entries: [] }),
    operator_note: () => ({ text: "", unreadable: false }),
    meeting_context: () => ({ text: "", unreadable: false }),
    save_operator_note: () => ({ unreadable: false }),
    save_meeting_context: () => ({ unreadable: false }),
    start_meeting: () => ({ ...captureSnapshot }),
    stop_meeting: () => {
      stopStatusSnapshot = {
        ...captureSnapshot,
        capture: "stopping",
        capture_state_started_at_epoch_seconds: Math.floor(Date.now() / 1000),
      };
      if (stopLandsMode) {
        setTimeout(() => {
          stopStatusSnapshot = {
            ...idleSnapshot,
            background_transcription_active: true,
            background_transcription_queued_count: 1,
          };
        }, 600);
      }
      if (mode === "stop-status") {
        setTimeout(() => {
          stopStatusSnapshot = {
            ...stopStatusSnapshot,
            capture: "transcribing",
            capture_state_started_at_epoch_seconds: Math.floor(Date.now() / 1000),
          };
        }, 500);
      }
      return { ...stopStatusSnapshot };
    },
    preview_library_search: ({ query }) => {
      const normalized = String(query || "").trim().toLowerCase();
      if (normalized === "noresult") return { state: "no-results", results: [], message: "No retained transcript, title, or folder matched that search." };
      if (normalized === "incomplete") return { state: "incomplete", results: [], message: "No retained transcript, title, or folder match was found among readable meetings. 1 could not be searched." };
      if (normalized === "withheld") return {
        state: "results",
        results: [{ handle: "withheld-search-handle", kind: "withheld", meetingId: "harness-meeting-2", text: null }],
        message: "",
      };
      if (normalized === "metadata") return {
        state: "results",
        results: [{ handle: "metadata-search-handle", kind: "meeting", meetingId: "harness-meeting-2", text: null }],
        message: "",
      };
      if (normalized === "slow") return {
        state: "results",
        results: [{ handle: "slow-search-handle", kind: "transcript", meetingId: "harness-meeting-2", text: "The exact budget decision is recorded here." }],
        message: "",
      };
      if (["changed", "unavailable"].includes(normalized)) return {
        state: "results",
        results: [{ handle: `${normalized}-search-handle`, kind: "transcript", meetingId: "harness-meeting-2", text: "The exact budget decision is recorded here." }],
        message: "",
      };
      return {
        state: "results",
        results: [{ handle: "transcript-search-handle", kind: "transcript", meetingId: "harness-meeting-2", text: "The exact budget decision is recorded here." }],
        message: "",
      };
    },
    preview_library_open_search_result: ({ handle }) => {
      openedSearchMeeting = true;
      searchResultHandle = handle;
      searchTranscriptReadCount = 0;
      if (handle === "slow-search-handle") return new Promise((resolve) => setTimeout(() => resolve({
        state: "transcript", transcriptHandle: "search-transcript-handle", meetingId: "harness-meeting-2", sourceTurnIndex: 1, start: 0, end: 36,
        message: "Opening the exact retained transcript turn that matched.",
      }), 180));
      if (handle === "withheld-search-handle") return {
        state: "withheld", transcriptHandle: "search-transcript-handle", meetingId: "harness-meeting-2", sourceTurnIndex: 2, start: null, end: null,
        message: "A voice check withheld this matching turn. It is not shown as transcript text.",
      };
      if (handle === "metadata-search-handle") return {
        state: "metadata-only", transcriptHandle: null, meetingId: "harness-meeting-2", sourceTurnIndex: null, start: null, end: null,
        message: "No transcript was created for this retained meeting.",
      };
      if (handle === "changed-search-handle") return {
        state: "transcript", transcriptHandle: "search-transcript-handle", meetingId: "harness-meeting-2", sourceTurnIndex: 1, start: 0, end: 36,
        message: "Opening the exact retained transcript turn that matched.",
      };
      return {
        state: "transcript", transcriptHandle: "search-transcript-handle", meetingId: "harness-meeting-2", sourceTurnIndex: 1, start: 0, end: 36,
        message: "Opening the exact retained transcript turn that matched.",
      };
    },
    // mode=summary-failed: the same meeting after a rejected generation,
    // audio released, with a source pin so the retry control renders (R23).
    library_open_note: ({ handle } = {}) => (stopLandsMode && handle === "row-handle-2" ? {
      // `&selected=1`: the earlier meeting, opened while the recording runs.
      meetingId: "harness-meeting-2",
      state: "transcript-only",
      claims: [],
      noteGenerationAvailable: false,
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-2",
      transcriptHandle: null,
      microphonePlaybackHandle: null,
      systemPlaybackHandle: null,
      message: "No admitted note is available. Retained transcript text remains available.",
      audioRetention: { state: "released", message: "Audio was released." },
      capturePauses: null,
    } : mode === "mic-change-stop" || stopLandsMode ? {
      // The just-stopped take, still queued: library_reader's "transcribing"
      // note, no transcript handle yet.
      meetingId: "harness-meeting-1",
      state: "transcribing",
      claims: [],
      noteGenerationAvailable: false,
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: null,
      microphonePlaybackHandle: "mic-playback-1",
      systemPlaybackHandle: "system-playback-1",
      message: "Your audio is saved on this Mac and is being transcribed. The transcript will appear here when it's ready.",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    } : transcribingMode ? {
      meetingId: "harness-meeting-1",
      // Before: library_reader's "transcribing" note -- no transcript handle,
      // audio and personal note as for any captured meeting. After: the
      // ordinary transcript-ready note.
      state: transcriptLanded || legacyContract ? "transcript-only" : "transcribing",
      claims: [],
      noteGenerationAvailable: false,
      operatorNote: { text: savedOperatorNote, unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: transcriptLanded ? "transcript-handle-1" : null,
      microphonePlaybackHandle: "mic-playback-1",
      systemPlaybackHandle: "system-playback-1",
      meetingDeletionHandle: "deletion-handle-1",
      message: transcriptLanded
        ? "No admitted note is available. Retained transcript text remains available."
        : legacyContract
          ? "No transcript was created for this retained meeting."
          : "Your audio is saved on this Mac and is being transcribed. The transcript will appear here when it's ready.",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    } : searchMode && openedSearchMeeting ? {
      meetingId: "harness-meeting-2",
      state: "transcript-only",
      claims: [],
      noteGenerationAvailable: true,
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-2",
      transcriptHandle: "transcript-handle-2",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    } : sheetMode ? { ...sheetNote } : mode === "interrupted" ? {
      // mode=interrupted (Rail): a recording that stopped at 6:41 with its
      // audio kept and one timed line typed before the stop. Invented.
      meetingId: "harness-meeting-1",
      state: "recovered-interrupted",
      claims: [],
      noteGenerationAvailable: false,
      operatorNote: { text: "gutters: get a second quote", moments: [{ line: 0, seconds: 95 }], unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: null,
      microphonePlaybackHandle: "mic-playback-1",
      meetingDeletionHandle: "deletion-handle-1",
      message: "The recording was interrupted before it could be transcribed. The audio it kept can still be played.",
      audioRetention: { state: "retained", policy: "7-days", deadlineEpochSeconds: Math.floor(Date.now() / 1000) + 6 * 24 * 60 * 60, message: "Audio retained on this Mac." },
      capturePauses: { state: "not-paused", count: 0, totalPausedSeconds: 0, message: "Recording was not paused during this meeting.", spans: [] },
    } : mode === "ledger" ? {
      meetingId: "harness-meeting-1",
      state: "transcript-only",
      claims: [],
      noteGenerationAvailable: false,
      operatorNote: { ...ledgerNote },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      microphonePlaybackHandle: "mic-playback-1",
      systemPlaybackHandle: "system-playback-1",
      meetingDeletionHandle: "deletion-handle-1",
      message: "No admitted note is available. Retained transcript text remains available.",
      audioRetention: { state: "retained", policy: "7-days", deadlineEpochSeconds: Math.floor(Date.now() / 1000) + 6 * 24 * 60 * 60, message: "Audio retained on this Mac." },
      capturePauses: { state: "paused", count: 1, totalPausedSeconds: 45, message: "Recording was paused 1 time, for 0:45 in total. Nothing was captured during that gap.", spans: [{ pausedAtSeconds: 300, resumedAtSeconds: 345 }] },
    } : mode === "saved-draft" ? {
      meetingId: "harness-meeting-1",
      state: "note",
      claims: [
        { ordinal: 1, claimType: "decision", claim: "Launch on Friday.", handle: "claim-handle-1", locatorCount: 1, spans: [{ sourceTurnIndex: 0, start: 0, end: 30, text: currentTurns[0].text }] },
        { ordinal: 2, claimType: "action", claim: "Send the pricing note to finance.", handle: "claim-handle-2", locatorCount: 1, spans: [{ sourceTurnIndex: 1, start: 30, end: 60, text: currentTurns[1].text }] },
      ],
      turnsCited: [{ turn: 0, claimOrdinals: [1] }, { turn: 1, claimOrdinals: [2] }],
      operatorNote: { text: "Confirm the final owner.", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    } : mode === "saved-draft-unreadable" ? {
      // A ready meeting whose saved AI draft this install cannot project
      // (no note projector admitted). Audio retained, personal note present.
      meetingId: "harness-meeting-1",
      state: "saved-note-unreadable",
      claims: [],
      operatorNote: { text: "Confirm the final owner.", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      microphonePlaybackHandle: "mic-playback-1",
      systemPlaybackHandle: "system-playback-1",
      meetingDeletionHandle: "deletion-handle-1",
      audioRetention: { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    } : mode === "summary-failed" ? {
      meetingId: "harness-meeting-1",
      state: "summary-failed",
      claims: [],
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      audioRetention: { state: "released", message: "Audio released." },
      capturePauses: null,
    } : {
      meetingId: "harness-meeting-1",
      state: "transcript-only",
      claims: [],
      noteGenerationAvailable: true,
      regenerationSourceSha256: "0000000000000000000000000000000000000000000000000000000000000000",
      operatorNote: { text: "", unreadable: false },
      operatorNoteHandle: "note-handle-1",
      transcriptHandle: "transcript-handle-1",
      audioRetention: mode === "fidelity"
        ? { state: "released", message: "Audio released." }
        : { state: "retained", message: "Audio retained on this Mac." },
      capturePauses: null,
    }),
    library_open_transcript: () => (searchResultHandle === "unavailable-search-handle" ? { state: "unavailable", currentTranscriptSha256: null, turns: [] } : searchMode && openedSearchMeeting ? {
      meetingId: "harness-meeting-2",
      state: "available",
      currentTranscriptSha256: searchResultHandle === "changed-search-handle" && ++searchTranscriptReadCount > 1
        ? "3333333333333333333333333333333333333333333333333333333333333333"
        : "2222222222222222222222222222222222222222222222222222222222222222",
      transcriptFileHandle: "transcript-file-handle-2",
      turns: searchTurns.map((turn) => ({ ...turn })),
    } : {
      meetingId: "harness-meeting-1",
      state: "available",
      currentTranscriptSha256: "0000000000000000000000000000000000000000000000000000000000000000",
      transcriptFileHandle: "transcript-file-handle-1",
      turns: (mode === "ledger" ? ledgerTurns : currentTurns).map((turn) => ({ ...turn })),
    }),
    // R30. `transcript_retry_pending` answers "is a candidate already waiting";
    // null means no, which sends `openTranscriptRetry` down the start path and
    // makes the sheet appear from one click on Retry transcript.
    transcript_retry_pending: () => null,
    transcript_retry_start: () => ({ ...retryComparison }),
    library_save_operator_note: (args) => (savedOperatorNote = args?.text || "", {
      operatorNote: { text: args?.text || "", moments: Array.isArray(args?.moments) ? args.moments : [], unreadable: false },
      operatorNoteHandle: "note-handle-1",
    }),
    library_open_transcript_file: () => ({ state: "opened", transcriptFileHandle: "transcript-file-handle-1", message: "Synthetic file opened." }),
    library_export_meeting: () => ({ state: "exported", transcriptFileHandle: "transcript-file-handle-1", withheld: [], message: "Synthetic meeting exported." }),
    library_retained_audio_playback_status: () => ({ state: "idle", source: null, message: "" }),
  };
  // Synthetic transitions exercise the real click handlers; no downloads run.
  let setupOverride = null;
  const initialSnapshot = responses.app_snapshot;
  responses.app_snapshot = () => setupOverride || initialSnapshot();
  window.__setupCalls = [];
  window.__harnessCalls = [];
  responses.install_apple_speech_assets = () => {
    window.__setupCalls.push("install_apple_speech_assets");
    const initial = initialSnapshot();
    setupOverride = { ...initial, transcriptionEngine: { ...initial.transcriptionEngine, operationActive: true, canChange: false, apple: { state: "installing", locale: "en-US", reason: "Preparing Apple speech…" } } };
    setTimeout(() => {
      setupOverride = { ...idleSnapshot, transcriptionEngine: { selected: "apple-native", canChange: true, operationActive: false, apple: { state: "ready", locale: "en-US", reason: null }, whisper: { state: "download-required", reason: null } } };
    }, 800);
    return setupOverride.transcriptionEngine;
  };
  responses.install_transcript_model = ({modelId}) => {
    window.__setupCalls.push(["install_transcript_model", modelId]);
    setupOverride = { ...initialSnapshot(), model_setup: { ...initialSnapshot().model_setup, state: "downloading", selectedModelId: modelId, totalBytes: 463665005, downloadedBytes: 100000000 } };
    setTimeout(() => { setupOverride = { ...idleSnapshot, transcriptionEngine: { selected: "whisper", canChange: true, operationActive: false, apple: { state: "unavailable", locale: "en-US", reason: "Apple speech is unavailable." }, whisper: { state: "ready", reason: null } } }; }, 800);
    return setupOverride;
  };
  responses.select_transcription_engine = ({engine}) => {
    window.__setupCalls.push(["select_transcription_engine", engine]);
    setupOverride = { ...idleSnapshot, transcriptionEngine: { ...initialSnapshot().transcriptionEngine, selected: engine } };
    return setupOverride.transcriptionEngine;
  };
  responses.open_settings_window = () => { window.__setupCalls.push("open_settings_window"); return null; };
  window.__TAURI__ = {
    core: {
      invoke: (command, args) => {
        window.__harnessCalls.push(command);
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
