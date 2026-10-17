# FluidVoice and FluidMeet, reviewed against Yawn

2026-10-16 · for the operator · FluidVoice 1.6.11 (stable) installed on this Mac

## The answer first

FluidVoice is a free, open-source (GPLv3) Mac dictation app. Since 1.6.11 it
also ships **FluidMeet**, a local meeting recorder. FluidMeet is the closest
comparable to Yawn found so far: on-device, no bot, no account, transcript and
speaker labels after the meeting, audio kept for 7 days by default.

It differs from Yawn on the one thing Yawn treats as non-negotiable. **FluidMeet
records with no consent step at all.** Pressing Start recording began capturing
the room microphone with no prompt, and the source has no participant-consent
or recording-notice step. Yawn asks for three confirmations before every
recording.

Five FluidMeet behaviors fit the product brief and are worth taking (see
[What to take](#what-to-take)). Its summaries, AI-assistant links, local data
service, meeting auto-detection and analytics are outside the brief by
decision; they are listed as context, not as gaps.

FluidVoice is GPLv3. Study its behavior only, never its code. Keep the
documenting role and the building role apart, as the clean-room section of the
operator's 2026-10-15 comparables note (`docs/learning-from-comparable-apps-2026-10-15.md`,
not yet committed) describes.

## What was checked, and how

| Layer | Method | Evidence |
|---|---|---|
| Positioning | Read fluidvoice.com, the FluidMeet blog post, the README and the 1.6.11 and 1.6.12-beta.1 release notes | Sources at the end |
| Install | `brew install --cask fluidvoice` (official cask, GitHub release). Gatekeeper: notarized Developer ID, team `V4J43B279J` | `spctl -a -vv` |
| Functional | Ran onboarding, FluidMeet settings, one in-room recording, one file transcription, Voice Engine, light and dark themes | Captures in [`comparables-fluidvoice-2026-10-16/`](comparables-fluidvoice-2026-10-16/) |
| Storage | Read `~/Library/Application Support/FluidVoice/` and the meeting `session.json` | Shell inspection |
| Technical | A read-only review of the v1.6.11 source (554 Swift files, ~243k lines). Two claims re-checked by hand: the local service has no authentication, and detailed analytics default on | File paths below |

Not checked: an online call (Zoom, Teams, Meet) with system audio, because
Screen & System Audio access was not granted; the 1.6.12 beta; long meetings;
crash recovery in practice; contrast measurement.

## Functional review

### What it does

FluidVoice has two products in one window. The dictation side is a global
hotkey (Right ⌥) that types speech into any app, with optional AI cleanup.
FluidMeet records a meeting, shows live captions while recording, then after
Stop runs speaker identification and a full transcript.

Capture modes: **Online call** records the meeting app's audio plus the mic as
separate tracks; **In-room** records the mic only. It can watch for Zoom, Teams
and Webex calls and offer to record them. It never starts recording by itself.

### The recording run

One 1:55 in-room recording was made through the MacBook microphone while two
scripted computer voices played through the built-in speakers.

- **No consent step.** Recording started on the first click
  ([13](comparables-fluidvoice-2026-10-16/13-recording-no-captions.jpg)).
- **Live captions showed nothing** for the whole recording, only the warning
  "Live captions dropped audio and resynced." The live-caption model folder
  was created at the moment recording started, so this is likely a
  first-use download, not the steady state. Not re-tested.
- **The first Start click was blocked** with "Wait for the active meeting
  transcription to finish," when no meeting existed
  ([12](comparables-fluidvoice-2026-10-16/12-start-blocked-copy.jpg)). Likely the
  same first-use loading, but the wording names the wrong cause.
- **The transcript did not contain the scripted dialogue.** It held about 100
  words of unrelated speech that was audible in the room at the time. The most
  likely cause is the Mac's own echo cancellation removing what its speakers
  played. **This run measures nothing about accuracy or speaker labels.** That
  transcript is private room audio and is not reproduced here.
- **After Stop**, a four-step list (Saving, Identifying speakers,
  Transcribing, Finalizing) showed where processing stood, with "You can close
  this window" ([14](comparables-fluidvoice-2026-10-16/14-processing-stages.jpg)).
  This is clear and worth copying.
- The source menu kept reading "Automatic" after choosing "In person," but the
  saved meeting recorded `mode: inRoom`. The label lies; the setting held.

### The file run (accuracy and speaker labels)

A 25-second file of two synthetic voices (Samantha and Daniel, four turns, 68
words) went through File Transcription with speaker labels on and the speaker
count on Auto.

- **Words:** 67 of 68 correct. The one error was the name "Priya," transcribed
  as "Kria." Numbers were normalized ("twentieth" → "20th", "seven days" → "7
  days").
- **Speakers:** both voices were labeled **Speaker 1**. Speaker labeling failed
  on this sample.
- One run, synthetic voices, clean audio. It is a smoke test, not a benchmark.

### Storage and retention

- Meetings live in `~/Library/Application Support/FluidVoice/Meetings/<id>/`
  as `session.json`, a chunk ledger, and audio tracks.
- Audio is **uncompressed Float32 PCM at 48 kHz**: 11.5 MB for the first
  minute, about 690 MB per hour of mic-only recording.
- Every chunk carries a SHA-256 and a three-stage ledger (intent, checkpoint,
  terminal). That is the basis of its crash recovery.
- Audio retention: after transcription, 7 days (default), 30 days, or forever.
  Transcripts are always kept. Expired audio is removed only while the app is
  running ([10](comparables-fluidvoice-2026-10-16/10-meet-settings-retention.jpg)).

### Downloads it made

The onboarding card said ~242 MiB. The app downloaded far more:

| Folder | Size | When | Why |
|---|---|---|---|
| `FluidAudio/Models/fluid-parakeet-mini-coreml` | 242 MB | Onboarding | The model chosen |
| `FluidIntelligence/Models/fluid-1-mini-…` | 1.6 GB | One minute later, unprompted | First-run setup prefetches the private AI model as soon as the speech model is ready (`ASRService.swift:6646-6651`) |
| `FluidAudio/Models/parakeet-eou-streaming` | 428 MB | Recording start | Live captions |
| `FluidAudio/Models/parakeet-tdt-0.6b-v2-coreml` | 443 MB | After Stop | Meeting transcript |

`~/Library/Application Support/FluidAudio/` is **shared** with Yawn's own
FluidAudio research. Its `parakeet-tdt-0.6b-v3`, `nemotron-3-diarization`,
`sortformer` and `speaker-diarization` folders predate this install
(2026-09-07 and 2026-10-12) and are not FluidVoice's. Never delete that folder
to uninstall FluidVoice.

## Technical review

From the v1.6.11 source; clean-room, no code reproduced.

- **Stack.** Swift, SwiftUI on AppKit, macOS 15+. Its own forks of FluidAudio
  and DynamicNotchKit; a vendored WebRTC echo canceller.
- **Speech engines.** Parakeet on Core ML (batch and streaming), Nemotron,
  Whisper through a whisper.cpp-style library, Apple Speech, and external Core
  ML models.
- **Meeting pipeline.** System audio comes from ScreenCaptureKit, restricted
  to the one meeting app. The mic is a separate track with echo cancellation.
  After the meeting it runs Nemotron Sortformer diarization, then optional
  voice matching, then Parakeet, one model at a time. Live captions are skipped
  when memory is short.
- **Auto-detection** only prompts. It triggers on the mic turning on, then
  confirms the owning app and a meeting-titled window. Any ambiguity means no
  prompt. A prompt auto-dismisses after 20s, and an explicit dismiss mutes that
  app for 30 minutes.
- **Local data service.** Off by default (`LocalAPIModels.swift:14-15`). When
  on, it listens on port 47733 and drops non-loopback connections, but has
  **no authentication at all**: no token, no Host or Origin check
  (`LocalAPIServer.swift:35-38, 97-109`). Any local process, or a web page
  using DNS rebinding, could list meetings and read full transcripts.
- **Analytics.** PostHog EU. A daily activity ping always sends. "Detailed"
  analytics default **on** (`SettingsStore.swift:1519`), and the event fields
  include `meeting_count`. No transcript text was seen in the event fields.
- **Downloads are verified unevenly.** Its own model feed is Ed25519-signed and
  archives are SHA-256 checked. Whisper downloads get only a size check.
- **Engineering.** About 2,300 tests, ~980 of them for meetings, including 136
  crash-recovery tests. CI lints and tests but builds unsigned. Signed releases
  come from a script outside the repo. Several files exceed 6,000 lines, and
  about 6,100 lines of experiment harness ship in the product.
- **Closed component.** "Fluid Intelligence" (the AI cleanup and summaries) is
  a private runtime plugged in at build time behind a compile flag.

## Visual style and UI/UX

### Visual style

- Dark by default, with a navy sidebar, a teal selection fill, blue primary
  buttons, and a serif display face for page titles ("Be in the
  conversation.") on top of SF for everything else
  ([08](comparables-fluidvoice-2026-10-16/08-fluidmeet-home.jpg)). The light
  theme is a faithful inversion
  ([18](comparables-fluidvoice-2026-10-16/18-file-transcript-light.jpg)).
- Onboarding is a full-bleed marketing sequence with a glowing logo and an
  italic blue tagline ([01](comparables-fluidvoice-2026-10-16/01-welcome.jpg)).
  It reads as a launch page, not a Mac app.
- Three accent colors (teal, blue, orange-yellow), plus speed and accuracy
  gradient bars, compete across screens. Yawn's surfaces use one restrained
  accent and no decorative meters
  ([Yawn ledger, fixture data](screen-reviews/mac-text-size-installed-2026-10-16/default-ledger.png)).

### UI/UX findings

| # | Finding | Effect on the reader | Capture |
|---|---|---|---|
| 1 | No consent or recording notice before capture | The person recording is never reminded that others are being recorded | 13 |
| 2 | Start is blocked with "Wait for the active meeting transcription to finish" when no meeting exists | Points the reader at a cause that does not exist | 12 |
| 3 | Live captions stay empty; the only feedback is "dropped audio and resynced" | The reader cannot tell whether anything is being heard | 13 |
| 4 | The source menu reads "Automatic" after choosing "In person" | The reader cannot trust the control's label | 08 |
| 5 | With the meeting list open, the home card cuts off "Allow meeting audio access" | The one button that enables call capture is clipped | 08 |
| 6 | Model cards show "Accuracy 96%"; the tooltip says these are relative ratings, not measured accuracy | A made-up number is presented as a measurement | 03, 16 |
| 7 | Onboarding says ~242 MiB; about 2.7 GB arrived, 1.6 GB of it unprompted | The disk cost is not what the reader agreed to | 05 |
| 8 | The sidebar has 13 destinations across two products | Meetings are one row among dictation settings | 07 |
| 9 | "Speaker labels are automatic and may not be exact. Click a name to fix it." | Good: states the limit and the fix in one line | — |
| 10 | Every transcript turn has Edit and Audio (play this segment) | Good: correction and checking are one click from the text | — |

## Yawn against FluidMeet

Yawn's facts come from the [product brief](../product-brief.md),
`DEPLOYMENT.md` and Yawn's source, not memory. Yawn captures are fixture
data.

| | Yawn | FluidMeet |
|---|---|---|
| Consent before recording | Three confirmations every time (`apps/desktop/ui/main.js:93`) | None |
| Where data lives | This Mac only | This Mac only |
| Audio retention | 1, 7 or 30 days, chosen explicitly | After transcription, 7 days (default), 30 days or forever |
| Your own notes during the meeting | Yes, an operator-owned page; note moments are decided but not built | No note-taking in FluidMeet |
| Speech engine | Apple speech when ready, otherwise Whisper Q4; every downloaded byte checked against a catalog signed into the app | Parakeet v2 for the meeting transcript; feed signed, Whisper size-checked only |
| Speaker labels | Nemotron diarizer, pinned build | Nemotron Sortformer through FluidAudio |
| Interrupted run | Stated plainly, never shown as complete | Recovery is offered on launch for complete chunk sets; failed sessions keep their audio |
| AI summaries | Retired from the contract 2026-09-30 | Private local model; "coming soon" in 1.6.11, back in the 1.6.12 beta |
| Local network listener | None in the app (the one `TcpListener` is in a test, `note_projector_process.rs:2860-2862`) | Optional, unauthenticated, off by default |
| Analytics | None | PostHog, detailed on by default |
| Price and licence | — | Free, GPLv3 |

## What to take

These fit the brief as written. None needs a new product decision.

1. **A chunk ledger with recovery on launch.** Write audio in rotating chunks,
   each with an atomically written ledger entry. Offer recovery only for
   complete sessions, and keep the audio of failed ones. This strengthens
   "an interrupted run is stated plainly" without changing it.
2. **Check free disk space before recording**, sized to an hour of the actual
   recording format, and say so before Record rather than failing mid-meeting.
3. **A stage list after Stop** (Saving, Identifying speakers, Transcribing,
   Finalizing) with "you can close this window."
4. **One line on speaker-label limits**, next to the labels: they are automatic,
   may be wrong, and here is how to fix one.
5. **Play the audio of a single turn** from the transcript row, for checking
   one line without scrubbing.

## Out of scope by decision

FluidMeet ships these; the brief excludes them or would need a new operator
decision. They are listed so a future session does not treat them as gaps.

- AI meeting summaries (retired 2026-09-30).
- Claude and ChatGPT links over meeting notes ("coming soon" in 1.6.11).
- A local data service for other apps.
- Detecting meetings automatically and prompting to record.
- Usage analytics.

## Left on this Mac

- FluidVoice is installed and running. Meeting auto-detection is on, the Right
  ⌥ dictation hotkey is active, and detailed analytics are on.
- The test meeting holds about 2 minutes of room audio recorded at 20:52.
  FluidVoice will keep the audio until 2026-10-24 and the transcript
  indefinitely. It is at
  `~/Library/Application Support/FluidVoice/Meetings/B426C6C9-491B-4620-AA7E-BDD82BF5088A/`.
  Delete it from FluidMeet's meeting list if you do not want it kept.
- About 2.7 GB of models were downloaded for FluidVoice (table above).

## Sources

- [FluidVoice repository](https://github.com/altic-dev/FluidVoice) and its README
- [v1.6.11 release notes](https://github.com/altic-dev/FluidVoice/releases/tag/v1.6.11)
  and [v1.6.12-beta.1](https://github.com/altic-dev/FluidVoice/releases/tag/v1.6.12-beta.1)
- [fluidvoice.com](https://fluidvoice.com/) and the
  [FluidMeet post](https://fluidvoice.com/fluid/blog/fluidmeet)
- Source paths cite tag `v1.6.11`
