---
surface: ipad-paper (first run, the meeting list, the prepared Rail components)
kind: cold
build: 78158ca (components); 7acb46b for first run and the list, unchanged since
device: iPad Air 11-inch (M4) simulator, iOS 26.5, portrait, 820 by 1180 points at 2x (not a device capture)
reviewer: four fresh subagents (Claude), one per pass, each with the frames and the job questions only
implementer: the session that built it (Claude), not any reviewer
cold: true
states: [first-run-light, first-run-dark, first-run-xxxl, first-run-increased-contrast, sidebar-light, sidebar-dark, components-light, components-dark, components-xxxl, components-consent]
verdict: revise
---

# Yawn for iPad, the Rail paper language — cold review, 2026-10-16

**Question.** With the paper palette on the existing screens and the Rail
components prepared for the recorder and page slices, does the first screen
read as a page one will write on, is Record's unavailability honest, and do
the prepared components read as the states they carry?

**Captures.** `ipad-paper-2026-10-16/` with its `MANIFEST.md`: both
appearances, the largest standard text size, Increase Contrast, the sidebar
open, and a debug-only specimen page of the prepared components reached with
the `-paperSpecimen` launch argument. Simulator state was read before and
after each run and restored. Not device captures: the operator's iPad was not
connected; the build-plan rule that each slice is run on the iPad still
holds for the recorder slice this prepares.

**Passes.** Each pass was a fresh reviewer with neutral frame names (i1 to
i10, mapping in the manifest) and no earlier pass.

| Pass | Frames | Verdict | What it found |
|---|---|---|---|
| 1 | i1-i9 | screens revise; components not reviewable | The implementer's packet defect: i7-i9 duplicated first run, because the hand-written Xcode project never defined DEBUG and the specimen route was compiled out. Screens: the headline tells you to record while Record says it is unavailable; the Record capsule still reads as a button; the vertical line reads as a stray divider; a forced bar background painted a band over the floating sidebar's top corners; Increase Contrast changed nothing visible; the empty list's symbol and bold title outranked the page |
| 2 | i1-i9, recaptured | screens revise; components revise | Increase Contrast barely visible; the Record caption touches the bar's edge at the largest text size; Paused differs from Recording by tint only; the active tool reads as active by color only; the tool capsules differ in width at the largest size; the consent card was cut off by the frame; the selected row's locator meets the rounded corner |
| 3 | i1-i10 | screens revise; components revise | Tool capsules still unequal at the largest size; the active tool still color-first; Start faded with no reason and no keep period chosen; the specimen's tall empty large-title band; Increase Contrast "real but small" |
| 4 | i7-i10 | components revise | The consent statement truncated in the toggle's label; the keep-period control shows no choice; Paused the same color as Recording; the unavailable Record reads merely grey; the pen glyph (`pencil.tip`) unclear; the rows' dots unexplained |

**Changed, by pass.**

- After pass 1: the project defines DEBUG for Debug builds (the specimen
  compiles only then); the catalog's Hairline, Hairline strong, Label 2 and
  Label 3 sets carry high-contrast variants from `tokens.json`'s
  `contrastMore` block, so Increase Contrast darkens them as
  `prefers-contrast` does on the Mac; the sidebar keeps the system's own bar
  (the band is gone); the empty list is a title and one line, no symbol.
- After pass 2: Paused shows two bars, not a dimmer dot; the active tool
  carries a Moment ring; symbol-only capsules take a minimum width; the
  unavailable Record's caption tightens at the larger text sizes; the
  consent card got its own frame.
- After pass 3: symbol-only capsules are one fixed round size; the active
  tool is bold as well; the card says under its faded Start why it waits.
- After pass 4, checked by the implementer only: the consent statement
  wraps (`fixedSize(horizontal: false, vertical: true)`); the specimen page
  has an inline title instead of the empty band.

**Not changed, and why.**

- **The empty rail on first run.** Every cold reviewer on both platforms
  (Mac passes 1 and 2, iPad passes 1 to 3) read the bare line as a stray
  divider. It is Concept C's structure, which the operator selected and this
  task was told not to change. This is the one finding the operator should
  decide: keep the bare rail as the page's margin, or draw it only once a
  mark exists. **Decided 2026-10-16:** kept as the page's margin; recorded
  in `DESIGN.md`.
- **The unavailable Record capsule and its caption.** Pass 1 of the slice-1
  review asked for exactly this (a disabled toolbar button faded almost out
  of sight). It ends when the recorder slice lands; `feat/ipad-recorder`
  holds that work unmerged.
- **The headline "Record a meeting" beside an unavailable Record.** Same
  cause, same end.
- **Two empty messages side by side (list and page).** They describe two
  objects; the list's is now a title and a line. Kept.
- **Paused on Record soft.** DESIGN.md gives the recording badge and paused
  the same surface; the glyph and the word carry the state. Kept.
- **Keep period with no choice shown.** The product brief makes the keep
  period an explicit choice; the hint under Start says so. Kept.
- **The pen glyph, the rows' dots, undo in the tool column.** Prepared for
  the page and list slices, which will judge them on the device with ink and
  real rows.
- **The bottom-right arc, the sidebar toggle's halo, the panel's shadow.**
  iPadOS's own chrome.
- **Increase Contrast is subtle.** It applies the measured `contrastMore`
  values (`docs/evidence/design-contrast-2026-10-15.md`); the same values
  the Mac uses. A stronger step is a token decision, not a view's.

**Removed, combined, demoted, or hidden.** Removed: the empty list's symbol;
the forced background on the sidebar's bar. Demoted: the unavailable
Record's caption at the larger text sizes (caption2, no gap). Hidden:
nothing. Combined: nothing.

**Still owed.** Device captures on the operator's iPad before the recorder
slice merges; the Increase Contrast and largest-text frames again on the
device.
