# Yawn for iPad, first run (build plan slice 1) — cold review, 2026-10-15

**Kind:** cold screen review, two passes, each by a fresh subagent. Each
reviewer saw only three captures and six neutral questions. Neither had source
code, design notes, rationale, or the other pass. Each was told only that the
app is for recording an in-person meeting while writing notes by hand, that
nothing had been recorded, and that recording is not in this early build. Pass
2 was also told that the curve in the bottom-right corner is iPadOS's window
resize handle.

**Captures:** iPad Air 11-inch (M4) simulator, iOS 26.5, portrait. Not device
captures: the operator's iPad was locked when the build was installed. The
device capture is owed before slice 2 merges.

**Question:** Opening the app for the first time, can a person tell what it is
for, how recording will start and that it cannot yet, and where meetings go?

| Pass | Frames | Verdict | Main findings |
|---|---|---|---|
| 1 | first run (light), sidebar open (light, dark) | revise | (a) Disabled Record faded to a pale grey title, nearly invisible in dark mode, and its "not in this build" note was the faintest text, far below it. (b) The headline wrapped to a lone "it records." (c) The page read as a blank void with no writing cue. (d) A stray curve in the bottom-right corner |
| 2 | the same three states after the fixes | pass | Recording's unavailability is clear. Notes: the page sentence still says "Record a meeting" as if Record works; the left-edge line reads as an unexplained divider; the sidebar card has weak edge definition on white and on black; the empty-state description is small and low contrast |

Frames: `ipad-first-run-2026-10-15/pass1-*.png` and `pass2-*.png`.

**Pass 1, changed:**
- (a) Until recording exists, the bar shows a plain, non-tappable Record label
  in a grey capsule with "Not available in this build yet" directly under it.
  The live red Record control waits for slice 2.
- (b) The headline sits on one line where it fits, otherwise two even lines.
- (c) The page shows the time rail down its left margin, Concept C's own
  writing guide, empty until slice 3 adds a mark per line.
- (d) Not changed: the curve is iPadOS's window resize handle.

**Pass 2, not changed, and why:**
- "Record a meeting" implying Record works ends in slice 2, when it does.
- The unexplained left-edge line is the empty rail. Slice 3 gives it marks; its
  cold review checks whether it still reads as a stray divider.
- Sidebar edge definition and the empty-state text are iPadOS's own sidebar
  and `ContentUnavailableView` styling, kept native per the experience brief.

**Removed, combined, demoted, or hidden:** the disabled Record button is
replaced by a non-interactive label until slice 2; nothing else, since the
screen is new.
