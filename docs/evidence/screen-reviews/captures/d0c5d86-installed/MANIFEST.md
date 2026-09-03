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

## Findings

**F1 — Generate note runs for minutes, then fails silently and writes nothing.**
The headline. `08-dark-generating` shows the state entered correctly: "Preparing
your meeting note.", the button disabled as "Generating note…", and "The note
model is reading this transcript on your Mac." Five minutes later the document
had reverted to **the pre-generation screen**. Compared pixelwise against
`01-dark-open`, `09-dark-after-generate` is visually identical: 18 of 3,888,000
pixels differ by more than 8/255, and none by more than 60 — antialiasing, not
content.

Nothing recorded the attempt. No note, no diagnostic file, no change to
`attempt.json`, no entry in the unified log: `find` over the whole preview
application-support tree reports zero files modified in the surrounding two
hours. The note model is installed and complete (gemma-3-12b-it-qat-4bit, 7.5 GB,
both safetensors shards, matching `active-note-model.json`), and the worker
processes were alive at 0.0% CPU afterwards.

So the operator's second attempt is indistinguishable from never having tried:
same heading, same body, same button, no error, no timestamp, no attempt count.
The product brief requires that "an interrupted or failed run is stated plainly."
This is a failed run stated as nothing at all.

**Reproduced** in `12-dark-second-generate-attempt`, with the app relaunched
directly from its binary so its stderr was captured. Four minutes; the app wrote
**zero bytes** to stderr, spawned no note-projector child (only the two standing
python workers, both at 0.0% CPU), and the document reverted exactly as before.

What that rules out: the bundle is not missing the projector -- `note-bridge.py`,
`note-generator-mlx.py`, `note-runtime-project.json`,
`note-runtime-generate.json` and `note-validator.zip` are all present in
`Contents/Resources`. The failure is upstream of anything that writes, logs, or
forks. (The installed `/Applications/Yawn.app` has none of those files, which is
**not** a finding: it is 0.5.7 from Aug 12 and predates that packaging.)

**F2 — On the delete sheet, the irreversible action and the escape hatch are the
same colour.** Measured from the full-resolution frames, not eyeballed:

| Sheet | Confirm button | Cancel button |
|---|---|---|
| Move to Trash (04) | `rgb(58,58,58)` | `rgb(58,58,58)` |
| Lock meeting (06) | `rgb(59,130,247)` | `rgb(58,58,58)` |

The reversible action gets a full blue primary. The irreversible one is pixel-
identical to Cancel.

This is not R34 failing. R34 deliberately removed an inert `danger` class so the
control would render as a plain `.btn`, which is what DESIGN.md wants, and that
is exactly what the frame shows. The finding is the asymmetry the fix leaves
behind, which R34 explicitly declined to settle and handed to this review:
whether a destructive action deserves any distinct treatment. The frames now
answer the half that was unmeasurable before — on this build the destructive
confirm carries *less* visual weight than the reversible one, because Lock
claims `.primary` and Trash claims nothing.

**F3 — The Source transcript header is broken.** `10a` at full resolution: the
descriptive sentence is collapsed into two ~40px columns, one word per line
("read the / complete / retained / conversation." and "9 / turns / are / cited /
by / the / note."), and the three buttons are painted on top of it — the word
"of" is visible behind "Copy transcript". This is in the main reading path, on a
packaged, signed build.

**F4 — "Delete transcript" carries no ellipsis and no confirmation**, while both
of its neighbours in the same menu do. It is the one action in that popover that
destroys retained text, and the only one offered without a sheet.

**F5 — The delete sheet and the menu item disagree on the verb.** The menu says
"Move to Trash…"; the sheet says "Delete this meeting?" and its button says
"Delete meeting". The body then explains that it moves to Trash for 30 days. The
gentler, accurate word is the one the confirmation drops.

**F6 — Both sheets frame the meeting title in a red-bordered box**, including the
Lock sheet, which is not destructive.

**F7 — The note document has an Overview and nothing else.** No decisions,
follow-ups, or open questions sections appear in `02-dark-note`, which the
product brief names as the structure after the overview. This may be correct for
this meeting's content; it is recorded as observed, not diagnosed.

**F8 — The auto-derived title is raw transcript text**: "um, rule-based like
system where it's like these products and these fields," — used as the meeting
title, the window title, and the sidebar row.

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
