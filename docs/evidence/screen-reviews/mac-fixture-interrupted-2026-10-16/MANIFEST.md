# Mac interrupted and unreadable-note pages, installed build, 2026-10-16

Captures of the installed **Yawn Fixture** app (`com.ninochavez.local-meeting-notes.fixture`),
the same signed build as `mac-rail-installed-2026-10-16` (main's UI; this PR changes
only the fixture seeder). Each frame is the app's own window, `screencapture -o -l
<window id>`, 1080 by 900 points at 2x.

## How the frames were made

- **Data.** The previous fixture root was moved aside
  (`…fixture.archive-20261016-interrupted`). A fresh root was seeded with this branch's
  `rendered-review-fixture`. No meeting was moved by hand. The app launched straight
  onto the ledger meeting (`launch-dark.png`), not the "Yawn cannot record yet" wall
  that the old tampered transcript caused.
- **Interrupted meeting (`aaaaaaaa-…`).** The seed wrote an incomplete capture (an
  ownership receipt and two 6:00 silent audio legs) and ran the real startup recovery
  over it. Recovery bound the audio, wrote the interruption receipt and set the
  lifecycle to recovered-interrupted.
- **Unreadable-note meeting (`77777777-…`).** A valid transcript, plus an
  `operator-note.json` that does not parse.
- **Driving.** Codex Computer Use clicked rows, the Settings gear and the Theme choice.
  Nothing was typed.
- **Appearance.** Dark is the Mac's own appearance. Light is Settings → Appearance →
  Light. The theme was set back to System afterwards.
- **Every frame verified.** The capture helper activates the app, captures, and rejects
  a frame whose interior is one flat color (unpainted) or that shows the purple
  Computer Use indicator over the traffic lights. Exactly one Fixture process ran for
  each frame. The pass paused once, because the Mac's screen locked; a locked screen
  leaves the window unpainted, and the helper rejected those frames.

## Frames

| File | State | Appearance |
|---|---|---|
| `launch-dark.png` | Launch on a freshly seeded root: the ledger meeting, the interrupted row at the bottom of the sidebar | Dark |
| `interrupted-dark.png` | Interrupted recording: audio kept, "This recording did not finish", stop mark at 6:00 | Dark |
| `interrupted-light.png` | The same | Light |
| `unreadable-note-dark.png` | A meeting whose saved note cannot be read: "Not editable" | Dark |
| `unreadable-note-light.png` | The same | Light |

`launch-dark.png` is a launch receipt, not a judged frame. The other four went to the
blind cold review as `n1`-`n4` (map in `review-map.txt`).

## Not covered

- The live recording still exists only as harness frames, because it needs real
  capture attestations.
- Increased contrast and large text were not captured. See the 2026-10-16 installed
  record; large text has its own session.
