# Visual refit brief — 2026-09-03

Operator verdict on packaged build 21: *"this doesn't feel like a high grade
desktop app at all"*, naming **font size and placement, button spacing, and text
inside buttons**. This is a **refit**: the structure `docs/product-brief.md`
specifies is correct and stays. The visual system is what changes.

Baseline: `docs/evidence/screen-reviews/captures/d0c5d86-installed/` (12 device
frames) and the cold review beside it. Every number below is measured from those
frames or read from source, not estimated.

## Diagnosis

The design system is not missing. `tokens.css` is well formed — a correct macOS
type ramp (11/13/15/17/22), 22px control height, 6px radius, 62ch measure, real
semantic colours. **The system is sound and its application drifted.** Four
mechanical causes produce everything the operator is seeing.

**1. There is no spacing scale, and 31 distinct pixel values stand in for one.**
`margin`/`padding`/`gap` in `styles.css` use every integer from 1 to 22, plus 24,
26, 28, 32, 38, 40, 48, 62, 78. No `--space-*` token exists in either stylesheet.
That is the direct cause of the measured vertical rhythm in the reading pane —
gaps of 1, 4, 10, 15, 16, 20, 20, 30, 33, 56, 56pt between consecutive blocks.
Irregular rhythm is what the eye reads as unfinished before it can name why.

**2. Two type scales compete, giving fourteen sizes where five were designed.**
`tokens.css` defines `--t-caption/body/read/title/large` (11/13/15/17/22).
`styles.css` defines a second, parallel family — `--text-micro` 9, `--text-caption`
10, `--text-meta` 11, `--text-label` 12, `--text-body` 13, `--text-body-lg` 14,
`--text-subtitle` 15, `--text-intro` 16, `--text-lg` 17, `--text-metric` 18,
`--text-section` 20, plus three aliases — and 62 uses across the file. Six more
hero sizes are `clamp()` literals deliberately outside both systems. Sizes one
pixel apart cannot be told apart, so *more* sizes produce *less* hierarchy. This
is the "font size" half of the verdict.

**3. Section headings are smaller than the body text they head.** `.read` is
15px; `.read h2` is `var(--t-body)` = 13px. "Overview" and "Your notes" are
physically smaller than the sentences beneath them, carried only by weight.
This is the "placement" half: nothing tells the eye where a section starts.

**4. The Record button is rendering disabled, and nothing says why.** Measured
fill rgb(43,43,43) and border rgb(52,52,52) against a toolbar of rgb(30,30,30).
`opacity: 0.45` — the `.btn:disabled` rule — reproduces both exactly: 0.45x58 +
0.55x30 = 42.6, and 0.45x78 + 0.55x30 = 51.6. `main.js:602` disables it when
`canOpenStart(snapshot, permissions)` is false. So the product's primary verb is
permanently greyed on this build with no explanation on screen. Not a polish
defect; it outranks the rest.

## Preserve list — not touched by this refit

- Every semantic element, `data-action`, ARIA attribute and keyboard path.
- The product brief's structure: the three moments, note-before-transcript
  reading order, operator notes distinct from generated claims, withheld turns
  rendered as withheld.
- `DESIGN.md`'s reserved-hue rule: one job per hue. No new colours.
- The `tokens.css` colour values themselves, light and dark.
- Copy. No wording changes; `DIRECTION.md`'s content-reads table governs those.
- Retired screens stay retired.

## Slices, in order

1. **Spacing scale.** Add `--space-*` (4/8/12/16/24/32/48) to `tokens.css` and
   migrate `styles.css` to it, one region at a time, snapping each existing value
   to the nearest step. Highest leverage and the most mechanical.
2. **Collapse the type scale** onto the five `tokens.css` sizes; map each
   `--text-*` name to its nearest token and delete the parallel family.
3. **Fix the heading inversion** so a section head outranks its body.
4. **Buttons**: give Manage more weight than Rename since it opens destructive
   actions; give Generate note real separation from its caption; state the
   Record disabled reason on screen.
5. **Re-capture and cold-review.** A refit is not done on a passing test run.

Gate at every slice: `npm run test:ui` from `apps/desktop`, plus a device frame
compared against the baseline.
