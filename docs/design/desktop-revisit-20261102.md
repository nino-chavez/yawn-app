# Yawn desktop: Current and Refined Rail

Design intent: **refit**, within the selected [Rail direction](../../decisions/family-direction-2026-10-15.md). Nino asked to revisit the Profile site, then the Yawn desktop app, on November 2. The rest of the fleet is deferred. This is a review comparison, not an app release or a new direction selection.

## Surfaces

- After a meeting: notes and nearest transcript, resting, related-passage, and expanded-fold states.
- During recording: note canvas and recording controls.
- First run, needs attention, and Settings as preservation checks.
- Native WebKit content captures at 1080×900 in light and dark; Settings at 580×640.
- Interactive frontend at 1080×900 and 960×760 with synthetic meetings.

## Recommendation

Keep the existing warm paper, graphite, red recording, amber attention and blue moment colors. They already have distinct jobs. Make an existing note/transcript relationship easier to see when a visitor inspects it; do not recolor the whole window merely to create a new palette.

The visitor wants to stay in a meeting, write their own notes, then check a detail against retained speech. The screen should feel quiet, private, readable, precise and responsive to inspection. It should avoid decorative pastel fields, blue on every control, a task dashboard, duplicated note editors and implied audio seeking.

## Preserve

Keep the selected sidebar-plus-document structure, serif meeting title, the operator's notes as the largest text, the single native textarea and its caret/undo history. Preserve every word, source passage, existing control action, fold state, focus order, retained-audio rule and product boundary. No generated-note feature returns.

The current layout intentionally moves transcript blocks down to avoid collision while leaving the editor fixed. A difference in vertical position is not, by itself, a layout defect. The proposed cues preserve this behavior rather than splitting the editor into row-sized fields.

## Proposed changes

- Existing transcript hover and disclosure focus reveal the corresponding note line and rail mark using the existing moment blue and its soft tint. The mirror remains `aria-hidden` and pointer-inert; the note stays one textarea. No new keyboard stop or backend command is added.
- Disclosure buttons have a 22px minimum height, larger than the current 16px captured control. The existing layout measures the new control height and keeps transcript blocks separate.
- A faint guide separates transcript evidence from the primary writing column without adding a surrounding card.

Nothing is removed, combined or hidden. Transcript evidence remains secondary to personal notes; disclosure content still opens and closes through the original controls. Recording and attention colors retain their meanings. This adds no audio-seeking action to the rail.

## Interaction and motion

Hover a retained transcript passage, or focus its existing disclosure, to reveal the relationship. Enter opens a disclosure; Escape closes it and returns focus. Moving to the note editor removes the inspection cue. Existing disclosure motion and reduced-motion behavior remain; no transition or animation is added.

## Evidence limits

`apps/desktop/ui/` and the Rust backend are unchanged. All comparison code lives in `ui-harness/revisit/` plus the separate review runner. Native frames use the same frontend in an offscreen WKWebView with synthetic command responses, not the installed app or real meetings. Related native frames explicitly pose the visual relationship state because the hidden window cannot receive physical hover or keyboard focus. Actual hover, keyboard focus, fold behavior, note editing and undo are exercised separately in the browser.

The comparison does not prove audio capture, playback delivery, native storage, installed-app acceptance or release readiness. The untouched Current render is retained in both appearances. Selection and any later production implementation remain separate.

## Local comparison — November 3

[Open Current vs proposed](http://127.0.0.1:8876/). Switch between the two designs, Light and Dark, and the meeting scenes. **Native captures** shows rendered WKWebView content. **Try interactions** lets the reviewer hover transcript passages, focus and open disclosures, edit notes, use native undo, and search synthetic meeting titles. A smaller 960 × 760 window is available in the interactive view.

The review uses the existing semantic colors: graphite for chrome and writing, blue for a note’s moment, red for recording, amber for attention. The proposal makes the blue correspondence visible on inspection. It does not add another theme.

## Reproduction and checks

`apps/desktop/ui-harness/revisit/preview.py` serves a flat review origin and can regenerate all 28 native content frames with `--capture`. It requires `--out` for a persistent capture directory. Subsequent starts verify the recorded source hashes before showing the captures. The server binds only to loopback and uses synthetic responses; it never invokes a native app command. The additional review stub implements title filtering with the existing `library_snapshot` fields (`total`, `shown`, `filterActive`), retaining the unfiltered total.

`revisit/verify-browser.mjs` requires an available `@playwright/test` installation, resolved normally or through `NODE_PATH`; `YAWN_REVIEW_OUT` sets its persistent receipt directory. The browser checks cover all native-frame choices, actual hover and disclosure focus, expansion and Escape, one textarea’s editing and undo across polling, synthetic title search and empty results, wrapped note correspondence at 960px, reduced-motion disclosure behavior, and the comparison’s narrow wrapper. These checks do not establish screen-reader or installed-app acceptance.

On November 3, the project’s 215 UI tests passed, 28 native content captures passed, and 34 browser records passed. The first two browser attempts exposed cursor-placement and undo-grouping assumptions in the test; the search attempt exposed a fixture that ignored title filters. The promoted test uses explicit selection and one native text insertion for its undo unit. Production source remains unchanged.

The cold review accepted the meeting states and found two blank Settings captures. The separate Settings review page imported the settings module without mounting the current window host. It now uses `settings-window.js`, includes the standalone stylesheet, and fails capture when the settings groups do not mount. Both Current and proposed Settings use this same corrected host. The original review and focused verification are retained under `docs/evidence/screen-reviews/desktop-revisit-20261103*`.

The focused Settings verification accepted both corrected appearances. This completes the prototype’s cold rendered review; it does not approve the proposal for product integration. Current and rejected comparison states remain available.

Nothing is removed, combined or hidden in the proposal. The study demotes transcript emphasis until inspection, while preserving the note editor, disclosures and controls. Any later integration needs a separate source change, app checks and installed-app acceptance.
