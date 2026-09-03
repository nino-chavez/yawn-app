// Tonal Ledger Canvas geometry gate. Runs the real production frontend in the
// WKWebView harness against the transcript-only fixture, then compares browser
// computed values with the single selected-reference contract.
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
if (!row) return { state: "transcript-only / no meeting note", error: "meeting row never appeared" };
row.click();
if (!await waitFor(".generate-note-section .btn")) {
  return { state: "transcript-only / no meeting note", error: "transcript-only fixture did not render Generate note" };
}

const contract = await fetch("tonal-ledger-canvas.contract.json").then((response) => response.json());
const read = (element, check) => {
  const style = getComputedStyle(element);
  if (check.measurement === "rect.height") return element.getBoundingClientRect().height;
  if (check.measurement === "distance.top") {
    const relative = q(check.relativeTo);
    return relative ? element.getBoundingClientRect().top - relative.getBoundingClientRect().top : null;
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
