# iPad Ink research spike

This is a research spike, not the Yawn product. It tests whether in-person note
taking works on an iPad: microphone-only audio, Apple Pencil strokes timed to
that audio, after-Stop on-device transcription, and replay.

All meeting data stays on the iPad. The app has no account, cloud service,
analytics, or network calls. The only network use is an explicit Apple
on-device speech-model download when the selected locale does not have its
speech model installed.

## Install on an iPad

Open `IPadInk.xcodeproj` in Xcode 27 or later. In Signing, choose a development
team. Connect an iPad running iPadOS 26 or later with Developer Mode enabled,
then build and run the `IPadInk` scheme.

Before recording, confirm that everyone who can be heard agreed and choose an
audio keep period: 1, 7, or 30 days. The next launch deletes only `audio.wav`
for expired meetings; strokes, transcript, and drawing remain. The keep period is
recorded in a private ledger in the app's Application Support folder, so editing
or deleting `meeting.json` in Files does not stop audio deletion. A folder you
rename, duplicate or move out of `meetings/` in Files is yours and leaves the
sweep. If the ledger is ever unreadable, it is set aside and rebuilt once from
the `meeting.json` files of folders still named by their meeting ID; in that one
case the exported `meeting.json` is the only record left, and a keep period other
than 1, 7 or 30 days, or a future date, is read as the shortest period. The
ledger existed before this app first launched on any device, so no meeting
predates it.

## Exports

Files are saved under Files > On My iPad > iPad Ink > `meetings/<uuid>/` (`UIFileSharingEnabled` comes from `IPadInk-Info.plist`, merged with the generated Info.plist):

- `audio.wav` — local microphone recording.
- `strokes.json` — `[{index, start_seconds, end_seconds}]`; untimed strokes use
  `null` for both time values.
- `transcript.json` — `[{start, end, text, final}]`; turns are deliberately
  unlabeled. A finished run saves final text only; in-progress text is shown
  while transcribing but never saved.
- `meeting.json` — `schema`, `created_at`, consent, keep period, and sample rate.
- `drawing.data` — PencilKit’s `PKDrawing.dataRepresentation()`.

## Scoring a recording on the Mac

The private Mac review kit lives in `spike/speech-engine-comparison`. First run
`ipad_meeting_review.py check` against an exported meeting folder. Then run
`label-speakers` with a local FluidAudio CLI, build a private blind `packet`,
listen and fill in `labels.json`, and run `score`. The JSON summaries contain
only counts, durations, hashes, and labels; the packet folder is the only place
clip audio is written. Run `sweep` regularly against the private packet root so
clips are deleted when the meeting's audio keep period expires.

## Limits of simulator evidence

The simulator can exercise ordinary touch ink and compile the app, but it
cannot establish Apple Pencil latency, a real microphone recording, the local
speech model’s availability or download, or device replay behavior. Those need
an iPad review.

## Recording through a meeting

While recording, the app keeps the iPad screen awake. Its audio background mode
means locking the screen keeps the recording running. If a call, another app,
an audio-system restart, or a microphone change interrupts recording, the app
stops, saves the audio and ink already captured, and says why and when it
stopped. That note stays on screen until New meeting, while everything after it
works as after Stop: transcription, the speech-model prompt, and failures. Replay
waits until the interruption ends (or the app comes back to the front), because
iOS will not hand the audio session back during a call. After an audio-system
restart the app rebuilds its audio engine. It does not resume recording
automatically.

Checked in the simulator: recording continued on the home screen and the lock
screen. A real call or audio-system restart has not been tried.

## Writing on the page

The page grows as you write: there is always room below the lowest stroke, so
writing large never uses it up. A finger or the Pencil writes, and two fingers
scroll. With iPadOS's "Only Draw with Apple Pencil" setting on, only the Pencil
writes and one finger also scrolls; the app rechecks that setting each time it
comes back to the front. Replay and the transcript sit beside the page in
landscape and below it in portrait.

## Erase and undo

The spike keys each stroke by its PencilKit path creation time
and random seed. The vector eraser removes whole strokes, while undo and redo
restore the same keys, so the saved stroke timing remains attached to the right
stroke. `strokes.json` remains the same position-indexed export beside
`drawing.data`. On iPadOS 27, `PKStroke.strokeID` can replace this key.

The pen, eraser, undo and redo buttons sit above the page, so they never cover
writing.

## App icon

`IPadInk/AppIcon.icon` is an Icon Composer document, a copy of concept C2 from
`docs/design/concepts/ipad-icon-2026-10-14/`. The target's
`ASSETCATALOG_COMPILER_APPICON_NAME` is `AppIcon`, and the synchronized
`IPadInk` folder includes the file without a project entry. Why C2:
[`decisions/ipad-icon-selection-2026-10-14.md`](../../decisions/ipad-icon-selection-2026-10-14.md).

## Known limits of this spike

- Transcription runs after Stop, not live. Live transcription was cut to fit.
- Stroke times come from PencilKit's stroke creation date and its last point's
  time offset. Undoing or erasing strokes is not supported: stroke records are
  matched to the drawing by position.
- Replay owns the canvas after Stop: drawing is off, strokes later than the
  playhead are dimmed, and tapping a stroke seeks to it. "New meeting" asks for
  consent and a keep period again and starts a fresh canvas.
- Audio is converted from the microphone's own format to 16 kHz mono and stored
  as 16-bit PCM WAV, with voice processing off (measurement mode).
