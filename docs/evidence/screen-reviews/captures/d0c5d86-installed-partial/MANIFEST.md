# Installed-app capture — build 21 (d0c5d86), 2026-09-02 night — PARTIAL

**This set is one frame and the pass did not finish.** The run was stopped by a
tooling permission boundary, not by a defect and not by anything on screen; see
"Why it stopped" below. Filed anyway because the single frame settles three
roadmap rows on a device, and because a partial set that says so is worth more
than a set nobody took.

Device: this Mac, packaged `Yawn Preview.app` built from d0c5d86 (build 21,
binary 21:03, 199 Mach-Os signed with the Developer ID,
`codesign --verify --deep --strict` exit 0), real storage with the same four
meetings, window at its 1080x900 default at screen origin 360,80, captured by
live window bounds and read back from disk before inclusion. Dark only. The
appearance default was never written, so the app followed the system
throughout; nothing in storage was modified and all four meetings are intact.

| File | State | Settles |
|---|---|---|
| 01-dark-summary-failed | The Sep 1 meeting whose generation failed: paused-capture fact, released-audio fact, the recovery pair, Generate note, the operator's own notes area, Full transcript | R23, R24, R24b |

What the frame shows, against what each row predicted:

- **R23.** The document renders the recovery inline — "Your meeting note needs
  another try." with "Yawn could not create a note. Your transcript is
  unchanged and you can try again." — and keeps the Generate note control.
  Before R23 this state rendered a bare enum label and nothing else, which is
  what the 630d08a review saw.
- **R24.** The caption reads "Sep 1, 2026 · **Note not created**", not the
  title-cased storage enum "Summary Failed" a cold reader took as a constraint.
- **R24b.** Neither sentence promises a note that does not exist. The audio
  fact reads "The transcript remains available, but this meeting cannot be
  retranscribed" and the recovery detail says the transcript is unchanged;
  both are the no-note branch, which is correct for this meeting.

Also observed, not photographed:

- **D-OPENFREEZE.** Accessibility read the window continuously for 5.0 s after
  a sidebar click — eight polls, all answered. Before the fix that open
  re-hashed 8.06 GB on the main thread and the window was unreadable for about
  5 s, which earlier passes recorded as "the row click resolves late". This is
  the first packaged build where it does not.
- The process answered to **both** "Yawn Preview" and
  "local-meeting-notes-desktop" in the same run, which is the rename the
  capture primitives were written to survive.

**Not captured, and why.** Everything after the first frame needed a click into
the web view. The window exposes only an `AXGroup` and the three traffic-light
buttons to accessibility, so there are no elements to click by name, and the
app's menus reach only recording, sidebar, transcript and Settings — not
meeting selection, Generate note, or either sheet. `System Events`'
`click at {x, y}` was tried first and silently does nothing at an arbitrary
point. A CGEvent-based click was built as the replacement and its execution was
refused by the session's permission boundary, twice. The run was stopped there
rather than routed around.

So these remain unproven on a device and stay on the pass plan: the generating
state, a regenerated note, the light note, the light needs-attention surface
(**R35**, whose contrast figures are still computed rather than observed), the
retry decision sheet and the delete confirmation sheet (**R27**, **R32**,
**R34**), and D-FREEZE's responsiveness during an actual generation.

Two states are not reachable on this machine at all and should be dropped from
the plan rather than kept as gaps: **transcript-only**, because no meeting is
in that state any more (the nine-turn meeting has carried a generated note
since build 20), and **model setup**, which requires no speech model installed.
