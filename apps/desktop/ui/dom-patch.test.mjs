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
