---
kind: cold
surface: Yawn desktop Rail refinement final frontend
build: Input hashes in source-manifest.json; frames in final-frames/manifest.json
reviewer: Codex worker 01a1a1e0-2e11-7771-9632-76ca96a8fe86; Operator dispatch 6a01a12e-f6e0-4cc0-b76a-5c48866f5b38
implementer: Codex parent chat 01a198a4-399c-7363-b9d1-babb0e76f147
cold: true
device: Offscreen WKWebView content on this Mac; 1080x900; default text; OS contrast unverified
states: Resting, related, expanded and recording; light and dark
verdict: All eight accepted; missing verified context noted; no material layout defect
---

| State | Light | Dark |
|---|---|---|
| Resting ledger | Accept | Accept |
| Related | Accept | Accept |
| Expanded | Accept | Accept |
| Recording | Accept, with missing context noted | Accept, with missing context noted |

**Actual attention order**

- **Resting:** meeting title → date/time, duration, and retention → paused-recording notice → playback/source controls → notes and nearest transcript.
- **Related:** selected note/time and its blue-linked transcript turn → nearby note/transcript context → the rest of the ledger.
- **Expanded:** selected note → expanded preceding turns → matching transcript turn → surrounding ledger.
- **Recording:** “Recording now” and elapsed time → timer, Pause, and Stop → meeting title → note prompt.

**Visible defects**

- No material layout defect in either appearance. The selected note/transcript relationship is immediately legible in the related and expanded frames; the expanded turns remain visually grouped.
- Dark-mode supporting text is deliberately subdued but remains readable in these captures; no contrast failure can be established from the stated unknown OS contrast.

**Missing verified context, not a layout or affordance defect**

- **Recording, main pane/left rail:** the still establishes that recording is active and what can be done next, but does not establish who is involved, the meeting’s purpose, or a location. The collapsed optional prompt does not supply that context in-frame.
- **Ledger, main pane/right transcript rail:** date, time, duration, retention, named people in notes, and speaker roles are visible. A location and an identity behind “Them” are not shown. No unshown context is assumed.
- These static captures show apparent controls—playback, source selection, turn expansion, search, record/pause/stop—but do not verify their physical interaction.
