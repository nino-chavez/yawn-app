---
kind: cold
surface: Yawn desktop Rail refinement
build: Input hashes in source-manifest.json; base fc23a11 plus the captured frontend change
reviewer: Codex worker 01a1a1cb-cda7-7f02-8ca0-63f03542c3a4; Operator dispatch 5bcf3cbc-bd7d-403f-aa8c-f582339294fa
implementer: Codex parent chat 01a198a4-399c-7363-b9d1-babb0e76f147
cold: true
device: Offscreen WKWebView content on this Mac; 1080x900; default text; OS contrast unverified
states: 01 resting, 02 inspected, 03 expanded, 04 recording; light and dark
verdict: 01 accepted; 04 baseline suggestions; 02 and 03 omitted by this pass and require focused review
---

**01 light / 01 dark — Accept.** The screen clearly identifies the selected meeting, its date, duration, retention date, paused interval, next notes, people named in those notes, and visible review controls. No clipping observed.

**04 light / 04 dark — Revise.** Recording status and Pause/Stop are clear, but the otherwise empty central pane does not make the next useful action or meeting context sufficiently clear.

Material fixes:

1. In the 04 central pane, make the blank “Your notes” area visibly actionable—an entry affordance or explicit primary prompt—rather than leaving only the small collapsed “What is this meeting for?” line.

2. In 04, consolidate the duplicated live-state signals across the top bar (“New Recording,” `0:31`) and left rail (“Recording now,” `0:31`) so one region is unmistakably the current-recording identity and the other provides supporting detail.

3. In 04, add lightweight meeting context near the central “Meeting” title (at least current date/time; location if known). The still presently shows elapsed time only, so when and where are not recoverable from the main work area.