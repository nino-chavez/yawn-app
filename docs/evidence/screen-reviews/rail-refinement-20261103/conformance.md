---
kind: conformance
surface: Yawn desktop moment inspection
build: Input hashes in source-manifest.json; source base fc23a11 with this frontend change
reviewer: Codex parent chat 01a198a4-399c-7363-b9d1-babb0e76f147
implementer: Same parent; author conformance check, separate from the independent cold review
cold: false
device: Offscreen WKWebView content on this Mac at 1080x900, light and dark; synthetic commands
states: Resting, inspected, expanded and recording
verdict: matches the selected Proposed refinement within the captured scope
---

The parent opened the actual native related and expanded frames and compared
them with the selected Proposed reference in PR 167. The implementation keeps
Rail’s paper, graphite, Moment blue, Record red and attention colors, the single
textarea, the note-first hierarchy, and the existing sidebar/document structure.
The paired note and transcript tint, elapsed-time mark, guide and enlarged
fold controls match the selected treatment. Native fidelity checks retain the
existing geometry in both appearances.

Actual hover and keyboard focus were exercised separately in Chromium and
WebKit at 1080px and 960px, including wrapped and inserted lines. Those checks
also verified the live mark stays red, disclosures preserve editor geometry,
Escape returns focus, native undo survives polling, and reduced motion remains
usable. Clearing the inspection classes made the cue disappear in the
negative control. Offscreen related-state captures pose the inspection state;
they do not prove physical pointer behavior or installed-app acceptance.

Nothing is removed, combined or hidden. Transcript emphasis is temporary during
inspection. No extra keyboard stop, backend command, animation or audio-seeking
action is added. The comparison observer is replaced by scoped hover/focus delegation and a
layout refresh, using the existing external CSS under the packaged CSP. Signed packaging, installation, real meetings, audio capture,
backend storage and public release are outside this source-integration receipt.

The final static-CSS source was recaptured into `final-frames/`; its input
hashes are in `source-manifest.json`. The independent cold review used the
earlier `frames/` and `reviewed-source-manifest.json`. The parent reopened the
final related frame: paired tint, text and time mark align. Capture frames are
byte-identical, while the ledger frames wrap differently. A direct browser
comparison against the base source at 1080x900 verified identical ledger, note,
editor and mirror widths and editor height. Only the transcript stack height
grows with the enlarged disclosures. This final implementation check does not
rewrite the independent reviewers' verdicts or imply an installed-app review.

Validation: 215 frontend tests; four complete Chromium/WebKit light/dark
journeys under the packaged CSP; native inspection, Rail fidelity and capture
undo checks. Durable full logs: repository `.artifacts/rail-refinement-20261103`.
