# ui-harness

Behavioral verification for the frontend's editor-preservation contract: undo
history, focus, and selection in the `ui/` text fields must survive `render()`.
The harness runs the real `ui/` frontend in a WKWebView — the same engine the
packaged Tauri app uses — with `tauri-stub.js` standing in for the Rust
backend. It exists because `npm run test:ui` runs without a DOM, so nothing
there can catch a render loop that destroys editor nodes.

This is a manual check, not CI. Run it after changing `render()`,
`dom-patch.mjs`, or anything that rebuilds DOM around an editable field. It
was built for the D9 native-text audit's finding 3, where each 900 ms poll
tick recreated the focused textarea and silently reset WebKit's per-element
undo stack.

## run.sh

    ./run.sh [capture|library|smoke|all]

Serves the real `../ui/` files plus the harness page from a temporary local
HTTP origin, compiles `runner.swift`, drives the scenario, and prints a JSON
result. Requires the Xcode toolchain (`swiftc`) and python3.

- `capture` — fakes an active recording, types into the operator-note
  textarea via `execCommand("insertText")` (registers with the same editing
  undo stack cmd-Z hits), waits past two poll ticks, then undoes. Healthy:
  `sameNodeAfterTick`, `focusedAfterTick` true, and `valueAfterUndo1` shows
  the typed text reverting. `controlUndoWorks` proves undo works at all
  before any tick, so a failure after the tick is meaningful.
- `library` — fakes one finished meeting, opens it, types into transcript
  search (each keystroke re-renders synchronously). Healthy:
  `sameNodeAfterKeystroke`, `focusedAfterKeystroke`,
  `detailsOpenAfterKeystroke` true, both keystrokes present in `valueTyped`,
  and undo reverting.
- `smoke` — walks interactive flows (start-sheet attestations and retention
  select, meeting note autosave, rename and vocabulary sheets, view
  switches) and reports per-step booleans plus any page errors collected by
  the stub. Healthy: every step `ok`, `errors` empty.
- `all` — the three in sequence.

## Files

- `runner.swift` — opens a WKWebView, loads the harness page, executes the
  scenario body via `callAsyncJavaScript`, prints its JSON return. A window
  appears briefly; the scenario needs it key for editing commands.
- `harness.html` — `ui/index.html` with the stub loaded as a classic script
  before the `main.js` module.
- `tauri-stub.js` — fake `window.__TAURI__` bridge. `?mode=capture` serves an
  active-recording snapshot; `?mode=library` serves an idle snapshot with one
  finished meeting and a two-turn transcript. Also collects page errors on
  `window.__errors`.
- `scenario.js`, `smoke.js` — scenario bodies for the runner.

## Limits

The Rust backend, real capture, and the packaged app are not exercised —
command responses are canned, so this proves frontend render behavior only.
WebKit coalesces consecutive insertions, so one undo may revert a typed run
rather than a single character; that matches native macOS text fields. The
stub must grow alongside any new Tauri command the polled render path calls,
or scenarios fail with `harness: unstubbed command <name>`.
