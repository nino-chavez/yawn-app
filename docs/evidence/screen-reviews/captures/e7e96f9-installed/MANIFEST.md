# Installed-app captures — e7e96f9 preview, 2026-09-01 evening (corrected)

Device: this Mac, packaged Yawn Preview.app. CORRECTION: the first commit of
this manifest shipped three invalid captures; a blind cold reviewer caught it
from the frames, and the read-back audit confirmed. The lesson is now a rule
for this directory: a capture is not evidence until the FILE has been read
back and matched against the intended state — watching the screen at capture
time does not verify what landed on disk.

Valid captures:

| File | State | Notes |
|---|---|---|
| 02-home-first-run-empty-state | Teaching empty state + guided invitation (genuine) | Contaminated at the right edge by an overlapping unrelated window; content legible. CONFIRMED REAL FINDING: the copy says "Press Record" but on a true first run the only rendered button is "Allow system audio" — the instruction names a control that does not exist until audio setup completes. |
| 07-needs-attention-retention | Terminal capture view with the retention banner | The same blocking sentence rendered as an unstyled footnote beside an apparently-enabled "Record another meeting" — one condition, two contradictory severities (see 07b). |
| 07b-needs-attention-blocking | Full blocking screen, "Yawn cannot record yet" | Check again loops; no cause, no destination. |

Removed as invalid (why, exactly):
- 01 (intended: first-run sheet): full-screen capture landed on a different
  app's fullscreen space — the frame shows an unrelated terminal. The sheet
  itself WAS observed rendering correctly (viewfinder screenshot in the
  session transcript), but no valid file exists; recapture owed.
- 03 (intended: library) and 04 (intended: meeting view with the pause
  receipt): both region captures show the blocking screen instead — during
  the D-LOCK incident the router was REASSERTING the blocker on a cycle,
  and it flipped the view between the staging click and the shell capture.
  Both intended states were observed live (transcript); recapture owed.
  The reassertion cycle itself is evidence for D-LOCK's fix scope: the
  blocked app flaps between the terminal view and the blocker.

Still owed once screen-recording is re-granted (the grant was one-time):
first-run sheet, library, meeting view with pause receipt, Settings, During,
Paused. Withheld-turn: no data exists; never staged. Accessibility states:
operator-run.

## Two live defects found during this pass (unchanged, real)

1. HARD LOCK, quit-during-finalize: recording stopped normally, app quit
   mid-finalize; the unfinalized meeting is refused by the transcription
   queue, quarantined by retention, and blocks ALL recording. Check again
   loops; Back to Meetings routes INTO the blocker; the library is
   unreachable. Zero in-app recovery. Evidence preserved at
   quarantine-evidence/ beside the storage root. Additional observation:
   while blocked, the router flaps between the terminal capture view and
   the blocker, and the same condition renders at two contradictory
   severities (07 vs 07b).
2. Cryptic toast: "The installation check is not waiting for a retry."
