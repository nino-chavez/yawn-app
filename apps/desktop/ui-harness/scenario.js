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

if (mode === "mic-change-stop") {
  // No click: the backend stops on its own. Wait past the stub's switch and
  // one 900 ms snapshot poll.
  const live = await waitFor('.record-control .record.live');
  result.startedLive = !!live;
  await sleep(2400);
  const toast = q('#toast-notice');
  result.noticeShown = !!toast && toast.getAttribute('role') === 'status'
    && toast.textContent.includes("microphone setup changed")
    && toast.textContent.includes("saved and queued for transcription");
  // The real post-stop snapshot is idle with the meeting cleared, so Record
  // is offered again and no capture phase is shown.
  result.backToIdle = !!q('.record-control [data-action="open-start"], .record-control [data-action="start-recording"], [data-action="open-start"]');
  result.noLiveControls = !q('.record-control .record.live') && !q('.record-control [data-action="stop-recording"]');
  result.noCapturePhase = !["Transcribing on this Mac", "Stopping recording"].includes(q('.toolbar-title')?.textContent);
  result.errors = window.__errors || [];
  result.pass = result.startedLive && result.noticeShown && result.backToIdle
    && result.noLiveControls && result.noCapturePhase && result.errors.length === 0;
  return result;
}

if (mode === "mic-change-failed" || mode === "mic-change-resume-failed") {
  const note = await waitFor('.canvas-context-note');
  const expected = mode === "mic-change-failed"
    ? ["Recording did not start", "No audio was captured"]
    : ["Recording could not resume", "kept as interrupted"];
  result.failureShown = !!note && expected.every((words) => note.textContent.includes(words));
  result.noSavedClaim = !document.body.textContent.includes("saved and queued for transcription");
  result.errors = window.__errors || [];
  result.pass = result.failureShown && result.noSavedClaim && result.errors.length === 0;
  return result;
}

if (mode === "stop-status") {
  const stop = await waitFor('.record-control [data-action="stop-recording"]');
  if (!stop) return { ...result, error: "live Stop control never appeared" };
  result.liveShowsStop = !!q('.record-control .record.live')
    && q('.sidebar-scroll [aria-current="true"] .row-title')?.textContent === "Recording now";
  stop.click();
  await sleep(100);
  result.stoppingShowsPhase = q('.record-control [role="status"]')?.textContent === "Stopping recording"
    && q('.toolbar-title')?.textContent === "Stopping recording"
    && q('.sidebar-scroll [aria-current="true"] .row-title')?.textContent === "Stopping recording";
  result.stoppingHasNoLiveControls = !q('.record-control .record.live')
    && !q('.record-control [data-action="stop-recording"]');
  await sleep(1100); // past one 900 ms snapshot poll after the stub advances
  result.transcribingShowsPhase = q('.record-control [role="status"]')?.textContent === "Transcribing on this Mac"
    && q('.toolbar-title')?.textContent === "Transcribing on this Mac"
    && q('.sidebar-scroll [aria-current="true"] .row-title')?.textContent === "Transcribing on this Mac";
  result.noNewRecordingPrompt = !document.querySelector('.sidebar-scroll')?.textContent?.includes("Press Record");
  result.transcribingHasNoLiveControls = !q('.record-control .record.live')
    && !q('.record-control [data-action="stop-recording"]');
  result.savedAudioIsExplicit = document.querySelector('.canvas-caption')?.textContent?.includes("Recording stopped. Audio saved on this Mac.");
  result.errors = window.__errors || [];
  result.pass = result.liveShowsStop && result.stoppingShowsPhase
    && result.stoppingHasNoLiveControls && result.transcribingShowsPhase
    && result.transcribingHasNoLiveControls && result.savedAudioIsExplicit
    && result.noNewRecordingPrompt
    && result.errors.length === 0;
  return result;
}

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
