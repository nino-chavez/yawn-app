import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

// settings.js runs top-level DOM lookups and side effects at module load
// (document.querySelector against settings.html's fixed ids, then an
// immediate invoke() call), so it cannot be imported directly in a headless
// test -- the same constraint main.js's own tests work around (see
// dom-patch.test.mjs, view-model.test.mjs) by reading the source and
// asserting on its structure rather than executing it. These pin state and
// role: which branch a rejection routes to, and what that branch does or
// does not schedule -- not the exact sentence, beyond the marker string the
// branch itself keys on.

test("a rejection is read for its message before settings.js decides what kind of failure it is", async () => {
  const source = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  // The bare `catch {}` D-MODEL traced this to must be gone from
  // refreshModels specifically -- the sibling catches in useModel/removeModel/
  // refreshNoteModels etc. are untouched and out of this fix's scope.
  const fn = source.slice(source.indexOf("async function refreshModels"), source.indexOf("async function useModel"));
  assert.doesNotMatch(fn, /\}\s*catch\s*\{/, "refreshModels must read the rejection, not discard it");
  assert.match(fn, /catch \(error\)/);
  assert.match(fn, /const rejection = String\(error \|\| ""\);/);
  assert.match(fn, /modelBuiltIn = rejection\.includes\(MODEL_BUILT_IN_MARKER\);/);
});

test("the build-fact branch renders no attention tone and schedules no retry", async () => {
  const source = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  const renderFn = source.slice(source.indexOf("function renderModels"), source.indexOf("function renderNoteModels"));
  const buildFactBranch = renderFn.slice(renderFn.indexOf("if (modelBuiltIn)"), renderFn.indexOf("if (!models)"));
  // No `.state`/`data-tone="attention"` pill -- this isn't "checking",
  // "unavailable", or any of `tone()`'s known states, so no badge is invented.
  assert.doesNotMatch(buildFactBranch, /class="state"/);
  assert.doesNotMatch(buildFactBranch, /Retrying/);
  assert.match(buildFactBranch, /modelMessage\.dataset\.tone = "neutral";/);

  const pollFn = source.slice(source.indexOf("function scheduleModelPoll"), source.indexOf("function scheduleNoteModelPoll"));
  assert.match(pollFn, /if \(modelBuiltIn\) return;/);
});

test("every other rejection keeps the genuine-failure path: one row error, retried", async () => {
  const source = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  const refreshFn = source.slice(source.indexOf("async function refreshModels"), source.indexOf("async function useModel"));
  assert.match(refreshFn, /modelLoadError = modelBuiltIn \? "" : "Yawn could not check the saved speech model\.";/);
  const renderFn = source.slice(source.indexOf("function renderModels"), source.indexOf("function renderNoteModels"));
  // Unaffected by this change: a genuine failure still gets the "unavailable"
  // pill and the row's own "Retrying…" line (refit R11's one-line fix).
  assert.match(renderFn, /row\("Couldn't check speech model", "unavailable", `\$\{modelLoadError\} Retrying…`\)/);
});
