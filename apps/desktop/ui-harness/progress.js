// Transcription progress on the meeting page, in a real WKWebView: the page
// follows the worker's amount about once a second, patches only the bar and the
// percent, never re-reads the meeting, never disturbs the note editor, keeps the
// last amount when the backend stops reporting one while the note still says
// "transcribing", and lines the status block up with the page's column.
// State, role, structure and geometry only; the wording is pinned by
// view-model.test.mjs against DIRECTION.md. Run: `./run.sh progress`, which runs
// this against the stub modes `transcribing-progress` (an amount) and
// `transcribing-progress-none` (never an amount).
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
const left = (el) => Math.round(el.getBoundingClientRect().left);

const editor = await waitFor('[data-field="library-operator-note"]');
if (!editor) return { mode, error: "meeting page never opened" };
await waitFor('[data-meeting-status="transcribing"]');
await sleep(300);

// The status block shares the column the title, the transport and "Your notes"
// use, and has the page's 24px between sections above it. (Layout, not wording:
// it holds with and without a bar.)
const sentence = q('[data-meeting-status="transcribing"] #meeting-note-heading');
const section = q('[data-meeting-status="transcribing"]');
const notes = q('#meeting-your-notes');
const title = q('#meeting-title');
const notesHeading = q('#operator-note-heading');
step("status sentence is in the title's column", sentence && title && left(sentence) === left(title), `${sentence && left(sentence)} vs ${title && left(title)}`);
step("status sentence is in the 'Your notes' column", sentence && notesHeading && left(sentence) === left(notesHeading), `${sentence && left(sentence)} vs ${notesHeading && left(notesHeading)}`);
const gap = Math.round(section.getBoundingClientRect().top - notes.getBoundingClientRect().bottom);
step("status block has the page's 24px between sections above it", gap >= 24 && gap <= 26, gap);

if (mode === "transcribing-progress-none") {
  // The Whisper path, or before the first report: today's surface, no bar, and
  // polling the backend for an amount does not conjure one.
  await sleep(2300);
  step("no amount ever reported, no bar", !q('[data-transcription-progress]'));
  step("it polled and was told there is none", calls("library_transcription_progress") >= 2, calls("library_transcription_progress"));
  step("status sentence still there", Boolean(sentence));
  return { mode, steps, errors: window.__errors || [], pass: steps.every((item) => item.ok) && (window.__errors || []).length === 0 };
}

let bar = await waitFor('[data-transcription-progress] progress');
if (!bar) return { mode, error: "no progress bar at 40%", steps };
const holder = () => q('[data-transcription-progress]');
const percentText = () => holder()?.querySelector('.transcription-progress-percent')?.textContent;
step("bar is a determinate progressbar with its range and name",
  bar.getAttribute("role") === "progressbar" && bar.getAttribute("aria-valuemin") === "0"
  && bar.getAttribute("aria-valuemax") === "100" && bar.getAttribute("aria-valuenow") === "40"
  && Boolean(bar.getAttribute("aria-label")), bar.outerHTML);
step("percent beside it is hidden from assistive technology and agrees",
  holder().querySelector('.transcription-progress-percent').getAttribute("aria-hidden") === "true" && percentText() === "40%", percentText());
step("bar is outside the status region", !bar.closest('[role="status"]') && Boolean(q('[data-meeting-status="transcribing"]')));
step("bar is in the same column as the sentence", left(holder()) === left(sentence), `${left(holder())} vs ${left(sentence)}`);
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

// An amount the page cannot use changes nothing.
window.__harnessSetProgress(5000, 3664);
await sleep(1500);
step("an impossible amount leaves the last one showing", q('[data-transcription-progress] progress')?.value === 99 && percentText() === "99%", percentText());

// The request ends and the backend forgets the amount, but the transcript has not
// landed: the note still says "transcribing", and the bar stays at the last value.
window.__harnessSetProgress(null);
await sleep(2300);
step("backend reports none, note still transcribing: bar stays at 99",
  q('[data-transcription-progress] progress') === barBefore && barBefore.value === 99 && percentText() === "99%"
  && Boolean(q('[data-meeting-status="transcribing"]')), percentText());
step("it was still polling", calls("library_transcription_progress") - pollsBefore >= 5, calls("library_transcription_progress") - pollsBefore);
step("editor survived all of it", q('[data-field="library-operator-note"]') === editorBefore && document.activeElement === editorBefore && editorBefore.value.endsWith("kept"));

// The transcript lands: the note changes state, the page re-reads, and both the
// status and the bar are gone.
window.__harnessLandTranscript();
let gone = false;
for (let i = 0; i < 60 && !gone; i += 1) {
  await sleep(100);
  gone = !q('[data-meeting-status="transcribing"]') && !q('[data-transcription-progress]');
}
step("transcript lands: status and bar are gone", gone);

return { mode, steps, errors: window.__errors || [], pass: steps.every((item) => item.ok) && (window.__errors || []).length === 0 };
