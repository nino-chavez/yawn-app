# Carry the proposed desktop Rail refinement into Yawn

- Date: 2026-11-03
- Status: Selected for frontend integration
- Scope: Mac desktop note/transcript inspection; existing Rail layout and colors

Nino said “go” after the Current vs proposed comparison and recommendation to
carry Proposed forward. The comparison source is [draft PR 167](https://github.com/nino-chavez/yawn-app/pull/167),
commit `473e6ce84ce6da6ae126287b3e8733e596da4f5d`. It remains separate from the
bundled frontend change.

Use existing Moment blue to connect an inspected transcript passage with its
note and elapsed-time mark. Enlarge the existing disclosures and add the quiet
transcript guide. Keep the single textarea, native editing and undo, note
moments, disclosure state, keyboard order, and existing recording/attention
colors. Nothing is removed, combined or hidden. No new motion, seeking action,
storage contract or generated-note feature is introduced.

The implementation uses external CSS and recomputes inspected lines from the browser’s hover/focus state after layout and polling. It preserves the packaged CSP and installs no comparison observer. The original Current view remains
the baseline; no alternate palette or unrelated screen structure is adopted.

Source integration, offscreen native content checks, signed fixture acceptance,
and a public app release are separate evidence states. This decision approves
the frontend refinement; it is not a receipt for installing or publishing a new
Yawn release.
