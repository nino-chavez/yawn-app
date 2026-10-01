// Fixture-only Settings gate. It loads the production Settings markup and
// module through the local harness bridge, then proves retired note setup is
// neither shown nor invoked.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const q = (selector) => document.querySelector(selector);
for (let attempt = 0; attempt < 80 && !q("#storage"); attempt += 1) await sleep(50);

const text = document.body.textContent;
const navigation = [...document.querySelectorAll(".settings-nav a")].map((link) => link.getAttribute("href"));
const noteCalls = (window.__harnessCalls || []).filter((command) => /note_model|install_note|remove_note|use-note/.test(command));

return {
  mode: "settings-retirement",
  navigation,
  settingsPresent: Boolean(q("#storage")),
  noNotesNavigation: !navigation.includes("#notes"),
  noNoteSetup: !text.includes("Meeting notes") && !text.includes("Local note model"),
  noteCalls,
  pass: Boolean(q("#storage")) && !navigation.includes("#notes")
    && !text.includes("Meeting notes") && !text.includes("Local note model")
    && noteCalls.length === 0,
};
