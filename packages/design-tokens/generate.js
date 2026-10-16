#!/usr/bin/env node
/*
 * Generates apps/desktop/ui/tokens.css and the iPad app's color catalog
 * (apps/ipad/Yawn/Paper/Paper.xcassets) from tokens.json.
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
  moment: c.moment.light, onMoment: c.onMoment.light, momentSoft: c.momentSoft.light,
  rail: c.rail.light, railMark: c.railMark.light, recordSoft: c.recordSoft.light, inkDim: c.inkDim.light,
  accent: c.accent.light,
  onAccent: c.onAccent.light,
  record: c.record.light,
  onRecord: c.onRecord.light,
  attention: c.attention.light,
  attentionBg: c.attentionBackground.light,
  evidence: c.evidence.light,
  ...Object.fromEntries(Object.entries(c.neutral).map(([k, v]) => [k, v.light])),
};
const dark = {
  moment: c.moment.dark, onMoment: c.onMoment.dark, momentSoft: c.momentSoft.dark,
  rail: c.rail.dark, railMark: c.railMark.dark, recordSoft: c.recordSoft.dark, inkDim: c.inkDim.dark,
  accent: c.accent.dark,
  onAccent: c.onAccent.dark,
  record: c.record.dark,
  onRecord: c.onRecord.dark,
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
   Concept A owns the window structure; Rail, selected 2026-10-15, owns the
   visual system in both appearances (the paper language, the time rail, the
   moment color). Generated from packages/design-tokens/tokens.json; do not
   edit this file directly. */
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
  --toolbar-h: ${layout.toolbar}; --sidebar-w: ${layout.sidebar}; --control-h: ${layout.control.default}; --control-h-secondary: ${layout.control.secondary};
  --motion: ${motion.default};
  /* Spacing scale (visual refit 2026-09-03): the rhythm styles.css's 31
     ad hoc margin/padding/gap values collapsed onto. 32 is the one addition
     to DESIGN.md's original 4/6/8/12/16/18/24/48 -- three legacy sheet
     values (28, 32, 38px) clustered there and would otherwise lose 14-37%
     snapping straight to 24 or 48. */
  ${space}

  --window: ${light.window}; --sidebar: ${light.sidebar}; --content: ${light.content}; --panel: ${light.panel};
  --notes: ${light.notes}; --notes-border: ${light.notesBorder};
  --separator: ${light.separator}; --separator-strong: ${light.separatorStrong};
  --label: ${light.label}; --label-2: ${light.label2}; --label-3: ${light.label3}; --label-4: ${light.label4};
  --control: ${light.control}; --control-border: ${light.controlBorder}; --control-shadow: ${light.controlShadow};
  --selection: ${light.selection}; --selection-active: ${light.selectionActive}; --selection-line: ${light.selectionLine}; --on-selection: ${light.onSelection};
  --accent: ${light.accent}; --on-accent: ${light.onAccent};
  --record: ${light.record}; --on-record: ${light.onRecord};                 /* the only red on screen; spent by Record and the live state */
  --attention: ${light.attention}; --attention-bg: ${light.attentionBg};
  --evidence: ${light.evidence};  /* the current turn's surface; the same tint as --moment-soft */
  --moment: ${light.moment}; --on-moment: ${light.onMoment}; --moment-soft: ${light.momentSoft};  /* the playhead, a tapped line, the current turn */
  --rail: ${light.rail}; --rail-mark: ${light.railMark}; --record-soft: ${light.recordSoft}; --ink-dim: ${light.inkDim};
}
:root[data-theme="dark"] {
  --window: ${dark.window}; --sidebar: ${dark.sidebar}; --content: ${dark.content}; --panel: ${dark.panel};
  --notes: ${dark.notes}; --notes-border: ${dark.notesBorder};
  --separator: ${dark.separator}; --separator-strong: ${dark.separatorStrong};
  --label: ${dark.label}; --label-2: ${dark.label2}; --label-3: ${dark.label3}; --label-4: ${dark.label4};
  --control: ${dark.control}; --control-border: ${dark.controlBorder}; --control-shadow: ${dark.controlShadow};
  --selection: ${dark.selection}; --selection-active: ${dark.selectionActive}; --selection-line: ${dark.selectionLine}; --on-selection: ${dark.onSelection};
  --accent: ${dark.accent}; --on-accent: ${dark.onAccent}; --record: ${dark.record}; --on-record: ${dark.onRecord};
  --attention: ${dark.attention}; --attention-bg: ${dark.attentionBg};
  --evidence: ${dark.evidence};
  --moment: ${dark.moment}; --on-moment: ${dark.onMoment}; --moment-soft: ${dark.momentSoft};
  --rail: ${dark.rail}; --rail-mark: ${dark.railMark}; --record-soft: ${dark.recordSoft}; --ink-dim: ${dark.inkDim};
}
html, body { margin: 0; height: 100%; }
body { font: var(--t-body)/var(--lh-body) var(--font); color: var(--label); background: var(--window);
  -webkit-font-smoothing: antialiased; }
* { box-sizing: border-box; }
button, input { font: inherit; color: inherit; }
.btn { display: inline-flex; min-height: var(--control-h); align-items: center; justify-content: center; padding: 0 10px; border-radius: var(--r-control); background: var(--control);
  border: 1px solid var(--control-border); box-shadow: var(--control-shadow); cursor: default; font-size: 12px; font-weight: 500; line-height: 20px; white-space: nowrap; color: var(--label); }
.btn.primary { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
.btn.record { color: var(--record); font-weight: 600; }
.btn.record.live { background: var(--record); color: var(--on-record); border-color: transparent; }
.caption { font-size: var(--t-caption); color: var(--label-2); }
.tertiary { color: var(--label-3); }
.hairline { border-top: 1px solid var(--separator); }
.search { height: 24px; width: 170px; border-radius: 6px; border: 1px solid var(--control-border);
  background: var(--control); padding: 0 8px 0 24px; color: var(--label-2);
  background-image: none; position: relative; }
.search::before { content: "⌕"; position: absolute; left: 7px; top: 1px; color: var(--label-3); }
.read { font-size: var(--t-read); line-height: var(--lh-read); max-width: var(--measure); }
/* Canvas keeps section labels compact while the 16px prose carries the note. */
.read h2 { font-size: var(--t-body); line-height: var(--lh-body); font-weight: 600; color: var(--label); text-transform: none; letter-spacing: 0; margin: 20px 0 6px; }
.read p, .read li { margin: 0 0 6px; }
.claim { border-bottom: 1px dashed var(--separator-strong); cursor: default; }
.claim.open { background: var(--evidence); border-bottom-color: var(--accent); }
.traffic { display: inline-flex; gap: 8px; margin-right: 12px; }
.traffic i { width: 12px; height: 12px; border-radius: 50%; display: inline-block; }
.traffic i:nth-child(1){background:#ff5f57}.traffic i:nth-child(2){background:#febc2e}.traffic i:nth-child(3){background:#28c840}
.fixture-tag { position: absolute; right: 8px; bottom: 6px; font-size: 10px; color: var(--label-3); font-family: var(--font-mono); }

/* Focus, pressed, disabled — one rule each, system-shaped. */
:focus-visible { outline: 2px solid var(--moment); outline-offset: 1px; border-radius: var(--r-control); }
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

// -- The iPad asset catalog ---------------------------------------------------
//
// DESIGN.md Color: "the SwiftUI app carries the same values as named colors
// in its asset catalog". Generated here so the two apps cannot drift: one
// color set per role, light and dark, named Paper<Role>. Xcode generates a
// ColorResource symbol per set (`.paperPage`), and apps/ipad/Yawn/Paper/Paper.swift
// is the one place that maps those symbols to SwiftUI colors; views use
// Paper.page, never a literal.
const CATALOG_DIR = path.join(ROOT, "apps", "ipad", "Yawn", "Paper", "Paper.xcassets");
const CATALOG_SETS = [
  // [asset name, token, increased-contrast token?] -- the DESIGN.md role each
  // one carries. A third entry gives the set high-contrast variants, which
  // iPadOS picks under Increase Contrast (DESIGN.md Accessibility: separators
  // and Label 3 darken, no new colors); the Mac reads the same values from
  // tokens.css under prefers-contrast.
  ["PaperWindowChrome", c.neutral.window],      // Window chrome
  ["PaperSidebar", c.neutral.sidebar],          // Sidebar
  ["PaperPage", c.neutral.content],             // Page
  ["PaperPanel", c.neutral.panel],              // Panel
  ["PaperHairline", c.neutral.separator, c.contrastMore.separator], // Hairline
  ["PaperHairlineStrong", c.neutral.separatorStrong, c.contrastMore.separatorStrong], // Hairline, strong
  ["PaperLabel", c.neutral.label],              // Label
  ["PaperLabel2", c.neutral.label2, c.contrastMore.label2], // Label 2
  ["PaperLabel3", c.neutral.label3, c.contrastMore.label3], // Label 3
  ["PaperLabel4", c.neutral.label4],            // Label 4
  ["PaperGraphite", c.accent],                  // Graphite (accent)
  ["PaperOnGraphite", c.onAccent],              // On graphite
  ["PaperSelection", c.neutral.selection],      // Selection
  ["PaperControl", c.neutral.control],          // Control
  ["PaperControlBorder", c.neutral.controlBorder], // Control border
  ["PaperRail", c.rail],                        // Rail
  ["PaperRailMark", c.railMark],                // Rail mark
  ["PaperRecord", c.record],                    // Record
  ["PaperOnRecord", c.onRecord],                // On record
  ["PaperRecordSoft", c.recordSoft],            // Record, soft
  ["PaperAttention", c.attention],              // Attention
  ["PaperAttentionSoft", c.attentionBackground], // Attention, soft
  ["PaperMoment", c.moment],                    // Moment
  ["PaperOnMoment", c.onMoment],                // On moment
  ["PaperMomentSoft", c.momentSoft],            // Moment, soft
  ["PaperInkDim", c.inkDim],                    // Ink, dim
];

// `#rrggbb` or `rgba(r,g,b,a)` -> the catalog's sRGB components.
function catalogComponents(value) {
  const hex = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(value.trim());
  if (hex) {
    return {
      alpha: "1.000",
      blue: `0x${hex[3].toUpperCase()}`,
      green: `0x${hex[2].toUpperCase()}`,
      red: `0x${hex[1].toUpperCase()}`,
    };
  }
  const rgba = /^rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*([\d.]+)\s*\)$/i.exec(value.trim());
  if (rgba) {
    return {
      alpha: Number(rgba[4]).toFixed(3),
      blue: `0x${Number(rgba[3]).toString(16).toUpperCase().padStart(2, "0")}`,
      green: `0x${Number(rgba[2]).toString(16).toUpperCase().padStart(2, "0")}`,
      red: `0x${Number(rgba[1]).toString(16).toUpperCase().padStart(2, "0")}`,
    };
  }
  throw new Error(`not a catalog color: ${value}`);
}

function colorSetJson(token, contrast) {
  const entry = (value, { dark = false, high = false } = {}) => {
    const appearances = [
      ...(dark ? [{ appearance: "luminosity", value: "dark" }] : []),
      ...(high ? [{ appearance: "contrast", value: "high" }] : []),
    ];
    return {
      ...(appearances.length ? { appearances } : {}),
      color: { "color-space": "srgb", components: catalogComponents(value) },
      idiom: "universal",
    };
  };
  const colors = [entry(token.light), entry(token.dark, { dark: true })];
  if (contrast) {
    colors.push(entry(contrast.light, { high: true }));
    colors.push(entry(contrast.dark, { dark: true, high: true }));
  }
  return JSON.stringify({
    colors,
    info: { author: "packages/design-tokens/generate.js", version: 1 },
  }, null, 2) + "\n";
}

// Color sets on disk that no token names any more. A leftover set still
// compiles, so Xcode keeps generating its symbol and a view can go on using
// a role the tokens no longer define; the check fails on one and the write
// removes it.
function staleCatalogSets() {
  if (!fs.existsSync(CATALOG_DIR)) return [];
  const expected = new Set(CATALOG_SETS.map(([name]) => `${name}.colorset`));
  return fs.readdirSync(CATALOG_DIR)
    .filter((entry) => entry.endsWith(".colorset") && !expected.has(entry))
    .map((entry) => path.join(CATALOG_DIR, entry));
}

// Every file the catalog consists of, keyed by path.
function renderCatalog() {
  const files = new Map();
  files.set(path.join(CATALOG_DIR, "Contents.json"), JSON.stringify({
    info: { author: "packages/design-tokens/generate.js", version: 1 },
  }, null, 2) + "\n");
  for (const [name, token, contrast] of CATALOG_SETS) {
    files.set(path.join(CATALOG_DIR, `${name}.colorset`, "Contents.json"), colorSetJson(token, contrast));
  }
  return files;
}

function main() {
  const output = render();
  const catalog = renderCatalog();
  const checkOnly = process.argv.includes("--check");

  if (checkOnly) {
    const current = fs.existsSync(OUT_PATH) ? fs.readFileSync(OUT_PATH, "utf8") : null;
    if (current !== output) {
      process.stderr.write(`tokens.css is out of date with tokens.json: ${OUT_PATH}\n`);
      process.exitCode = 1;
      return;
    }
    for (const [file, contents] of catalog) {
      const existing = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : null;
      if (existing !== contents) {
        process.stderr.write(`the iPad catalog is out of date with tokens.json: ${path.relative(ROOT, file)}\n`);
        process.exitCode = 1;
        return;
      }
    }
    const stale = staleCatalogSets();
    if (stale.length) {
      process.stderr.write(`the iPad catalog holds color sets tokens.json no longer names: ${stale.map((file) => path.relative(ROOT, file)).join(", ")}\n`);
      process.exitCode = 1;
      return;
    }
    process.stdout.write("tokens.css and the iPad catalog match tokens.json\n");
    return;
  }

  fs.writeFileSync(OUT_PATH, output);
  process.stdout.write(`wrote ${path.relative(ROOT, OUT_PATH)}\n`);
  for (const [file, contents] of catalog) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, contents);
  }
  for (const stale of staleCatalogSets()) {
    fs.rmSync(stale, { recursive: true });
    process.stdout.write(`removed ${path.relative(ROOT, stale)}\n`);
  }
  process.stdout.write(`wrote ${catalog.size} files under ${path.relative(ROOT, CATALOG_DIR)}\n`);
}

main();
