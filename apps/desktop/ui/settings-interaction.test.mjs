import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

test("Settings switches both ways, preserves optional notes, and shows a rejected change", async () => {
  const nodes = new Map();
  const handlers = {};
  const calls = [];
  const node = (id) => {
    if (!nodes.has(id)) nodes.set(id, { innerHTML: "", textContent: "", dataset: {}, setAttribute() {} });
    return nodes.get(id);
  };
  let selected = "whisper";
  let refuse = false;
  let restarting = false;
  const timers = new Map();
  let timerId = 0;
  const engine = () => ({ selected, canChange: !restarting, operationActive: restarting, apple: { state: "ready", locale: "en-US", reason: null } });
  const models = { state: "ready", canChange: true, changeActive: false, options: [{ id: "small", title: "Smaller download", active: true, stored: true, downloadBytes: 463665005 }], error: null };
  const context = {
    document: { querySelector: node, addEventListener: (name, handler) => { handlers[name] = handler; } },
    window: { __TAURI__: { core: { invoke: async (command, args) => {
      calls.push(command);
      if (command === "get_transcription_engine_settings") return engine();
      if (command === "select_transcription_engine") {
        if (refuse) throw new Error("Finish queued transcription before changing the engine.");
        selected = args.engine;
        restarting = true;
        return engine();
      }
      if (command === "install_transcript_model") { selected = "whisper"; return {}; }
      if (command === "transcript_model_settings") return { ...structuredClone(models), canChange: !restarting, unavailableReason: restarting ? "Startup pending" : null };
      if (command === "note_model_settings") return { canChange: true, options: [], changeActive: false, unavailableReason: "No note model is installed." };
      if (command === "first_run_permissions") return { microphone: "authorized", systemAudio: "authorized" };
      throw new Error(`Unexpected command ${command}`);
    } } }, confirm: () => true },
    mergePermissions: (_, value) => value,
    setTimeout: (callback) => { timers.set(++timerId, callback); return timerId; },
    clearTimeout: (id) => timers.delete(id), console,
  };
  const source = readFileSync(new URL("./settings.js", import.meta.url), "utf8").replace(/^import[^\n]+\n/, "");
  vm.runInNewContext(source, context);
  const settle = async () => { for (let i = 0; i < 24; i++) await Promise.resolve(); };
  const click = async (action) => {
    handlers.click({ target: { closest: () => ({ disabled: false, dataset: { action, modelId: "small" } }) } });
    await settle();
  };
  await settle();
  assert.match(node("#models").innerHTML, /In use/);
  await click("use-apple-speech");
  assert.equal(selected, "apple-native");
  assert.match(node("#transcription-engine").innerHTML, /In Use/i);
  assert.doesNotMatch(node("#models").innerHTML, /In use/);
  assert.match(node("#models").innerHTML, /data-action="use-model"/);
  assert.match(node("#models").innerHTML, /data-action="use-model"[^>]*disabled/);
  restarting = false;
  for (const callback of [...timers.values()]) callback();
  await settle();
  assert.doesNotMatch(node("#models").innerHTML, /data-action="use-model"[^>]*disabled/);
  assert.notEqual(node("#model-message").textContent, "Startup pending");
  await click("use-model");
  assert.equal(selected, "whisper");
  assert.match(node("#models").innerHTML, /In use/);
  refuse = true;
  await click("use-apple-speech");
  assert.match(node("#transcription-engine").innerHTML, /Finish queued transcription/);
  assert.equal(selected, "whisper");
  assert.equal(calls.includes("install_note_model"), false);
});
