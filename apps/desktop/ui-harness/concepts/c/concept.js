// Concept C — Dated stream. List and document are the same column: renders
// the meeting list grouped by day as one scrolling stream, with the current
// or selected meeting expanded in place. No sidebar (the deliberate
// departure from the brief's adopt table, per the concept-C brief).
import { meetings, noteMeeting, during, attention, states } from "../shared/fixtures.js";
import { state } from "../shared/state.js";

document.title = `Concept C — ${states[state] || ""}`;

const [m1, m2, m3, m4] = meetings;

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function dayHeader(label) {
  const h = el("div", "day-header");
  h.appendChild(el("span", "caption", label));
  return h;
}

function collapsedRow(m) {
  const row = el("div", "item-row");
  const main = el("div", "item-main");
  const titleLine = el("div", "item-title-line");
  if (m.status === "recovered-interrupted") titleLine.appendChild(el("span", "attn-dot"));
  titleLine.appendChild(el("span", "item-title", m.title));
  main.appendChild(titleLine);
  main.appendChild(el("div", "item-caption caption", `${m.time} · ${m.length} · ${m.statusText}`));
  main.appendChild(el("div", "item-excerpt", m.excerpt));
  row.appendChild(main);
  row.appendChild(el("span", "chevron", "›"));
  return row;
}

function claimLi(claim, openSource) {
  const li = document.createElement("li");
  const isOpen = claim.source === openSource;
  li.appendChild(el("span", "claim" + (isOpen ? " open" : ""), claim.text));
  if (isOpen) {
    const turn = noteMeeting.transcript.find((t) => t.i === claim.source);
    const box = el("div", "source-inline");
    box.appendChild(el("p", "source-quote", `“${turn.text}”`));
    box.appendChild(el("div", "source-meta caption", `${turn.t} · ${turn.who}`));
    box.appendChild(el("button", "btn source-open", "Open full transcript"));
    li.appendChild(box);
  }
  return li;
}

function claimList(items, openSource) {
  const ul = document.createElement("ul");
  items.forEach((c) => ul.appendChild(claimLi(c, openSource)));
  return ul;
}

function expandedNote(m) {
  const wrap = el("div", "item expanded");
  wrap.dataset.id = m.id;
  const head = el("div", "item-head");
  const titleLine = el("div", "item-title-line");
  titleLine.appendChild(el("span", "item-title large", m.title));
  head.appendChild(titleLine);
  head.appendChild(el("div", "item-caption caption", `${m.time} · ${m.length} · ${m.statusText}`));
  wrap.appendChild(head);

  const read = el("div", "read");
  read.appendChild(el("h2", null, "Overview"));
  read.appendChild(el("p", null, m.note.overview));
  read.appendChild(el("h2", null, "Decisions"));
  read.appendChild(claimList(m.note.decisions, 2));
  read.appendChild(el("h2", null, "Follow-ups"));
  read.appendChild(claimList(m.note.followups, 2));
  read.appendChild(el("h2", null, "Open questions"));
  read.appendChild(claimList(m.note.questions, 2));
  wrap.appendChild(read);

  wrap.appendChild(el("span", "fixture-tag", m.fixture));
  return wrap;
}

function expandedNow(d) {
  const wrap = el("div", "item expanded now");
  wrap.dataset.id = "now";
  const head = el("div", "item-head");
  const titleLine = el("div", "item-title-line");
  titleLine.appendChild(el("span", "item-title large", "Now"));
  head.appendChild(titleLine);
  const pauseWord = d.pauses === 1 ? "once" : `${d.pauses} times`;
  head.appendChild(el("div", "item-caption caption", `Started ${d.startedAt} · paused ${pauseWord}, ${d.pausedFor}`));
  wrap.appendChild(head);

  const canvas = el("div", "note-canvas", d.notes);
  wrap.appendChild(canvas);

  const details = document.createElement("details");
  details.className = "live-transcript";
  details.appendChild(el("summary", null, `Live transcript (${d.liveTurns.length} turns)`));
  d.liveTurns.forEach((t) => {
    const turn = el("div", "turn");
    turn.appendChild(el("span", "turn-meta caption", `${t.t} · ${t.who}`));
    turn.appendChild(el("p", null, t.text));
    details.appendChild(turn);
  });
  wrap.appendChild(details);

  wrap.appendChild(el("span", "fixture-tag", d.fixture));
  return wrap;
}

function expandedAttention(a) {
  const wrap = el("div", "item expanded attention");
  wrap.dataset.id = a.meeting.id;
  const title = el("div", "attn-title");
  title.appendChild(el("span", "attn-dot"));
  title.appendChild(document.createTextNode(a.headline));
  wrap.appendChild(title);
  wrap.appendChild(el("p", "attn-body", a.detail));
  const actions = el("div", "attn-actions");
  a.actions.forEach((label, i) => {
    actions.appendChild(el("button", "btn" + (i === 0 ? " primary" : ""), label));
  });
  wrap.appendChild(actions);
  return wrap;
}

function section(label, rows) {
  const s = el("section", "day-group");
  s.appendChild(dayHeader(label));
  rows.forEach((r) => s.appendChild(r));
  return s;
}

function sectionsForState() {
  switch (state) {
    case 2:
      return [
        section("Now", [expandedNow(during)]),
        section("Yesterday", [collapsedRow(m1), collapsedRow(m2), collapsedRow(m3)]),
        section("Previous 30 days", [collapsedRow(m4)]),
      ];
    case 3:
      return [
        section("Today", [expandedNote(noteMeeting)]),
        section("Yesterday", [collapsedRow(m1), collapsedRow(m2), collapsedRow(m3)]),
        section("Previous 30 days", [collapsedRow(m4)]),
      ];
    case 4:
      return [
        section("Yesterday", [collapsedRow(m1), expandedAttention(attention), collapsedRow(m3)]),
        section("Previous 30 days", [collapsedRow(m4)]),
      ];
    case 1:
    default:
      return [
        section("Yesterday", [collapsedRow(m1), collapsedRow(m2), collapsedRow(m3)]),
        section("Previous 30 days", [collapsedRow(m4)]),
      ];
  }
}

function renderRecordControl() {
  const wrap = document.querySelector(".record-group");
  wrap.innerHTML = "";
  if (state === 2) {
    wrap.classList.add("live");
    const btn = el("button", "btn record live");
    btn.appendChild(el("span", "rec-dot"));
    btn.appendChild(document.createTextNode(during.elapsed));
    wrap.appendChild(btn);
    wrap.appendChild(el("button", "btn", "Pause"));
    wrap.appendChild(el("button", "btn", "Stop"));
  } else {
    wrap.appendChild(el("button", "btn record", "Record"));
  }
}

function render() {
  renderRecordControl();
  const col = document.querySelector(".stream-col");
  col.innerHTML = "";
  sectionsForState().forEach((s) => col.appendChild(s));
  col.appendChild(el("div", "trash-row caption", "Trash"));
}

render();
