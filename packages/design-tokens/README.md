# @yawn/design-tokens

Yawn's design tokens, in the same schema shape as Minder's
(`packages/design-tokens/tokens.json` in the Minder repo): `meta`, `color`,
`font`, `fontSize`, `spacing`, `radius`, `motion`, `layout`. **Yawn and Minder
share the schema, not the values.** Minder derives OKLCH roles from a brand
triplet, for a one-handed phone screen. Yawn authors direct light/dark hex
and rgba constants, approximating macOS system colors for a Mac document
window (DESIGN.md §Color). Don't copy a value across. Do keep the shape
aligned, so both apps are edited the same way.

## Source of truth

`tokens.json` is authoritative. `apps/desktop/ui/tokens.css` is generated —
edit values in `tokens.json`, never in `tokens.css` directly. `DESIGN.md` at
the repo root is the binding design spec. A value change starts there, then
`tokens.json`, then regenerate.

## Regenerate

```sh
node packages/design-tokens/generate.js          # writes apps/desktop/ui/tokens.css
node packages/design-tokens/generate.js --check  # verifies tokens.css is current; exits 1 if stale
```

Or from `apps/desktop/`:

```sh
npm run tokens         # regenerate
npm run tokens:check   # verify only
```

No dependencies — plain Node, `fs`/`path` only.

## What's generated vs. what's documented

`tokens.json` is a superset of what `tokens.css` currently exposes as CSS
custom properties. Minder's own `tokens.json` works the same way: it carries
values its generated CSS doesn't yet consume. Two gaps worth knowing about
(a third, `fontSize.transcript`, was wired in during the 2026-09-03 visual
refit — `tokens.css` now emits `--t-transcript`/`--lh-transcript`, and
`apps/desktop/ui/styles.css`'s transcript turn text reads them instead of a
duplicate `--text-transcript-body`):

- `layout.inspector` (320px) and `layout.control` (22px / 26px) are
  documented per DESIGN.md. Neither has an `--inspector-w` or control-height
  custom property yet; inspector width and control heights are still
  hand-set at the component/JS layer.
- `motion.duration` (`fast: 120ms`, `standard: 200ms`) is DESIGN.md's
  two-speed motion scale. The single `--motion` custom property the CSS
  actually emits today is `motion.default` (`160ms ease-out`) — a
  pre-existing blended value. The generator preserves it as-is rather than
  silently reconciling it against the documented scale.

When one of these gets wired into a real custom property, add the mapping in
`generate.js`. Don't hand-edit `tokens.css` — the next regenerate overwrites it.

## Adding a new token

1. Add the value to `tokens.json`, under the right top-level key (`color`,
   `font`, `fontSize`, `spacing`, `radius`, `motion`, `layout`). Light/dark
   pairs go under `{ "light": ..., "dark": ... }`. Omit a slot you have no
   value for rather than inventing one — this matches Minder's convention of
   leaving unused schema slots absent.
2. If it should become a CSS custom property, wire it into the template in
   `generate.js`, under the matching `:root` / `[data-theme="dark"]` /
   contrast / motion block.
3. Run `node generate.js` and check the diff on `apps/desktop/ui/tokens.css`.
   A token value change should produce exactly one changed value, nothing
   else.
