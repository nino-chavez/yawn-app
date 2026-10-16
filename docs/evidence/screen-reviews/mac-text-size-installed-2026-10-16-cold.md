---
surface: mac-text-size (the page after a meeting and Settings, at Default and Largest text size)
kind: cold
build: f0bd6f2 (pass 2 judged frames from this build; pass 1 judged frames from 5004bd1)
device: installed Yawn Fixture app on this Mac, 1080 by 900 at 2x, the app's own window captured with screencapture -l; Dark (theme System)
reviewer: two fresh subagents (Claude), one per pass, each with the frames under shuffled neutral names, the capture settings and the five job questions only
implementer: the session that built and captured it (Claude), not any reviewer
cold: true
states: [default-ledger, largest-ledger, default-settings, largest-settings]
verdict: accept (Default ledger, both Settings frames); Largest ledger revise, kept as design with reasons below
---

# Yawn for Mac, text size — installed-build cold review, 2026-10-16

**Question.** Does the Mac UI hold at its largest text size: the page after a
meeting and Settings, judged against the same frames at Default, on the
installed build at the default window size.

**Captures.** `mac-text-size-installed-2026-10-16/` with its `MANIFEST.md`.

**Why an in-app setting.** Apple's Text Size setting applies to "the apps
listed", and Apple's own apps change text in the app with ⌘+ and ⌘− (Apple
Support, "Increase font size and icons on Mac", macOS 27). On this Mac the list
(`com.apple.universalaccess` `FontSizeCategory`) names thirteen `com.apple.*`
bundles and a global value; no API was found for another app to join it. Yawn
does the same: Settings → Appearance → Text size (Default, Large, Larger,
Largest) and View → Bigger, Smaller, Actual Size (⌘+, ⌘−, ⌘0) set one scale
that multiplies every type size by 1, 1.15, 1.3 or 1.5.

**What getting here found (fixed before pass 1, `5004bd1`).** The first Largest
capture broke elapsed times across lines ("12:0 / 3", "Elapse / d") and clipped
the search placeholder: the rail gutter, the time column and the search field
were fixed px. They now derive from the scale.

**Passes.**

| Pass | Frames (build) | Verdict | What it found |
|---|---|---|---|
| 1 | all four (`5004bd1`) | both Settings and the Default ledger accept; Largest ledger revise | At Largest the player kept the recording choice on one line and squeezed its track to about 80px; sidebar captions cut the meeting's status to "transcript ..."; notes wrap to three lines with no separation; the transcript column drifts from the notes; the toolbar title truncates |
| 2 | all four, recaptured (`f0bd6f2`) | both Settings and the Default ledger accept; Largest ledger revise | The player and sidebar findings are gone. Remaining: a time dot centers on the middle line of a three-line note; notes have no separation; the toolbar title truncates. Judged usable: "nothing overlaps, nothing is unreadable, and controls stay on screen" |

**Changed after pass 1 (`f0bd6f2`).**

- The transport wraps: at a large size the recording choice moves to its own
  line and the track keeps at least 160px. One line at Default, unchanged.
- A sidebar row caption takes up to two lines, so the status stays whole.

**Kept, and why.**

- **The dot centers on its note.** The ledger places each mark at the middle of
  the note line it belongs to (`layoutLedgers`, `top + height / 2`). That holds
  at every size: at Default a two-line note puts the dot between its lines, which
  pass 2 accepted; at Largest a three-line note puts it on the middle line. The
  rule is unchanged by this work; whether a mark should sit on a wrapped note's
  first line instead is a ledger design question at every size, for the operator.
- **No separation between notes.** The notes are the lines of one editor; the
  rail's dots are what separate them. Unchanged at every size.
- **The transcript column drifts from the notes.** Turns are pushed down in
  order so they never overlap (`layoutLedgers`); the column is labeled "Nearest
  in the transcript". Unchanged at every size.
- **Three-line notes, less on screen, the toolbar title truncates.** At 1.5x
  in a 1080 by 900 window the page width does not grow, the rail gutter does
  (so times stay whole), and the turn column stays 324px, so the note column
  narrows while its text grows. Rebalancing the columns at large sizes is a
  product call, below. The page scrolls; the full title is the page heading.

**Not reviewed here.** Light appearance and increased contrast at Largest; the
Large and Larger steps (between the two judged ends); the live recording and the
first-run states at Largest.

**Removed, combined, demoted, hidden.** Nothing. One row (Text size) added to
the existing Appearance group in Settings.

**For the operator.**

1. Should a rail mark sit on the first line of a wrapped note instead of its
   middle? It applies at every text size.
2. At the larger sizes, should the 324px turn column narrow so the notes keep
   more of the page?

**Checked outside the captures.** Clicking Text size live (Largest, Larger,
Default) re-scales the page at once, moves the pressed state, changes the detail
line and stores the choice; an opened meeting renders at the chosen size. This
was checked in the browser harness (`ui/review/harness.html`),
not on the installed build, because Computer Use's approval went unanswered.
The View menu items were added after the captures and are covered by tests, not
by a frame. The Default detail line also changed after the captures, from "The Mac's
standard size." (in `default-settings.png`) to "Yawn's standard size.", so it no
longer suggests Yawn follows the Mac's setting.
