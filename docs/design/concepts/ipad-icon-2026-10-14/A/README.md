# Concept A: the name

Yawn's mark is the oval a slow yawn opens in the air: a tall, hollow shape that
leans back, with its weight settling at the bottom and a thin rim at the top.

## Why it fits Yawn

- **Unhurried.** The 12-degree backward lean and the heavy bottom rim read as
  settling, not moving. Nothing in it points forward.
- **Quiet.** One shape, one color: `#6855a9` on `#faf8fb` (dark: `#b8a7ce` on `#2c2a35`).
- **Paper-first.** The opening is page-colored space with a margin of rim, so the
  mark is mostly room left to write in.
- **Glanceable.** A closed shape with one hole survives 40 px
  (`renders/A-small.png`). It is a brand mark, not a notes or recorder symbol, so
  it can move to the Mac app unchanged.

## Appearances (contrast measured on the 1024 renders, at the heavy bottom rim)

| Appearance | What it does | Holds? |
|---|---|---|
| Default | Violet glyph on page color; glass lightens the lower rim | Yes, 4.3:1 |
| Dark | Lavender glyph on `#2c2a35` | Yes, 5.2:1 |
| TintedLight | Near-white glyph on the system tint | Yes, 4.2:1 |
| TintedDark | Glyph takes the tint at low brightness on near-black | Weakest, 2.1:1 |
| ClearLight | White glass glyph; the tile follows the wallpaper | Yes on gray, 3.3:1 |
| ClearDark | White glass glyph on a dark tile | Yes, 9.7:1 |

Tinted also held at `--tint-color` 0.08, 0.35, 0.6, 0.9. All six hold at 58 and 40 px.

## What is weak

- **It can read as the letter O or a zero**, most at 40 px. It breaks no hard
  anti-goal (no word, no Y), but only the lean and the heavy bottom separate it
  from a plain letter, and at small size those are subtle.
- It also reads as an egg, an earring, or a ring-shaped gadget. Nothing says
  "yawn" without the name; the idea needs a sentence to land.
- TintedDark is dim. The layer is already white and the system sets brightness.
  Turning glass off for tinted lifted it only to about 2.3:1 and lost the depth.
- Only `ictool` renders were checked: no device or real wallpaper.

## Layers

One group (limit is four), one layer: `opening` (`Assets/opening.svg`), the
weighted ring, even-odd fill, no raster. The background is a per-appearance solid.

## icon.json keys beyond the probe (each A/B-rendered against the same icon without it)

- Top-level `fill-specializations` (an entry with no appearance plus
  `"appearance": "dark"`), **replacing** the probe's `fill`. Removing the dark
  entry changed 92% of Dark pixels; removing the default entry, 87% of Default.
  With `fill` also present the specializations are ignored (0 pixels differ).
- Layer `fill-specializations`, `dark` entry: 17% of Dark pixels.
- Layer `fill-specializations`, `tinted` entry: about 16% of pixels in each of
  TintedLight, TintedDark, ClearLight, ClearDark. It also drives the Clear renders.

Tried, not in the final: `"idiom": "square"` on the dark entry (dark not applied);
`blend-mode: "screen"` (0 pixels differ); `opacity` (works, 19% of Default, used
only in a dropped echo-ring layer); `glass: false` / tinted `glass-specializations`
(works, 20% of TintedDark, rejected above). `automatic-gradient` was inconclusive.
