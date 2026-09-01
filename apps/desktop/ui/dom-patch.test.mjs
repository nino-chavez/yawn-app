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

// W9-B: the motion vocabulary's entrance animations are plain, permanent CSS
// rules on `.modal-backdrop` / `.start-sheet` (styles.css). They fire once,
// on first insertion, and never replay across the 900 ms poll -- but only
// because those elements carry no `id` or `data-field`, so the patcher above
// matches and morphs them in place (see `compatible()`/`patchChildren`)
// instead of removing and reinserting them while a sheet stays open. There
// is no jsdom in this suite to run `patchInto` and observe that directly
// (ui-harness/ is the real-WKWebView proof of the reused-node claim, run
// manually, not part of `npm run test:ui`); this test pins the two things
// that would silently break it: a stray identity on a backdrop/dialog, or a
// JS-driven class toggle standing in for the CSS rule.
test("no modal backdrop or dialog carries an id, so it is reused across a patch tick instead of re-animating", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  const backdropOpenTags = [...main.matchAll(/<div class="modal-backdrop"[^>]*>/g)];
  assert.equal(backdropOpenTags.length, 7, "expected all seven sheets' backdrop tags");
  for (const [tag] of backdropOpenTags) {
    assert.doesNotMatch(tag, /\bid=/, `modal-backdrop must stay unkeyed: ${tag}`);
  }
  const dialogOpenTags = [...main.matchAll(/<section class="start-sheet[^>]*role="dialog"[^>]*>/g)];
  assert.equal(dialogOpenTags.length, 7, "expected all seven sheets' dialog tags");
  for (const [tag] of dialogOpenTags) {
    assert.doesNotMatch(tag, /\bid=/, `sheet dialog must stay unkeyed: ${tag}`);
    assert.doesNotMatch(tag, /data-field=/, `sheet dialog must stay unkeyed: ${tag}`);
  }
});

test("the two toast kinds carry distinct stable ids, so switching between them is a real insert, not a content swap on a reused node", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  // Without distinct ids the patcher would match them by tag-name position
  // (both are a bare <aside>): a notice replaced by an error toast in the
  // same tick would reuse the notice's node, and the entrance animation
  // (which already played for the notice) would never fire for the error
  // that silently took its place.
  assert.match(main, /<aside id="toast-notice" class="toast toast-notice"/);
  assert.match(main, /<aside id="toast-error" class="toast"/);
});

test("no JS toggles a motion class -- the entrance animation is a permanent CSS rule, not something render() switches on", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  for (const cls of ["modal-backdrop", "start-sheet", "evidence-popover", "toast"]) {
    const toggle = new RegExp(`classList\\.(add|remove|toggle)\\(["']${cls}`);
    assert.doesNotMatch(main, toggle, `${cls} must not be toggled from JS`);
  }
});

test("the shared motion tokens are defined once and every new animation reads them, never a literal duration", async () => {
  const styles = await readFile(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /--motion-fast:\s*120ms;/);
  assert.match(styles, /--motion-standard:\s*200ms;/);
  assert.match(styles, /--motion-ease:\s*ease-out;/);
  // Token-level reduced motion: one override, not a rule per consumer.
  assert.match(
    styles,
    /@media \(prefers-reduced-motion: reduce\) \{\s*:root \{\s*--motion-fast: 0ms;\s*--motion-standard: 0ms;\s*\}\s*\}/,
  );
  const animationDeclarations = [...styles.matchAll(/animation:\s*([^;]+);/g)].map(([, value]) => value);
  const motionDeclarations = animationDeclarations.filter((value) => value.includes("motion-fade-in") || value.includes("motion-rise-in"));
  assert.equal(motionDeclarations.length, 4, "expected backdrop, dialog, popover, and toast to each declare one animation");
  for (const value of motionDeclarations) {
    assert.match(value, /var\(--motion-(fast|standard)\)/, `duration must read a token, not a literal: ${value}`);
    assert.doesNotMatch(value, /\d+m?s\b/, `no literal duration alongside the token: ${value}`);
  }
  // Two keyframes total for the whole vocabulary -- no keyframe zoo.
  const motionKeyframes = [...styles.matchAll(/@keyframes (motion-[a-z-]+)/g)];
  assert.equal(motionKeyframes.length, 2);
});

test("the motion vocabulary only ever animates transform and opacity, never a layout property", async () => {
  const styles = await readFile(new URL("./styles.css", import.meta.url), "utf8");
  const fadeIn = styles.match(/@keyframes motion-fade-in \{([\s\S]*?)\n\}/)?.[1] ?? "";
  const riseIn = styles.match(/@keyframes motion-rise-in \{([\s\S]*?)\n\}/)?.[1] ?? "";
  for (const body of [fadeIn, riseIn]) {
    assert.match(body, /opacity/);
    assert.doesNotMatch(body, /\b(width|height|top|left|right|bottom|margin|padding)\b\s*:/);
  }
});
