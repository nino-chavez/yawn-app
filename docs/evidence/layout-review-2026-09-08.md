# Yawn layout review — 2026-09-08

The reviewed screens now keep text inside its reading area and leave controls
reachable. The sweep found and fixed title collisions, horizontal overflow,
clipped source previews, and cramped transcript and Trash layouts. No material
layout defect remains in the reviewed states and window sizes.

This is a review of the current UI rendered in **native macOS WebKit with
synthetic data**. It is not acceptance of the older installed Yawn build.
Recording quality and the previously deferred note-quality check remain separate.

## What changed

| Observed problem | Correction |
| --- | --- |
| An unbroken meeting title ran through Rename and Manage. The inspector made the title column particularly narrow. | Titles can wrap within their column; actions move below when needed. The inspector view uses smaller document side insets. |
| Long tokens escaped note text, transcript text, source previews, sidebar errors, startup errors, and notifications. | Reading and message surfaces permit emergency wrapping. Normal words retain their usual wrapping. |
| The transcript toolbar could leave only a sliver for reading at the minimum window size. | The transcript has a minimum reading area, while the outer document remains scrollable. |
| The hidden transcript-search label extended the page beyond the app window. | Its positioned container now keeps it inside the reading pane. |
| Restore this turn overflowed its narrow grid cell. | The action occupies its own row with room for the full label. |
| Trash had no content inset or usable list scrolling. Long names pushed Restore offscreen. | Trash has document spacing and vertical scrolling. Names wrap; Restore keeps its width. |
| The empty-library invitation made the sidebar scroll sideways. | Its button wraps inside the sidebar. |
| A source preview near the bottom opened outside the window. | It opens above the claim when needed and remains within the window; unusually long excerpts can scroll. |
| Try Apple speech again touched the download card below it. | The retry action has a separating gap. |

The changes preserve the existing colors, typography, navigation, and data
operations. Compact sidebar and toolbar titles still shorten deliberately;
full titles are available in the document. Single-line edit fields still scroll
inside their own bounds. Long dialogs and lists use intentional vertical scrolling.

## Coverage

The source inventory includes the main window, Settings, custom dialogs, and
transient overlays. The reproducible [case list](../../apps/desktop/ui-harness/layout/cases.json)
contains 77 synthetic states and 232 native captures. These are coverage counts,
not a measure of user experience.

- **Library:** first run, empty, populated, loading, stalled, unavailable,
  title-search empty results, transcript-search results, no matches, incomplete
  coverage, and empty/populated Trash.
- **Meeting:** transcript-only, generated note, long title/text, source inspector,
  source preview, locked/unavailable/recovered states, generation in progress,
  unavailable generation, saved-audio controls, and notices/errors.
- **Transcript:** full reader, corrected channel label, withheld turn and Restore,
  vocabulary empty/populated/delete confirmation, and retry comparison with a
  generated note, without one, and with word differences unavailable.
- **Capture:** arming, recording, paused, pause/resume pending, stopping,
  captured, transcribing, summarizing, completed transcript, both processing
  failures, interrupted recovery, and expanded meeting context.
- **Setup:** browser-only notice, startup check/error, model choice, download,
  verification/failure, and Apple preparation required/in progress/failed.
- **Dialogs:** Start with normal/guided/denied access, Rename, channel label,
  delete recording/transcript/meeting, lock/unlock, and unavailable confirmation.
- **Settings:** all four sections, authorized/denied permissions, Apple assets
  missing/failed/installing, model downloads/errors, and no note model installed.

Every state was captured in light and dark appearance at the supported minimum:
900 × 600 for the main window and 560 × 480 for Settings. The main document,
inspector, transcript, title, and Start dialog also have 1080 × 900 captures;
Settings also has 720 × 720 captures. Long dialogs, Trash, notes, and Settings
sections include scrolled captures to expose lower controls.

The root reviewer opened rendered frames across every screen family. An
independent reviewer also inspected the corrected native reading surfaces,
Trash, dialogs, and Settings. Deliberately unbroken synthetic words wrap mid-word;
that is the fallback for such input, not a new typography treatment.

## Verification and evidence

All 232 native capture cases completed successfully. A final 30-case recheck
covered the corrected reading surfaces, popup, startup error, empty library,
and Apple retry spacing. The recheck also asserted pane overflow, title/action
separation, popup bounds, and absence of page scrolling outside the main panes.
The full sweep preceded the last Apple-spacing adjustment; the recheck contains
its final captures. The [receipts and source hashes](screen-reviews/captures/layout-2026-09-08/checks.json)
identify which pass produced each retained result.

Existing checks passed: `npm run test:ui`, `npm run tokens:check`, and the native
`ui-harness/run.sh all`, `search`, and `fidelity` checks. The layout review uses
real UI markup and rendering code. The local server replaces backend responses
with invented meeting state and disables periodic polling for stable captures.
It does not read meeting storage, start recording, download models, or grant
permissions. Screenshots establish appearance; the existing interaction tests
establish only their named synthetic scenarios.

Representative corrected native frames:

- [Long title](screen-reviews/captures/layout-2026-09-08/long-title-dark-900x600.png), [source inspector](screen-reviews/captures/layout-2026-09-08/inspector-light-900x600.png), [transcript](screen-reviews/captures/layout-2026-09-08/transcript-light-900x600.png), [Restore turn](screen-reviews/captures/layout-2026-09-08/transcript-withheld-light-900x600.png).
- [Trash](screen-reviews/captures/layout-2026-09-08/trash-light-900x600.png), [source preview](screen-reviews/captures/layout-2026-09-08/popover-dark-900x600.png), [Apple retry spacing](screen-reviews/captures/layout-2026-09-08/apple-failed-light-900x600.png).
- [Start dialog bottom](screen-reviews/captures/layout-2026-09-08/start-denied-dark-900x600-bottom.png), [retry decisions](screen-reviews/captures/layout-2026-09-08/retry-light-900x600-bottom.png), [capture context](screen-reviews/captures/layout-2026-09-08/capture-context-dark-900x600.png).
- [Transcription settings](screen-reviews/captures/layout-2026-09-08/settings-dark-560x480-transcription.png), [note settings](screen-reviews/captures/layout-2026-09-08/settings-light-560x480-notes.png), [storage settings](screen-reviews/captures/layout-2026-09-08/settings-dark-560x480-storage.png).

Before-fix browser frames are retained for [title collision](screen-reviews/captures/layout-2026-09-08/before-browser-long-title-light.png), [Trash](screen-reviews/captures/layout-2026-09-08/before-browser-trash-light.png), [transcript](screen-reviews/captures/layout-2026-09-08/before-browser-transcript-light.png), and [preview clipping](screen-reviews/captures/layout-2026-09-08/before-browser-popover.png).
The before and after sets use different engines; they are evidence of the named
layout failures and corrections, not a pixel-difference comparison.

Full capture sets remain local under `.artifacts/layout-audit-2026-09-08/final`
and `recheck`. Selected frames and all native case receipts are retained here.
Reproduce on macOS with Swift, Python, and Pillow available:

```sh
python3 apps/desktop/ui-harness/layout/run.py
# Narrow recheck:
python3 apps/desktop/ui-harness/layout/run.py --scenes transcript,trash,popover
```

The harness compiles the existing native runner, starts an isolated loopback
server, captures the requested states, and shuts its server down. Any failed
layout assertion returns a nonzero exit code. The harness also checks the applied
scene identity and records hashes of the UI, fixture files, and native runner,
alongside the source commit. These provenance checks passed in an additional
14-case run covering Settings, the source popup, and the browser-only notice;
the earlier full-sweep receipts predate that added identity check.

## Acceptance boundary

This closes the enumerated UI layout review at the stated window sizes. It does
not prove every possible user string, system text-size setting, macOS-owned
Touch ID/permission/file-picker dialog, or interaction with live meeting data.
The installed `/Applications/Yawn.app` was not replaced. Installing and checking
that build remains a separate acceptance step; no release or push occurred here.
