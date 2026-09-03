# Capture findings — build 21 (d0c5d86), 2026-09-03

**Do not read this file if you are writing a cold review of these frames.** It is
the capture author's own reading of them. `MANIFEST.md` beside it is the frame
index and is safe to read.

## Findings, most severe first

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
