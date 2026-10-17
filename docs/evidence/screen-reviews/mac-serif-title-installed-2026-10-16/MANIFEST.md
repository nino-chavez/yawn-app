# Mac serif meeting title, installed captures, 2026-10-16

Nine frames from the installed **Yawn Fixture** app, built from `2a18d23`
(branch `design/serif-meeting-title`) with `npm run fixture-build` and signed
with `scripts/prepare-preview-bundle.sh sign`. Each frame is the app's own
window, taken with `screencapture -x -o -l <window id>` at 1080 by 900 points
and 2x.

## How the frames were made

- **Data.** The shared Fixture root as it stood, not reseeded. The ledger
  meeting (`99999999-…`) is the launch selection. Its title, "Fixture: this is
  the launch checklist review, recorded as a synthetic meeting" (77
  characters), is the long-title case. It wraps to two lines at Default and at
  Largest.
- **Appearance and text size.** Set by writing `yawn.appearance` and
  `yawn.textSize` into the Fixture app's WebKit LocalStorage while the app was
  quit, the same values the Settings controls write. The original
  `yawn.appearance` (`system`) was restored and `yawn.textSize` removed
  afterwards. Dark frames are the Mac's own appearance with theme System.
- **First run.** `meetings/` was moved aside and the app relaunched to capture
  the sheet. The empty `first-run-seen.flag` that Continue writes was then
  created to capture the page. Afterwards the flag was removed (it was absent
  before), the empty `meetings/` the app had created was removed with
  `rmdir`, and the folder was moved back (8 meetings). A relaunch matched the
  earlier ledger frame except for the window's inactive traffic lights.
- **No Computer Use in any kept frame.** Computer Use draws macOS's purple
  screen-capture indicator over the traffic lights while it drives the app.
  So every kept frame was taken after a relaunch without it, and quits went
  through a System Events menu-item click.
- **Checks that can fail.** Each frame was checked twice:
  - for a painted interior. A first capture of a hidden 1000 by 1000 window
    was rejected.
  - for the indicator. The first threshold missed a known-indicator frame, so
    it was fixed and proven on that frame before use.

## Frames

| File | State | Appearance | Text size |
|---|---|---|---|
| `dark-ledger-default.png`, `light-ledger-default.png` | after the meeting: the ledger with the long title | both | Default |
| `dark-ledger-largest.png`, `light-ledger-largest.png` | the same | both | Largest (×1.5) |
| `dark-firstrun-sheet.png`, `light-firstrun-sheet.png` | no meetings yet: the first-run sheet | both | Default |
| `dark-firstrun.png`, `light-firstrun.png` | no meetings yet: the first-run page | both | Default |
| `light-firstrun-largest.png` | the first-run page | light | Largest |

`review-map.txt` maps the shuffled neutral names the reviewer saw (`s01` to
`s09`) to these files.

## Not captured

- **A title of unbroken tokens** (the `cur0 cur1 … cur20` fixture meeting).
  Selecting it needs a click, and the click brings Computer Use's indicator
  with it. It was seen during setup, wrapping to two lines in the serif with no
  clipping, but it is not evidence of record.
- **Increased contrast**, as in the earlier Rail reviews.
