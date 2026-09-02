# Installed-app captures — bfa0a80 preview, 2026-09-02 morning

Device: this Mac, packaged `Yawn Preview.app` built 05:20 from bfa0a80, real
window at its default 1080x900 frame, captured by window bounds read live
from System Events (not a hard-coded region), 2x, downscaled to 1400px wide
for review. Every file was read back from disk and matched against the
intended state before it was added here (the read-back rule from the
e7e96f9-installed manifest). Light appearance was produced by
`NSRequiresAquaSystemAppearance` on the preview bundle's own defaults domain,
then removed; no system setting was changed. Storage staging (first-run flag,
meetings folder set aside) was reversed after capture; the four real meetings
are intact.

| File | State | How staged |
|---|---|---|
| 01-dark-first-run-sheet | Three-moments sheet over blurred Home | first-run flag removed, meetings set aside |
| 02-dark-first-run-empty | Teaching empty state after Got it | same, sheet dismissed |
| 03-dark-launch-view | What the app opens to with meetings present: the terminal "Your meeting is ready to read" view, not the library | cold launch, no staging |
| 04-dark-library | Home with four real meetings | Back to Meetings from 03 |
| 05-dark-meeting-a | A recovered-interrupted meeting (D-LOCK surface after the fix) | opened from row 2 |
| 06-dark-meeting-b | Transcript-ready meeting with the pause receipt | opened from row 1 |
| 07-dark-settings | Settings window, speech model row reading "Checking" | gear from 03; window had been open about a minute |
| 08-light-launch-view | Same state as 03 in light | cold launch in light |
| 09-light-library | Same state as 04 in light | Back to Meetings |
| 10-light-meeting-b | Same state as 06 in light | row 1 |
| 11-light-settings | Settings in light; retaken at a 10 s wait after a first attempt came back blank at 4 s. NOT in the blind review set (the reviewer had already started on 01-10). Speech model row reads "Checking" here too, as in 07 after about a minute: probable defect, not a slow check | gear, 10 s wait |

Not captured, and why:
- During and Paused: need a live recording, which needs the consent,
  headphone, and single-operator attestations. Those are the operator's to
  click. Owed when the operator is at the machine.
- Withheld turn: no data exists for it.
- Needs-attention: the storage no longer holds a blocking state after the
  D-LOCK fix; the honest recovered-interrupted state is 05.
- Settings in light at a 4 s wait came back blank white; at 10 s it rendered
  (11). The first attempt was a paint race, not a defect.
- Large text and increased contrast: operator-run per DIRECTION.md.

Observed during the pass, outside any frame: a Back to Meetings click on the
launch view took longer than 2 s to change the view (a 2 s capture showed the
old view three times; a later frame showed it had navigated).
