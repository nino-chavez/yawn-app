---
surface: mac-ledger-fold (the page after a meeting: the ledger's fold rows closed and opened, and the transport's recording choice)
kind: cold
build: the commit that adds this record (frames in mac-ledger-fold-2026-10-16/harness are from its tree); passes 1 to 4 judged the trees noted per pass
device: WKWebView harness, 1080 by 900 at 2x, stub data, both appearances (not the installed build)
reviewer: four fresh subagents (Claude), one per pass, each with the frames and the job questions only
implementer: the session that built it (Claude), not any reviewer
cold: true
states: [ledger-light, ledger-dark, ledger-fold-light, ledger-fold-dark]
verdict: accept (folds closed); revise (folds opened; the recording choice)
---

# Yawn for Mac, ledger fold expansion and the recording choice — cold review, 2026-10-16

**Question.** The Rail wave's cold review (`mac-rail-2026-10-16-cold.md`)
said "+ N turns" did not read as openable and the Microphone / System audio
chips were unexplained. Built from `DESIGN.md` (Components, Fold row:
collapsed, expanded; Accessibility, "a fold is a disclosure named '7 more
turns'"): does the fold read as something to open, do its turns read as its
contents when open, and can a person tell what the recording choice does?

**Captures.** `mac-ledger-fold-2026-10-16/` with its `MANIFEST.md`: four
frames, the ledger with folds closed and with its first two folds opened, in
both appearances. Harness frames from stub data, not the installed build;
large text and increased contrast were not captured.

**Passes.** Each pass was a fresh reviewer, neutral frame names, the four
job questions, no rationale and no earlier pass.

| Pass | Tree | Closed | Opened | What it found |
|---|---|---|---|---|
| 1 | 8fa5d68 | revise | revise | Fold rows read as captions, not controls; opened turns' left rule too faint and the gap after a group too tight; "Listen to [Microphone] [System audio]" unexplained, the unselected chip not visibly a button |
| 2 | after pass-1 fixes | revise | revise | Closed and open used different symbols ("+ … ⌄" vs "− … ⌃"); the closed row still plain text while the open one was a pill; the pill hung left of the column; group end still tight; selected chip faint; chips do not say what each recording holds |
| 3 | after pass-2 fixes | revise | revise | Fold pills now read as buttons; the closed pill crowded the line below it, so it read as that line's heading; the recording choice still did not say what each track contains |
| 4 | after pass-3 fixes | **accept** (pills read as openable; low pill contrast in dark) | revise | Open: the label stays "+ N turns", only the chevron turns; the left rule faint in light; the frame crops the lengthened column. Recording choice: now "suggests which recorded track plays", the selected state weak in dark |

**Changed, by pass.**

- After pass 1: fold label Label 2, open fold filled; the opened turns' rule
  2px; 14px after an opened group; the chips outlined so both read as
  buttons; the group label "Recording to play".
- After pass 2: one label, "+ N turns", in both states, with the chevron
  turning (pass 2 faulted the changing sign); the fold a bordered pill in
  both states, flush with the column; 20px after an opened group; the
  selected chip bordered and bold.
- After pass 3: the pill 8px from the line below, as far as from the line
  above; the chips "Microphone · you" and "System audio · the call".
- After pass 4 (not re-reviewed): the opened turns' rule in Label 3.

**Not changed, and why.**

- Pass 4's "the label never changes" is the opposite of pass 2's
  objection to a changing sign. The chevron carries the state and VoiceOver
  hears `aria-expanded`; the operator can choose a different open label.
- The frame cropping the opened column is the window: the page scrolls.
- Passes 1 and 4 read the rail running past the last note as empty or
  stray. That is decided design: Nino kept the rail as the page's margin
  (2026-10-16, PR #141, DESIGN.md First run), so it is not a defect here.
- Recurring items outside this change, as in the Rail review: the turn
  column not aligned row for row with the notes (DESIGN.md: rows never
  reflow the note, so a pushed turn moves away from its line), "Nearest in
  the transcript" unexplained, small grey type in dark, "After the meeting"
  wrapping in the gutter.

**What was removed, combined, demoted or hidden.** Nothing removed. The fold
row became a button in place of a plain label; the folded turns are hidden
until asked for, as before, and now reachable in place instead of only in
the full transcript. The chips gained a visible label and a word each; no
banner was added.

**Mechanical checks.** `npm run test:ui` 175 pass. `run.sh fidelity` passes
in light and dark with 34 checks, 8 of them new: it presses a fold and checks
it is a closed then open disclosure (`aria-expanded`, `aria-controls`), keeps
focus, shows its turns under it and above the next turn row, that the turn
column has no overlap, that the note and its line positions do not move, and
that Escape closes it and returns focus. With the push removed it failed
(`next-turn-pushed-below` −109, `turn-column-no-overlap` 1), so it can see
the defect.

**Owed.** The installed-build pass with large text and increased contrast,
as for the Rail wave; the operator's read of the new strings (DIRECTION.md,
content reads, 2026-10-16).
