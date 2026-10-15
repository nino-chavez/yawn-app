# Concept C: the link between ink and time

## The idea

A handwritten stroke leaves a round mark on a vertical rail: the written line is tied to a moment you can go back to.

## Why it fits Yawn

- **Reason to exist.** The iPad's job is that a handwritten line is tied to the moment it was written, so tapping it plays what was said. The icon draws that link and nothing else. It is the rail from the selected "Time rail" screen, reduced to one mark.
- **Paper-first.** The page is the fill, not a drawn object: no edge, fold, lines or cover. The ink is one finger-weight stroke of even width, as a finger writes.
- **Quiet and glanceable.** Three shapes, low-chroma Tonal Ledger colors, no record red. It holds at 40 px.
- **Candid and unhurried.** One loose cursive loop, not a logo flourish. There is no microphone, waveform, clock, play triangle or sparkle.

## Appearances (all six rendered with ictool, design generation 27)

- **Default.** Page `#faf8fb`, ink `#211f25`, accent node `#6855a9`, rail `#8c7ca8`. Holds. The node reads as a bead on the rail with the stroke leaving it.
- **Dark.** Fill `#2c2a35`, ink `#e8e2ed`, node `#b8a7ce`, rail `#a294b1`. Holds, same strength as Default.
- **TintedLight.** Stroke and node white on the tint field, rail dimmed. Strongest of the four tinted and clear renders.
- **TintedDark.** Stroke and node take the tint color on near-black. Readable, but the lowest contrast of the six. Rendered at four other tint colors: still readable on all four.
- **ClearLight / ClearDark.** White glass strokes on a gray chiclet. ClearDark is strong. ClearLight is white on light gray, so only the glass edge and shadow separate it.
- **Small sizes.** At 58 px and 40 px all six still show rail, node and open loop (see `renders/C-small.png`, which has Default and Dark; I checked the other four by hand).

## What is weakest

- **It reads as a letter.** A cursive loop looks like a handwritten "e" or "l". The brief bans words and a plain "Y", not this, but it is the idea's main risk, and it edges toward concept A's ground (the identity mark). I spent one render round on alternatives: a flat loop on the baseline (reads more strongly as "e"), two small loops in a row (the loops fill in at 40 px and it reads as a spring), a small curl on a long swash (the curl fills in), and a far-leaning loop (still an "l"). None read less like a letter and held at 40 px, so the tall open loop stays.
- **The link is implied.** Anyone who does not know the product sees a stroke on a line. The mark means "tied to a moment" only once the user knows the rail.
- **Tinted and Clear lose the node.** All four tinted and clear renders take one shared value, so node and stroke are the same tone. The node still reads as a dot at the start of the stroke, but the accent color is gone.
- **ClearLight at 40 px** is the faintest of the set.
- **White on a light tint** (the yellow tint I tried) is low contrast in TintedLight.

## Layers (front to back, three groups, under the four-group limit)

1. `node.svg`: accent disc on the rail. Translucency off.
2. `stroke.svg`: the handwritten stroke, 64 px wide, round caps.
3. `rail.svg`: the vertical rail, 35 px wide, round caps.

All SVGs are 1024 x 1024 hand-authored paths, no raster. The fill is the page color.

## icon.json keys beyond the probe

All confirmed by rendering with and without the key (pixel counts at 1024 px):

- **Top-level `fill-specializations`** (a base entry plus `appearance: "dark"`), replacing the probe's `fill`. Removing the dark entry changes only the Dark render (950,213 px). A `fill` key next to it made the specialization be ignored (0 px).
- **Layer `fill-specializations`** (`dark`, `tinted`). Removing them changes Dark, both Tinted and both Clear renders (132,051 to 148,233 px each) and leaves Default unchanged (0), because the SVG colors rule Default. Both Clear renders and TintedLight follow the `tinted` value only: with `light` and `dark` both set to black and `tinted` white, they were pixel-identical (0 px) to the final render. A `light` entry does change Default (149,434 px), but I did not use one.
- **Group `translucency`** `{ "enabled": false, "value": 0.5 }` on the node group. Removing it changes all six renders (about 15,000 px each): the rail shows through the node without it. The `value` is required; ictool fails with "The data couldn't be read because it is missing" without it.

Tried and did nothing, or not as hoped:

- `"lighting": "combined"` on a group: 0 px in four renditions.
- `fill` together with `fill-specializations`: the specializations were silently ignored. Use only `fill-specializations`.
- `fill: "automatic"` and `fill: { "automatic-gradient": ... }`: 0 px against the solid page fill, compared on Dark only.
- `image-name-specializations` next to a base `image-name`: 0 px. It changed the render only when the layer had no `image-name`. Not used.
- `idiom: "square"` on a fill specialization: untested cleanly, because a top-level `fill` was present.

Changed the render but unused: group `shadow`, layer `opacity`, layer `glass: false`, group `specular: false`.
