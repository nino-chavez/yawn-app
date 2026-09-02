# Installed-app captures — 88da6b6 preview (R15-R19 refit), 2026-09-02 evening

Device: this Mac, packaged `Yawn Preview.app` built from 88da6b6, runtime staged
with `build-alpha-external` (model catalog present), real storage with the
same four meetings, window at its 1080x900 default, captured by live window
bounds, read back from disk before inclusion. Light appearance via the
preview bundle's own defaults domain, removed after. First run staged by
flag and folder, restored after (four meetings intact).

Since the 41c026b set: the first-run sheet is the only ask on screen and
names the permission step (R15); the needs-attention block keeps its title
and sits centered as one 520px column (R16, R17); Settings is a grouped
list with three type sizes, one primary per group, and a scrollable window
(R18); the notes box has a bounded height and the recovered-interrupted
banner has one dismiss (R19). Settings buttons render at the specimen size
and weight.

| File | State | Notes |
|---|---|---|
| 01-dark-launch | What the app opens to: library with the most recent meeting selected, a transcript-only meeting with no note | |
| 02-dark-interrupted | An interrupted recording whose audio was released, opened from the list | Needs-attention block, one action |
| 03-dark-back-to-first | The first meeting opened again from the interrupted one | Row-to-row navigation |
| 04-dark-settings | Settings window, grouped list, speech model catalog with the active model | Window is scrollable; the Note model group continues below the fold |
| 05-light-launch | Same as 01 in light | |
| 06-light-interrupted | Same as 02 in light | |
| 07-light-settings | Same as 04 in light | |
| 08-dark-first-run | First run: orientation sheet over the empty library, one Continue | |

Not captured: During and Paused (need the operator's attestations); a
meeting with a generated note and its inspector (no real meeting on this Mac
has a generated note); withheld turn (no data); large text and increased
contrast (operator-run). The Record control is disabled in every frame
because system-audio access has not been granted to the preview bundle.
