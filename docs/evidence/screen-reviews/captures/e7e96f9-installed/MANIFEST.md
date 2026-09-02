# Installed-app captures — e7e96f9 preview, 2026-09-01 evening

Device: this Mac, packaged Yawn Preview.app (ad-hoc signed), real window at
its default 960x900 frame; captures are 1085x910pt regions at 2x
(screencapture -R), window position default-centered. Operator authorized the
session; storage staging below was reversible and restored after each state.

| File | State | How staged |
|---|---|---|
| 01-home-first-run-sheet | First-run three-moments sheet over blurred Home | first-run-seen.flag removed; meetings dir set aside (restored after) |
| 02-home-first-run-empty-state | Teaching empty state + guided invitation | Got it clicked on the state above |
| 03-home-with-meetings | Library with four real meetings | true storage, no staging |
| 04-after-meeting-view | Meeting read surface with the live pause receipt ("paused 1 time, for 0:03") | opened the morning's real pause-take |
| 07-needs-attention-retention | Retention-attention banner on the terminal capture view | occurred naturally (see defect note) |
| 07b-needs-attention-blocking | Full blocking screen, "Yawn cannot record yet" | occurred naturally (see defect note) |

Not captured, with reasons:
- Settings (state observed loaded and correct — privacy table + model card —
  but the shell screen-recording grant was one-time and expired mid-run).
- During capture / Paused (same permission expiry; both states were observed
  live earlier today in this build).
- Withheld transcript turn (no withheld turns exist in this Mac's real data;
  never staged synthetically per the brief).
- Large text / increased contrast (system-settings changes are operator-run).

## Two live defects found during this pass (not staged, real)

1. HARD LOCK, quit-during-finalize: a recording stopped normally but the app
   was quit seconds later, mid-worker-finalize. The resulting meeting
   (lifecycle "captured", session.json never finalized) is refused by the
   transcription queue ("not eligible", 8 diagnostics), quarantined by the
   retention pass ("quarantined without mutation"), and then BLOCKS ALL
   RECORDING: the blocker screen's Check again loops, and Back to Meetings
   routes INTO the blocker — the library is unreachable, so the stuck
   meeting cannot even be deleted in-app. Zero in-app recovery. Unblocked
   out-of-band by moving the meeting dir to quarantine-evidence/ (preserved
   beside the storage root). Crash-during-recording has recovery;
   quit-during-finalize has none.
2. Cryptic toast: "The installation check is not waiting for a retry."
   surfaced to the operator during the blocked state — internal-state
   vocabulary, no operator meaning, no action.
