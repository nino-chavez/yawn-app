// A still: load a stub mode, let it settle, and return. With
// HARNESS_CAPTURE_DIR set, run.sh's runner then writes the frame, which is
// how the cold review gets WKWebView captures of states the installed
// fixture app cannot reach (a recording in progress, an interrupted one).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
await sleep(800);
// `&open-folds=N`: open the ledger's first N folds the way a person does, by
// pressing them, so the frame shows the expanded state.
const openFolds = Number(new URLSearchParams(location.search).get("open-folds")) || 0;
for (let index = 0; index < openFolds; index += 1) {
  const fold = document.querySelectorAll(".ledger-fold[aria-expanded='false']")[0];
  if (!fold) return { ok: false, error: `only ${index} closed folds to open` };
  fold.click();
  await sleep(250);
}
// The runner's page is hidden (document.visibilityState), so CSS animations
// and transitions never advance there; finish them so the frame shows the
// settled state a visible window reaches in 120 ms.
if (openFolds) document.getAnimations().forEach((animation) => animation.finish());
return { ok: true, errors: window.__errors || [] };
