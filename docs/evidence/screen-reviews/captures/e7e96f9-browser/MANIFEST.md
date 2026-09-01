# Browser-rendered captures — e7e96f9

## What these are

Screenshots of Yawn's real desktop frontend (`apps/desktop/ui/index.html`,
`main.js`, `styles.css`, `settings.html`, `settings.js`, `settings-window.css`
— all unmodified), rendered in Chrome instead of the packaged Tauri app,
because the Mac's screen was locked and the installed app could not be
screenshotted directly.

**These are not the installed app.** A harness under
`apps/desktop/ui/review/` (dev-only, never shipped — see that directory's own
header comments) defines `window.__TAURI__` before the real `main.js` /
`settings.js` load, and answers every `invoke()` call with an invented,
synthetic fixture instead of a live Tauri backend. No real meeting, recording,
transcript, or model data exists anywhere in this repo or on this Mac for
these screenshots — every name, quote, and timestamp below was written for
this harness and does not correspond to anything said or recorded.

- Commit: `cdeb599e3318aab8f04f72d99717d1a9abad1490` (branch `feat/yawn-direction`,
  captured from worktree branch `yawn-captures`)
- Window size: 960×900 (main window), 720×720 (Settings window) — logical
  points, matching `apps/desktop/src-tauri/tauri.conf.json`'s `windows[0]`
  entry (`width`/`height`) and the Settings `WebviewWindowBuilder` in `main.rs`.
  Captured via Chrome DevTools Protocol `Emulation.setDeviceMetricsOverride`
  at `deviceScaleFactor: 1`, but Chrome's screenshot capture ignored that
  factor and rendered at the host display's actual backing scale (2x), so
  every PNG below is 1920×1800 (main) or 1440×1440 (Settings) — double the
  logical size, same as an ordinary Retina screenshot. No cropping or
  resampling was applied; scale by 2 to get logical points.
- Harness: `apps/desktop/ui/review/harness.html` (main window) and
  `apps/desktop/ui/review/settings-harness.html` (Settings window), both
  loading `apps/desktop/ui/review/harness.js` before the real app script.
  Served locally with the `preview` helper; captured with `browse-tool`
  (`browse-nav`, `browse-eval` for clicks/scrolls, `browse-cdp` for the
  viewport override, `browse-screenshot`).

## Captures

| File | State | Fixture / interaction |
|---|---|---|
| `01-home-first-run.png` | Home, first run — zero meetings, three-moments sheet showing | `?state=home-first-run`: empty library, `firstRunSheetSeen: false`. No interaction; the sheet shows automatically per `firstRunSheetVisible()`. |
| `02-home-with-meetings.png` | Home with meetings — rows with note previews | `?state=home-with-meetings`: 3 synthetic library rows (one untitled/date-labeled, one locked with no preview). |
| `03-during-capture.png` | During capture — canvas, recording state, pause control | `?state=during-capture`: `app_snapshot.capture: "recording"`, 7:02 elapsed, synthetic operator note and meeting-context text loaded. |
| `04-after-note.png` | After — landing view on opening a finished meeting | `?state=after-note`, then a scripted click on the first meeting row (`[data-action="open-meeting"]`). This is the page's natural top-of-scroll state. |
| `04b-after-note-scrolled-to-note.png` | After — the generated note (Overview, Decisions, Follow-ups, Open questions, "Show source" links) | Same interaction as 04, then scrolled to `.meeting-note`. See note below — this content is *not* the first thing visible in 04. |
| `05-withheld-turn.png` | A withheld transcript turn | Same meeting opened, then the "Full transcript" `<details>` opened programmatically and scrolled to the withheld turn — renders "This turn was withheld by the voice check." with a "Restore this turn" action. |
| `06-needs-attention.png` | Needs-attention — a real failure state | `?state=needs-attention`: `app_snapshot.startup: "error"`, `error: "Yawn could not verify its local recording engine after the last restart."` |
| `07-settings-active-model.png` | Settings with the active model | Settings window harness; `transcript_model_settings` fixture reports the compact model as active/in use, one inactive full model available; `note_model_settings` reports one active local note model. Top of the Settings window (Speech model section, "Compact model — In use") is what's in frame; Note model and Audio access sections are below the fold at 720×720. |
| `08-home-with-meetings-large-text.png` | Platform accessibility: large text | Same fixture as 02, with `document.documentElement.style.zoom = "1.25"` applied (the app defines no text-size preference of its own — confirmed by grep, no match for any text-size/zoom mechanism in `main.js`/`view-model.mjs`). |
| `09-home-with-meetings-increased-contrast.png` | Platform accessibility: increased contrast | Same fixture as 02, with CDP `Emulation.setEmulatedMedia` forcing `prefers-contrast: more`. |

## States not representable, and why

None of the roster's seven named states plus two accessibility variants were
unrepresentable — all nine were captured. Two things are worth flagging about
how they were captured, factually, not as design commentary (that's the blind
reviewer's job):

- **`04-after-note.png` and `04b-after-note-scrolled-to-note.png` are the same
  app state at two scroll positions**, not two different states. The meeting
  detail page's "Meeting workspace" is a CSS grid with the notes/context aside
  and the generated note as siblings; `apps/desktop/ui/styles.css`'s
  `@media (max-width: 980px)` rule (the window's default 960px width is under
  that threshold) sets `order: -1` on the aside, so the aside (Listen to saved
  audio / Meeting context / Your notes) renders before the generated note in
  document flow at this window size. 04 is what's on screen at the page's
  natural landing scroll position (top); 04b is scrolled down to bring the
  note's Overview/Decisions/Follow-ups/Open-questions and "Show source" links
  into frame, since the roster asked specifically for that content.
- **`09-home-with-meetings-increased-contrast.png` is byte-for-byte identical
  to `02-home-with-meetings.png`** (confirmed by SHA-256 checksum). Grepping
  `styles.css` and `settings-window.css` for `prefers-contrast` finds no rule
  in either file — the app currently has no CSS response to that media
  feature, so the harness applied the emulation correctly but there was
  nothing on the page for it to change.

## Fixture-shape corrections made during capture (harness bugs, not app bugs)

Two fixture-data mistakes in `harness.js` were caught and fixed while
capturing, both purely in the synthetic fixture, not the app:

1. The synthetic transcript SHA-256 was 63 hex characters instead of 64,
   which failed `withheldTurnPresentation()`'s format check in
   `view-model.mjs` and silently hid the "Restore this turn" action.
2. The synthetic `capturePauses` field used the wrong shape
   (`{ pauses: [] }` instead of `{ state: "not-paused" }`), which made
   `capturePausePresentation()` fall through to its "Pauses could not be
   checked" branch and showed a spurious warning banner on every finished
   meeting.

Both are fixed in the committed `harness.js`.
