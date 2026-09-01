// Body for WKWebView callAsyncJavaScript. `mode` arrives as an argument.
// Uses document.execCommand("insertText") so each edit registers with the
// editor's undo stack exactly like typed text, and execCommand("undo"),
// which routes through the same WebCore editing undo stack cmd-Z hits.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const waitFor = async (selector, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    const el = q(selector);
    if (el) return el;
    await sleep(100);
  }
  return null;
};
const result = { mode };

if (mode === "capture") {
  const field = '[data-field="operator-note"]';
  const el = await waitFor(field);
  if (!el) return { ...result, error: "operator-note textarea never appeared" };
  el.focus();
  document.execCommand("insertText", false, "alpha ");
  document.execCommand("undo");
  result.controlUndoWorks = el.value === "";
  document.execCommand("insertText", false, "alpha ");
  document.execCommand("insertText", false, "bravo");
  result.valueTyped = el.value;
  const stash = el;
  await sleep(2100); // past two 900 ms poll ticks
  const now = q(field);
  result.sameNodeAfterTick = now === stash;
  result.focusedAfterTick = document.activeElement === now;
  result.valueAfterTick = now ? now.value : null;
  document.execCommand("undo");
  await sleep(30);
  result.valueAfterUndo1 = q(field)?.value ?? null;
  document.execCommand("undo");
  await sleep(30);
  result.valueAfterUndo2 = q(field)?.value ?? null;
  return result;
}

// mode === "library": open the meeting, then type into transcript search.
const row = await waitFor('[data-action="open-meeting"]');
if (!row) return { ...result, error: "meeting row never appeared" };
row.click();
const details = await waitFor("details.transcript-disclosure");
if (!details) return { ...result, error: "transcript disclosure never appeared" };
details.open = true;
const field = '[data-field="transcript-search"]';
const search = q(field);
if (!search) return { ...result, error: "transcript-search input missing" };
search.focus();
result.focusTook = document.activeElement === search;
const stash = search;
document.execCommand("insertText", false, "a");
await sleep(30);
const afterFirst = q(field);
result.sameNodeAfterKeystroke = afterFirst === stash;
result.detailsOpenAfterKeystroke = q("details.transcript-disclosure")?.open ?? null;
result.focusedAfterKeystroke = document.activeElement === afterFirst;
result.valueAfterKeystroke = afterFirst ? afterFirst.value : null;
if (document.activeElement !== afterFirst && afterFirst) {
  afterFirst.focus();
  afterFirst.setSelectionRange(afterFirst.value.length, afterFirst.value.length);
  result.hadToRefocus = true;
}
document.execCommand("insertText", false, "l");
await sleep(30);
result.valueTyped = q(field)?.value ?? null;
result.matchStatus = q("#transcript-search-status")?.textContent ?? null;
document.execCommand("undo");
await sleep(30);
result.valueAfterUndo1 = q(field)?.value ?? null;
document.execCommand("undo");
await sleep(30);
result.valueAfterUndo2 = q(field)?.value ?? null;
return result;
