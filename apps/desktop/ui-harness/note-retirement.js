// Fixture-only retirement gate for the production UI. The mode is selected
// through the harness URL: transcript-retirement, saved-draft, or
// summary-failed. It proves the useful retained surfaces remain and no new
// generation affordance reaches the browser.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const waitFor = async (selector, tries = 80) => {
  for (let attempt = 0; attempt < tries; attempt += 1) {
    const element = q(selector);
    if (element) return element;
    await sleep(50);
  }
  return null;
};

const fixtureMode = new URLSearchParams(location.search).get("mode") || "transcript-retirement";
const row = await waitFor('[data-action="open-meeting"]');
if (!row) return { mode: fixtureMode, error: "meeting fixture did not load" };
row.click();

const notes = await waitFor('[data-field="library-operator-note"]');
const transcript = await waitFor("details.transcript-disclosure");
// This negative control proves the gate rejects an injected generation button.
if (new URLSearchParams(location.search).has("negative-control")) {
  const control = document.createElement("button");
  control.dataset.action = "generate-note";
  document.body.append(control);
}
const noGenerationControl = !q('[data-action="generate-note"]')
  && !document.body.textContent.includes("Generate note")
  && !document.body.textContent.includes("Regenerate note");
const hasSavedDraft = Boolean(q(".meeting-note-list"));
const hasHistoricalFact = document.body.textContent.includes("Automatic note generation is no longer available.");
const hasWithheldDraft = Boolean(q('[data-note-recovery="saved-note-unreadable"]'));
const hasPlayback = Boolean(q('[data-action="play-retained-audio"]'));
const steps = [];
if (notes && transcript) {
  const original = notes.value;
  notes.value = "Synthetic personal note: verify the owner.";
  notes.dispatchEvent(new Event("input", { bubbles: true }));
  await sleep(900);
  steps.push({ name: "personal note saved", ok: window.__harnessCalls.includes("library_save_operator_note") });
  notes.value = original;
  notes.dispatchEvent(new Event("input", { bubbles: true }));
  await sleep(900);
  transcript.open = true;
  await sleep(150);
  for (const [action, command] of [
    ["open-library-transcript-file", "library_open_transcript_file"],
    ["export-meeting", "library_export_meeting"],
  ]) {
    const control = q(`[data-action="${action}"]`);
    control?.click();
    await sleep(200);
    steps.push({ name: action, ok: Boolean(control && !control.disabled) && window.__harnessCalls.includes(command) });
  }
  if (fixtureMode === "saved-draft") {
    const source = q('[data-action="open-evidence-split"]');
    source?.click();
    await sleep(350);
    steps.push({ name: "saved draft source link", ok: Boolean(source) && Boolean(q('#evidence-split-column .transcript-turn.highlighted')) });
    q('[data-action="close-evidence-split"]')?.click();
  }
  // Leave the first view at its natural entry position for a cold reader.
  transcript.open = false;
  document.querySelector(".document-scroll")?.scrollTo(0, 0);
}
return {
  mode: fixtureMode, notesPresent: Boolean(notes), transcriptPresent: Boolean(transcript),
  noGenerationControl, hasSavedDraft, hasHistoricalFact, hasWithheldDraft, hasPlayback, steps,
  errors: window.__errors || [],
  pass: Boolean(notes) && Boolean(transcript) && noGenerationControl
    && steps.every((step) => step.ok) && !(window.__errors || []).length
    && (fixtureMode === "saved-draft" ? hasSavedDraft : true)
    && (fixtureMode === "summary-failed" ? hasHistoricalFact : true)
    && (fixtureMode === "saved-draft-unreadable" ? hasWithheldDraft && !hasSavedDraft && hasPlayback : true),
};
