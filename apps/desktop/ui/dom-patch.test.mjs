import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

// D9 native-text audit, finding 3: rebuilding root.innerHTML wholesale on
// every 900 ms snapshot tick (and on every transcript-search keystroke)
// replaced each editor node, which reset WebKit's per-element undo stack and
// dropped focus for the search input inside its re-created, collapsed
// <details>. These assertions pin the structural fix: render() must patch the
// existing tree in place, and the patcher must never touch a control whose
// rendered content did not change.

test("render() patches the DOM in place and never assigns root.innerHTML wholesale", async () => {
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(source, /import \{ patchInto \} from "\.\/dom-patch\.mjs";/);
  assert.match(source, /patchInto\(root, `/);
  // Any wholesale innerHTML assignment on root would reintroduce the undo
  // reset on the next poll tick, silently — nothing else fails when it does.
  assert.doesNotMatch(source, /root\.innerHTML\s*=/);
});

test("the patcher keys editors the same way focus restoration matches them", async () => {
  const patcher = await readFile(new URL("./dom-patch.mjs", import.meta.url), "utf8");
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // data-field plus data-meeting-id is the identity restoreEditorFocus already
  // uses; the patcher must reuse a node only under the same identity so a
  // meeting switch still replaces the editor (and its stale undo history).
  assert.match(patcher, /getAttribute\("data-field"\)/);
  assert.match(patcher, /getAttribute\("data-meeting-id"\)/);
  assert.match(main, /candidate\.dataset\.meetingId === focus\.meetingId/);
});

test("live control state is pushed only when the rendered value actually differs", async () => {
  const patcher = await readFile(new URL("./dom-patch.mjs", import.meta.url), "utf8");
  // While the operator types, state and DOM agree, so the control must be
  // left untouched — writing an equal value would still clear undo history.
  assert.match(patcher, /if \(oldNode\.value !== incoming\) \{/);
  assert.match(patcher, /\} else if \(oldNode\.value !== newNode\.value\) \{/);
});

test("a reader-opened details element never snaps shut on a render", async () => {
  const patcher = await readFile(new URL("./dom-patch.mjs", import.meta.url), "utf8");
  // The renderer never emits `open`, so removing it during attribute sync
  // would collapse the transcript disclosure under the operator's cursor —
  // the pre-fix behavior that lost focus on the first search keystroke.
  assert.match(patcher, /oldNode\.tagName === "DETAILS" && attribute\.name === "open"/);
});

// Design intake D5: the evidence popover and split view needed no new
// preservation rule in this file at all. Two different reasons, one per
// depth:
//
// - The hover popover (depth 1) is never part of the patched tree in the
//   first place -- it is appended straight to `document.body` by
//   `showEvidencePopover` in main.js, so `patchInto`/`patchChildren` never
//   walks it and cannot detach or mismatch it mid-hover.
// - The split view (depths 2/3) needs no *node* preserved at all: its
//   visible/highlighted state (`state.selected.evidenceSplit`) is read at
//   render time and rendered back into the template
//   (`data-evidence-split`, `transcript-line-target`) on every call, the
//   same way the app's status pill or save-state label already are. There is
//   nothing for the patcher to lose, because nothing about it lives only in
//   the DOM between renders.
test("the evidence popover never enters the patched tree, and the split's visible state is state-driven, not DOM-cached", async () => {
  const patcher = await readFile(new URL("./dom-patch.mjs", import.meta.url), "utf8");
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // No popover-specific key, class, or id check was added to the patcher.
  assert.doesNotMatch(patcher, /evidence-popover/);
  assert.doesNotMatch(patcher, /evidenceSplit/);
  // The popover lives outside `root` (and therefore outside `patchInto`'s
  // reach) for its entire lifetime -- created on `document.body`, removed by
  // reference, never looked up through `root.querySelector`.
  assert.match(main, /document\.body\.appendChild\(el\)/);
  assert.match(main, /evidencePopoverEl\?\.remove\(\)/);
  // The split's own layout attribute is computed from `state` on every
  // `renderMeetingWorkspace` call, not read back from a previous DOM node.
  assert.match(main, /data-evidence-split="\$\{splitActive \? "open" : "closed"\}"/);
});
