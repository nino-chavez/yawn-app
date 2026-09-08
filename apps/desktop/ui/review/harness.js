// Yawn design-review harness -- NOT part of the shipped app.
//
// Stubs window.__TAURI__ so the real index.html/main.js/settings.js/styles.css
// can render in a plain browser without a Tauri runtime, driven by a
// `?state=<name>` query parameter. Every value below is an invented,
// plainly-synthetic fixture -- see review/MANIFEST.md. This file must never
// be referenced by the shipped app; it exists only for browser-rendered
// capture of the real frontend for a blind design review.
//
// This script is a plain (non-module) <script>, loaded before main.js's
// module script, so window.__TAURI__ exists before main.js's top-level
// `const invoke = window.__TAURI__?.core?.invoke;` runs.
(function () {
  "use strict";

  const params = new URLSearchParams(window.location.search);
  const STATE = params.get("state") || "home-with-meetings";
  const NOW = Math.floor(Date.now() / 1000);
  const SHA_A = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";

  // ---- shared fixture fragments ------------------------------------------------

  function permissionsFixture() {
    return { microphone: "authorized", systemAudio: "authorized", probeUnavailable: false, prompted: true };
  }

  function libraryRows() {
    return [
      {
        handle: "h-1",
        meetingId: "m-1",
        label: "Weekly sync — design review",
        labelSource: "operator",
        folderId: null,
        createdAtEpochSeconds: NOW - 3600,
        transcriptAvailable: true,
        notePreview: "The team agreed to ship the redesigned onboarding flow next sprint.",
        locked: false,
      },
      {
        handle: "h-2",
        meetingId: "m-2",
        label: null,
        labelSource: "date",
        folderId: null,
        createdAtEpochSeconds: NOW - 90000,
        transcriptAvailable: true,
        notePreview: "Budget approval is still pending finance sign-off.",
        locked: false,
      },
      {
        handle: "h-3",
        meetingId: "m-3",
        label: "1:1 with a teammate",
        labelSource: "operator",
        folderId: null,
        createdAtEpochSeconds: NOW - 190000,
        transcriptAvailable: false,
        notePreview: null,
        locked: true,
      },
    ];
  }

  function librarySnapshotFixture() {
    const empty = STATE === "home-first-run";
    const rows = empty ? [] : libraryRows();
    return {
      state: "ready",
      rows,
      unavailableCount: 0,
      metadataRevision: 1,
      folders: [],
      total: rows.length,
      shown: rows.length,
      filterActive: false,
      message: "",
      searchProbeEnabled: false,
      // First-run sheet only shows on a genuinely empty, never-dismissed
      // library -- see firstRunSheetVisible() in view-model.mjs.
      firstRunSheetSeen: !empty,
    };
  }

  function readySnapshot(extra) {
    return Object.assign(
      {
        startup: "ready",
        capture: "idle",
        meeting_id: null,
        started_at_epoch_seconds: null,
        capture_state_started_at_epoch_seconds: null,
        transcription_last_worker_heartbeat_at_epoch_seconds: null,
        background_transcription_active: false,
        background_transcription_queued_count: 0,
        capture_pause_change_pending: false,
        startup_message: "",
        model_setup: { state: "ready", options: [], selectedModelId: null, downloadedBytes: 0, totalBytes: 0, error: null },
        turns: [],
        current_transcript_sha256: null,
        error: null,
      },
      extra || {},
    );
  }

  function currentSnapshotFixture() {
    if (STATE === "during-capture") {
      return readySnapshot({
        capture: "recording",
        meeting_id: "m-live-1",
        started_at_epoch_seconds: NOW - 420,
        capture_state_started_at_epoch_seconds: NOW - 420,
        transcription_last_worker_heartbeat_at_epoch_seconds: NOW - 2,
        turns: [
          { sourceTurnIndex: 0, sourceSpeaker: "Speaker 1", speaker: "Speaker 1", speakerCorrected: false, start: 4.2, text: "Let's start with the design review.", withheld: false },
          { sourceTurnIndex: 1, sourceSpeaker: "Speaker 2", speaker: "Speaker 2", speakerCorrected: false, start: 18.6, text: "Sounds good, I pulled up the latest screens.", withheld: false },
        ],
      });
    }
    if (STATE === "needs-attention") {
      return readySnapshot({
        startup: "error",
        error: "Yawn could not verify its local recording engine after the last restart.",
      });
    }
    return readySnapshot();
  }

  function transcriptTurns() {
    return [
      { sourceTurnIndex: 0, sourceSpeaker: "Speaker 1", speaker: "Speaker 1", speakerCorrected: false, start: 4.2, text: "Let's start with the design review.", withheld: false },
      { sourceTurnIndex: 1, sourceSpeaker: "Speaker 2", speaker: "Speaker 2", speakerCorrected: false, start: 18.6, text: "Sounds good, I pulled up the latest screens.", withheld: false },
      { sourceTurnIndex: 2, sourceSpeaker: "Speaker 1", speaker: "Speaker 1", speakerCorrected: false, start: 41.0, text: "So we're all good shipping the new onboarding flow next sprint, right?", withheld: false },
      { sourceTurnIndex: 3, sourceSpeaker: "Speaker 2", speaker: "Speaker 2", speakerCorrected: false, start: 52.0, text: "I'll chase finance on the budget line before Friday.", withheld: false },
      { sourceTurnIndex: 4, sourceSpeaker: "Speaker 1", speaker: "Speaker 1", speakerCorrected: false, start: 60.0, text: "Do we need another round of testing before this ships?", withheld: false },
      { sourceTurnIndex: 5, sourceSpeaker: null, speaker: null, speakerCorrected: false, start: 73.0, text: "", withheld: true },
    ];
  }

  function libraryOpenNoteFixture() {
    return {
      state: "note",
      transcriptHandle: "th-1",
      operatorNoteHandle: "onh-1",
      audioDeletionHandle: "adh-1",
      microphonePlaybackHandle: "mph-1",
      systemPlaybackHandle: "sph-1",
      transcriptDeletionHandle: "tdh-1",
      meetingDeletionHandle: "mdh-1",
      meetingId: "m-1",
      regenerationSourceSha256: SHA_A,
      claims: [
        {
          handle: "c-0",
          ordinal: 0,
          claimType: "summary",
          claim: "The team reviewed the new onboarding flow and agreed on next steps.",
          evidenceState: "evidenced",
          locatorCount: 1,
          spans: [{ sourceTurnIndex: 2, start: 41.0, end: 47.5, text: "So we're all good shipping the new onboarding flow next sprint, right?" }],
        },
        {
          handle: "c-1",
          ordinal: 1,
          claimType: "decision",
          claim: "Ship the redesigned onboarding flow next sprint.",
          evidenceState: "evidenced",
          locatorCount: 1,
          spans: [{ sourceTurnIndex: 2, start: 41.0, end: 47.5, text: "So we're all good shipping the new onboarding flow next sprint, right?" }],
        },
        {
          handle: "c-2",
          ordinal: 2,
          claimType: "action",
          claim: "Confirm budget sign-off with finance by Friday.",
          evidenceState: "evidenced",
          locatorCount: 1,
          spans: [{ sourceTurnIndex: 3, start: 52.0, end: 58.0, text: "I'll chase finance on the budget line before Friday." }],
        },
        {
          handle: "c-3",
          ordinal: 3,
          claimType: "question",
          claim: "Whether the redesign needs a second round of user testing.",
          evidenceState: "evidenced",
          locatorCount: 1,
          spans: [{ sourceTurnIndex: 4, start: 60.0, end: 65.0, text: "Do we need another round of testing before this ships?" }],
        },
      ],
      audioRetention: {
        state: "retained",
        policy: "7-day",
        deadlineEpochSeconds: NOW + 500000,
        retainedBytes: 18000000,
        message: "Recording audio is kept for 7 days.",
      },
      capturePauses: { state: "not-paused" },
      operatorNote: { text: "Ask about the Q4 budget line before we close.", unreadable: false },
      meetingContext: { text: "Decide whether to ship the onboarding redesign this sprint.", unreadable: false },
      turnsCited: [
        { turn: 2, claimOrdinals: [0, 1] },
        { turn: 3, claimOrdinals: [2] },
        { turn: 4, claimOrdinals: [3] },
      ],
      lock: { locked: false, unreadable: false },
      lockToken: null,
      canConfirmOperator: true,
      message: "",
    };
  }

  function libraryOpenTranscriptFixture() {
    return {
      state: "ready",
      meetingId: "m-1",
      transcriptFileHandle: "tfh-1",
      currentTranscriptSha256: SHA_A,
      turns: transcriptTurns(),
      warnings: [],
      message: "",
    };
  }

  function transcriptModelSettingsFixture() {
    return {
      state: "ready",
      options: [
        { id: "whisper-large-v3-turbo-q4", title: "Compact model", detail: "Smaller download, less disk space.", downloadBytes: 463665005, installedBytes: 463665005, stored: true, active: true },
        { id: "whisper-large-v3-turbo", title: "Full model", detail: "Original Turbo weights, using about 1.61 GB.", downloadBytes: 1613977880, installedBytes: 0, stored: false, active: false },
      ],
      activeModelId: "whisper-large-v3-turbo-q4",
      selectedModelId: "whisper-large-v3-turbo-q4",
      downloadedBytes: 463665005,
      totalBytes: 463665005,
      error: null,
      changeActive: false,
      canChange: true,
      unavailableReason: null,
    };
  }

  function noteModelSettingsFixture() {
    return {
      state: "ready",
      options: [
        { id: "gemma-3-12b-it-qat-4bit", title: "Local note model", detail: "Turns finished transcripts into meeting notes, on this Mac.", downloadBytes: 8063332687, installedBytes: 8063332687, stored: true, active: true },
      ],
      activeModelId: "gemma-3-12b-it-qat-4bit",
      selectedModelId: "gemma-3-12b-it-qat-4bit",
      downloadedBytes: 8063332687,
      totalBytes: 8063332687,
      error: null,
      changeActive: false,
      canChange: true,
      unavailableReason: null,
    };
  }

  function transcriptionEngineSettingsFixture() {
    const selected = window.__reviewSelectedEngine || "whisper";
    return {
      selected,
      canChange: true,
      operationActive: false,
      apple: { state: "ready", reason: null, locale: "en-US" },
      whisper: { state: "ready", reason: null },
    };
  }

  // ---- dispatch ------------------------------------------------------------

  function respond(command, args) {
    switch (command) {
      case "app_snapshot":
        return currentSnapshotFixture();
      case "library_snapshot":
        return librarySnapshotFixture();
      case "preview_list_trash":
        return { state: "empty", entries: [] };
      case "first_run_permissions":
      case "first_run_request_microphone":
      case "first_run_request_system_audio":
        return permissionsFixture();
      case "operator_note":
        return { text: "Ask about the Q4 budget line before we close.", unreadable: false };
      case "meeting_context":
        return { text: "Decide whether to ship the onboarding redesign this sprint.", unreadable: false };
      case "library_open_note":
        return libraryOpenNoteFixture();
      case "library_open_transcript":
        return libraryOpenTranscriptFixture();
      case "transcript_retry_pending":
        return null;
      case "library_retained_audio_playback_status":
      case "library_stop_retained_audio":
        return { state: "idle", source: null, message: "No recording is playing." };
      case "transcript_model_settings":
        return transcriptModelSettingsFixture();
      case "get_transcription_engine_settings":
        return transcriptionEngineSettingsFixture();
      case "select_transcription_engine":
        window.__reviewSelectedEngine = args?.engine || "whisper";
        return transcriptionEngineSettingsFixture();
      case "install_transcript_model":
        window.__reviewSelectedEngine = "whisper";
        return currentSnapshotFixture();
      case "install_apple_speech_assets":
        window.__reviewSelectedEngine = "apple-native";
        return { ...transcriptionEngineSettingsFixture(), selected: "apple-native" };
      case "note_model_settings":
        return noteModelSettingsFixture();
      case "dismiss_first_run_sheet":
        return null;
      default:
        // Unstubbed command for the states this harness captures -- logged,
        // not thrown, so an unrelated interaction never crashes the render.
        console.warn("[review-harness] unstubbed invoke:", command);
        return null;
    }
  }

  window.__TAURI__ = {
    core: {
      invoke: function (command, args) {
        try {
          return Promise.resolve(respond(command, args));
        } catch (error) {
          return Promise.reject(error);
        }
      },
    },
    event: {
      // Only consumer is main.js's note-capture-hotkey listener; a resolved
      // no-op unlisten function is all it needs.
      listen: function () {
        return Promise.resolve(function unlisten() {});
      },
    },
  };
})();
