// A still: load a stub mode, let it settle, and return. With
// HARNESS_CAPTURE_DIR set, run.sh's runner then writes the frame, which is
// how the cold review gets WKWebView captures of states the installed
// fixture app cannot reach (a recording in progress, an interrupted one).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
await sleep(800);
return { ok: true, errors: window.__errors || [] };
