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

test("settings groups follow grouped-list structure: heading outside, rows inside, aria-labelledby", async () => {
  // Refit R18: settings groups use the macOS System Settings grouped-list
  // pattern: a group heading (h2) outside the .settings-group-box, rows inside
  // with aria-labelledby connecting them. No label copy is asserted.
  const html = await readFile(new URL("./settings.html", import.meta.url), "utf8");

  // Check for the grouped-list structure in the HTML:
  // 1. Each section.settings-group has aria-labelledby
  // 2. Each section contains an h2 followed by .settings-group-box
  // 3. The h2 has an id matching the section's aria-labelledby

  // Pattern: <section ... aria-labelledby="IDNAME"> ... <h2 id="IDNAME">
  assert.match(html, /aria-labelledby="[a-z\-]+".*?<h2 id="[a-z\-]+"/s);

  // Find all sections and their aria-labelledby values
  const sectionPattern = /section[^>]*class="[^"]*settings-group[^"]*"[^>]*aria-labelledby="([^"]+)"/g;
  const sections = [...html.matchAll(sectionPattern)];
  assert.ok(sections.length > 0, "settings.html must contain section.settings-group elements");

  sections.forEach((match) => {
    const labelledBy = match[1];
    // Check that this h2 id exists in the HTML
    const h2Pattern = new RegExp(`<h2[^>]*id="${labelledBy}"`, "s");
    assert.match(html, h2Pattern, `h2#${labelledBy} referenced by aria-labelledby must exist`);
  });

  // Verify that .settings-group-box elements exist and follow the h2
  assert.match(html, /<h2[^>]*id="[^"]*".*?<\/h2>.*?<div[^>]*class="[^"]*settings-group-box/s,
    "section must have heading outside the box, then .settings-group-box inside");
});

test("button primary actions follow one-per-group rule", async () => {
  // Refit R18: one primary (accent-filled) button per group maximum.
  // Permission rows: primary. Model rows: only the unstored option is primary.
  const source = await readFile(new URL("./settings.js", import.meta.url), "utf8");

  // Check that model/note-model buttons apply the "primary" class (allow-button)
  // only when the option is not stored.
  const renderModels = source.slice(source.indexOf("function renderModels"), source.indexOf("function renderNoteModels"));
  assert.match(
    renderModels,
    /const isPrimaryUseAction = !option\.stored;/,
    "must determine primary action per option, based on stored status"
  );
  assert.match(
    renderModels,
    /class="\$\{isPrimaryUseAction \? "allow-button" : "quiet-button"\}"/,
    "must apply allow-button (primary) only for unstored options"
  );
});

test("settings owns native speech preparation and guarded switching", async () => {
  const source = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  assert.match(source, /get_transcription_engine_settings/);
  assert.match(source, /install_apple_speech_assets/);
  assert.match(source, /select_transcription_engine/);
  assert.match(source, /transcriptionEngine\?\.canChange === false/);
  assert.match(source, /Apple manages this download in macOS/);
});

test("the settings review harness exposes native engine states and actions", async () => {
  const source = await readFile(new URL("./review/harness.js", import.meta.url), "utf8");
  assert.match(source, /get_transcription_engine_settings/);
  assert.match(source, /install_apple_speech_assets/);
  assert.match(source, /select_transcription_engine/);
  assert.match(source, /\|\| "whisper"/);
  assert.match(source, /window\.__reviewSelectedEngine = args\?\.engine/);
  assert.match(source, /selected: "apple-native"/);
});
