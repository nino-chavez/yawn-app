# Installed-app captures — 41c026b preview (R14 document-pane port, D-OPEN fix), 2026-09-02 evening

Device: this Mac, packaged `Yawn Preview.app` built from 41c026b, runtime staged
with `build-alpha-external` (model catalog present), real storage with the
same four meetings, window at its 1080x900 default, captured by live window
bounds, read back from disk before inclusion. Light appearance via the
preview bundle's own defaults domain, removed after. First run staged by
flag and folder, restored after (four meetings intact).

Since the 2765401 set: the document pane was ported to concept A (three
text sizes, one button shape, one hairline), R8-R13 landed, Settings renders
the model catalog, and the meetings that opened as "unavailable" now open,
because the reader admits a recovered-interrupted meeting whose audio was
released and the list refreshes its handles before each open (roadmap
D-READ, D-OPEN).

| File | State | Notes |
|---|---|---|
| 01-dark-launch | What the app opens to: library with the most recent meeting selected, a transcript-only meeting with no note | |
| 02-dark-interrupted | An interrupted recording whose audio was released, opened from the list | Needs-attention pane with one action |
| 03-dark-back-to-first | The first meeting opened again from the interrupted one | Proves row-to-row navigation after the D-OPEN fix |
| 04-dark-settings | Settings window, Preferences-style, speech model catalog with the active model | |
| 05-light-launch | Same as 01 in light | |
| 06-light-interrupted | Same as 02 in light | |
| 07-light-settings | Same as 04 in light | |
| 08-dark-first-run | First run: orientation sheet flush under the toolbar over the empty library | |

Not captured: During and Paused (need the operator's attestations); a
meeting with a generated note and its inspector (no real meeting on this Mac
has a generated note); withheld turn (no data); large text and increased
contrast (operator-run). The Record control is disabled in every frame
because system-audio access has not been granted to the preview bundle.
