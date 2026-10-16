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

return {
  name: contract.name,
  viewport: contract.viewport,
  state: contract.state,
  pass: checks.every((check) => check.pass),
  checks,
};
