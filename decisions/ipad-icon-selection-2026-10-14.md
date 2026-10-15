# Yawn's app icon: writing on the time rail (C2)

2026-10-14 · chosen by Nino Chavez, the operator · judged-screen pattern § 2c

## Decision

Yawn's iPad app uses **concept C2**: a short line of loose finger handwriting
runs off one emphasized mark on a vertical rail of quieter moment marks. It
draws the product's reason to exist on iPad: a written note is tied to the
moment it was written, and the rail is the same time rail as the selected page
screen ([decision](ipad-concept-selection-2026-10-14.md)).

The concepts, renders and comparison sheets are in
[`docs/design/concepts/ipad-icon-2026-10-14/`](../docs/design/concepts/ipad-icon-2026-10-14/README.md).

## How it was chosen

1. Three divergent concepts were drawn as Icon Composer documents and
   rendered by Apple's `ictool` in all six iPadOS 27 looks: A (the name, a
   leaning oval), B (one line of ink on the page), C (a stroke anchored to a
   rail).
2. The operator set A aside because it did not make sense to him, and
   narrowed the pick to B or C, reading C as writing on a timeline.
3. C read as a slider and its stroke as a cursive "e"; B's stroke read as
   "lm". The revision drew C2 and derived B2 from it, so the two differed only
   in the rail. The operator chose C2.

## What was taken, from where

- **From C:** the rail and the linked mark, now five marks so it reads as a
  timeline rather than a slider.
- **From B:** the ink. A loose, uneven finger stroke in accent violet replaced
  C's single loop.
- **Left out:** A's oval, which carried identity but no meaning the operator
  could read. B2's rail-less page, which was calmer but said only "write
  here". Record red, which the Mac design reserves for the Record control.

## Known weaknesses, accepted

- At 167 px a faint "n" and "e" can be read in the stroke. No word forms.
- At 20 pt the quiet marks become a dotted line, and the rail with a line
  running off it can read as a chart axis.
- Tinted dark is the weakest look: one tinted setting feeds both tinted looks.

## Scope

- The iPad research build (`spike/ipad-ink/`) uses it as
  `IPadInk/AppIcon.icon`. Its display name stays "iPad Ink".
- The Mac app's `icon.png` is still the screenshot of the retired landing
  page. Using C2, or a variant of it, on the Mac is a separate decision.
- Checked on the device 2026-10-15: a home-screen screenshot and a cold review
  ([record](../docs/evidence/screen-reviews/ipad-icon-home-screen-2026-10-15-cold.md)).
  It reads as a handwriting or notes app and no letters were read. The rail was
  faint at home-screen size.
- Revised 2026-10-15 at the operator's request: the rail stroke went from 24 to
  44 px, the quiet marks from radius 38 to 46, and the rail color from
  `#8c7ca8` to `#7d6e9c` (dark: `#968aa8` to `#a597b8`). The linked mark
  (radius 60) and the ink are unchanged. The installed app uses the revised
  `AppIcon.icon`; `docs/design/concepts/ipad-icon-2026-10-14/C2/` stays the
  as-chosen record. Device before and after:
  [`home-screen-rail-before-after.png`](../docs/evidence/screen-reviews/ipad-icon-2026-10-15/home-screen-rail-before-after.png).
