// Real UI in WKWebView; the bridge uses only invented meetings.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
const wait = async (condition) => {
  for (let i = 0; i < 60; i += 1) { if (condition()) return true; await sleep(50); }
  return false;
};
const steps = [];
const check = (name, ok) => { steps.push({ name, ok: Boolean(ok) }); if (!ok) throw new Error(name); };
const click = (action) => { const button = q(`[data-action="${action}"]`); check(`${action} is reachable`, button && !button.disabled); button.click(); };
const key = (key, shiftKey = false) => document.dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey, bubbles: true, cancelable: true }));
const finish = () => { document.getAnimations().forEach((animation) => animation.finish()); return { steps, errors: window.__errors }; };
const frame = new URLSearchParams(location.search).get("frame");
await wait(() => q('[data-action="open-trash"]'));
click("open-trash");
await wait(() => q(".trash-list li"));
check("Trash lists both meetings", document.querySelectorAll(".trash-list li").length === 2);
await wait(() => !q('[data-action="empty-trash"]')?.disabled);
if (frame === "populated") return finish();
if (frame === "unavailable") {
  window.__harnessTrash.state = "unavailable";
  click("open-trash");
  await wait(() => q('.trash-pane [role="alert"]'));
  check("unavailable Trash is not presented as empty", !q(".trash-list") && q('[data-action="empty-trash"]').disabled);
  return finish();
}
click("empty-trash");
await wait(() => q('[data-action="confirm-empty-trash"]'));
check("Cancel gets initial focus", document.activeElement === q('[role="dialog"] [data-action="close-modal"]'));
check("opening the confirmation never deletes", !window.__harnessCalls.includes("empty_trash"));
check("confirmation exposes name and description", q('[role="dialog"]').getAttribute("aria-describedby") === "empty-trash-detail");
if (frame === "confirm") return finish();
if (!frame) {
  key("Tab", true);
  check("reverse Tab stays in the dialog", document.activeElement === q('[data-action="confirm-empty-trash"]'));
  key("Tab");
  check("Tab cycles to Cancel", document.activeElement === q('[role="dialog"] [data-action="close-modal"]'));
  click("close-modal");
  await sleep(50);
  check("Cancel closes without deleting", !q('[role="dialog"]') && !window.__harnessCalls.includes("empty_trash"));
  check("Cancel returns focus to Empty Trash", document.activeElement === q('[data-action="empty-trash"]'));
  click("empty-trash");
  await wait(() => q('[data-action="confirm-empty-trash"]'));
  key("Escape");
  await sleep(50);
  check("Escape closes without deleting", !q('[role="dialog"]') && !window.__harnessCalls.includes("empty_trash"));
  click("empty-trash");
  await wait(() => q('[data-action="confirm-empty-trash"]'));
}
window.__harnessTrash.delay = frame === "busy" ? 10000 : 150;
if (["failure", "partial"].includes(frame)) window.__harnessTrash.outcome = frame;
const confirm = q('[data-action="confirm-empty-trash"]');
confirm.click();
confirm.click();
key("Escape");
check("busy confirmation cannot be dismissed or repeated", q('[role="dialog"]') && q('[data-action="confirm-empty-trash"]').disabled && !q('[role="dialog"] [data-action="close-modal"]'));
check("one confirmation dispatches one deletion", window.__harnessCalls.filter((command) => command === "empty_trash").length === 1);
if (frame === "busy") return finish();
await wait(() => !q('[role="dialog"]'));
if (frame === "failure" || frame === "partial") {
  check("failed deletion shows an error", q('[role="alert"]'));
  check("undeleted meetings remain visible", document.querySelectorAll(".trash-list li").length === (frame === "partial" ? 1 : 2));
  return finish();
}
check("successful deletion stays in Trash with an empty list", q(".trash-pane") && !q(".trash-list"));
check("empty Trash disables the control", q('[data-action="empty-trash"]').disabled);
check("focus returns to the empty Trash heading", document.activeElement === q("#trash-heading"));
if (!frame) {
  window.__harnessTrash.state = "unavailable";
  click("open-trash");
  await wait(() => q('.trash-pane [role="alert"]'));
  check("unavailable listing cannot authorize deletion", q('.trash-pane [role="alert"]') && q('[data-action="empty-trash"]').disabled);
}
return finish();
