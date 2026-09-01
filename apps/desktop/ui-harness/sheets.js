// W9-B: real-DOM proof that a sheet's entrance animation fires once, not on
// every render() tick while it stays open. `npm run test:ui` has no DOM
// (dom-patch.test.mjs asserts the mechanism from source instead -- see the
// comment there), so this is the one place the actual claim can be observed:
// a real WKWebView, the real ui/dom-patch.mjs, the real CSS.
//
// The proof does not need the literal 900 ms snapshot poll -- library mode's
// idle snapshot makes `shouldPollSnapshot` false, so that poll never fires an
// unprompted render() here. Clicking each start-sheet attestation checkbox
// fires a synchronous `change` -> render() (see handleChange in main.js) with
// `state.modal` unchanged, which is mechanically the same case: render() runs
// again while the sheet stays open. Three checkboxes clicked twice each is
// six such render() calls, more than the three poll ticks named in the brief.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const waitFor = async (selector, tries = 50) => {
  for (let i = 0; i < tries; i += 1) {
    const el = q(selector);
    if (el) return el;
    await sleep(100);
  }
  return null;
};
const result = { steps: [] };
const step = (name, ok, detail) => result.steps.push({ name, ok, ...(detail ? { detail } : {}) });

let backdropAnimationStarts = 0;
let dialogAnimationStarts = 0;
document.addEventListener("animationstart", (event) => {
  if (event.target instanceof Element && event.target.classList.contains("modal-backdrop")) backdropAnimationStarts += 1;
  if (event.target instanceof Element && event.target.classList.contains("start-sheet")) dialogAnimationStarts += 1;
}, true);

const record = await waitFor('[data-action="open-start"]');
if (!record) return { error: "no Record button", errors: window.__errors };
record.click();

const backdrop = await waitFor(".modal-backdrop");
step("sheet opens", Boolean(backdrop));
const dialog = q(".start-sheet");
step("dialog renders inside the backdrop", Boolean(dialog));

// Let the entrance animation actually finish before the render-tick loop, so
// a coincidental restart (were one to happen) shows up as a distinct second
// `animationstart`, not a continuation of the first.
await sleep(400);
result.animationStartsAfterOpen = { backdrop: backdropAnimationStarts, dialog: dialogAnimationStarts };

const boxes = Array.from(document.querySelectorAll('[data-field="attestation"]'));
step("three attestations present", boxes.length === 3);
let renderTicks = 0;
for (const box of boxes) {
  box.click();
  renderTicks += 1;
  await sleep(60);
  box.click();
  renderTicks += 1;
  await sleep(60);
}
result.renderTicksSimulated = renderTicks;

const backdropAfter = q(".modal-backdrop");
const dialogAfter = q(".start-sheet");
step("backdrop node identity survives every render tick", backdropAfter === backdrop);
step("dialog node identity survives every render tick", dialogAfter === dialog);
step("start button reachable (sheet still open, not rebuilt)", Boolean(q('[data-action="start-recording"]')));

result.animationStartsAfterTicks = { backdrop: backdropAnimationStarts, dialog: dialogAnimationStarts };
step("backdrop entrance animation fired exactly once across all render ticks", backdropAnimationStarts === 1);
step("dialog entrance animation fired exactly once across all render ticks", dialogAnimationStarts === 1);

q('[data-action="close-start"]').click();
await sleep(50);
step("sheet closes instantly (no lingering animating node)", !q(".modal-backdrop"));

result.errors = window.__errors;
return result;
