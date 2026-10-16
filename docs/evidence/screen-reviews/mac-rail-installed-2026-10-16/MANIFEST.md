# Mac Rail, installed build, 2026-10-16

Captures of the installed **Yawn Fixture** app (`com.ninochavez.local-meeting-notes.fixture`),
built from this branch at `4588e87` with
`npx tauri build --config src-tauri/tauri.fixture.conf.json --features preview-surface`
and signed with `scripts/prepare-preview-bundle.sh sign`. Each frame is the app's
own window, `screencapture -o -l <window id>`, 1080 by 900 points at 2x. These
frames replace the WKWebView harness frames in `mac-rail-2026-10-16/harness` as
the evidence of record for the Rail Mac wave.

## How the frames were made

- **Data.** The fixture root was archived (`…fixture.archive-20261016-installed2`)
  and reseeded with `rendered-review-fixture`, which seeds the 23-minute ledger
  meeting (`99999999-…`) with note moments. Meeting `77777777-…` (the
  export-tampered transcript) was moved out of the root for the whole pass: its
  tampered transcript makes startup recovery block the entire app ("Yawn cannot
  record yet. / meeting recovery requires attention"), so nothing else is
  reachable while it is present. That is recovery failing closed by design; it is
  recorded as a fixture defect, not judged here.
- **Driving.** Codex Computer Use clicked rows, the gear, the theme choice and
  the first-run sheet's Continue; nothing typed into an editor.
- **Appearance.** Dark is the Mac's own appearance. Light is the new in-app
  choice (Settings → Appearance → Light), which also sets the native window
  chrome, so the light frames show a light title bar.
- **Every frame verified.** The capture helper activates the app, captures, and
  rejects a frame whose interior is one flat color (a window that has loaded but
  not painted, which happens when it launches inactive); the detector was checked
  on known blank and known content frames first. Exactly one Fixture process was
  running for each frame.
- **First run.** The `meetings/` folder was moved aside, the app relaunched, the
  first-run sheet captured, Continue clicked, the page captured, then the folder
  restored and the first-run marker cleared.

## Frames

| File | State | Appearance |
|---|---|---|
| `dark-ledger.png`, `light-ledger.png` | after the meeting: the ledger (rail, notes, nearest turns, folds, after-Stop line, one pause), transport | both |
| `dark-nodraft.png`, `light-nodraft.png` | a meeting whose saved AI draft can no longer be shown, with meeting context | both |
| `dark-transcript-only.png`, `light-transcript-only.png` | a meeting with a transcript and no notes | both |
| `dark-settings-top.png`, `light-settings-top.png` | Settings as a page in the main window: Appearance, Recording, Transcription | both |
| `dark-settings-bottom.png`, `light-settings-bottom.png` | Settings scrolled: Speakers, Storage, Updates | both |
| `dark-firstrun-sheet.png`, `light-firstrun-sheet.png` | no meetings yet: the first-run sheet | both |
| `dark-firstrun.png`, `light-firstrun.png` | no meetings yet: the first-run page after Continue | both |

`review-map.txt` maps the neutral names the cold reviewer saw (`m01`…`m14`,
shuffled) to these files.

## Not captured, and why

- **The live recording and the interrupted recording.** The fixture seeds no
  interrupted meeting, and recording needs the operator's consent attestations;
  both stay on the harness frames (`mac-rail-2026-10-16/harness`) until the
  fixture seeds an interrupted meeting.
- **Increased contrast.** It is a system setting the agent does not change; the
  operator chose to record the gap (2026-10-16). `tokens.css` carries a
  `prefers-contrast: more` rule for four tokens that no installed capture has
  shown yet.
- **Large text.** Not captured: every font size in the UI is a fixed pixel value
  and the app is absent from the system's per-app text-size list, so the setting
  has no effect. That is a gap against DESIGN.md's proof list, not a rendering to
  review.
