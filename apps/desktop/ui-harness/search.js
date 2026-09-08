// Search-probe browser harness. It drives the real sidebar and reader through
// synthetic backend responses only; the probe is still default-off in every
// normal harness mode and in product storage.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const waitFor = async (selector, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    const element = q(selector);
    if (element) return element;
    await sleep(100);
  }
  return null;
};
const result = { steps: [] };
const step = (name, ok, detail) => result.steps.push({ name, ok, ...(detail ? { detail } : {}) });

const search = await waitFor('[data-field="library-search"]');
if (!search) return { error: "library search never appeared", errors: window.__errors };

const setQuery = async (query) => {
  const field = q('[data-field="library-search"]');
  field.value = query;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await sleep(320); // past the title-search refresh debounce
};

if (mode === "search-capture") {
  await setQuery("budget");
  const unavailable = await waitFor('.transcript-search-affordance[data-state="unavailable"]');
  step("capture leaves the probe unavailable", unavailable?.textContent === "Search across meetings is unavailable while recording.");
  result.errors = window.__errors;
  return result;
}

const searchCurrentQuery = async (query) => {
  await setQuery(query);
  const action = await waitFor('[data-action="search-transcripts"]');
  if (!action) return null;
  action.click();
  await sleep(80);
  return q(".transcript-search-results");
};

const noResult = await searchCurrentQuery("noresult");
step("no-result response stays explicit", noResult?.dataset.state === "no-results" && /No retained transcript/.test(noResult.textContent));

const incomplete = await searchCurrentQuery("incomplete");
step("incomplete response keeps its coverage warning", incomplete?.dataset.state === "incomplete" && /could not be searched/.test(incomplete.textContent));

const withheld = await searchCurrentQuery("withheld");
step("withheld result never renders invented source text", /withheld this matching turn/.test(withheld?.textContent || "") && !/exact budget decision/i.test(withheld?.textContent || ""));

const metadata = await searchCurrentQuery("metadata");
metadata?.querySelector('[data-action="open-search-result"]')?.click();
await sleep(120);
step("metadata-only hit opens normally without inventing a turn target", q("#meeting-title")?.textContent === "Earlier harness meeting" && !q(".transcript-line-target") && q("details.transcript-disclosure")?.open === false);

const changed = await searchCurrentQuery("changed");
changed?.querySelector('[data-action="open-search-result"]')?.click();
await sleep(120);
step("a changed transcript digest refuses the old locator", !q(".transcript-line-target") && /Transcript changed; search again\./.test(q("#toast-notice")?.textContent || ""));

const match = await searchCurrentQuery("budget");
const openMatch = match?.querySelector('[data-action="open-search-result"]');
step("cross-meeting retained match is offered", Boolean(openMatch));
openMatch?.click();
const target = await waitFor('.transcript-line-target[data-turn-index="1"]');
await sleep(30);
step("cross-meeting match opens the matching turn", q("#meeting-title")?.textContent === "Earlier harness meeting" && target?.textContent.includes("exact budget decision"));
step("matching turn is focused in the open transcript", q("details.transcript-disclosure")?.open === true && document.activeElement === target);

q('[data-action="open-speaker-correction"][data-source-turn-index="1"]')?.click();
await sleep(40);
step("the system-audio channel label explains its shared scope", /names the whole channel, not individual voices\./.test(q(".speaker-correction-sheet")?.textContent || ""));
q('[data-action="close-modal"]')?.click();
await sleep(30);
q('[data-action="open-speaker-correction"][data-source-turn-index="0"]')?.click();
await sleep(40);
step("a microphone channel does not get the system-audio warning", !/names the whole channel, not individual voices\./.test(q(".speaker-correction-sheet")?.textContent || ""));
q('[data-action="close-modal"]')?.click();
await sleep(30);

await setQuery("withheld");
const withheldAgain = await waitFor('[data-action="search-transcripts"]');
withheldAgain?.click();
await sleep(80);
q('[data-action="open-search-result"]')?.click();
const withheldTarget = await waitFor('.transcript-line-target[data-turn-index="2"]');
step("withheld turn can be located without restoring its text", /This turn was withheld by the voice check\./.test(withheldTarget?.textContent || "") && !/exact budget decision/i.test(withheldTarget?.textContent || ""));

const transcriptSearch = q('[data-field="transcript-search"]');
transcriptSearch.value = "Opening";
transcriptSearch.dispatchEvent(new Event("input", { bubbles: true }));
await sleep(30);
step("a new transcript query clears the prior result focus", !q(".transcript-line-target"));

const slow = await searchCurrentQuery("slow");
slow?.querySelector('[data-action="open-search-result"]')?.click();
q('[data-action="open-trash"]')?.click();
await sleep(240);
step("leaving the reader wins over a slower search open", q("#trash-heading")?.textContent === "Trash" && !q("#meeting-title"));

result.errors = window.__errors;
return result;
