# Installed-app captures — 2765401 preview (concept A rebuild), 2026-09-02 afternoon

Device: this Mac, packaged `Yawn Preview.app` built from 2765401 (the concept-A
implementation wave: UI rebuild, native shell, token port, plus two
attention-pane fixes), real storage with the same four meetings as the
morning set, window at its new 1080x900 default, captured by live window
bounds, read back from disk before inclusion. Light appearance via the
preview bundle's own defaults domain, removed after. First run staged by
flag and folder, restored after (four meetings intact).

| File | State | Notes |
|---|---|---|
| 01-dark-launch | What the app opens to: library with the most recent meeting selected | Fixes the morning's 03/08 finding (stale terminal view on launch). "No meeting note yet." renders above the 22pt ceiling |
| 02-dark-recovered | A recovered-interrupted meeting selected | Renders the needs-attention pane with the reader's failure: the library reader returns an unavailable note (no handles) for a meeting whose capture files are partials. Real defect D-READ, filed in the roadmap; the pane's copy ("could not read this meeting") is the UI's, not the reader's |
| 03-dark-settings | Settings window, Preferences-style | Speech model row now reads "Couldn't check speech model · Unavailable · Retrying" instead of hanging on "Checking"; underlying failure still open (manifest hashes and catalog id verified correct; cause not yet found) |
| 04-light-launch | Same as 01 in light | |
| 05-light-recovered | Same as 02 in light | |
| 06-light-settings | Same as 03 in light | |
| 07-dark-first-run | First run: one-paragraph orientation sheet over the empty library | The teaching copy behind the sheet sits in the sidebar; Record is dimmed because system audio is not yet allowed in the preview |

Not captured: During and Paused (need the operator's attestations); a
meeting with a generated note and its inspector (no real meeting on this Mac
has a generated note; the harness render of that state is in the rebuild
report, not here); withheld turn (no data); large text and increased
contrast (operator-run). The Record control is disabled in every frame
because system-audio access has not been granted to the preview bundle; the
path to granting it now lives in Settings only.
