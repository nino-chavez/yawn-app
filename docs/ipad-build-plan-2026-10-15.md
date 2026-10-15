# Yawn for iPad — build plan

2026-10-15 · for the operator and the sessions that build it

## The plan in brief

Build the real Yawn iPad app as a new app in `apps/ipad/`, on the Time rail
design, in seven slices. Each slice ends with something the operator can use
on the iPad. The app records the room after a consent step, transcribes it live
in the background, ties every stroke to the moment it was written, and keeps
everything on the iPad.

The research spike in `spike/ipad-ink/` stays as the lab for experiments. Its
tested parts (stroke timing, erase and undo, retention, level measurement, the
replay boost) move into a shared Swift package that both apps use. The spike's
screens and store are not carried over; the app is written fresh against the
selected design.

Two calls are left for the operator, and neither blocks the first slices:
whether a meeting can be shared (a share button that makes a PDF of the page
and transcript, without audio), and confirming the app's identity on the home
screen and in App Store Connect before the TestFlight slice.

## What is already decided, and where

| Decision | Source |
|---|---|
| The iPad app is part of Yawn, native SwiftUI, finger-first, data stays on the iPad, export through Files is the only way off it | [Product brief, iPad section](product-brief.md#ipad-in-person-mode--decided-2026-10-13) |
| Consent and a 1, 7 or 30 day keep period before every recording; no headphone check | Product brief |
| The transcript shows no speaker names until a table recording proves them. The first one did not ([result](evidence/ipad-table-test-2026-10-14.json)) | Product brief; table test |
| Screens follow Concept C, Time rail, with three changes: fold the transcript between notes, open to a page with the meeting list in a sidebar, and test the rail against real writing | [Selection record](../decisions/ipad-concept-selection-2026-10-14.md) |
| The operator writes top to bottom, so the rail stays in the page's left margin (change 3 closed) | Operator, 2026-10-15; the table recording's 11 strokes ran top to bottom in time order |
| Record in measurement mode; boost replay instead of the recording | [Microphone mode test](evidence/ipad-mic-mode-ab-2026-10-15.json) |
| Transcribe live during the recording, not after Stop | This plan, from research; see below |
| The app icon is C2, writing on the time rail | [Icon selection](../decisions/ipad-icon-selection-2026-10-14.md) |
| Surfaces, hierarchy, density, copy and the object/action/state table | [Experience brief](experience-brief-ipad-2026-10-14.md) |

### Why transcribe live

The spike transcribed after Stop because live transcription was cut to keep
the spike small (`spike/ipad-ink/SPEC.md`), not because it failed. Research
answers the question the operator would otherwise have to test with an hour
of meeting:

- Apple's own Notes app on iPad already records with "the live transcript"
  and lets you take notes while recording
  ([Apple Support](https://support.apple.com/guide/ipad/record-and-transcribe-audio-ipadd0bde806/ipados),
  read 2026-10-15). Recording, transcribing live and writing together is a
  shipped pattern on this hardware class.
- Apple's speech framework is built for it. `SpeechAnalyzer` takes a live
  stream of audio (`start(inputSequence:)`), names the audio format it wants
  (`bestAvailableAudioFormat(compatibleWith:)`), can be warmed up before the
  meeting (`prepareToAnalyze(in:)`), and finishes cleanly at Stop
  (`finalizeAndFinishThroughEndOfInput()`)
  ([Apple Developer documentation](https://developer.apple.com/documentation/speech/speechanalyzer),
  read 2026-10-15).
- After-Stop transcription is fast but not free. Apple's transcriber ran at
  about 70 times real time on long recorded conversations on an M4 Mac mini
  ([Argmax](https://www.argmaxinc.com/blog/apple-and-argmax), read
  2026-10-15). This repo measured a 38:54 meeting in 31.6 s on an M3 Max
  ([comparison](transcription-engine-comparison.md)). The iPad's M1 is slower,
  and no published figure covers it. Live transcription removes the wait
  instead of measuring it.

Live transcription runs in the background while recording. Concept C's
recording screen gives the page at least 85% of the screen, so the transcript
is not shown until Stop. The point is that it is ready when the meeting ends.
If live analysis fails or falls behind, the app transcribes the saved audio
after Stop, the spike's proven path, so a meeting never ends without its
transcript.

## What the app does

Seven surfaces, from the experience brief, drawn as Concept C:

- **First run.** A blank page with Record. No sample meetings, no tips.
- **Before.** A full-screen step: "Everyone who can be heard agreed to this
  recording", a keep period of 1, 7 or 30 days, "Stays on this iPad" said
  once, and Start.
- **During.** The page fills the screen. A slim bar above it shows that it is
  recording, the elapsed time, and Stop, in the recording color and nowhere
  else. Pen, eraser and undo sit in a narrow column on the right. The rail down
  the left edge marks each line as it is written. The meeting list is hidden.
- **After.** The rail becomes the scrubber. Each line's mark shows its time;
  tapping a stroke or a mark plays that moment. Beside the page, the turn
  spoken nearest each note sits level with it, and the rest fold into a count
  that opens them (selection change 1). The full transcript opens from the
  meeting view; it can be searched and copied. Replay is boosted so speech is
  easy to hear. Strokes added after Stop sit below the rail's end, under "After
  the meeting".
- **Needs attention.** An interrupted recording keeps what it captured and
  says when and why it stopped, in one plain line. No speech model, or a
  failed transcript, says so with one action.
- **Meeting list.** A sidebar beside the page. Each row shows the date, time,
  length, a small rail of when notes were written, and the top of the page;
  meetings have no typed titles. Search finds words in transcripts. A meeting
  can be deleted, with confirmation, because deleting cannot be undone.
- **Export to Files.** Copies a meeting's folder (audio while kept, page,
  strokes, transcript) to a place the operator picks in Files. This is how a
  meeting reaches the Mac.

Not in this plan: sync, accounts, sharing (pending the operator's call),
speaker names, summaries or any generated text, folders, templates, paper
styles, a full tool palette, and the Mac app reading iPad exports.

## How it is built

```
apps/ipad/
  Yawn.xcodeproj            iPadOS 26+, display name "Yawn", icon C2
  Yawn/
    App/                    scenes, sidebar + page layout, orientation, Stage Manager
    Recording/              audio session, engine, live transcription, interruptions
    Page/                   PencilKit canvas, tool column, rail
    Meeting/                after-Stop view, replay, folded transcript, full transcript
    Library/                meeting index, list rows, search, delete, export
packages/yawn-ink-core/     moved from spike/ipad-ink/InkCore; the spike depends on it too
  StrokeLedger, StrokeTimeline, TranscriptTimeline, Retention,
  MeetingExport, LevelAccumulator, RecordingCheck, ReplayBoost, + tests
```

One microphone tap feeds three consumers, so nothing records twice:

```
microphone → AVAudioEngine input tap (measurement mode, background audio)
   ├─ converter → 16 kHz mono WAV on disk            (audio.wav)
   ├─ level meter                                      (shown while recording)
   └─ converter → SpeechAnalyzer live input stream    (transcript.json, finals only)
Stop → finish the stream → "finishing" → transcript ready
        (on failure: transcribe audio.wav, the spike's path)
```

- **Session and lifecycle.** Measurement mode, the background audio mode, and
  the screen kept awake while recording. A call or another app taking the
  microphone ends the recording with a note. A reconfiguration on the same
  microphone restarts capture in place (spike PR #131). These carry over from
  the spike as behavior, rewritten in the new store.
- **Storage.** One folder per meeting in the app's Documents, visible in
  Files: `audio.wav`, `drawing.data`, `strokes.json`, `transcript.json`,
  `meeting.json`. The keep-period ledger lives in Application Support, out of
  reach of Files, as in the spike. When the keep period ends, the audio is
  deleted and the page and transcript stay, as on the Mac.
- **Meeting index.** Built at launch from the meeting folders, with transcript
  text for search and a cached picture of the top of each page. The folders
  stay the source of truth.

## Build order

Each slice is one pull request, built and installed on the operator's iPad
before it merges. Workers may draft a slice in its own worktree, but the
orchestrating session builds and installs: the worker sandbox cannot run
`xcodebuild` or `swift test` (spike lesson, PR #123).

| Slice | What it adds | Done when |
|---|---|---|
| 1. Foundation | The app project, the package move (the spike builds against it), the C2 icon, "Yawn" on the home screen, first run | The app installs beside the spike, opens to a blank page with Record, and all package tests pass |
| 2. Recorder | Before step, record and stop bar, measurement-mode capture, live transcription, interruptions, keep period | An unattended one-hour recording, with an audiobook playing at the iPad and the screen locked, ends with its audio and transcript; the transcript is ready within a few seconds of Stop; battery use is noted |
| 3. Page and rail | Finger-first canvas with a thicker default pen, tool column, erase and undo with timing kept, rail marks while writing | Writing during a recording places a mark per line; erase and undo keep each stroke's time; a resting palm is checked on the device |
| 4. After | Rail scrubber, boosted replay, folded aligned transcript, full transcript with search and copy, "After the meeting" strokes | With a public AMI meeting played at the iPad, so the transcript runs at a real talking pace (the concept's invented one ran far slower), the folded transcript stays level with the ink, and six turns show beside the page in landscape |
| 5. Meeting list | Sidebar hidden while recording, rows with date, length, small rail and page top, transcript search, open, delete | Eight meetings show without scrolling in landscape; search finds a word in an older meeting; delete asks first |
| 6. Export and edge states | Export to Files, needs-attention states, speech model install | An exported meeting folder reaches the Mac through Files and the Mac review kit reads it (the Mac app does not import iPad meetings); each needs-attention state is reachable and says one plain thing |
| 7. Craft and release | Accessibility (VoiceOver labels, Dynamic Type for chrome), polish passes, cold screen review, TestFlight | A cold review of device captures exists under `docs/evidence/screen-reviews/`, and a TestFlight build is installable |

Slice 7's TestFlight step creates an app record in the operator's App Store
Connect account. That is an outward action and waits for the operator.

## Build and install

From the repository root, with the iPad connected and unlocked:

```
xcodebuild -project apps/ipad/Yawn.xcodeproj -scheme Yawn -configuration Debug \
  -destination id=<iPad UDID> -derivedDataPath apps/ipad/.derived \
  DEVELOPMENT_TEAM=34VZ63G58M -allowProvisioningUpdates build
xcrun devicectl device install app --device <iPad UDID> \
  apps/ipad/.derived/Build/Products/Debug-iphoneos/Yawn.app
(cd packages/yawn-ink-core && swift test)
```

`xcrun devicectl list devices` names the UDID. The app installs beside the
spike, as "Yawn" with the C2 icon.

## How each slice is checked

- Package logic has unit tests that fail when broken on purpose, as the level
  and replay tests do.
- Every slice is installed and run on the operator's iPad Air, not only the
  simulator. The simulator cannot show finger drawing policy, the real
  microphone, or the lock screen.
- Long-running behavior is tested unattended: an audiobook or a public
  meeting recording played at the iPad, not a meeting with people. The run
  happens where nobody else can be heard, uses a 1-day keep, and only its
  content-free numbers (duration, restarts, levels, transcript-ready time,
  battery) leave the iPad. Its audio is never copied to the Mac, and this
  public repository holds numbers only.
- A changed surface is not done until a cold review of device captures
  exists (repository rule, `CLAUDE.md`). Passing tests prove code, not the
  screen.

## What would change this plan

- **Live transcription cannot keep up for an hour on the M1.** Slice 2's
  unattended hour shows it. The app then transcribes after Stop, which already
  works, and the "finishing" state shows progress.
- **A palm resting on the glass leaves marks** with finger drawing on. Slice 3
  checks on the device before choosing a fix.
- **The folded transcript hides the line the operator needed** in real use.
  The selection record names this as a reason to revisit change 1.
- **A later table recording proves speaker labels.** Names then get room in
  the aligned column, and the product brief changes first.
- **The operator approves sharing.** A share button joins slice 6.

## Decisions the operator still owns

1. **Sharing.** Recommended: one Share button on a meeting that makes a PDF of
   the handwritten page with its transcript and sends it through the standard
   share menu (AirDrop, Mail, Messages, Files). Audio stays off it, because
   people agreed to be recorded, not to the recording being passed around. The
   product brief lists sharing as needing a separate decision, so it is out of
   this plan until then.
2. **Identity, before slice 7.** Recommended: display name "Yawn" and bundle
   identifier `com.ninochavez.yawn.ipad`, separate from the spike's
   `com.ninochavez.yawn.ipad-ink-spike`. The Mac app keeps its own
   identifier, `com.ninochavez.local-meeting-notes`. Slice 1 uses the
   recommended values so the app can install; changing them costs nothing
   until the App Store Connect record exists.

## Provenance

Checked on 2026-10-15:

- The product brief, experience brief, concept and icon selection records,
  roadmap, and spike README and SPEC, read in this repository.
- Live transcription: Apple's Notes support page and the `SpeechAnalyzer`
  documentation, fetched directly. Speed: Argmax's benchmark page, fetched
  directly, and this repo's own measurement. A MacStories hands-on (34 minutes
  of audio in 45 s) was seen only as a search summary, so it is not relied on.
- Background recording: Apple's background-modes page describes the `audio`
  mode in terms of playback. Recording apps (Voice Memos, Notability) rely on
  the same mode, and the spike uses it. Slice 2's unattended hour is the
  check.
- Not checked: transcription speed on an M1 iPad, battery use over an hour,
  palm rejection with finger drawing, and transcript accuracy from a
  microphone on a table. Each has a slice that measures it.
