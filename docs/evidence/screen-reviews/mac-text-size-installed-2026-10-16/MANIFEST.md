# Yawn for Mac, text size — installed captures, 2026-10-16

Four frames from the installed Yawn Fixture app built from `f0bd6f2` (branch
`feat/mac-text-size`). Window 1080 by 900 at 2x, captured with
`screencapture -l` on the app's own window. Dark (the Mac's appearance; theme
choice System). Standard contrast and motion.

## How the frames were made

- **Data.** The shared Fixture root as reseeded by PR #145 (the 23-minute ledger
  meeting is the launch selection). Not reseeded or edited here.
- **Text size.** Default is the stored choice absent. For Largest, the same
  stored value the Settings button writes (`yawn.textSize` = `largest`) was
  written into the Fixture app's WebKit LocalStorage while the app was quit,
  because Computer Use's approval dialog went unanswered and the page's buttons
  are not reachable through System Events. The value was deleted afterwards.
- **Settings.** Opened from the app menu's Settings… item, clicked as an element
  through System Events on the process found by unix id. No keystrokes.
- **Every frame read back.** Each was opened after capture and checked for a
  painted window and the absence of the screen-capture indicator. One frame
  taken while the screen was locked came out unpainted and was discarded.
- **Pass 1** judged frames from `5004bd1`; they were overwritten when
  `f0bd6f2`'s fixes were recaptured. Pass 2 judged the frames here.

## Frames

| File | State | Text size |
|---|---|---|
| `default-ledger.png` | after the meeting: the ledger (rail, notes, nearest turns, folds, after-Stop line, one pause), transport | Default |
| `largest-ledger.png` | the same | Largest (x1.5) |
| `default-settings.png` | Settings: Appearance (Theme, Text size), Recording, Transcription, Speakers | Default |
| `largest-settings.png` | the same | Largest (x1.5) |

`review-map-pass1.txt` and `review-map-pass2.txt` map the neutral, shuffled
names each reviewer saw to these files.
