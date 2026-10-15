# Concept C2: writing on a timeline

## The idea

A short line of loose finger handwriting runs off one emphasized mark on a rail of quieter moment marks: this note, written at that moment.

## What changed from C

- **Timeline, not slider.** C had one knob on a line, and a cold reader (a separate model, blind to which tile was which) called it a slider. C2 has five marks on the rail: four small quiet ones and one large linked one. The same reader called the C2 tile a timeline.
- **B's ink, not one loop.** Accent violet `#6855a9`, 60 px, uneven and rising, with a loop at the end. It has no ascender loop, no closed bowl and no matching arches.
- **Linked mark.** The node and the ink are the same violet, so they read as one thing. The rail and quiet marks are the quiet gray-violet `#8c7ca8`.
- **No red.** The accent already says "this one".

## Letters at 167 px

- **My read, and the result:** no word and nothing firm, but I can still see a faint "n" in the first arch and a faint "e" or "r" in the end curl. Under the rule "any plausible letter means redraw" this weakly fails.
- **Cold readers (support, not a pass):** two blind passes by separate model readers at glance size, 0 to 3 scale. The C2 ink (second pass) scored m/n 1, e 1, s 1. In the same pass B scored m/n 2 and C scored e 3. They rated an earlier render with the same ink (32 px marks, rail and marks as two layers), not the final.
- **Limit of that check:** the readers did not read B's "lm" strongly, so a low reading is not proof. Apple Vision text recognition (swept over 9 sizes) read nothing in C2 Default or Dark. It read B at 3 of 9 sizes ("en", "vs", "er"), so it can see this defect, but only sometimes.
- **Not solved:** every undulation is a little "m", "n" or "w" if you want it to be. Families I drew and dropped: separate pen-lift "words" (I read "h" and "2"), a coil (I read "a"), a lazy S (readers: "s" or "2" at 1), a low even wave (readers: "m" or "w" at 2), and a spring of regular humps ("mm").

## Appearances (all six rendered with ictool, design generation 27)

- **Default.** Page `#faf8fb`, violet ink and node, gray-violet rail. Holds at 167 and 58 px. At 40 px the quiet marks are specks, so the timeline reading weakens; node and ink hold.
- **Dark.** Fill `#2c2a35`, ink and node `#b8a7ce`, rail `#968aa8`. Holds, same as Default.
- **TintedLight.** White ink and node on the tint field. Strongest of the tinted and clear set.
- **TintedDark.** Ink and node take the tint color on near-black. Readable. The rail value for tinted renders was raised from 0.62 to 0.85, which keeps the quiet marks visible at 58 and 40 px (only the tinted and Clear renders changed, about 25,800 px each; Default and Dark 0 px). The ink is not brighter than in C (see below).
- **ClearLight / ClearDark.** White glass ink. ClearDark is strong. ClearLight relies on the glass edge.

## What is weakest

- **TintedDark ink.** One `tinted` entry feeds both tinted renditions and Clear, and white is already the maximum. Mean ink luminance is 0.302 in C and 0.322 in C2, which is not a visible gain. `blend-mode` and `opacity` did nothing. I did not trade the light look for it.
- **40 px.** The rail reads as a beaded line, not a timeline of five.
- **It reads as a squiggle more than handwriting.** The end curl is the only handwriting cue.
- **Five marks, one line.** The request was one mark per written line. A second, quieter line made it read as a line chart, so I dropped it, and the quiet marks stand without lines.
- **Off-token dark rail.** Dark rail `#968aa8` is a midtone I picked, not a token. Light rail `#8c7ca8` is the selection-line token.
- **Without the rail** (groups 0 and 2 removed), the ink sits about 55 px above center and starts left of center. That variant renders cleanly; I checked it.
- **Translucency off** on all three groups makes the violet flat and solid, less glassy. With it on, the ink washed to light violet toward the tail.

## Layers (front to back, three groups)

| Group | Layer | Part | Drop for the rail-less variant |
|---|---|---|---|
| 0 | `node.svg` | the linked mark, r 60 | yes |
| 1 | `ink.svg` | the writing, one stroked path | no |
| 2 | `rail.svg` | rail line plus four quiet marks | yes |

SVGs are hand-placed paths and circles, no raster.

## icon.json keys beyond the probe

Each was rendered with and without the key (differing pixels at 1024 px):

- **Top-level `fill-specializations`** (base plus `dark`): removing the dark entry changes Dark only (889,108).
- **Layer `fill-specializations`** (`dark`, `tinted`): removing them leaves Default at 0 and changes the other five (100,071 to 103,219).
- **Group `translucency`** `{ "enabled": false, "value": 0.5 }` on all three groups. Removing it changes all six renders: node group 10,500 to 11,200 px, ink group 58,649 to 62,528, rail group 25,191 to 25,775.

Did nothing: layer `blend-mode` (`screen`, `plus-lighter`, `overlay`), `blend-mode-specializations` (`tinted`) and `opacity: 1.0`, all 0 px on TintedDark. A top-level `fill` beside `fill-specializations` silently disables the specializations (tested on C). I removed the group `name` key: no way to confirm it.
