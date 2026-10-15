# Concept B: the page

**Idea.** The whole icon is the sheet, and one finger-written line of ink crosses it, entering from the left edge and stopping mid-motion.

## Why it fits Yawn

- **Paper-first:** no cover, no ruling, no border. The page fills the icon and the ink is the only mark.
- **Quiet and unhurried:** two tones from the Tonal Ledger palette and the page below the line left empty, so it reads as room still to fill.
- **Candid:** the stroke is uneven, slanted, off-center and unfinished. It looks written, not drawn.
- **Glanceable:** three features (one looped ascender, one hump, one arch) on a 68 px stroke. It still reads at 40 px.
- It says "you write here". It does not say "recorded": no mic, wave, record red, rail or clock.

The line runs off the left edge on purpose: the page is larger than the icon, which is how the iPad page behaves.

## Appearances

| Rendition | What it does | Holds? |
|---|---|---|
| Default | `#6855a9` ink on `#faf8fb` page, flat | Yes, including 40 px |
| Dark | `#b8a7ce` ink on `#2c2a35` page | Yes, best contrast |
| TintedLight | system tint as page, white ink | Yes, strongest of the tinted pair |
| TintedDark | near-black page, ink in a saturated tint-violet | Weakest of the six: lowest contrast, dim at 40 px |
| ClearLight / ClearDark | white ink on the system's clear glass | Legible, but the ink is opaque and flat |

## What fails or is weakest

- **It reads as the letters "lm" at 167 px.** It is not a word, but it brushes the "no words" anti-goal, and "lm" matches the retired name, local-meeting-notes. A variant with an "e" loop read as "he", which was worse, so I kept this one.
- **TintedDark** is the weakest appearance. A single `tinted` entry feeds both tinted renditions, so I cannot make the dark one brighter without hurting the light one.
- **`glass: false` goes against the platform.** Glass washed the ink toward the page color on its lower half and cost contrast at 40 px. Flat ink is clearer, but in Clear the ink is opaque white instead of glass, which may look less native.
- **Generic.** Handwriting on white could be any notes or signing app. Nothing here says meeting. A and C are meant to carry that.
- Default is almost white: on a light wallpaper only the system's thin rim separates it.
- The hand-made feel is hand-placed points, not captured ink. It has not been tried against a real finger stroke.

## Layers

Background via top-level `fill-specializations`; one group, one layer `ink` (`ink.svg`, a single stroked path, no raster).

## Keys used beyond the probe

Each was rendered with and without it at 512 px with ictool (generation 27). Figures are differing pixels.

- **Top-level `fill-specializations`** (base entry, no `appearance`, plus `dark`). Base set to red changes Default only (206593). Dark entry removed changes Dark only (206422). With a top-level `fill` present, the specializations were ignored; I removed `fill`.
- **Layer `fill-specializations`** (base, `dark`, `tinted`). Base red changes Default only (42433). Dark red changes Dark only (246828). Tinted red changes TintedLight (42417), TintedDark (42400), ClearLight (159338) and ClearDark (42414). It also fixes color: without it Default still differs by 40784 px, because the SVG's untagged `#6855a9` does not render as exact sRGB.
- **Layer `glass: false`.** Removing it changes all six: 87042, 246828, 89463, 59221, 78797, 246803.

**Tried, did nothing:** `image-name-specializations` (zero pixel change, with and without a base entry).
