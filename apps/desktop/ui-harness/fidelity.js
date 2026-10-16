// Rail geometry gate (DESIGN.md, selected 2026-10-15). Runs the real
// production frontend in the WKWebView harness against the ledger fixture --
// a transcript-ready meeting with seven timed lines -- then compares browser
// computed values with the single contract.
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

const row = await waitFor('[data-action="open-meeting"]');
if (!row) return { state: "ledger / transcript ready", error: "meeting row never appeared" };
row.click();
if (!await waitFor(".ledger.has-rail.has-turns")) {
  return { state: "ledger / transcript ready", error: "the ledger fixture did not render a rail with a turn column" };
}
if (q("[data-action='generate-note']") || document.body.textContent.includes("Generate note")) {
  return { state: "ledger / transcript ready", error: "retired generation control rendered" };
}
await sleep(120);

const contract = await fetch("rail.contract.json").then((response) => response.json());
const read = (element, check) => {
  const style = getComputedStyle(element);
  if (check.measurement === "rect.height") return element.getBoundingClientRect().height;
  if (check.measurement === "distance.top") {
    const relative = q(check.relativeTo);
    return relative ? element.getBoundingClientRect().top - relative.getBoundingClientRect().top : null;
  }
  if (check.measurement === "distance.left") {
    const relative = q(check.relativeTo);
    return relative ? element.getBoundingClientRect().left - relative.getBoundingClientRect().left : null;
  }
  if (check.measurement === "distance.after") {
    const relative = q(check.relativeTo);
    return relative ? element.getBoundingClientRect().top - relative.getBoundingClientRect().bottom : null;
  }
  const property = check.measurement.replace("style.", "");
  return Number.parseFloat(style[property]);
};

const checks = contract.checks.map((check) => {
  const element = q(check.selector);
  if (!element) return { ...check, actual: null, pass: false, reason: "selector missing" };
  const actual = read(element, check);
  const pass = Number.isFinite(actual) && Math.abs(actual - check.expected) <= contract.tolerancePx;
  return { id: check.id, expected: check.expected, actual, pass };
});

// The open fold (DESIGN.md, Fold row: expanded). Press the first fold the
// way a person does, then check that it is a disclosure that says it is open,
// that its turns sit under it and above the next turn row, that no row in the
// turn column overlaps another, that the note did not move, and that Escape
// closes it and gives focus back to it.
const noteRect = () => {
  const editor = q(".ledger-editor").getBoundingClientRect();
  const lines = Array.from(document.querySelectorAll(".ledger-mirror-line")).map((line) => line.getBoundingClientRect().top);
  return { top: editor.top, height: editor.height, left: editor.left, width: editor.width, lines };
};
const foldCheck = (id, pass, actual) => ({ id, expected: true, actual, pass: Boolean(pass) });
const before = noteRect();
const fold = q(".ledger-fold");
const turnRows = () => Array.from(document.querySelectorAll(".ledger-turns > .ledger-turn, .ledger-turns > .ledger-fold, .ledger-turns > .ledger-fold-turns"))
  .map((element) => ({ element, rect: element.getBoundingClientRect() }))
  .sort((a, b) => a.rect.top - b.rect.top);
if (fold) {
  checks.push(foldCheck("fold-is-closed-disclosure", fold.tagName === "BUTTON" && fold.getAttribute("aria-expanded") === "false", `${fold.tagName} aria-expanded=${fold.getAttribute("aria-expanded")}`));
  const key = fold.dataset.fold;
  fold.focus();
  fold.click();
  await sleep(250);
  const openFold = q(`.ledger-fold[data-fold="${key}"]`);
  const opened = q(`.ledger-fold-turns[data-fold="${key}"]`);
  const nextTurn = q(`.ledger-turn[data-fold="${key}"]`);
  checks.push(foldCheck("fold-opens", openFold?.getAttribute("aria-expanded") === "true" && opened && opened.id === openFold.getAttribute("aria-controls"), openFold?.getAttribute("aria-expanded")));
  checks.push(foldCheck("fold-keeps-focus", document.activeElement === openFold, document.activeElement?.className));
  if (opened && openFold && nextTurn) {
    const foldBox = openFold.getBoundingClientRect();
    const openedBox = opened.getBoundingClientRect();
    const turnBox = nextTurn.getBoundingClientRect();
    checks.push(foldCheck("fold-turns-under-fold", openedBox.top >= foldBox.bottom - 0.5, openedBox.top - foldBox.bottom));
    checks.push(foldCheck("next-turn-pushed-below", turnBox.top >= openedBox.bottom + 4, turnBox.top - openedBox.bottom));
    checks.push(foldCheck("fold-turn-count", opened.querySelectorAll(".ledger-folded-turn").length === Number((openFold.getAttribute("aria-label").match(/\d+/) || [0])[0]), opened.querySelectorAll(".ledger-folded-turn").length));
  }
  const rows = turnRows();
  const overlaps = rows.filter((row, index) => index > 0 && row.rect.top < rows[index - 1].rect.bottom - 0.5).length;
  checks.push(foldCheck("turn-column-no-overlap", overlaps === 0, overlaps));
  const after = noteRect();
  const moved = ["top", "height", "left", "width"].some((side) => Math.abs(after[side] - before[side]) > 0.5)
    || after.lines.some((top, index) => Math.abs(top - before.lines[index]) > 0.5);
  checks.push(foldCheck("note-does-not-move", !moved, moved ? "moved" : "still"));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await sleep(250);
  const closed = q(`.ledger-fold[data-fold="${key}"]`);
  checks.push(foldCheck("escape-closes-fold", closed?.getAttribute("aria-expanded") === "false" && !q(`.ledger-fold-turns[data-fold="${key}"]`) && document.activeElement === closed, closed?.getAttribute("aria-expanded")));
} else {
  checks.push(foldCheck("fold-present", false, "no fold rendered"));
}

return {
  name: contract.name,
  viewport: contract.viewport,
  state: contract.state,
  pass: checks.every((check) => check.pass),
  checks,
};
