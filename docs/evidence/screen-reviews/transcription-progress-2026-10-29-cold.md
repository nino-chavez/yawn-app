---
surface: transcription-progress (the meeting page's transcribing status: a determinate bar and a percent under the status sentence, and the status block's alignment)
kind: cold
build: feat/transcription-progress at 31ccaa8 (pass 1), ce30247 (pass 2), 5207d01 (pass 3)
device: ui-harness (`apps/desktop/ui-harness/harness.html?mode=transcribing-progress`, `-cap`, `-none`; the shipped `ui/` frontend against `tauri-stub.js`) at 1080 by 760 CSS pixels, scaled to 800 wide by the capture; light and dark by window appearance; invented amounts for a 3664 s meeting; not an installed app
reviewer: fresh subagents (Claude), one per pass, each given only the frames under neutral names, the capture settings and the job questions
implementer: the session that built it and its desktop-lane subagent (Claude), not any reviewer
cold: true
states: [progress-40-light, progress-40-dark, progress-99-cap-light, progress-99-cap-dark, no-progress-light, no-progress-dark]
verdict: revise on every pass. Pass 3 trusted the reading ("The bar fill (about 144 of 360 px) matches the number, so I trust it") and found no contradiction on this change. Its remaining findings are outside this change (below)
---

# Transcription progress on the meeting page — cold review, 2026-10-29

**Question:** A person recorded a meeting earlier and opens it while Yawn transcribes it. Can they tell what is happening, how far along it is, and whether to wait? Does anything look broken or contradict anything else?

**Before:** the page said "Your audio is saved on this Mac and is being transcribed. The transcript will appear here when it’s ready." It gave no measure of progress. The sentence sat 56 px (at 800 wide) left of the title and notes column, directly under the notes placeholder with no gap.

| Pass | Frames | Verdict | Findings on this change |
|---|---|---|---|
| 1 | 6 (40%, 99%, none; light and dark) | revise | The bar is about 195 px under a sentence about 440 px wide, so it looks undersized and lines up with nothing. Its track nearly disappears in dark. The "40%"/"99%" label is small and gray. Reader would "half-trust" the percent. The bar fill matched the number, light and dark were consistent, and nothing else in the layout was broken or cramped. |
| 2 | 6, new order | revise | "Nothing is broken. All three states are legible in both appearances." The reader can't tell what the bare percent is a percent of. They saw a possible blob at the end of the 99% fill in light. At 2x it is the last 1% of track past the fill's round cap, so nothing was changed for it. The dark track still read as faint, though it measures 3.25:1 against the page. |
| 3 | 6, new order | revise | "40% transcribed" was understood and trusted. The 99% frames read "99% transcribed, with a full bar and a small stub at the right end". On this change: the label's right edge (about x=730) stops short of the player row's (about x=770). The status block keeps the status sentence's measure, which was the case before this change. Every other finding is outside this change and was raised before, or comes from the stub data (below). |

**Changed after pass 1:**
- The bar line now uses the status sentence's measure, at the same left edge (620 px at 1080 CSS px). The percent sits at its right end.
- A new `--progress-track` token, in both appearances. Measured contrast:

  | Pair | Light | Dark |
  |---|---|---|
  | fill on track | 4.89:1 | 3.83:1 |
  | track on page | 3.23:1 | 3.25:1 |
  | percent text (`--label`) | 15.79:1 | 12.43:1 |

- The percent uses the sentence's size (`--t-body`) and tabular numerals.

**Changed after pass 2:** the visible read became "<n>% transcribed".

**Pass 3 also raised, outside this change:**
- The status reads weaker than the notes placeholder above it. The header's "Transcribing" is the faintest text on the page.
- The harness stub's "Earlier harness meeting" (9:30 AM, no date) sits under "PREVIOUS 30 DAYS". That is stub data.
- The scrubber has no handle. The source pills show selection only by weight.

**Findings outside this change (recorded, not acted on):**
- "Is it alive, and can I walk away?" Passes 1 and 2 both wanted a line saying whether transcription continues if the page is left, the app quits or the Mac sleeps. That rewrites the operator-accepted 2026-10-07 status sentence, so it is the operator's call.
- With no progress (the Whisper path, or a meeting still queued), the page shows no moving element. Passes 1 and 2 both read it as possibly stalled and suggested an indeterminate "Starting…" bar. That would be false for a queued meeting, and the no-progress page is the accepted 2026-10-07 surface, so it was not added. Both reviewers raised it, so it is for the operator to decide.
- No cancel or stop control (pass 2).
- The player row's right edge (the source chips) extends past the status block's measure (pass 2). This was true of the status sentence before this change.
- A time estimate, or a stage name at the 99% tail. Time estimates are excluded by design (DIRECTION.md 2026-10-29 row). The tail between full coverage and the transcript is seconds. "Finishing" is the note-generation word.
- The sidebar row says "transcribing" without a percent. It is left unchanged by design.
- Already true before this change: the notes placeholder reads as faded body text with no field edge; the header meta line and the "Microphone · you" / "System audio · the call" chips have low contrast in dark; the title appears in both the toolbar and the heading.

Frames: `transcription-progress-2026-10-29/pass-1/`, with the neutral-name map in `review-map-pass1.txt`.
