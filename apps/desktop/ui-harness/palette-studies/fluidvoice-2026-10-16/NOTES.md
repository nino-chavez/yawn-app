# FluidVoice-derived palettes, light and dark, 2026-10-16

Status: a study, not a selection. Rail stays the production palette until the
operator picks. Nothing in `tokens.json` changed.

The question: which of FluidVoice's colors could Yawn adopt in both
appearances? The source values were sampled from the operator's FluidVoice
screenshots (`docs/evidence/comparables-fluidvoice-2026-10-16.md`, "Colors and
style"). They are approximate, and the light values for each candidate are
derived, because FluidVoice was sampled mostly in dark.

## The three palettes

`palettes.json` owns the values. Each candidate overrides only the tokens it
names; every other token is Rail's.

1. **Rail (current).** A warm paper page, graphite chrome and a blue moment.
   No overrides.
2. **Cool Paper.** FluidVoice's cool neutrals across the whole window, in both
   appearances: dark sidebar `#1f212e` and page `#24252a`, as sampled, with a
   light counterpart. The labels, the graphite accent and the rail move to cool
   grays. Record, attention and moment are unchanged.
3. **Navy Chrome, Teal Moment.** Rail's warm page, with a navy-tinted sidebar
   and toolbar (dark sidebar `#1f212e`). FluidVoice's teal becomes the one
   moment color: `#4fcfcf` in dark and `#0f6b6d` in light, darkened for
   contrast on the page.

## How the frames were made

- **Renderer.** The real `ui/` frontend in the harness's native WKWebView
  runner (`../../run.sh palette <mode> <scenario> <name>`, added for this
  study). It uses stub data with synthetic content, at 1080 by 900 and 2x,
  saved as JPEG.
- **Palette.** Each candidate's tokens are applied by its generated scenario
  in `scenarios/`.
- **Appearance.** Set by the runner's `appearance` query. Every scenario
  reported `ok: true`, no page errors, and the expected `data-theme`.
- **States.** The ledger after a meeting (`ledger`) and an interrupted
  recording with its attention surface (`interrupted`), each in both
  appearances.
- **Comparisons.** Each `compare-*.jpg` sets Rail, Cool Paper and Navy Chrome
  side by side, left to right.

Not covered: Settings, the first run, Largest text, increased contrast, the
installed app and the iPad.

## Contrast

`contrast.json` covers fourteen foreground and background pairs per palette
and appearance. Every text pair in both candidates meets 4.5:1. The lowest
pair is Record's red on its button in dark, at 4.49:1, the same in all three
palettes because no candidate changes Record. Record on the window chrome
reads 4.30 to 4.34:1 in light for all three, but the Record label sits on a
control, not on the chrome.

## What the frames show

- **Cool Paper** is coherent: one temperature from the toolbar to the page.
  The amber attention surface separates from it more clearly than on Rail.
  The serif title holds. The cost is the paper character DESIGN.md names
  first ("the page is a warm sheet"). It is also close to **Blue Hour**, the
  cool slate dark palette studied on 2026-09-08 (`../NOTES.md`) and not
  selected.
- **Navy Chrome** is the most recognizably FluidVoice candidate. Against the
  warm page the cool sidebar reads as two temperatures, a clash most visible in
  dark. The teal moment barely registers on the Mac. There the moment color is
  used only by the full transcript's target line (`--evidence`, `.transcript-
  line-target`) and the accent's soft tint, so neither frame shows it. Most of
  the teal's effect would land on the iPad, where moment marks the playhead
  and the current turn.

## If one is picked

- **Rules.** Both candidates need a DESIGN.md Color and Character amendment.
  Character says the page is a warm sheet and the chrome is graphite, and
  Color lists Rail's values.
- **iPad.** `tokens.json` also generates the iPad color catalog, so a pick
  changes the iPad unless the values are split by platform.
- **Proof.** Then a `tokens.json` change, `npm run tokens`, installed Fixture
  captures in both appearances, and a cold review.
