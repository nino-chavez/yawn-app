# Rethink concepts — phase 2 of the 2026-09-02 design rethink

Three divergent whole-screen concepts (judged-screen pattern § 2c), each
rendering the same four states from `shared/fixtures.js` at the real window
size (1080x900), in both appearances via `?theme=dark|light`. Not product
code: nothing here ships. The brief they answer is
`docs/experience-brief-2026-09-02.md`.

- `a/` Source list beside document (the Notes / Bear shape) — **SELECTED 2026-09-02** (`docs/design-direction-decision.md`)
- `b/` Notepad first, list as a popover (the Granola / Wispr shape)
- `c/` Dated stream, newest expanded inline (the Agenda shape)

Serve: `preview apps/desktop/ui-harness/concepts 8791`, then
`http://localhost:8791/a/index.html?state=1&theme=dark`.
Capture all 24 frames and the comparison sheet: `./capture.sh`.

`out/` holds the 24 captured frames the selection was made from; `compare.html`
lays them out by state. B and C are kept, not deleted, so the comparison
stays inspectable. Capture notes: each frame needs its own navigate, viewport
override, screenshot sequence (the override does not survive a navigation),
and parallel agents need a distinct `BROWSE_SESSION` or they share one tab.
