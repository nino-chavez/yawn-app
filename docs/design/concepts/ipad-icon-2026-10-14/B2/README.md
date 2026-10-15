# Concept B2: the ink alone

B2 is [C2](../C2/README.md) with the rail removed. It exists so the operator
can compare two icons that differ only in the rail: the page with one line of
finger writing (B's idea) against the same writing tied to a moment (C's idea).

## How it was made

- `B2.icon/icon.json` is C2's document with the node and rail groups dropped,
  so the ink group, its colors and its per-look settings are identical.
- `B2.icon/Assets/ink.svg` is C2's stroke, centered on the canvas and scaled
  1.1 times to use the room the rail left. The path itself is unchanged.
- Rendered with `ictool` at design generation 27 in all six looks;
  `renders/B2-strip.png` and `renders/B2-small.png` are assembled from them.

## What it inherits from C2

- The same stroke, so the same letter risk: a faint "n" in the first arch and
  a faint "e" or "r" in the end curl at 167 px (see C2's README).
- The same tinted-dark weakness: one `tinted` setting feeds both tinted looks,
  so the ink cannot be brightened for dark alone.

## Read against B

- B's stroke ran off the left edge and read as "lm"; B2's sits inside the page
  and reads as a loose line of writing more than as letters.
- With no rail, nothing in it says "recorded" or "meeting". It says "write here".
