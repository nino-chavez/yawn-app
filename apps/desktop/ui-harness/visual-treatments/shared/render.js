import { attentionMeeting, denseMeeting, noNoteMeeting, observedMeetings, sparseMeeting } from "./fixtures.js";

const params = new URLSearchParams(location.search);
const state = ["sparse", "no-note", "dense", "attention"].includes(params.get("state")) ? params.get("state") : "sparse";
const theme = ["dark", "light"].includes(params.get("theme")) ? params.get("theme") : "dark";
document.documentElement.dataset.state = state;
document.documentElement.dataset.theme = theme;

const list = document.getElementById("meeting-list");
const surface = document.getElementById("meeting-surface");
const toolbarTitle = document.getElementById("toolbar-title");
const recordReason = document.getElementById("record-reason");

const selectedId = state === "dense" ? denseMeeting.id : state === "attention" ? attentionMeeting.id : state === "no-note" ? noNoteMeeting.id : sparseMeeting.id;
const meetings = state === "dense"
  ? [{ id: denseMeeting.id, group: "Today", title: denseMeeting.title, meta: "10:05 AM · 24:18 · note ready", excerpt: "Reviewed the launch checklist and confirmed next steps." }, ...observedMeetings]
  : state === "no-note"
    ? [{ id: noNoteMeeting.id, group: "Previous 30 days", title: noNoteMeeting.title, meta: "5:29 PM · transcript available", excerpt: "" }, ...observedMeetings]
  : observedMeetings;

function esc(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[character]));
}

function slug(value) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");
}

function renderList() {
  const groups = [];
  for (const meeting of meetings) {
    let group = groups.find((item) => item.label === meeting.group);
    if (!group) {
      group = { label: meeting.group, meetings: [] };
      groups.push(group);
    }
    group.meetings.push(meeting);
  }
  list.innerHTML = groups.map((group) => `
    <section class="meeting-group" aria-labelledby="group-${slug(group.label)}">
      <h2 class="group-label" id="group-${slug(group.label)}">${esc(group.label)}</h2>
      ${group.meetings.map((meeting) => `
        <button class="meeting-row${meeting.id === selectedId ? " selected" : ""}" type="button">
          <span class="row-title">${meeting.attention ? '<i class="attention-dot" aria-hidden="true"></i>' : ""}${esc(meeting.title)}</span>
          <span class="row-meta">${esc(meeting.meta)}</span>
          ${meeting.excerpt ? `<span class="row-excerpt">${esc(meeting.excerpt)}</span>` : ""}
        </button>
      `).join("")}
    </section>
  `).join("");
}

function managementActions() {
  return `
    <div class="management-actions" aria-label="Meeting actions">
      <button class="button secondary-button" type="button">Rename</button>
      <button class="button secondary-button manage-button" type="button">Manage</button>
    </div>`;
}

function sparseView() {
  toolbarTitle.textContent = sparseMeeting.title;
  recordReason.textContent = sparseMeeting.recordReason;
  surface.innerHTML = `
    <div class="document-scroll">
      <article class="meeting-document sparse-document">
        <header class="document-header">
          <div class="document-heading">
            <h1>${esc(sparseMeeting.title)}</h1>
            <p class="document-meta">${esc(sparseMeeting.metadata)}</p>
          </div>
          ${managementActions()}
        </header>
        <p class="state-fact">${esc(sparseMeeting.stateFact)}</p>
        <section class="note-section overview-section">
          <h2>Overview</h2>
          ${sparseMeeting.overview.map((paragraph) => `<p>${esc(paragraph)}</p>`).join("")}
        </section>
        <section class="operator-notes" aria-labelledby="operator-notes-heading">
          <header class="notes-heading">
            <h2 id="operator-notes-heading">Your notes</h2>
            <span class="save-fact">Stored on this Mac</span>
          </header>
          <textarea aria-label="Your notes" placeholder="Write down the detail you will want to verify later."></textarea>
          <p class="notes-help">Saved separately from the transcript. These are your notes, not generated claims.</p>
        </section>
        <details class="transcript-disclosure">
          <summary>Full transcript <span aria-hidden="true">⌄</span></summary>
        </details>
      </article>
    </div>`;
}

function noNoteView() {
  toolbarTitle.textContent = noNoteMeeting.title;
  recordReason.textContent = noNoteMeeting.recordReason;
  surface.innerHTML = `
    <div class="document-scroll">
      <article class="meeting-document no-note-document">
        <header class="document-header">
          <div class="document-heading">
            <h1>${esc(noNoteMeeting.title)}</h1>
            <p class="document-meta">${esc(noNoteMeeting.metadata)}</p>
          </div>
          ${managementActions()}
        </header>
        <p class="state-fact">${esc(noNoteMeeting.stateFact)}</p>
        <section class="note-section no-note-state" aria-labelledby="no-note-heading">
          <h2 id="no-note-heading">No meeting note yet.</h2>
          <button class="button primary-button" type="button">Generate note</button>
          <p class="generation-help">${esc(noNoteMeeting.generationHelp)}</p>
        </section>
        <section class="operator-notes" aria-labelledby="operator-notes-heading">
          <header class="notes-heading">
            <h2 id="operator-notes-heading">Your notes</h2>
            <span class="save-fact">Stored on this Mac</span>
          </header>
          <textarea aria-label="Your notes" placeholder="Write down the detail you will want to verify later."></textarea>
          <p class="notes-help">Saved separately from the transcript. These are your notes, not generated claims.</p>
        </section>
        <details class="transcript-disclosure">
          <summary>Full transcript <span aria-hidden="true">⌄</span></summary>
        </details>
      </article>
    </div>`;
}

function listItems(items) {
  return `<ul>${items.map((item) => {
    const value = typeof item === "string" ? item : item.text;
    const source = typeof item === "object" && item.source;
    return `<li><button class="claim${source ? " open" : ""}" type="button">${esc(value)}</button></li>`;
  }).join("")}</ul>`;
}

function denseView() {
  toolbarTitle.textContent = denseMeeting.title;
  recordReason.textContent = "";
  surface.innerHTML = `
    <div class="document-scroll">
      <article class="meeting-document dense-document">
        <header class="document-header">
          <div class="document-heading">
            <h1>${esc(denseMeeting.title)}</h1>
            <p class="document-meta">${esc(denseMeeting.metadata)}</p>
          </div>
          ${managementActions()}
        </header>
        <section class="note-section"><h2>Overview</h2><p>${esc(denseMeeting.overview)}</p></section>
        <section class="note-section"><h2>Decisions</h2>${listItems(denseMeeting.decisions)}</section>
        <section class="note-section"><h2>Follow-ups</h2>${listItems(denseMeeting.followups)}</section>
        <section class="note-section"><h2>Open questions</h2>${listItems(denseMeeting.questions)}</section>
      </article>
    </div>
    <aside class="source-inspector" aria-label="Claim source">
      <h2>Source</h2>
      ${denseMeeting.transcript.map((turn) => `
        <article class="transcript-turn${turn.selected ? " selected" : ""}">
          <p class="turn-meta"><strong>${esc(turn.who)}</strong><span>${esc(turn.time)}</span></p>
          <p>${esc(turn.text)}</p>
        </article>
      `).join("")}
      <button class="button secondary-button" type="button">Open full transcript</button>
    </aside>`;
}

function attentionView() {
  toolbarTitle.textContent = attentionMeeting.title;
  recordReason.textContent = "Your last meeting needs attention before Yawn can record again.";
  surface.innerHTML = `
    <div class="document-scroll">
      <article class="meeting-document attention-document">
        <header class="document-header">
          <div class="document-heading">
            <h1>${esc(attentionMeeting.title)}</h1>
            <p class="document-meta">${esc(attentionMeeting.metadata)}</p>
          </div>
          ${managementActions()}
        </header>
        <section class="attention-state">
          <p class="state-label">Needs attention</p>
          <h2>${esc(attentionMeeting.headline)}</h2>
          <p>${esc(attentionMeeting.detail)}</p>
          <div class="attention-actions">
            <button class="button primary-button" type="button">Transcribe retained audio</button>
            <button class="button destructive-button" type="button">Move to Trash</button>
          </div>
        </section>
      </article>
    </div>`;
}

renderList();
if (state === "dense") denseView();
else if (state === "attention") attentionView();
else if (state === "no-note") noNoteView();
else sparseView();

document.title = `Yawn — ${document.documentElement.dataset.treatment} — ${state} — ${theme}`;
