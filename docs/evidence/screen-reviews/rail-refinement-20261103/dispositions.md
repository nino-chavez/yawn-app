# Review dispositions

The independent cold pass accepted resting frame 01 and omitted inspected and
expanded frames 02 and 03. A single focused completion pass reviews those four
omitted light/dark frames without source or direction context.

The pass also suggested a stronger note-entry cue, fewer recording indicators,
and more meeting context in frame 04. The parent opened the actual recording
captures and the Current comparison frames. Both light and dark PNGs are
byte-identical by SHA-256 to Current in the preserved comparison artifact.
These are baseline product suggestions, not defects introduced by this change.
The existing large note placeholder is visible; this is not an absent editor.

Disposition: retain the selected recording canvas. Adding context or changing
its chrome would extend the approved note/transcript refinement. No date,
location, person or recording fact is invented. The recording suggestions are
recorded here for a separate product decision rather than silently applied.

The source explorer's token warning was rejected against
`apps/desktop/ui/tokens.css`: production defines both `--moment` and
`--moment-soft` in light and dark. The implementation and browser checks use
those existing tokens. The live-mark guard was retained and exercised with an
active inspection cue in both browser engines.

Native offscreen programmatic focus does not establish `:focus-visible`, so
physical keyboard modality is tested separately through headless Chromium and
WebKit. macOS WebKit's Option-Tab path reaches the controls without modifying
system preferences. Native editor checks wait for the asynchronous meeting-open
projection to settle before typing, then verify insertion and undo across a
polling interval. No assertion was weakened to accept missing text or lost focus.

The focused pass completed frames 02/03. It requested named participants and a
meeting channel/location, while reporting usable contrast and no other material
still-frame defect. These labels are unchanged from the selected comparison.
The frontend has no verified meeting-location field to display, and retained
speaker labels must not be replaced by guessed names. Adding participant or
calendar enrichment would change the product contract rather than fix this
presentation slice. Disposition: record the baseline request; preserve actual
source labels and the approved local notepad model. The independent revision
verdict is retained as written, not converted into an unconditional acceptance.

A packaging check found the comparison-style inline rules would violate the
existing `style-src 'self'` CSP. The implementation now uses static external CSS
and derives inspected classes from browser hover/focus state after layout. The
browser journeys enforce the actual packaged CSP; no security policy is relaxed.

The exact-commit source review identified stale build references in the first
two cold records. They now point at `reviewed-source-manifest.json`. A fresh
cold reviewer inspected all eight final frames and accepted each, with missing
verified meeting context noted separately from layout defects (`cold-final.md`).

The review also found that multiple timed note lines could map to one retained
turn. The renderer now records every matching logical line on the passage and
disclosure; inspection emphasizes all matching lines and marks. A two-note,
one-passage fixture is exercised in all four browser journeys.

Inspection now runs in the routine `run.sh all` gate. Playwright is pinned in
the desktop dev dependencies; browser and server defaults use the same port.
The full gate exposed two stale checks: Rename must first open the More menu,
and hidden native views emit no entrance-animation events. The smoke scenario
now uses the existing menu path. The unchanged real-DOM sheet scenario runs in
a headless browser, where animations render, instead of accepting zero events
or showing a native window. No production sheet behavior changed.
