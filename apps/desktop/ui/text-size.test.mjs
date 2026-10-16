import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

// The text-size hook (2026-10-16): Settings offers Default and three larger
// steps, and every type size is its token times --text-scale.

function fakeDom({ stored, storageThrows = false } = {}) {
  const props = new Map();
  const store = new Map(stored === undefined ? [] : [["yawn.textSize", stored]]);
  const events = [];
  globalThis.document = {
    documentElement: {
      dataset: {},
      style: { setProperty: (name, value) => props.set(name, value) },
    },
  };
  globalThis.localStorage = {
    getItem: (key) => { if (storageThrows) throw new Error("refused"); return store.get(key) ?? null; },
    setItem: (key, value) => { if (storageThrows) throw new Error("refused"); store.set(key, value); },
  };
  globalThis.dispatchEvent = (event) => { events.push(event); return true; };
  return { props, store, events };
}

function clearDom() {
  delete globalThis.document;
  delete globalThis.localStorage;
  delete globalThis.dispatchEvent;
}

const mod = await import("./text-size.mjs");

test("the steps run from the standard size up, and only up", () => {
  assert.deepEqual(mod.TEXT_SIZES, ["default", "large", "larger", "largest"]);
  const scales = mod.TEXT_SIZES.map(mod.textScale);
  assert.equal(scales[0], 1);
  for (let i = 1; i < scales.length; i += 1) assert.ok(scales[i] > scales[i - 1]);
  assert.equal(mod.textScale("huge"), 1);
});

test("the scale lands on the root element and asks measured layout to run again", () => {
  const { props, events } = fakeDom();
  mod.applyTextSize("largest");
  assert.equal(props.get("--text-scale"), String(mod.TEXT_SCALES.largest));
  assert.equal(globalThis.document.documentElement.dataset.textSize, "largest");
  assert.equal(events.length, 1);
  assert.equal(events[0].type, mod.TEXT_SIZE_EVENT);
  clearDom();
});

test("a stored choice survives a relaunch; an unknown or missing one is Default", () => {
  let dom = fakeDom({ stored: "larger" });
  assert.equal(mod.getTextSize(), "larger");
  mod.applyTextSize();
  assert.equal(dom.props.get("--text-scale"), String(mod.TEXT_SCALES.larger));
  dom = fakeDom({ stored: "enormous" });
  assert.equal(mod.getTextSize(), "default");
  dom = fakeDom();
  assert.equal(mod.getTextSize(), "default");
  clearDom();
});

test("setting a size stores it; an unknown size changes nothing", () => {
  const { props, store } = fakeDom();
  mod.setTextSize("large");
  assert.equal(store.get("yawn.textSize"), "large");
  assert.equal(props.get("--text-scale"), String(mod.TEXT_SCALES.large));
  mod.setTextSize("enormous");
  assert.equal(store.get("yawn.textSize"), "large");
  assert.equal(props.get("--text-scale"), String(mod.TEXT_SCALES.large));
  clearDom();
});

test("a refused storage still applies the size for this session", () => {
  const { props } = fakeDom({ storageThrows: true });
  assert.equal(mod.getTextSize(), "default");
  mod.setTextSize("largest");
  assert.equal(props.get("--text-scale"), String(mod.TEXT_SCALES.largest));
  clearDom();
});

// Any type size written as a fixed px value would ignore the setting.
export function rawPxTypeSizes(css) {
  return css.split("\n")
    .map((line, index) => ({ line: line.trim(), number: index + 1 }))
    .filter(({ line }) => /(?:font-size|line-height)\s*:\s*[0-9.]+px\b/.test(line)
      || /\bfont\s*:[^;]*\b[0-9.]+px\b/.test(line));
}

test("the scan catches a fixed px type size", () => {
  assert.equal(rawPxTypeSizes(".a { font-size: 12px; }").length, 1);
  assert.equal(rawPxTypeSizes(".a { line-height: 18px; }").length, 1);
  assert.equal(rawPxTypeSizes(".a { font: 600 12px/1.4 var(--font); }").length, 1);
  assert.equal(rawPxTypeSizes(".a { font-size: calc(12px * var(--text-scale)); }").length, 0);
  assert.equal(rawPxTypeSizes(".a { font-size: var(--t-small); line-height: 1.4; }").length, 0);
});

test("no stylesheet sets a type size the text-size setting cannot reach", async () => {
  for (const name of ["tokens.css", "styles.css", "settings-window.css", "settings-standalone.css"]) {
    const css = await readFile(new URL(`./${name}`, import.meta.url), "utf8");
    assert.deepEqual(rawPxTypeSizes(css), [], name);
  }
  const tokens = await readFile(new URL("./tokens.css", import.meta.url), "utf8");
  assert.match(tokens, /--text-scale: 1;/);
  for (const token of ["label", "caption", "small", "body", "transcript", "read", "title", "large"]) {
    assert.match(tokens, new RegExp(`--t-${token}: calc\\([0-9.]+px \\* var\\(--text-scale\\)\\)`), token);
  }
});

test("Settings offers the text size beside the theme, and both windows apply it", async () => {
  const settings = await readFile(new URL("./settings.js", import.meta.url), "utf8");
  assert.match(settings, /id="text-size-choice"/);
  assert.match(settings, /data-settings-action="text-size"/);
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.match(main, /applyTextSize\(\)/);
  assert.match(main, /addEventListener\?\.\(TEXT_SIZE_EVENT, [^\n]*layoutLedgers/);
  const standalone = await readFile(new URL("./settings-window.js", import.meta.url), "utf8");
  assert.match(standalone, /applyTextSize\(\)/);
});

test("the after-the-meeting label centers on its line at any size", async () => {
  const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
  assert.doesNotMatch(main, /heights\[index\] \/ 2 - 13\}px/);
  assert.match(main, /after\.offsetHeight \/ 2/);
});
