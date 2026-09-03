# Yawn visual treatments

> Static comparison archive. The selected rules ship from the product token
> and stylesheet files, not from this directory.

```text
visual-treatments — compare active visual-system finalists against identical Yawn states

Usage:
  ./capture.sh
  open http://127.0.0.1:8792/compare.html
```

Every treatment uses the same Concept A structure and the same four
representative states. On 2026-09-03 the operator selected **Tonal Ledger —
Canvas**. Marquee Document — Quiet and the other directions are rejected but
retained for archaeology. The comparison keeps both refinements and their
previous-round baselines visible so the decision remains reviewable.

Brief: `docs/design/visual-system-rethink-brief-2026-09-03.md`.

## Treatments

Selected reference:

- `tonal-ledger-refined/` — the center pane becomes the containing canvas

Rejected finalist reference:

- `marquee-document-refined/` — quieter title scale and sentence-case sections

Previous-round finalist baselines:

- `tonal-ledger/` — Private Notebook containment with low-chroma tonality
- `marquee-document/` — Minder's dominant-object hierarchy translated to Yawn

Rejected history, retained on disk and omitted from `compare.html`:

- `native-editorial/` — a typeset note inside compact Mac chrome
- `private-notebook/` — a personal, bounded, visibly writable document
- `precision-utility/` — a compact, operational desktop surface
- `quiet-folio/` — neutral paper, graphite chrome, and serif only in the note

## Review

Serve this directory, then open:

`shared/frame.html?treatment=native-editorial&state=sparse&theme=dark`

Every treatment supplies only `styles.css` and `RATIONALE.md`. The shared frame
owns semantics, copy, and fixtures so the comparison cannot improve one concept
by quietly changing its content.

Run `./capture.sh` after changing an active treatment. It captures dark and
light versions of the sparse, transcript-only/no-note, dense, and attention
states at 1080 × 900. The transcript-only frame includes the real “No meeting
note yet.” / Generate note hierarchy rather than treating it as a variation of
the generated-note sparse frame. Open `compare.html` through the preview URL
printed by the script to review the 32 active and baseline frames on one surface.
