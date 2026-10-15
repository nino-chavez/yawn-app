# Yawn app icon — three concepts, 2026-10-14

Three divergent app-icon concepts for Yawn, drawn for the iPad app. Each is an
Icon Composer document (`<letter>/<letter>.icon`) rendered by Apple's own
`ictool` at iPadOS 27's design generation, so the light, dark, tinted and clear
looks are the system's, not an approximation.
[`comparison.png`](comparison.png) shows all three side by side.

Yawn has no real icon yet. The Mac app's `icon.png` is a screenshot of the
retired "local-meeting-notes" landing page, and the iPad research build has
none. Whichever concept is selected could replace the Mac icon too; that is a
separate decision.

## The three ideas

| | Idea | Strongest | Weakest |
|---|---|---|---|
| [A · The name](A/README.md) | The oval a slow yawn opens: a hollow ring leaning back | Simplest shape; clearest at 20 pt; reads as a brand, not a category | Can read as a zero, an O or an egg; nothing in it says notes or meetings |
| [B · The page](B/README.md) | The icon is the sheet, with one finger-written line of ink across it | Says "write here" at a glance; feels hand-made | The stroke reads as the letters "lm", the retired name's initials |
| [C · Ink and time](C/README.md) | A written stroke anchored to a mark on a vertical rail | The only one that shows why the app exists: a note tied to its moment | Reads as a cursive "e", and the rail and node look like a slider control at small sizes |

All three are weakest in **tinted dark**: the system draws the mark as
saturated violet on near-black, and one `tinted` setting feeds both tinted
looks, so it cannot be brightened for dark alone.

## What these renders do not show

They are `ictool` exports, not a home screen. The selected icon gets checked on
the operator's iPad, next to real neighbors, before it is called done.

## What the workers proved about the `.icon` format

Each key below was rendered with and without it and the pixels compared.
Details and counts are in each concept's README.

- Top-level `fill-specializations` (a base entry with no `appearance`, then
  `{"appearance": "dark", ...}`) sets per-appearance backgrounds, but only when
  there is no top-level `fill`. With both present, the specializations are
  silently ignored.
- Layer `fill-specializations` accepts `dark` and `tinted`. One `tinted` entry
  drives both tinted and both clear looks.
- Layer `"glass": false` and group `translucency` (which needs both `enabled`
  and `value`) change every look.
- `image-name-specializations` changed nothing next to a base `image-name`.
- At most four groups (Apple's Icon Composer documentation).

`probe-schema.icon` is the one-layer document used to prove the basic schema
before the concepts were drawn.

## Regenerate a render

```
"/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool" \
  A/A.icon --export-image --output-file A-Default.png --platform iOS \
  --rendition Default --width 1024 --height 1024 --scale 1 --design-generation 27
```

Renditions: `Default`, `Dark`, `TintedLight`, `TintedDark`, `ClearLight`,
`ClearDark`. Re-rendering the committed documents reproduced the workers'
renders exactly (RMSE 0), so per-look PNGs are not stored; each concept keeps
its strip and small-size sheet.

## Selection

Pending the operator's pick. It will be recorded under `decisions/`, with what
was taken from the other two.
