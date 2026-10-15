# iPad ink spike — specification

A research spike, not the Yawn product. It tests whether an iPad works for
in-person note taking: Apple Pencil strokes timed against a local microphone
recording, an on-device transcript, and replay. Its exports feed the Mac
scoring scripts in `spike/speech-engine-comparison/`. Nothing here changes
`apps/desktop`, `docs/product-brief.md`, `DIRECTION.md` or any product doc. A
cold screen review applies only if the spike is promoted to the product.

## Layout

- `InkCore/` — local Swift package (macOS 14 and iOS 26), pure logic, no UI.
- `IPadInk.xcodeproj` and `IPadInk/` — one iPad app target.
- `README.md`, `.gitignore` (for `.derived/`, `.cache/`, `InkCore/.build/`).

App target: `TARGETED_DEVICE_FAMILY = 2`, `IPHONEOS_DEPLOYMENT_TARGET = 26.0`,
bundle id `com.ninochavez.yawn.ipad-ink-spike`, automatic signing with an
empty `DEVELOPMENT_TEAM`, `GENERATE_INFOPLIST_FILE = YES`, one
file-system-synchronized group, `InkCore` linked as a local package.
Info.plist keys: `NSMicrophoneUsageDescription`,
`NSSpeechRecognitionUsageDescription`, `UIFileSharingEnabled = YES`,
`LSSupportsOpeningDocumentsInPlace = YES`. Structural template (read only):
`~/Workspace/dev/apps/minder/ios/Minder/Minder.xcodeproj/project.pbxproj`.

Do not use Swift macros (`@Observable`, `#Preview`, `@Model`): their plugin
server cannot start inside the build sandbox. Use `ObservableObject` and
`@Published`.

## App

1. **Start gate.** Recording cannot start until the user confirms "Everyone
   who can be heard agreed to this recording" and picks how long to keep the
   audio: 1, 7 or 30 days. No default selection.
2. **Recording.** Microphone only, `AVAudioEngine`, written as 16-bit PCM mono
   WAV (16 kHz preferred) to `Documents/meetings/<uuid>/audio.wav`. Record the
   start `Date`.
3. **Ink.** PencilKit canvas (`PKCanvasView` in a `UIViewRepresentable`, any
   input so the simulator works). A stroke's time is
   `stroke.path.creationDate` minus the recording start, in seconds. Strokes
   drawn while not recording have no time and show as untimed.
4. **Transcript.** `SpeechAnalyzer` with `SpeechTranscriber` for the device
   locale, fed from the same audio buffers. Volatile text is shown visibly as
   in progress; final text normally. Turns carry no speaker label, never "Me".
   If the locale model is not installed, show that state and an explicit
   Install button using `AssetInventory`; never show placeholder transcript
   text. If live transcription does not fit, transcribe `audio.wav` after
   Stop instead. Cut live transcription before anything else.
5. **Replay after Stop.** `AVAudioPlayer` with play/pause and a scrubber.
   Strokes timed at or before the playhead draw normally; later strokes are
   dimmed. Tapping a transcript line seeks to its start; tapping a stroke
   (hit test on `renderBounds`) seeks to its time.
6. **Keep period.** On launch, delete `audio.wav` for meetings older than
   their chosen days. Keep strokes and transcript; as in Yawn, the keep period
   applies to audio.
7. **Export**, in the meeting folder and visible in the Files app:
   - `audio.wav`
   - `strokes.json`: `[{index, start_seconds, end_seconds}]`; untimed strokes
     write `null` for both times (encode `null`, do not omit the key)
   - `transcript.json`: `[{start, end, text, final}]`, no speaker field
   - `meeting.json`: `{schema: "ipad-ink-meeting/1", created_at,
     consent_everyone_audible: true, keep_days, audio_sample_rate}`
   - `drawing.data`: `PKDrawing.dataRepresentation()`

Out of scope: accounts, network calls (`URLSession` and similar), analytics,
cloud, folders, handwriting recognition, speaker labels.

## InkCore

Moved to `packages/yawn-ink-core/` on 2026-10-15 so the Yawn iPad app and this spike share it; the module is still `InkCore`.

All testable logic: stroke-time mapping, stroke visibility at a playhead,
transcript merging (a final result replaces volatile results for the same
range), the keep-period decision (meetings with `created_at`, `keep_days`,
`now` → audio files to delete), and export JSON encoding. Tests cover an
expired meeting's audio selected and an unexpired one kept (this test must
fail if the sweep is broken), the 0-second and 30-day boundaries, a
volatile-then-final merge, visibility at a playhead, and JSON keys exactly as
above, including explicit `null`.

## Commands

Run from `spike/ipad-ink/`. Every cache stays inside it.

```sh
export CLANG_MODULE_CACHE_PATH="$PWD/.cache/clang"
(cd InkCore && swift test --scratch-path .build --cache-path ../.cache/swiftpm \
  --config-path ../.cache/swiftpm-config --security-path ../.cache/swiftpm-security)
xcodebuild build -project IPadInk.xcodeproj -scheme IPadInk -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath .derived \
  CODE_SIGNING_ALLOWED=NO
```

The simulator, Pencil latency, live microphone behavior and the speech model
can only be checked on a device.

## README

Say what the spike is (research, not the product), that it is in-person and
microphone only, that data stays on the iPad, that the only network use is
Apple's on-device speech model download, how to install on an iPad (iPadOS 26
or later, Developer Mode on, a team set in Signing), the export format, and
what the simulator cannot show.
