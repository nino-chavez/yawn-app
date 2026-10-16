---
surface: mac-rail (the page after a meeting, a meeting whose AI draft is retired, a transcript-only meeting, Settings as a page in the main window, first run)
kind: cold
build: 9f44753 (pass 2 judged frames from this build; pass 1 judged frames from 807a87e's tree)
device: installed Yawn Fixture app on this Mac, 1080 by 900 at 2x, the app's own window captured with screencapture -l; Dark (the Mac's appearance) and Light (the in-app choice)
reviewer: two fresh subagents (Claude), one per pass, each with the frames under shuffled neutral names and the five job questions only
implementer: the session that built and captured it (Claude), not any reviewer
cold: true
states: [ledger-light, ledger-dark, nodraft-light, nodraft-dark, transcript-only-light, transcript-only-dark, settings-top-light, settings-top-dark, settings-bottom-light, settings-bottom-dark, firstrun-sheet-light, firstrun-sheet-dark, firstrun-light, firstrun-dark]
verdict: accept (ledger, Settings top and bottom, both appearances); revise kept as design (first-run sheet, first-run page, retired-draft meeting, transcript-only meeting), reasons below
---

# Yawn for Mac, the Rail wave — installed-build cold review, 2026-10-16

**Question.** Built from `DESIGN.md` (Rail): on the installed build, does the
page after a meeting read as the operator's notes with the rail and the nearest
turns beside them; do the other meeting states, Settings and first run answer
the five job questions; and do both appearances hold. This replaces the
WKWebView harness review (`mac-rail-2026-10-16-cold.md`) as the evidence of
record for the states captured here.

**Captures.** `mac-rail-installed-2026-10-16/` with its `MANIFEST.md`: fourteen
frames, seven states in both appearances, from the installed Fixture app. Every
frame was checked for a painted window and for the macOS screen-capture
indicator before it was kept.

**What getting here found in the product (fixed on this branch, before pass 1).**

- A trash entry past its 30-day window deadlocked the whole app: scheduled
  retention held the storage sequence lock while the purge re-took it, and every
  library command then waited behind it. Clicking a meeting, searching and
  opening Settings all did nothing. Any install with an aged-out trash entry hung
  the same way (since fe32e38). Fixed in c1e811c with a regression test that runs
  the pass on a deadline.
- The ledger laid itself out only on render and window resize, so collapsing the
  sidebar left the rail marks beside the wrong lines. Fixed in c1e811c.
- Opening Settings was pulled back out on the next poll tick by the
  finished-capture auto-select. Fixed in 4588e87.
- The operator moved Settings into the main window (sidebar gear, menu item,
  ⌘,) and added System/Light/Dark (b6faf52 onward); those frames are part of this
  review.

**Passes.**

| Pass | Frames (build) | Verdict | What it found |
|---|---|---|---|
| 1 | all fourteen (807a87e's tree) | ledger, Settings bottom accept; the rest revise | The Theme detail ("System follows the Mac's...") contradicted a Light choice; sidebar titles cut mid-letter with no ellipsis; the dark first-run sheet had a bright outline the light one lacks; the first-run sheet flush under the toolbar over an empty window; the bare rail on first run and three prompts to record; the retired-AI-draft sentence in placeholder grey; dim secondary text in dark; a blue pill at top left (the macOS capture indicator, in six frames: a capture artifact, not app UI) |
| 2 | all fourteen, recaptured (9f44753) | ledger, Settings top and bottom accept in both appearances; first-run sheet, first-run page, retired-draft and transcript-only meetings revise | The Theme detail now reads correctly; no clipped titles or bright outline raised. Remaining: the first-run sheet's placement, the bare rail and the three prompts, the retired-draft sentence's grey, dim dark secondary text, the native checkbox looking different by appearance, the toolbar repeating a long title, the pause callout's left offset |

**Changed after pass 1 (9f44753).**

- Sidebar titles end in an ellipsis (the text span carries `text-overflow`;
  the flex container could not).
- In dark, sheets use the hairline border instead of white at 65%.
- The Theme detail says what the selected choice does.
- Frames carrying the capture indicator were recaptured; the capture helper now
  rejects them.

**Kept, and why.**

- **First-run sheet flush under the toolbar, over an opaque backdrop.** Both
  passes read it as clipped or half-rendered. It is the attached-sheet treatment
  R12 and R15 chose after earlier installed cold reviews (`styles.css`,
  `.first-run-backdrop`): a centered web modal read as the wrong category, and a
  legible gate behind it competed with the sheet. Two fresh readers now find the
  result broken-looking, so this goes to the operator rather than being reverted
  here.
- **The bare rail on first run.** Operator decision, 2026-10-16 (PR #141,
  DESIGN.md First run): the rail is the page's margin. Answered by that
  paragraph, not a revision.
- **Three prompts to record on first run** (sidebar copy, the guided "Try it",
  the page). The guided invitation is roadmap packet W10's; trimming it is a
  product call.
- **The retired-AI-draft sentence in placeholder grey.** It states a fact that
  needs no action ("nothing is needed from you"), so it is quiet by intent; both
  passes found it hard to tell from the placeholder, and pass 2 found "a saved AI
  draft you cannot see" confusing. For the operator: the copy predates the Rail
  rewrite of the note-area details.
- **Dim secondary text in dark.** Token values (`--label-2`, `--label-3`);
  the increased-contrast rule that strengthens them was not captured (operator
  chose to record the gap).
- **The toolbar repeating the meeting title.** The Rail toolbar names the open
  meeting; the long fixture titles exaggerate it.
- **The checkbox's look by appearance, the pause callout's offset.** The
  checkbox is the native control; the callout sits in the rail gutter by design.

**Not reviewed here.** The live recording and the interrupted recording (the
fixture seeds no interrupted meeting; both stay on the harness record), increased
contrast (not captured, operator's choice), and large text (fixed pixel sizes;
the setting has no effect, a gap against DESIGN.md's proof list). The
export-tampered meeting was moved out of the fixture root because startup
recovery blocks the whole app while it is present.

**Removed, combined, demoted, hidden.** Removed: the separate Settings window
and its `open_settings_window` command. Combined: Settings into the main
window's document pane. Nothing demoted or hidden.

**For the operator.**

1. The first-run sheet's attached placement reads as broken to two fresh
   reviewers; keep R12/R15's treatment or return to a centered sheet.
2. The retired-AI-draft sentence: keep, reword, or give it a stronger tone.
3. Large text: the Mac UI uses fixed pixel sizes and does not follow the
   system text size; DESIGN.md asks for it to be proven.
