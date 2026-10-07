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

// A just-stopped meeting whose transcript is still being made. The meeting is
// auto-opened (launch selection), so there is no click. Everything here is
// state, role or structure, not label copy, except two negative claims that are
// contracts: no "transcript remains" sentence and no "note only" row label while
// no transcript exists.
if (mode === "transcribing-meeting" || mode === "transcribing-meeting-lands") {
  const editor = await waitFor('[data-field="library-operator-note"]');
  if (!editor) return { ...result, error: "meeting page never opened" };
  await sleep(300);
  const status = () => q('[data-meeting-status="transcribing"]');
  const sidebarRow = () => q('.sidebar-scroll .row[data-status="transcribing"]');
  result.pageShowsStatus = !!status() && status().getAttribute('role') === 'status';
  result.pageClaimsNoRemainingTranscript = !/transcript remain/i.test(q('.meeting-note')?.textContent || "");
  result.captionNamesNoTranscript = (q('.doc-caption')?.textContent || "").split("\u00b7").pop().trim().toLowerCase() !== "transcript";
  result.rowShowsStatus = !!sidebarRow();
  result.rowNotNoteOnly = !/note only/.test(q('.sidebar-scroll')?.textContent || "");
  result.noTranscriptSection = !q("details.transcript-disclosure");
  result.notesEditable = !editor.disabled;
  result.audioPlayable = !!q('[data-action="play-retained-audio"]');
  result.noRecoveryPane = !q(".needs-attention");
  const states = ["pageShowsStatus", "pageClaimsNoRemainingTranscript", "captionNamesNoTranscript", "rowShowsStatus", "rowNotNoteOnly", "noTranscriptSection", "notesEditable", "audioPlayable", "noRecoveryPane"];
  if (new URLSearchParams(location.search).has("legacy")) {
    // The negative control inverts the verdict: it passes only when the
    // pre-fix defect is on screen (a "Transcript" caption, a "transcript
    // remains" sentence, a "note only" row). That is also what lets the runner
    // capture the "before" frame, which it does only for a passing run.
    result.defectReproduced = !result.captionNamesNoTranscript
      && !result.pageClaimsNoRemainingTranscript && !result.rowNotNoteOnly;
    result.errors = window.__errors || [];
    result.pass = result.defectReproduced && result.errors.length === 0;
    return result;
  }
  if (mode === "transcribing-meeting") {
    result.errors = window.__errors || [];
    result.pass = states.every((key) => result[key]) && result.errors.length === 0;
    return result;
  }
  // The transcript arrives while the person is typing in their own note. The
  // page must switch to the ready presentation without losing the typed text,
  // the focus, or the editor node itself.
  editor.focus();
  document.execCommand("insertText", false, "Check the Friday owner.");
  window.__harnessLandTranscript();
  const transcript = await waitFor("details.transcript-disclosure", 120);
  await sleep(300);
  const editorAfter = q('[data-field="library-operator-note"]');
  result.transcriptAppeared = !!transcript;
  result.statusGone = !status();
  result.rowStatusGone = !sidebarRow();
  result.rowReadsTranscriptAvailable = /transcript available/.test(q('.sidebar-scroll')?.textContent || "");
  result.typedTextKept = !!editorAfter && editorAfter.value.includes("Check the Friday owner.");
  result.editorKeptFocus = !!editorAfter && document.activeElement === editorAfter;
  result.editorNodeKept = editorAfter === editor;
  result.errors = window.__errors || [];
  result.pass = states.every((key) => result[key]) && result.transcriptAppeared && result.statusGone
    && result.rowStatusGone && result.rowReadsTranscriptAvailable && result.typedTextKept
    && result.editorKeptFocus && result.editorNodeKept && result.errors.length === 0;
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
