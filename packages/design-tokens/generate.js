#!/usr/bin/env node
/*
 * Generates apps/desktop/ui/tokens.css from tokens.json.
 *
 * No dependencies. Reads the JSON, pulls the values the shipped CSS actually
 * consumes, and writes them into a hand-formatted template that matches
 * apps/desktop/ui/tokens.css line for line. tokens.json holds a few values
 * (motion.duration, layout.inspector, layout.control) that document
 * DESIGN.md's full system but aren't wired into a CSS custom property yet —
 * this generator does not emit those; see README.md. fontSize.transcript
 * left that list in the 2026-09-03 visual refit (emitted below as
 * --t-transcript/--lh-transcript) once styles.css needed a real token for
 * transcript turn text instead of a duplicate --text-transcript-body.
 *
 * Usage: node generate.js [--check]
 *   --check   don't write; exit 1 if the generated output would differ from
 *             the file on disk (used by CI / the manual verification step).
 */
"use strict";

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..", "..");
const TOKENS_PATH = path.join(__dirname, "tokens.json");
const OUT_PATH = path.join(ROOT, "apps", "desktop", "ui", "tokens.css");

const t = JSON.parse(fs.readFileSync(TOKENS_PATH, "utf8"));

const c = t.color;
const light = {
  accent: c.accent.light,
  record: c.record.light,
  attention: c.attention.light,
  attentionBg: c.attentionBackground.light,
  evidence: c.evidence.light,
  ...Object.fromEntries(Object.entries(c.neutral).map(([k, v]) => [k, v.light])),
};
const dark = {
  accent: c.accent.dark,
  record: c.record.dark,
  attention: c.attention.dark,
  attentionBg: c.attentionBackground.dark,
  evidence: c.evidence.dark,
  ...Object.fromEntries(Object.entries(c.neutral).map(([k, v]) => [k, v.dark])),
};
const contrastLight = Object.fromEntries(Object.entries(c.contrastMore).map(([k, v]) => [k, v.light]));
const contrastDark = Object.fromEntries(Object.entries(c.contrastMore).map(([k, v]) => [k, v.dark]));

const fs_ = t.fontSize;
const rad = t.radius;
const layout = t.layout;
const motion = t.motion;
const space = Object.entries(t.spacing)
  .map(([k, v]) => `--space-${k}: ${v};`)
  .join(" ");

function render() {
  return `/* Yawn desktop design tokens. Source of record: DESIGN.md.
   Seeded 2026-09-02 from the selected concept (A); the tokens.json port
   regenerates this file. Approximations of macOS
   semantic colors so a concept reads as an installed app in both appearances.
   Not a design system; DESIGN.md is an output of the selection ADR. */
:root {
  color-scheme: light dark;
  --font: ${t.font.body};
  --font-mono: ${t.font.mono};
  --t-caption: ${fs_.caption.size}; --t-body: ${fs_.body.size}; --t-read: ${fs_.read.size}; --t-title: ${fs_.title.size}; --t-large: ${fs_.large.size};
  --lh-body: ${fs_.body.lineHeight}; --lh-read: ${fs_.read.lineHeight};
  /* Transcript turn text is its own documented size (DESIGN.md "transcript 14
     at 1.55"), distinct from body -- the visual refit (2026-09-03) wires it
     in so styles.css's transcript rendering stops leaning on a duplicate
     --text-transcript-body custom property for a value tokens.json already
     held. */
  --t-transcript: ${fs_.transcript.size}; --lh-transcript: ${fs_.transcript.lineHeight};
  --measure: ${t.measure};
  --r-control: ${rad.control}; --r-card: ${rad.card};
  --toolbar-h: ${layout.toolbar}; --sidebar-w: ${layout.sidebar};
  --motion: ${motion.default};
  /* Spacing scale (visual refit 2026-09-03): the rhythm styles.css's 31
     ad hoc margin/padding/gap values collapsed onto. 32 is the one addition
     to DESIGN.md's original 4/6/8/12/16/18/24/48 -- three legacy sheet
     values (28, 32, 38px) clustered there and would otherwise lose 14-37%
     snapping straight to 24 or 48. */
  ${space}

  --window: ${light.window}; --sidebar: ${light.sidebar}; --content: ${light.content}; --panel: ${light.panel};
  --separator: ${light.separator}; --separator-strong: ${light.separatorStrong};
  --label: ${light.label}; --label-2: ${light.label2}; --label-3: ${light.label3};
  --control: ${light.control}; --control-border: ${light.controlBorder}; --control-shadow: ${light.controlShadow};
  --selection: ${light.selection}; --selection-active: ${light.selectionActive}; --on-selection: ${light.onSelection};
  --accent: ${light.accent};                 /* system blue; the user's accent in the real app */
  --record: ${light.record};                 /* the only red on screen; spent by Record and the live state */
  --attention: ${light.attention}; --attention-bg: ${light.attentionBg};
  --evidence: ${light.evidence};  /* one evidence accent, tint only */
}
:root[data-theme="dark"] {
  --window: ${dark.window}; --sidebar: ${dark.sidebar}; --content: ${dark.content}; --panel: ${dark.panel};
  --separator: ${dark.separator}; --separator-strong: ${dark.separatorStrong};
  --label: ${dark.label}; --label-2: ${dark.label2}; --label-3: ${dark.label3};
  --control: ${dark.control}; --control-border: ${dark.controlBorder}; --control-shadow: ${dark.controlShadow};
  --selection: ${dark.selection}; --selection-active: ${dark.selectionActive};
  --accent: ${dark.accent}; --record: ${dark.record};
  --attention: ${dark.attention}; --attention-bg: ${dark.attentionBg};
  --evidence: ${dark.evidence};
}
html, body { margin: 0; height: 100%; }
body { font: var(--t-body)/var(--lh-body) var(--font); color: var(--label); background: var(--window);
  -webkit-font-smoothing: antialiased; }
* { box-sizing: border-box; }
button, input { font: inherit; color: inherit; }
.btn { height: 22px; padding: 0 10px; border-radius: var(--r-control); background: var(--control);
  border: 1px solid var(--control-border); box-shadow: var(--control-shadow); cursor: default; }
.btn.primary { background: var(--accent); color: #fff; border-color: transparent; }
.btn.record { color: var(--record); font-weight: 600; }
.btn.record.live { background: var(--record); color: #fff; border-color: transparent; }
.caption { font-size: var(--t-caption); color: var(--label-2); }
.tertiary { color: var(--label-3); }
.hairline { border-top: 1px solid var(--separator); }
.search { height: 22px; width: 200px; border-radius: 6px; border: 1px solid var(--control-border);
  background: var(--control); padding: 0 8px 0 24px; color: var(--label-2);
  background-image: none; position: relative; }
.search::before { content: "⌕"; position: absolute; left: 7px; top: 1px; color: var(--label-3); }
.read { font-size: var(--t-read); line-height: var(--lh-read); max-width: var(--measure); }
/* Refit 2026-09-03 (finding 3): this was var(--t-body) = 13px, smaller than
   the 15px prose it heads ("Overview" read smaller than its own sentences).
   DESIGN.md's Type section already specs "section headings (15 at 600)" --
   the same size as body, weight-only -- so 15/600 is the documented value,
   not a new decision. It stays on tokens.css's three-size document-pane rule
   ("22, 15, and one line of 13 -- if a fourth appears, something is
   misfiled"); a strict outranking would need a fourth size and is reported
   as blocked by that rule rather than added silently. */
.read h2 { font-size: var(--t-read); font-weight: 600; color: var(--label-2); text-transform: none; letter-spacing: 0; margin: 20px 0 6px; }
.read p, .read li { margin: 0 0 6px; }
.claim { border-bottom: 1px dashed var(--separator-strong); cursor: default; }
.claim.open { background: var(--evidence); border-bottom-color: var(--accent); }
.traffic { display: inline-flex; gap: 8px; margin-right: 12px; }
.traffic i { width: 12px; height: 12px; border-radius: 50%; display: inline-block; }
.traffic i:nth-child(1){background:#ff5f57}.traffic i:nth-child(2){background:#febc2e}.traffic i:nth-child(3){background:#28c840}
.fixture-tag { position: absolute; right: 8px; bottom: 6px; font-size: 10px; color: var(--label-3); font-family: var(--font-mono); }

/* Focus, pressed, disabled — one rule each, system-shaped. */
:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; border-radius: var(--r-control); }
.btn:active, .btn.pressed { filter: brightness(0.92); }
.btn:disabled, .btn.disabled { opacity: 0.45; }
/* Increased contrast: stronger separators and labels, no new colors. */
:root[data-contrast="more"], :root:not([data-contrast="less"]) { }
@media (prefers-contrast: more) { :root { --separator: ${contrastLight.separator}; --separator-strong: ${contrastLight.separatorStrong}; --label-2: ${contrastLight.label2}; --label-3: ${contrastLight.label3}; } :root[data-theme="dark"] { --separator: ${contrastDark.separator}; --separator-strong: ${contrastDark.separatorStrong}; --label-2: ${contrastDark.label2}; --label-3: ${contrastDark.label3}; } }
:root[data-contrast="more"] { --separator: ${contrastLight.separator}; --separator-strong: ${contrastLight.separatorStrong}; --label-2: ${contrastLight.label2}; --label-3: ${contrastLight.label3}; }
:root[data-contrast="more"][data-theme="dark"] { --separator: ${contrastDark.separator}; --separator-strong: ${contrastDark.separatorStrong}; --label-2: ${contrastDark.label2}; --label-3: ${contrastDark.label3}; }
@media (prefers-reduced-motion: reduce) { :root { --motion: ${motion.reduced}; } }
`;
}

function main() {
  const output = render();
  const checkOnly = process.argv.includes("--check");

  if (checkOnly) {
    const current = fs.existsSync(OUT_PATH) ? fs.readFileSync(OUT_PATH, "utf8") : null;
    if (current !== output) {
      process.stderr.write(`tokens.css is out of date with tokens.json: ${OUT_PATH}\n`);
      process.exitCode = 1;
      return;
    }
    process.stdout.write("tokens.css matches tokens.json\n");
    return;
  }

  fs.writeFileSync(OUT_PATH, output);
  process.stdout.write(`wrote ${path.relative(ROOT, OUT_PATH)}\n`);
}

main();
