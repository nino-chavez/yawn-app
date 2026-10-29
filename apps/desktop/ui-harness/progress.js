// Transcription progress on the meeting page, in a real WKWebView: the page
// follows the worker's amount about once a second, patches only the bar and the
// percent, never re-reads the meeting, and never disturbs the note editor.
// State, role and structure only; the wording is pinned by view-model.test.mjs
// against DIRECTION.md. Run: `./run.sh progress`.
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
const calls = (name) => window.__harnessCalls.filter((call) => call === name).length;
const steps = [];
const step = (name, ok, detail) => steps.push({ name, ok: Boolean(ok), ...(detail === undefined ? {} : { detail }) });

const editor = await waitFor('[data-field="library-operator-note"]');
if (!editor) return { mode, error: "meeting page never opened" };
let bar = await waitFor('[data-transcription-progress] progress');
if (!bar) return { mode, error: "no progress bar at 40%" };
await sleep(300);

const holder = () => q('[data-transcription-progress]');
const percentText = () => holder()?.querySelector('.transcription-progress-percent')?.textContent;
step("bar is a determinate progressbar with its range and name",
  bar.getAttribute("role") === "progressbar" && bar.getAttribute("aria-valuemin") === "0"
  && bar.getAttribute("aria-valuemax") === "100" && bar.getAttribute("aria-valuenow") === "40"
  && Boolean(bar.getAttribute("aria-label")), bar.outerHTML);
step("percent beside it is hidden from assistive technology and agrees",
  holder().querySelector('.transcription-progress-percent').getAttribute("aria-hidden") === "true" && percentText() === "40%", percentText());
step("bar is outside the status region", !bar.closest('[role="status"]') && Boolean(q('[data-meeting-status="transcribing"]')));
step("status sentence is still there, unchanged by the bar", Boolean(q('[data-meeting-status="transcribing"] #meeting-note-heading')));
step("bar has a drawn width", bar.getBoundingClientRect().width > 100 && bar.getBoundingClientRect().height >= 2, JSON.stringify(bar.getBoundingClientRect()));

editor.focus();
document.execCommand("insertText", false, "kept");
await sleep(150);
const editorBefore = editor;
const barBefore = bar;
const opensBefore = calls("library_open_note");
const pollsBefore = calls("library_transcription_progress");

// The worker reports more audio covered; about two ticks later the same bar
// shows it, patched in place.
window.__harnessSetProgress(2748, 3664);
await sleep(2300);
bar = q('[data-transcription-progress] progress');
step("bar follows a new amount, same node", bar === barBefore && bar.value === 75 && bar.getAttribute("aria-valuenow") === "75" && percentText() === "75%",
  `${bar?.value} ${percentText()}`);
step("polled about once a second", calls("library_transcription_progress") - pollsBefore >= 2, calls("library_transcription_progress") - pollsBefore);
step("never re-read the meeting", calls("library_open_note") === opensBefore, calls("library_open_note") - opensBefore);
step("note editor kept its node, focus and text", q('[data-field="library-operator-note"]') === editorBefore
  && document.activeElement === editorBefore && editorBefore.value.endsWith("kept"), editorBefore.value);

// Everything covered and no transcript yet reads 99, never 100.
window.__harnessSetProgress(3664, 3664);
await sleep(1500);
step("capped at 99", q('[data-transcription-progress] progress')?.value === 99 && percentText() === "99%", percentText());

// The amount goes away (the request ended or the worker stopped reporting): the
// page falls back to its plain status, and the editor still survives the render.
window.__harnessSetProgress(null);
await sleep(1500);
step("no amount, no bar", !holder() && Boolean(q('[data-meeting-status="transcribing"]')));
step("editor survived the bar leaving", q('[data-field="library-operator-note"]') === editorBefore && document.activeElement === editorBefore && editorBefore.value.endsWith("kept"));

window.__harnessSetProgress(368, 3664);
await sleep(1500);
step("bar returns when an amount does", q('[data-transcription-progress] progress')?.value === 10 && percentText() === "10%", percentText());

// An amount the page cannot use is no amount.
window.__harnessSetProgress(5000, 3664);
await sleep(1500);
step("an impossible amount shows no bar", !holder());

return { mode, steps, errors: window.__errors || [], pass: steps.every((item) => item.ok) && (window.__errors || []).length === 0 };
