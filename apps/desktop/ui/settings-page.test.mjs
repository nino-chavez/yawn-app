import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { resolveTheme, APPEARANCES } from "./appearance.mjs";
import { contentView } from "./view-model.mjs";

// Operator decision, 2026-10-16: Settings is a page in the main window,
// reached from the sidebar gear, the Settings menu item and ⌘,, and it offers
// System, Light or Dark.

const ready = { startup: "ready", capture: "idle" };

test("an open Settings page takes the document pane, but a recording still wins", () => {
  assert.equal(contentView({ hasInvoke: true, snapshot: ready, settingsOpen: true }), "settings");
  assert.equal(contentView({ hasInvoke: true, snapshot: ready, settingsOpen: true, hasSelected: true }), "settings");
  assert.equal(contentView({ hasInvoke: true, snapshot: { startup: "ready", capture: "recording" }, settingsOpen: true }), "capture");
  assert.equal(contentView({ hasInvoke: true, snapshot: ready }), "home");
});

test("System follows the Mac; Light and Dark hold regardless of it", () => {
  assert.deepEqual(APPEARANCES, ["system", "light", "dark"]);
  assert.equal(resolveTheme("system", true), "dark");
  assert.equal(resolveTheme("system", false), "light");
  assert.equal(resolveTheme("light", true), "light");
  assert.equal(resolveTheme("dark", false), "dark");
});

test("every Settings route opens the in-app page, never the separate window", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.doesNotMatch(main, /invoke\("open_settings_window"\)/);
  assert.match(main, /data-action="open-settings"[^>]*aria-label="Settings"/);
  assert.match(main, /tauriListen\("menu:open-settings"/);
  // The host is preserved across renders so the mounted page survives the
  // 900 ms poll; dom-patch honors the marker.
  assert.match(main, /id="settings-root" data-patch-preserve/);
  const patcher = await readFile(new URL("./dom-patch.mjs", import.meta.url), "utf8");
  assert.match(patcher, /hasAttribute\("data-patch-preserve"\)/);
});

test("the Settings page keeps its clicks from reaching the main window's handler", async () => {
  const settings = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  assert.doesNotMatch(settings, /data-action="/);
  assert.match(settings, /event\.stopPropagation\?\.\(\)/);
  assert.doesNotMatch(settings, /document\.(querySelector|addEventListener)/);
});

test("dark-only values follow the app's theme attribute, not the system media query", async () => {
  const styles = await readFile(new URL("./styles.css", import.meta.url), "utf8");
  assert.doesNotMatch(styles, /@media \(prefers-color-scheme: dark\)/);
});

test("an unmounted Settings page arms no further polls", async () => {
  const settings = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  for (const name of ["scheduleTranscriptionEnginePoll", "scheduleSpeakerModelPoll", "scheduleModelPoll", "scheduleUpdatePoll"]) {
    const body = settings.slice(settings.indexOf(`function ${name}()`), settings.indexOf(`function ${name}()`) + 120);
    assert.match(body, /if \(disposed\) return;/, name);
  }
  assert.match(settings, /return \(\) => \{\n\s+disposed = true;/);
});
