# Yawn visual treatments

Three visual-system treatments over the same Concept A structure and the same
three representative states. Nothing in this directory ships.

Brief: `docs/design/visual-system-rethink-brief-2026-09-03.md`.

- `native-editorial/`
- `private-notebook/`
- `precision-utility/`

Serve this directory, then open:

`shared/frame.html?treatment=native-editorial&state=sparse&theme=dark`

Every treatment supplies only `styles.css` and `RATIONALE.md`. The shared frame
owns semantics, copy, and fixtures so the comparison cannot improve one concept
by quietly changing its content.

After all three treatment stylesheets exist, run `./capture.sh`. It captures
dark and light versions of the sparse, dense, and attention states at 1080 ×
900. Open `compare.html` through the preview URL printed by the script to review
all 18 frames on one surface.
