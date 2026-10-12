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
case the exported `meeting.json` is the only record left.

## Exports

Files are saved under Files > On My iPad > iPad Ink > `meetings/<uuid>/` (`UIFileSharingEnabled` comes from `IPadInk-Info.plist`, merged with the generated Info.plist):

- `audio.wav` — local microphone recording.
- `strokes.json` — `[{index, start_seconds, end_seconds}]`; untimed strokes use
  `null` for both time values.
- `transcript.json` — `[{start, end, text, final}]`; turns are deliberately
  unlabeled.
- `meeting.json` — `schema`, `created_at`, consent, keep period, and sample rate.
- `drawing.data` — PencilKit’s `PKDrawing.dataRepresentation()`.

## Limits of simulator evidence

The simulator can exercise ordinary touch ink and compile the app, but it
cannot establish Apple Pencil latency, a real microphone recording, the local
speech model’s availability or download, or device replay behavior. Those need
an iPad review.

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
