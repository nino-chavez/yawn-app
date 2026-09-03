# Capture findings — build 21 (d0c5d86), 2026-09-03

**Do not read this file if you are writing a cold review of these frames.** It is
the capture author's own reading of them. `MANIFEST.md` beside it is the frame
index and is safe to read.

## Findings, most severe first

**F1 — CORRECTED 2026-09-03. Generate note is offered on a meeting that cannot
produce a note, and the refusal is never shown to the operator.**

The original F1 claimed generation failed silently and recorded nothing. **That
was wrong, and the error was mine.** The app recorded every attempt properly.
`operations/` holds a complete request/result/commit receipt for each of the
three clicks; today's reads `"status": "rejected"`, `"failure_code":
"note-rejected"`, `"lifecycle": "summary-failed"`. Request to commit was **23
seconds**, not five minutes — the five minutes was the gap between my click and
my screenshot.

The claim rested on a `find` that could never have matched. `find -newermt
"-2 hours"` is an invalid timestamp for this machine's `find`, which errors on
it; stderr was redirected to `/dev/null`, so an errored predicate returned no
rows and I read the empty output as evidence of absence. It is the same
fail-open shape as the three capture-guard defects fixed earlier the same day,
committed by the person who had just fixed them.

**The real cause, verified at source.** The meeting's retained transcript holds
**zero turns** — a healthy 28-second capture that recorded silence.
`worker/note_validator.py:957` refuses that by design:
`if not transcript.turns: raise GenerationRefused("no-generatable-transcript", True)`.
The mlx child is never spawned because the generator session is built lazily,
which is why no projector process appeared. All correct behaviour.

**What is still a defect, and it is the one that matters.** Nothing reaches the
operator. `crates/session-core/src/product_coordinator.rs` drops the failure
code and receipt outside `note_trace_enabled()`, and `library_reader.rs` renders
the summary-failed surface as a pure function of the meeting record — no attempt
count, no timestamp, no reason. So the operator clicks Generate note, waits, and
is returned to a screen identical to the one before the click. The product brief
requires that "an interrupted or failed run is stated plainly."

Sharper still: `library_reader.rs:1376-1382` sets `regeneration_source_sha256`
for a summary-failed meeting **with no turn-count check**, so the app offers an
action it already knows cannot succeed. Full trace with citations:
`docs/evidence/note-generation-silent-failure.md`.

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
