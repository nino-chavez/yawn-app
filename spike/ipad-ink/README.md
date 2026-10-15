# iPad Ink research spike ("Yawn Lab" on the iPad)

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

Files are saved under Files > On My iPad > Yawn Lab > `meetings/<uuid>/` (`UIFileSharingEnabled` comes from `IPadInk-Info.plist`, merged with the generated Info.plist):

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

The spike shows on the iPad as "Yawn Lab" with a grey copy of the C2 icon, so
it is never mistaken for the Yawn app (`apps/ipad/`), which uses the violet
original. `IPadInk/AppIcon.icon` is an Icon Composer document, a copy of
concept C2 from `docs/design/concepts/ipad-icon-2026-10-14/`. The target's
`ASSETCATALOG_COMPILER_APPICON_NAME` is `AppIcon`, and the synchronized
`IPadInk` folder includes the file without a project entry. Why C2:
[`decisions/ipad-icon-selection-2026-10-14.md`](../../decisions/ipad-icon-selection-2026-10-14.md).

## Microphone mode test

The first table recording sat about 25 dB below a normal speech level. The app
records in `AVAudioSession` mode `.measurement`, which Apple documents as
minimizing system-supplied signal processing and disabling some dynamics
processing. The **Microphone (test)** menu beside the keep period picks the
mode for the next recording. Measurement stays the default.

| Menu | Session | What Apple says it does |
|---|---|---|
| Measurement | `.record`, `.measurement` | Minimal system processing; primary microphone |
| Default | `.record`, `.default` | No stated processing |
| Video recording | `.record`, `.videoRecording` | "Appropriate audio-signal processing"; the microphone closest to the camera |
| Voice processing | `.playAndRecord`, `.voiceChat`, voice processing on the input node | Automatic gain correction and voice processing, tuned for a near talker |

While recording, an input-level bar shows the last 100 ms window in dBFS.
After Stop, the screen shows the take's numbers. The app also saves
`recording-check.json` beside the audio. It holds the requested mode, the
session the system actually configured, read back after start (category, mode,
voice processing, input port, data source, polar pattern, input gain), and level
statistics of the 16 kHz audio that was written. Those statistics are RMS, peak,
the 10th, 50th and 90th percentiles of 100 ms window levels, speech minus noise
(p90 minus p10), and the number of clipped samples. The file holds no audio and
no words.

**The test.** Every take records the same known source, so the takes differ
only in the mode:

1. Put the iPad where the operator sits. Put the Mac's speaker where the other
   person would sit, about a table's width away, and leave the Mac's volume
   alone for the whole test.
2. Run `python3 spike/speech-engine-comparison/ipad_mic_ab.py play --scratch
   <a folder outside the repo>`. It speaks the opening of the Gettysburg Address
   once per take, waiting for Enter, and warns if the volume changed.
3. Make five takes, in this order: measurement, default, video recording, voice
   processing, measurement. For each, choose the mode, tick consent, keep 1
   day, tap Start recording, press Enter on the Mac, and tap Stop recording when
   the passage ends. Wait for the transcript before the next take.
4. Pull the meetings to the Mac, then run `ipad_mic_ab.py score <meeting
   folders>`. Delete the Mac copies after scoring.

**Ranking rule, fixed before the takes.** Rank by word recall of the passage
in the app's own transcript, and break ties by speech minus noise. A mode is
out if it clips, or if it recalls fewer words than measurement does. Loudness
alone decides nothing, because software gain can raise any take. The two
measurement takes check the setup: if their recall differs by more than 5
points, or their speech level by more than 2 dB, the setup drifted and the run
is repeated. A voice-processing take that lingered would also show up as a
mismatch in the last measurement take.

Limits: one speaker, a synthetic voice, one room, one distance. A real second
person's voice and a longer meeting are the next test, not this one.

**Round 1, 2026-10-15 (Mac volume 50).** Measurement: speech −58.2 dBFS,
noise −76.1, gap 18.0 dB, recall 98.1%. Default: −42.5, −56.4, 13.9 dB, 98.1%.
Video recording (back microphone, subcardioid): −43.8, −58.5, 14.7 dB, 95.1%.
Voice processing stopped at 0:00. Enabling it reconfigures the microphone right
after start; the app read that as a hardware change and ended the take. The
closing measurement take was not run, so there is no drift check. The louder
modes raise the noise floor more than the speech. The session read-back showed
the input gain is settable on this iPad and sits at 0.5.

**Round 2, written before it ran.** What changed and why:

- Voice processing: a reconfiguration on the same input port (type and UID)
  now restarts capture into the same file, up to 3 times, counted as
  `engine_restarts` in `recording-check.json`. A different port still ends the
  recording with the note. A real hardware change (plugging in a USB-C
  microphone mid-recording) is still untested.
- New mode **Measurement, full input gain**: measurement with the input gain
  set to 1, restored after the take. Added after round 1's data showed the gain
  is settable. If speech and noise both rise by about the same number of dB,
  the gain is digital and no better than gain added in software; if the gap
  widens, it is analog and a real gain for measurement.
- Recall hit its ceiling at volume 50 (98.1% twice). Speech minus noise
  penalizes any mode with dynamics processing by construction, so it cannot
  show a processed mode helping. Two quieter volumes (25 and 12) repeat
  measurement against default so recall can separate them.
- A mode is out if it recalls **more than 5 points** below measurement (round
  1 said "fewer words than measurement"). Changed after seeing round 1, to
  match the 5-point drift tolerance; it puts video recording back in.

Takes, in order (`ipad_mic_ab.py play` default): 50 measurement, 50 voice
processing, 50 measurement full gain, 50 measurement, 25 measurement, 25
default, 12 measurement, 12 default. The script logs each take's volume, and
`score --log` attaches it. If voice processing still will not record, it is
recorded as not runnable in this spike and the experiment closes.

**Result: stopped after round 1 (operator, 2026-10-15).** Round 2 would most
likely have confirmed measurement rather than changed anything, so it was not
run. Measurement stays the default and the test menu stays for any rerun. Its
low level did not hurt Apple's transcriber (98.1% recall at −58 dBFS speech);
what failed at the table was a person listening to the clips, which the replay
boost below addresses. Voice processing (fixed but never re-run) and full input
gain (never measured) remain open. Numbers and limits:
[`docs/evidence/ipad-mic-mode-ab-2026-10-15.json`](../../docs/evidence/ipad-mic-mode-ab-2026-10-15.json).

## Replay boost

Replay plays louder than the room was recorded. On loading a meeting for
replay, `InkCore.ReplayBoost` measures speech the same way `recording-check.json`
does (the 90th percentile of 100 ms window levels). It applies enough gain to
bring speech to −20 dBFS, never a cut and at most 40 dB, then a tanh soft
limiter that keeps every sample under 0.99 of full scale. `AVAudioPlayer` plays the
result from a WAV held in memory. The saved `audio.wav`, the transcript and
the Mac kit all use the original audio. The replay panel says how many dB
louder it plays. On the microphone test's measurement take, this gave +38 dB:
speech −20 dBFS, background −38, peak −7, and nothing near the ceiling.

The boosted copy is built when replay opens, on the main thread, by reading
the file twice in 10-second chunks: once to measure speech, once to boost it.
Memory stays near the output's size, about 2 bytes per sample, so a ten-minute
meeting needs under 20 MB. Recordings over two hours, and any recording the
boost fails on, play as saved, and a boost failure never blocks transcription.

## Known limits of this spike

- Transcription runs after Stop, not live. Live transcription was cut to keep
  the spike small; the real app transcribes live
  ([build plan](../../docs/ipad-build-plan-2026-10-15.md)).
- Stroke times come from PencilKit's stroke creation date and its last point's
  time offset. Erasing and undoing keep each stroke's time (see "Erase and
  undo").
- Replay owns the canvas after Stop: drawing is off, strokes later than the
  playhead are dimmed, and tapping a stroke seeks to it. "New meeting" asks for
  consent and a keep period again and starts a fresh canvas.
- Audio is converted from the microphone's own format to 16 kHz mono and stored
  as 16-bit PCM WAV, with voice processing off (measurement mode).
