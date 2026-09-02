import { meetings, noteMeeting, during, attention, states } from "../shared/fixtures.js";
import { state } from "../shared/state.js";

const sidebarEl = document.getElementById("sidebar");
const contentEl = document.getElementById("content");
const recordControlEl = document.getElementById("record-control");
const windowTitleEl = document.getElementById("window-title");
const toggleEl = document.getElementById("sidebar-toggle");

document.title = `Yawn — Concept A — state ${state}: ${states[state] || ""}`;

const GROUP_ORDER = ["Today", "Yesterday", "Previous 30 days"];

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[c]));
}

function groupList(list) {
  const byGroup = {};
  list.forEach((m) => { (byGroup[m.group] ||= []).push(m); });
  return GROUP_ORDER.filter((g) => byGroup[g]);
}

function rowHTML(m, selectedId) {
  const selected = m.id === selectedId;
  const dot = m.status === "recovered-interrupted"
    ? '<span class="attention-dot" aria-hidden="true"></span>'
    : "";
  return `
    <div class="row${selected ? " selected" : ""}" data-id="${m.id}">
      <div class="row-title">${dot}<span>${escapeHtml(m.title)}</span></div>
      <div class="row-caption caption">${escapeHtml(m.time)} · ${escapeHtml(m.length)} · ${escapeHtml(m.statusText)}</div>
      <div class="row-excerpt">${escapeHtml(m.excerpt)}</div>
    </div>`;
}

function buildSidebar({ list, selectedId, recordingRow }) {
  const groups = groupList(list);
  const byGroup = {};
  list.forEach((m) => { (byGroup[m.group] ||= []).push(m); });

  const recordingHTML = recordingRow ? `
    <div class="row recording-now">
      <div class="row-title"><span>${escapeHtml(recordingRow.label)}</span></div>
      <div class="row-caption caption">${escapeHtml(recordingRow.time)}</div>
    </div>` : "";

  const groupsHTML = groups.map((g) => `
    <div class="group-label">${escapeHtml(g)}</div>
    ${byGroup[g].map((m) => rowHTML(m, selectedId)).join("")}
  `).join("");

  sidebarEl.innerHTML = `
    <div class="sidebar-scroll">
      ${recordingHTML}
      ${groupsHTML}
    </div>
    <div class="trash-row">
      <svg width="13" height="14" viewBox="0 0 13 14" fill="none" aria-hidden="true">
        <path d="M1 3.5h11M4.5 3.5V2a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5M2.5 3.5l.6 9a1 1 0 0 0 1 .95h4.8a1 1 0 0 0 1-.95l.6-9"
          stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
      <span>Trash</span>
    </div>`;
}

function buildRecordIdle() {
  recordControlEl.innerHTML = `<button class="btn record record-idle" title="Record (⌘R)">Record</button>`;
}

function buildRecordLive() {
  recordControlEl.innerHTML = `
    <span class="btn record live record-live-badge" title="Recording">${escapeHtml(during.elapsed)}</span>
    <button class="btn" title="Pause">Pause</button>
    <button class="btn" title="Stop (⌘.)">Stop</button>`;
}

function buildEmpty() {
  contentEl.innerHTML = `
    <div class="empty-pane">
      <p class="caption">Select a meeting, or press Record</p>
    </div>`;
}

function buildDuring() {
  const pausedText = during.pauses === 1
    ? `paused once, ${during.pausedFor}`
    : `paused ${during.pauses} times, ${during.pausedFor}`;
  contentEl.innerHTML = `
    <div class="canvas-wrap">
      <div class="canvas-caption caption">${escapeHtml(pausedText)}</div>
      <textarea class="notes-area" spellcheck="false">${escapeHtml(during.notes)}</textarea>
      <details class="transcript-disclosure">
        <summary>Live transcript (${during.liveTurns.length} turns)</summary>
        <div class="transcript-list">
          ${during.liveTurns.map((t) => `
            <div class="transcript-turn">
              <div class="t-meta"><span class="t-who">${escapeHtml(t.who)}</span><span class="caption">${escapeHtml(t.t)}</span></div>
              <p>${escapeHtml(t.text)}</p>
            </div>`).join("")}
        </div>
      </details>
      <span class="fixture-tag">${escapeHtml(during.fixture)}</span>
    </div>`;
}

function claimHTML(item, openSource) {
  const open = item.source === openSource;
  return `<li><span class="claim${open ? " open" : ""}" data-source="${item.source}">${escapeHtml(item.text)}</span></li>`;
}

function buildAfter() {
  const n = noteMeeting.note;
  const openSource = 2; // "Ship the retry review before Friday."
  contentEl.innerHTML = `
    <div class="doc-wrap">
      <article class="read">
        <h1>${escapeHtml(noteMeeting.title)}</h1>
        <p class="caption doc-caption">${escapeHtml(noteMeeting.date)} · ${escapeHtml(noteMeeting.time)} · ${escapeHtml(noteMeeting.length)} · ${escapeHtml(noteMeeting.statusText)}</p>
        <h2>Overview</h2>
        <p>${escapeHtml(n.overview)}</p>
        <h2>Decisions</h2>
        <ul>${n.decisions.map((d) => claimHTML(d, openSource)).join("")}</ul>
        <h2>Follow-ups</h2>
        <ul>${n.followups.map((d) => claimHTML(d, openSource)).join("")}</ul>
        <h2>Open questions</h2>
        <ul>${n.questions.map((d) => claimHTML(d, openSource)).join("")}</ul>
      </article>
      <span class="fixture-tag">${escapeHtml(noteMeeting.fixture)}</span>
    </div>
    <aside class="inspector">
      <div class="inspector-label">Source</div>
      ${noteMeeting.transcript
        .filter((t) => t.i >= openSource - 1 && t.i <= openSource + 1)
        .map((t) => `
          <div class="transcript-turn${t.i === openSource ? " highlighted" : " dim"}">
            <div class="t-meta"><span class="t-who">${escapeHtml(t.who)}</span><span class="caption">${escapeHtml(t.t)}</span></div>
            <p>${escapeHtml(t.text)}</p>
          </div>`).join("")}
      <button class="btn">Open full transcript</button>
    </aside>`;
}

function buildAttention() {
  contentEl.innerHTML = `
    <div class="attention-pane">
      <h1 class="attention-headline">${escapeHtml(attention.headline)}</h1>
      <p class="attention-detail">${escapeHtml(attention.detail)}</p>
      <div class="attention-actions">
        <button class="btn primary">${escapeHtml(attention.actions[0])}</button>
        <button class="btn">${escapeHtml(attention.actions[1])}</button>
      </div>
    </div>`;
}

function render() {
  switch (state) {
    case 2:
      windowTitleEl.textContent = "New Recording";
      buildSidebar({ list: meetings, selectedId: null, recordingRow: { label: "Recording now", time: during.elapsed } });
      buildRecordLive();
      buildDuring();
      break;
    case 3:
      windowTitleEl.textContent = noteMeeting.title;
      buildSidebar({ list: [noteMeeting, ...meetings], selectedId: noteMeeting.id, recordingRow: null });
      buildRecordIdle();
      buildAfter();
      break;
    case 4:
      windowTitleEl.textContent = attention.meeting.title;
      buildSidebar({ list: meetings, selectedId: attention.meeting.id, recordingRow: null });
      buildRecordIdle();
      buildAttention();
      break;
    default:
      windowTitleEl.textContent = "Yawn";
      buildSidebar({ list: meetings, selectedId: null, recordingRow: null });
      buildRecordIdle();
      buildEmpty();
  }
}

render();

// Lightweight interactivity — not required by the fixture states, harmless for the static capture.
toggleEl.addEventListener("click", () => {
  document.getElementById("app").classList.toggle("sidebar-collapsed");
});
sidebarEl.addEventListener("click", (e) => {
  const row = e.target.closest(".row[data-id]");
  if (!row) return;
  sidebarEl.querySelectorAll(".row.selected").forEach((r) => r.classList.remove("selected"));
  row.classList.add("selected");
});
