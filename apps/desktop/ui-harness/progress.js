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

// WCAG contrast, from the colors the page actually computed (both appearances).
const parseColor = (text) => {
  const value = String(text).trim();
  const hex = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(value);
  if (hex) return [parseInt(hex[1], 16), parseInt(hex[2], 16), parseInt(hex[3], 16), 1];
  const fn = /^rgba?\(([^)]+)\)$/.exec(value);
  if (!fn) return null;
  const parts = fn[1].split(/[\s,/]+/).filter(Boolean).map(Number);
  return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1];
};
const over = (front, back) => front[3] >= 1 ? front : [0, 1, 2].map((i) => front[i] * front[3] + back[i] * (1 - front[3])).concat(1);
const luminance = ([r, g, b]) => {
  const channel = (v) => { const c = v / 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
};
const ratio = (a, b) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
};
const pageBackground = (el) => {
  for (let node = el; node; node = node.parentElement) {
    const color = parseColor(getComputedStyle(node).backgroundColor);
    if (color && color[3] > 0) return color;
  }
  return parseColor("#ffffff");
};
const appearance = document.documentElement.dataset.theme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

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
step("the read beside it is hidden from assistive technology and says what the number is of",
  holder().querySelector('.transcription-progress-percent').getAttribute("aria-hidden") === "true" && percentText() === "40% transcribed", percentText());
step("bar is outside the status region", !bar.closest('[role="status"]') && Boolean(q('[data-meeting-status="transcribing"]')));
step("bar is in the same column as the sentence", left(holder()) === left(sentence), `${left(holder())} vs ${left(sentence)}`);
step("bar has a drawn width", bar.getBoundingClientRect().width > 100 && bar.getBoundingClientRect().height >= 2, JSON.stringify(bar.getBoundingClientRect()));

// The bar and its percent span the sentence's own measure: the same left edge and
// the same width, with the bar filling what the percent leaves.
const percentEl = holder().querySelector('.transcription-progress-percent');
const widths = () => ({
  sentence: Math.round(sentence.getBoundingClientRect().width * 100) / 100,
  line: Math.round(holder().getBoundingClientRect().width * 100) / 100,
  bar: Math.round(bar.getBoundingClientRect().width * 100) / 100,
  percent: Math.round(percentEl.getBoundingClientRect().width * 100) / 100,
});
const measured = widths();
const gapPx = parseFloat(getComputedStyle(holder()).columnGap);
step("bar line is as wide as the status sentence, from the same left edge", Math.abs(measured.line - measured.sentence) <= 0.5 && left(holder()) === left(sentence), JSON.stringify(measured));
step("the bar fills what the percent leaves", Math.abs(measured.bar - (measured.line - measured.percent - gapPx)) <= 1 && measured.bar > measured.line * 0.7, JSON.stringify({ ...measured, gapPx }));
step("percent is the status sentence's size and tabular", getComputedStyle(percentEl).fontSize === getComputedStyle(sentence).fontSize
  && getComputedStyle(percentEl).fontVariantNumeric.includes("tabular-nums"), `${getComputedStyle(percentEl).fontSize} ${getComputedStyle(percentEl).fontVariantNumeric}`);

// 1.4.11 and 1.4.3, measured on the colors the page actually drew.
const page = pageBackground(holder());
const rootStyle = getComputedStyle(document.documentElement);
// The fill is drawn in the bar's own `color` (the value pseudo-element is
// `currentColor`) and the track in its `background`; WebKit does not expose the
// pseudo-elements' own computed style.
const fill = parseColor(getComputedStyle(bar).color);
const track = over(parseColor(getComputedStyle(bar).backgroundColor), page);
const text = over(parseColor(getComputedStyle(percentEl).color), page);
const contrast = {
  appearance,
  fillOnTrack: ratio(fill, track),
  trackOnPage: ratio(track, page),
  fillOnPage: ratio(fill, page),
  percentText: ratio(text, page),
  sentenceText: ratio(over(parseColor(getComputedStyle(sentence).color), page), page),
};
step("fill against the track is at least 3:1", contrast.fillOnTrack >= 3, JSON.stringify(contrast));
step("track against the page is at least 3:1", contrast.trackOnPage >= 3, JSON.stringify(contrast));
step("fill against the page is at least 3:1", contrast.fillOnPage >= 3, JSON.stringify(contrast));
step("percent text is at least 4.5:1", contrast.percentText >= 4.5, JSON.stringify(contrast));
step("the bar's colors come from the tokens", rootStyle.getPropertyValue("--progress-track").trim().length > 0 && Boolean(parseColor(rootStyle.getPropertyValue("--progress-track"))), rootStyle.getPropertyValue("--progress-track"));

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
step("bar follows a new amount, same node", bar === barBefore && bar.value === 75 && bar.getAttribute("aria-valuenow") === "75" && percentText() === "75% transcribed",
  `${bar?.value} ${percentText()}`);
step("polled about once a second", calls("library_transcription_progress") - pollsBefore >= 2, calls("library_transcription_progress") - pollsBefore);
step("never re-read the meeting", calls("library_open_note") === opensBefore, calls("library_open_note") - opensBefore);
step("note editor kept its node, focus and text", q('[data-field="library-operator-note"]') === editorBefore
  && document.activeElement === editorBefore && editorBefore.value.endsWith("kept"), editorBefore.value);

// 9% and 10% have different digits but the same box: the bar does not change length.
window.__harnessSetProgress(330, 3664);
await sleep(1500);
const fits = () => percentEl.scrollWidth <= percentEl.clientWidth + 0.5;
const at9 = { text: percentText(), fits: fits(), ...widths() };
window.__harnessSetProgress(368, 3664);
await sleep(1500);
const at10 = { text: percentText(), fits: fits(), ...widths() };
window.__harnessSetProgress(3600, 3664);
await sleep(1500);
const at98 = { text: percentText(), fits: fits(), ...widths() };
step("bar and label keep their widths from 9% to 10% to 98%, and the words fit",
  at9.text === "9% transcribed" && at10.text === "10% transcribed" && at98.text === "98% transcribed"
  && at9.bar === at10.bar && at10.bar === at98.bar && at9.percent === at10.percent && at10.percent === at98.percent
  && at9.fits && at10.fits && at98.fits, JSON.stringify({ at9, at10, at98 }));

// Everything covered and no transcript yet reads 99, never 100.
window.__harnessSetProgress(3664, 3664);
await sleep(1500);
step("capped at 99", q('[data-transcription-progress] progress')?.value === 99 && percentText() === "99% transcribed", percentText());

// An amount the page cannot use changes nothing.
window.__harnessSetProgress(5000, 3664);
await sleep(1500);
step("an impossible amount leaves the last one showing", q('[data-transcription-progress] progress')?.value === 99 && percentText() === "99% transcribed", percentText());

// The request ends and the backend forgets the amount, but the transcript has not
// landed: the note still says "transcribing", and the bar stays at the last value.
window.__harnessSetProgress(null);
await sleep(2300);
step("backend reports none, note still transcribing: bar stays at 99",
  q('[data-transcription-progress] progress') === barBefore && barBefore.value === 99 && percentText() === "99% transcribed"
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

return { mode, appearance, measured, contrast, steps, errors: window.__errors || [], pass: steps.every((item) => item.ok) && (window.__errors || []).length === 0 };
