# Installed-app capture — build 21 (d0c5d86), 2026-09-03 morning

**Supersedes `d0c5d86-installed-partial/`**, which held one frame. Same bundle,
same storage, eleven more frames. The partial's three settled rows (R23, R24,
R24b) are re-confirmed here in `01-dark-open`.

Device: this Mac, packaged `Yawn Preview.app`, binary timestamped Sep 2 21:03 —
the same build 21 binary as the partial. The app was **not** rebuilt for this
pass; the repository moved (capture-script and roadmap commits only) but nothing
under `apps/desktop` or the Rust crate changed, so this set describes d0c5d86.

Real storage, the same four meetings. Window at its 1080x900 default at screen
origin 360,80 except `11-dark-settings`, which is the 720x720 Settings window at
540,211. Every frame was taken from live window bounds and read back from disk
before inclusion. Dark only — see "Not captured".

**Storage integrity.** Four meetings before the pass, four after. Nothing under
the preview's application-support tree was written during the pass. The delete
confirmation and the lock sheet were both opened and both cancelled; no
destructive action was confirmed.

## Frames

| File | State | Settles or shows |
|---|---|---|
| 01-dark-open | Sep 1 8:55 meeting, note-not-created | R23 recovery copy, R24 caption, R24b two sentences — re-confirmed |
| 02-dark-note | The Aug 10 meeting's generated note | R23's document: Overview, two claims with source affordances, Your notes, Full transcript |
| 03-dark-manage-menu | Manage popover | First capture. Lock meeting…, Delete transcript, Move to Trash… |
| 04-dark-trash-confirm | Delete confirmation sheet | **R27, R34 — first capture of this surface** |
| 05-dark-after-cancel | After Cancel | Cancel returns cleanly; meeting intact |
| 06-dark-lock-sheet | Lock meeting sheet | First capture. The styling control for 04 |
| 07-appearance-override-ignored | Light override written, app still dark | Evidence the per-app override does not take |
| 08-dark-generating | Note generation in flight | **First capture. Newly reachable at all** |
| 09-dark-after-generate | Five minutes later | The generation's actual outcome — see F1 |
| 10-dark-transcript | Full transcript expanded | Source-transcript header, find field, turns with speaker and timestamp |
| 10a-dark-transcript-header-detail | Full-res crop of 10 | The layout break in F3 |
| 11-dark-settings | Settings window | R22 grouped list, R26 dark primary, the three privacy facts |
| 12-dark-second-generate-attempt | Four minutes after a second Generate note | The reproduction in F1 |

## Observed, not photographed

**D-FREEZE — confirmed.** Twelve consecutive accessibility polls across 28 s
during an active generation, every one answered, window count 1 throughout.
Reply times were ~280 ms with two spikes to 1158 ms and 958 ms at t+6s and t+9s.
Before D-FREEZE the window was unreadable for the whole run. The two spikes are
reported rather than smoothed; they did not make the window unreadable.

**A launch-time accessibility flake.** Immediately after relaunch the window
count read 0 while `window 1` was simultaneously addressable and returned correct
geometry; it settled within about two seconds and stayed stable across six
consecutive reads. `cap_launch` returns on the first non-zero count, so a frame
taken in that window fails. This aborted one frame during the pass and is the
reason `07` was retaken.

## Not captured, and why

**Every light frame, and R35 with them.** The app ignores a per-app appearance
override. `defaults write com.ninochavez.local-meeting-notes.preview
AppleInterfaceStyle Light` was written to the correct domain — verified by
reading it back, and the domain matches the bundle's real `CFBundleIdentifier` —
and the app relaunched and rendered dark regardless. `07` is that frame, kept as
evidence of the failure rather than filed as a light frame.

The only remaining route is switching the **system** appearance, which
`cap_appearance` deliberately refuses to do so the operator's desktop is never
changed under him. That is an operator decision, not one this pass made. Until
it is taken, R35's light contrast figures (`--record` 4.14:1, `--attention`
4.48:1) stay computed rather than observed.

**The retry decision sheet (R27, R32).** No affordance for it was found. On the
failed meeting, Generate note goes straight into generating with no intervening
choice. On the meeting that already has a note there is no regenerate control at
all — its Manage popover offers only Lock meeting…, Delete transcript and Move to
Trash…. It appears unreachable on this build rather than merely unvisited.

**transcript-only and model setup**, unchanged from the previous record: no
meeting is in a transcript-only state any more, and model setup needs a Mac with
no speech model installed.

**D-OPENFREEZE** was not re-measured; the partial already settled it.

## What this file deliberately does not contain

The capture author's own findings are **not** here. They live beside this file in
`FINDINGS.md`.

The split exists because of a measured mistake. On 2026-09-03 a cold review was
commissioned as blind and pointed at this directory; the reviewer opened
`MANIFEST.md` expecting a frame index and found the capture author's conclusions
instead. It disclosed the read and recorded that its own measurements came
first, so its six new findings stand -- but the overlapping ones stopped being
independent confirmation, which was the whole point of commissioning the review.

A reviewer pointed at this directory can read this file safely. That is the
property the split is protecting.
