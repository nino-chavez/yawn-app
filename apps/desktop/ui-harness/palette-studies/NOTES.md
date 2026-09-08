# Softer dark palettes

Status: Nino selected Lavender Haze on 2026-09-08. Applied to production dark
tokens in this working tree; these captures preserve the original comparison.
Baseline: production dark tokens at `02f8500`, with the current layout fixes.

The comparison keeps Yawn's layout, typography, content, spacing, and navigation
identical. It varies only color. The aim is less contrast between surfaces,
gentler white text, and quieter accents while retaining distinct record,
attention, selection, and transcript-evidence roles.

1. **Lavender Haze:** the closest continuation of today's Yawn. Dusty lavender,
   lifted violet charcoal, and softened white. Recommended for continuity.
2. **Warm Graphite:** the warmest direction. Brown-gray surfaces, oatmeal text,
   and clay accents. The warning surface has less color separation from the
   surrounding palette, though its icon and structure remain distinct.
3. **Blue Hour:** cool slate and mist blue. Calm and airy, with a larger shift
   away from Yawn's violet character.

Open `index.html` through a local server to compare each option with Current.
The screen selector covers Meeting + source, Settings, and Needs attention.
Uncheck Compare with current for the larger single-screen view.

All screenshots are native WKWebView renders of the production UI with synthetic
content. The root reviewer opened all meeting variants and a secondary-screen
montage; an independent reviewer opened all twelve screen variants and found
no material loss of action or role legibility. Contrast checks in `contrast.json`
cover specified foreground/background token pairs only; they do not establish
whole-app accessibility, including inherited dimmed and disabled states.

`palettes.json` owns these candidate values. The selected values now live in production tokens; candidate snapshots remain
historical. Production also uses dark ink on the live rose Record badge.
`study.js` owns the shared synthetic scene. `capture.py` compiles the existing
native runner and captures every candidate on three screen states. Run it from
macOS with Swift, Python, and Pillow available:

```sh
python3 apps/desktop/ui-harness/palette-studies/capture.py
```

The comparison is a historical color study. Viewing or switching tabs does not
change Yawn. Source changes and installation remain separate states.

Production validation: 26 native render checks passed, followed by 10 Settings
rechecks after removing a hardcoded white button label. The final Settings
primary action uses `onAccent`; the live Record badge uses `onRecord`.
Their dark text contrast ratios are 7.03:1 and 7.37:1 respectively. Existing
light values and layout/type tokens were verified unchanged. Selected final
frames and receipts are in `production-captures/`; the check summary is
`production-validation.json`. These are synthetic renders, not installed-app
acceptance.
