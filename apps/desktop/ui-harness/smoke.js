// Smoke pass over interactive flows with the in-place patcher (library mode).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const waitFor = async (selector, tries = 50) => {
  for (let i = 0; i < tries; i += 1) {
    const el = q(selector);
    if (el) return el;
    await sleep(100);
  }
  return null;
};
const result = { steps: [] };
const step = (name, ok, detail) => result.steps.push({ name, ok, ...(detail ? { detail } : {}) });

// 1. Start sheet: checkboxes and retention select survive per-change renders.
const record = await waitFor('[data-action="open-start"]');
if (!record) return { error: "no Record button", errors: window.__errors };
record.click();
await sleep(50);
const boxes = Array.from(document.querySelectorAll('[data-field="attestation"]'));
step("start sheet opens with 3 attestations", boxes.length === 3);
for (const box of boxes) box.click();
await sleep(50);
const boxesAfter = Array.from(document.querySelectorAll('[data-field="attestation"]'));
step("attestations stay checked across renders", boxesAfter.every((box) => box.checked));
const select = q('[data-field="retention-days"]');
select.value = "30";
select.dispatchEvent(new Event("change", { bubbles: true }));
await sleep(50);
step("retention select holds 30 after render", q('[data-field="retention-days"]').value === "30");
step("start button enabled after consents", !q('[data-action="start-recording"]')?.disabled);
q('[data-action="close-start"]').click();
await sleep(50);
step("start sheet closes", !q('[data-action="start-recording"]'));

// 2. Meeting view: library note editor, details, rename modal.
const row = await waitFor('[data-action="open-meeting"]');
row.click();
const noteEditor = await waitFor('[data-field="library-operator-note"]');
step("meeting opens with note editor", Boolean(noteEditor));
noteEditor.focus();
document.execCommand("insertText", false, "remember the owner");
await sleep(700); // past the 600 ms save timer
step("library note save reached stub", q("#library-note-save-state")?.textContent === "Saved on this Mac");
step("library note editor kept node identity", q('[data-field="library-operator-note"]') === noteEditor);
const details = q("details.transcript-disclosure");
details.open = true;
await sleep(20);
const renameOpen = q('[data-action="rename-meeting"]');
renameOpen.click();
await sleep(50);
const title = q('[data-field="meeting-title"]');
step("rename modal opens", Boolean(title));
title.focus();
document.execCommand("insertText", false, " renamed");
step("rename input accepts typing", title.value.endsWith(" renamed"));
q('[data-action="close-modal"]').click();
await sleep(50);
step("rename modal closes", !q('[data-field="meeting-title"]'));
step("details still open after modal churn", q("details.transcript-disclosure")?.open === true);

// 3. Vocabulary sheet over the transcript workspace.
const vocabularyButton = q('[data-action="open-vocabulary"]');
if (vocabularyButton) {
  vocabularyButton.click();
  await sleep(150);
  const before = q('[data-field="vocabulary-before"]');
  step("vocabulary sheet opens", Boolean(before));
  before?.focus();
  document.execCommand("insertText", false, "Kibbel");
  step("vocabulary input accepts typing", before?.value === "Kibbel");
  q('[data-action="close-modal"]')?.click();
  await sleep(50);
} else {
  step("vocabulary control present", false, "open-vocabulary button missing");
}

// 4. The Tonal Ledger shell keeps the library beside the document instead of
// navigating back to a separate meetings screen. Prove that persistent list,
// then exercise the footer's Trash route.
step("meeting list stays beside the open document", Boolean(q('[data-action="open-meeting"]')));
q('[data-action="open-trash"]')?.click();
await sleep(150);
step("trash route opens from the persistent sidebar", q("#trash-heading")?.textContent === "Trash");

result.errors = window.__errors;
return result;
