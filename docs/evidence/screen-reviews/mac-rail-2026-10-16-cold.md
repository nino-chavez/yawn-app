---
surface: mac-rail (the page after a meeting, during a recording, an interrupted recording, a meeting with no AI draft, first run, Settings)
kind: cold
build: the commit that adds this record (frames in mac-rail-2026-10-16/harness are from its tree); passes 1 to 5 judged the trees noted per pass
device: WKWebView harness, 1080 by 900 at 2x, stub data, both appearances (not the installed build)
reviewer: five fresh subagents (Claude), one per pass, each with the frames and the job questions only
implementer: the session that built it (Claude), not any reviewer
cold: true
states: [ledger-light, ledger-dark, capture-light, capture-dark, interrupted-light, interrupted-dark, summary-failed-light, summary-failed-dark, first-run-light, first-run-dark, settings-light, settings-dark]
verdict: revise (ledger, interrupted, no AI draft, first run); accept (during, Settings)
---

# Yawn for Mac, the Rail wave (slices 1 to 3) — cold review, 2026-10-16

**Question.** Built from `DESIGN.md` (Rail, selected 2026-10-15): does the
page after a meeting read as the operator's notes with the rail and the
nearest turns beside them; does a recording in progress say so and how to
stop; does a condition that needs attention read as bounded, with its one
next action; is first run a page; is Settings one form?

**Captures.** `mac-rail-2026-10-16/harness/` with its `MANIFEST.md`: twelve
frames, six states in both appearances, rendered by the harness's WKWebView
runner from `apps/desktop/ui` against stub data (`run.sh still <mode>`).
The renderer is the installed app's; the data and the window chrome are the
stub's. These are not captures of the installed build. The Fixture app was
built from this tree and its root seeded with the 23-minute ledger meeting,
but driving it needed the operator at the Mac (the Computer Use approval
dialog went unanswered and the app would not come forward), so the
installed pass, with large text and increased contrast, is still owed.

**Passes.** Each pass was a fresh reviewer with neutral frame names (m1 to
m12, mapping in the manifest) and no earlier pass.

| Pass | Frames (tree) | Verdict | What it found |
|---|---|---|---|
| 1 | all twelve (ba30dff) | all six pairs revise | Ledger: the time column and the quote column unexplained, "+ N turns" not openable, the tick row and the Microphone/System audio chips unexplained, two Full transcript affordances and a stray "Transcript" in the meta line, "Saved" detached, the rail running past the last note, a long line crowding the quote column, quotes drifting from their notes. During: Stop styled like Pause, two recording indicators, a caption that repeats the placeholder. Interrupted: a destructive sole action, the fact said four times. No AI draft: said three times, fused with the placeholder. First run: an orphan rail, the sidebar copy repeating the page, "Try it" heavier than Record, tight title tracking. Settings: "Check again" reads page-level, state words below their titles, the frame crops the scrolling window |
| 2 | all twelve (75e167f's tree, before its last fixes) | during and Settings accept; ledger, interrupted, no AI draft, first run revise | The new "Nearest in the transcript" heading overprinted by the first turn's fold (the implementer's defect); the rest as pass 1 where unchanged |
| 3 | ledger, interrupted, no AI draft | all three revise | Judged a frame with the implementer's defect: the editor and its mirror wrapped at different widths, so "After the meeting" overprinted the last note and its mark was missing; the column headings offset from their columns; the stop mark a position, not a sentence; "no AI draft" twice explicit |
| 4 | ledger | revise | Headings still offset (a grid gap), "Typed at" reads as clock time; no overprint; every note line has its time; long lines clear of the quotes |
| 5 | ledger | revise | No rendering defect. Headings sit over their columns. Remaining: the quotes' grey, the headings' wording, the quote times differing from the note times, the rail past the last note, the tick row, the folds' affordance, the window title repeating the page title, a 34pt note-to-quote gap |

**Changed, by pass.**

- After pass 1: one Full transcript affordance (the disclosure); no state
  word in the meta line for an ordinary meeting; "Saved" beside "Your notes";
  Stop the one primary control while recording; the live page's meta names
  the state or the start time, never the placeholder's twin; a neutral
  note-area state is one caption with room above it; Settings' state words
  on their row titles; the first-run title's tracking; the stub's fixture
  rows at a fixed time of day; a heading over the turn column.
- After pass 2: no fold above the first turn (turns before the first note
  are not between notes; the full transcript holds them); "Elapsed" over the
  rail's times; a 24px gutter between the note and the turns; the stop mark
  without its sentence (the sentence is its title); "no AI draft" said in
  the meta line and once below.
- After pass 3: the head row starts where the ledger starts and shares its
  columns; the mirror sized like the editor so both wrap at one width;
  "After the meeting" in the gutter where the line's time would be.
- After pass 4: the head row's grid gap removed; "Typed at" became
  "Elapsed".
- After pass 5: nothing; what remains is design, below.

**Not changed, and why.**

- **The empty rail on first run.** Pass 1 and pass 2 (and every iPad pass)
  read the bare line as a stray divider. It is the Rail's page structure
  (`DESIGN.md`, Composition), selected by the operator. The operator should
  decide: keep the bare rail as the page's margin, or draw it only once a
  mark exists. **Decided 2026-10-16:** the operator kept the bare rail as
  the page's margin; recorded in `DESIGN.md` under First run.
- **The interrupted state's one action.** Reviewers asked for a recovery
  action first. None exists: the product has no transcription of a
  recovered recording's kept audio (`transcriptRetryPresentation` refuses a
  meeting in recovery, and the post-commit review of aa13b0b removed a Retry
  button that could never render). Move to Trash stays secondary; a primary
  waits on a product decision.
- **"+ N turns" not openable.** The fold's expansion is not built; the count
  is shown (product brief, "Note moments on the Mac") and the full
  transcript sits below. Building the expansion is a follow-up, not a
  styling change.
- **The transport's ticks and the Microphone/System audio chips.** The
  tick-per-turn scrubber is a graft the selection record names; the chips
  choose which retained leg plays. Kept; a label for the chips is a
  follow-up.
- **The rail running past the last note.** It is the page's margin line,
  on both platforms.
- **Quotes drifting from their notes.** Rows never reflow the note
  (`DESIGN.md`, Components); a turn taller than its note's line pushes the
  next turn down. Kept.
- **The quotes' grey, the small grey text in dark.** Label 2 and Label 3
  carry the measured values (`design-contrast-2026-10-15.md`, 4.5:1 or
  better). Kept; a stronger step is a token decision.
- **Two recording indicators.** The sidebar row is the meeting in the list;
  the toolbar badge is the control. Kept.
- **The sidebar's first-run copy and the "Try it" card.** W10's strings,
  approved by the operator as-is. Kept.
- **The window title during a recording ("New Recording") beside the page's
  "Meeting".** The window title is a tested presentation; kept.
- **The heading words "Elapsed" and "Nearest in the transcript".** Listed in
  `DIRECTION.md`'s content-reads table for the operator to change.

**Removed, combined, demoted, or hidden.** Removed: the transport's Full
transcript link (combined into the disclosure below the ledger); the meta
line's state word for an ordinary meeting; the fold above the first turn;
the stop mark's sentence; the neutral note state's headline line (combined
into one caption); the "Needs attention" eyebrow (slice 3). Demoted: "Saved"
from the far right to beside its heading; the live page's caption to the
state word. Hidden: nothing.

**Still owed.** The installed-build pass (both appearances, large text,
increased contrast) with the operator at the Mac; the operator's decision
on the interrupted state's action (the empty rail was decided 2026-10-16:
kept); the folds' expansion;
a label for the playback-source chips.
