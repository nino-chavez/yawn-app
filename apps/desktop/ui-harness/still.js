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
return { ok: true, errors: window.__errors || [] };
