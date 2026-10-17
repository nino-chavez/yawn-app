---
surface: mac-serif-title (the meeting title in the system serif; the ledger, the first-run sheet and page)
kind: cold
build: 2a18d23
device: installed Yawn Fixture app on this Mac, 1080 by 900 at 2x, the app's own window captured with screencapture -l; Dark (theme System) and Light; Default and Largest text size
reviewer: one fresh subagent (Claude), given the nine frames under shuffled neutral names, the capture settings and the five job questions only
implementer: the session that built and captured it (Claude), not the reviewer
cold: true
states: [dark-ledger-default, light-ledger-default, dark-ledger-largest, light-ledger-largest, dark-firstrun-sheet, light-firstrun-sheet, dark-firstrun, light-firstrun, light-firstrun-largest]
verdict: serif title accept in every frame it appears; the ledger at Default accepts in both appearances; the other revises repeat findings already decided in earlier reviews
---

# Mac serif meeting title, installed-build cold review, 2026-10-16

**Question.** Does the meeting title in Apple's system serif hold on the
installed build? The checks were long titles, the first run and the Largest
text size, in both appearances. The operator decided this amendment on
2026-10-16 (DESIGN.md, Type).

**Captures.** `mac-serif-title-installed-2026-10-16/` with its `MANIFEST.md`.

**The change.** `font.title` (`ui-serif, "New York", Georgia, serif`) was added
to `tokens.json` and applied to `.doc-head h1` only. Size, weight and line
height are unchanged (20px, 650, 1.25, scaled by the text size).

## What the reviewer said about the title

The reviewer did not know which element had changed.

- "The serif meeting title is the strongest element in the meeting frames. It
  reads first, and it gives the app a distinct look."
- The serif title, sans notes and small section labels "is coherent."
- On s03 (light, Largest): "The serif title is good."
- Category read: native Mac app. "The warm paper tones and the serif title give
  it a point of view without breaking that."
- The reviewer noted that the first-run states use only sans, which is the
  amendment as written: the serif belongs to a meeting's title and nothing
  else.

No frame raised a title defect. The 77-character title wraps to two lines at
Default and at Largest without clipping, and the toolbar repeats it in sans.

## Verdicts, mapped to frames

| Frame | Reviewer verdict | Finding | Disposition |
|---|---|---|---|
| `light-ledger-default` (s05) | accept, minor note | "After the meeting" rail label sits apart | Existing rail label, unchanged here |
| `dark-ledger-default` (s06) | accept | Dim secondary text legible | — |
| `light-ledger-largest` (s03) | revise | Sidebar rows wrap to three lines and truncate; rail crowds notes; notes about five words a line | Kept as design in `mac-text-size-installed-2026-10-16-cold.md`; not caused by the serif |
| `dark-ledger-largest` (s01) | revise | Same as s03; transcript column dim | Same |
| `dark-firstrun-sheet`, `light-firstrun-sheet` (s02, s09) | revise | Card flush to the toolbar; Continue's prominence differs by appearance | The attached sheet placement is already an operator item (`mac-rail-installed-2026-10-16-cold.md`). The Continue difference is the graphite accent in each appearance. Neither is touched here |
| `dark-firstrun`, `light-firstrun`, `light-firstrun-largest` (s04, s08, s07) | s08 accept, s04 and s07 revise | "Try it" chip heavier than the sidebar; the rail runs the empty page; copy wraps at Largest | The bare rail and the three prompts are already recorded in the Rail review; not caused by the serif |

The reviewer read s04 as large text; it is Default. Its findings stand without
that.

## What was removed, combined, demoted or hidden

Nothing. The change adds one font role to one element.

## Not proven

- One reviewer, one pass, Claude.
- A title made of unbroken tokens was seen during setup but not captured as
  evidence (see the manifest).
- Increased contrast.
- The iPad title is unchanged by decision.
