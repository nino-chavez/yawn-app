// Harness stub: stands in for the Tauri bridge so the real ui/main.js runs
// its real render loop (900 ms poll included) against fake backend state.
// mode=capture (default): an active recording, so the operator-note textarea
// is live and the snapshot poll re-renders every 900 ms.
// mode=library: idle capture with one finished meeting, so the transcript
// search input is live and every keystroke re-renders synchronously.
// mode=startup / mode=model-setup: the startup-check and no-model surfaces.
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
  const responses = {
    // mode=startup: the local startup check still running; mode=model-setup:
    // no speech model installed yet. Both render surfaces the other modes
    // never reach, so the harness can show them without a packaged build.
    app_snapshot: () => (mode === "capture" ? { ...captureSnapshot }
      : mode === "startup" ? { ...idleSnapshot, startup: "checking", startup_message: "Verifying on-device speech models." }
      : mode === "model-setup" ? { ...idleSnapshot, startup: "model-required", model_setup: { state: "idle", options: [], selectedModelId: "" } }
      : { ...idleSnapshot }),
    first_run_permissions: () => ({ microphone: "authorized", systemAudio: "authorized", probeUnavailable: false }),
    library_snapshot: () => (mode === "library" || mode === "summary-failed"
      ? { rows: [{ ...libraryRow }], total: 1, metadataRevision: 1 }
      : { rows: [], total: 0, metadataRevision: 1 }),
    preview_list_trash: () => ({ entries: [] }),
    operator_note: () => ({ text: "", unreadable: false }),
    meeting_context: () => ({ text: "", unreadable: false }),
    save_operator_note: () => ({ unreadable: false }),
    save_meeting_context: () => ({ unreadable: false }),
    // mode=summary-failed: the same meeting after a rejected generation,
    // audio released, with a source pin so the retry control renders (R23).
    library_open_note: () => (mode === "summary-failed" ? {
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
      turns: [
        { start: 1, end: 3, speaker: "Me", text: "alpha decision about the launch", sourceTurnIndex: 0 },
        { start: 4, end: 6, speaker: "Them", text: "bravo follow-up owed by Friday", sourceTurnIndex: 1 },
      ],
    }),
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
