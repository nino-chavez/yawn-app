# Ledger fold expansion and recording choice — frames, 2026-10-16

Rendered by the harness's WKWebView runner from `apps/desktop/ui` against the
`ledger` stub (invented data), 1080 by 900 at 2x. These are not captures of
the installed build.

| File | State | Command (from `apps/desktop`) |
|---|---|---|
| `harness/ledger-light.png` | folds closed, light | `HARNESS_APPEARANCE=light HARNESS_CAPTURE_DIR=… bash ui-harness/run.sh still ledger` |
| `harness/ledger-dark.png` | folds closed, dark | `HARNESS_APPEARANCE=dark … still ledger` |
| `harness/ledger-fold-light.png` | first two folds opened by pressing them, light | `HARNESS_APPEARANCE=light … still-fold` |
| `harness/ledger-fold-dark.png` | first two folds opened, dark | `HARNESS_APPEARANCE=dark … still-fold` |

Neutral names given to the reviewers, per pass:

| Pass | closed light | closed dark | open light | open dark |
|---|---|---|---|---|
| 1 | f4 | f2 | f1 | f3 |
| 2 | g3 | g1 | g2 | g4 |
| 3 | h2 | h4 | h3 | h1 |
| 4 | k1 | k3 | k4 | k2 |

The runner's page is hidden (`document.visibilityState` is `hidden`), so CSS
animations never advance there; `still-fold` finishes them before the frame.
Pass 1's frames were captured before that fix and happened to settle; the
first pass-2 capture did not (the opened turns at opacity 0), was caught by
the implementer before any reviewer saw it, and was re-captured.
