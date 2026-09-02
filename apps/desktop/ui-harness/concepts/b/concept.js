import { state } from "../shared/state.js";
import { meetings, noteMeeting, during, attention, states } from "../shared/fixtures.js";

document.title = `Yawn — Concept B — ${states[state] || ""}`;

const $ = (sel) => document.querySelector(sel);
const el = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
};

function caption(m) {
  return `${m.date} · ${m.time} · ${m.length} · ${m.statusText}`;
}

// ---------- popover (the list) ----------
function renderPopover() {
  const groups = new Map();
  for (const m of meetings) {
    if (!groups.has(m.group)) groups.set(m.group, []);
    groups.get(m.group).push(m);
  }
  const pop = $("#popover");
  pop.innerHTML = "";
  for (const [label, rows] of groups) {
    pop.appendChild(el(`<div class="popover-group-label">${label}</div>`));
    for (const m of rows) {
      const dot = m.status === "recovered-interrupted" ? '<span class="attn-dot"></span>' : "";
      pop.appendChild(el(`
        <div class="popover-row">
          <div class="row-title-line">${dot}<span class="row-title">${m.title}</span></div>
          <div class="row-caption">${caption(m)}</div>
          <div class="row-excerpt">${m.excerpt}</div>
        </div>
      `));
    }
  }
  pop.appendChild(el(`<div class="popover-trash">Trash</div>`));
}

function setPopoverOpen(open) {
  const pop = $("#popover");
  const btn = $("#meetingsBtn");
  pop.hidden = !open;
  btn.setAttribute("aria-expanded", String(open));
}

// ---------- toolbar record control ----------
function renderRecordControl() {
  const host = $("#recordGroup");
  if (state === 2) {
    host.innerHTML = `
      <button class="btn record live" type="button"><span class="dot-live"></span>${during.elapsed}</button>
      <button class="btn" type="button">Pause</button>
      <button class="btn" type="button">Stop</button>
    `;
  } else {
    host.innerHTML = `<button class="btn record" type="button">Record</button>`;
  }
}

// ---------- note surface, per state ----------
function renderNoteSurface() {
  const host = $("#noteSurface");
  host.innerHTML = "";

  if (state === 1) {
    const m = meetings[0];
    $("#tbTitle").textContent = m.title;
    host.appendChild(el(`
      <div class="note-body">
        <h1 class="note-title">${m.title}</h1>
        <p class="note-caption">${caption(m)}</p>
        <p class="note-empty-msg">No note yet. Transcript retained on this Mac.</p>
        <button class="btn" type="button">Generate note</button>
      </div>
    `));
    return;
  }

  if (state === 2) {
    $("#tbTitle").textContent = "New meeting";
    const body = el(`<div class="note-body"></div>`);
    const pre = el(`<p class="note-canvas"></p>`);
    pre.textContent = during.notes;
    pre.appendChild(el(`<span class="caret"></span>`));
    body.appendChild(pre);
    host.appendChild(body);
    return;
  }

  if (state === 3) {
    const m = noteMeeting;
    $("#tbTitle").textContent = m.title;
    const body = el(`
      <div class="note-body">
        <h1 class="note-title">${m.title}</h1>
        <p class="note-caption">${m.date} · ${m.time} · ${m.length} · ${m.statusText}</p>
        <div class="read"></div>
      </div>
    `);
    const read = body.querySelector(".read");

    read.appendChild(el(`<h2>Overview</h2>`));
    read.appendChild(el(`<p>${m.note.overview}</p>`));

    read.appendChild(el(`<h2>Decisions</h2>`));
    const dl = el(`<ul></ul>`);
    for (const d of m.note.decisions) {
      const isOpen = d.source === 2;
      dl.appendChild(el(`<li><span class="claim${isOpen ? " open" : ""}">${d.text}</span></li>`));
    }
    read.appendChild(dl);

    read.appendChild(el(`<h2>Follow-ups</h2>`));
    const fl = el(`<ul></ul>`);
    for (const f of m.note.followups) fl.appendChild(el(`<li><span class="claim">${f.text}</span></li>`));
    read.appendChild(fl);

    read.appendChild(el(`<h2>Open questions</h2>`));
    const ql = el(`<ul></ul>`);
    for (const q of m.note.questions) ql.appendChild(el(`<li><span class="claim">${q.text}</span></li>`));
    read.appendChild(ql);

    host.appendChild(body);
    return;
  }

  if (state === 4) {
    const m = attention.meeting;
    $("#tbTitle").textContent = m.title;
    host.appendChild(el(`
      <div class="note-body">
        <h1 class="note-title">${attention.headline}</h1>
        <p class="attention-detail">${attention.detail}</p>
        <div class="attention-actions">
          <button class="btn primary" type="button">${attention.actions[0]}</button>
          <button class="btn" type="button">${attention.actions[1]}</button>
        </div>
      </div>
    `));
    return;
  }
}

// ---------- transcript split (state 3 only: the open claim's source) ----------
function renderTranscriptSplit() {
  const split = $("#transcriptSplit");
  if (state !== 3) {
    split.hidden = true;
    split.innerHTML = "";
    return;
  }
  split.hidden = false;
  const citedIndex = 2; // "Ship the retry review before Friday." → transcript turn 2
  const turns = noteMeeting.transcript.filter((t) => Math.abs(t.i - citedIndex) <= 1);

  const inner = el(`
    <div class="transcript-split-inner">
      <div class="transcript-split-head">
        <span class="caption">Source · turn ${citedIndex}</span>
      </div>
    </div>
  `);
  const head = inner.querySelector(".transcript-split-head");
  for (const t of turns) {
    const cls = t.i === citedIndex ? "highlight" : "dim";
    inner.insertBefore(
      el(`
        <div class="t-turn ${cls}">
          <span class="who">${t.who}</span>
          <span class="time">${t.t}</span>
          <span class="text">${t.text}</span>
        </div>
      `),
      inner.querySelector(".transcript-split-foot")
    );
  }
  inner.appendChild(el(`
    <div class="transcript-split-foot">
      <button class="btn" type="button">Open full transcript</button>
    </div>
  `));
  split.innerHTML = "";
  split.appendChild(inner);
}

// ---------- status bar (state 2 only) ----------
function renderStatusbar() {
  const bar = $("#statusbar");
  if (state !== 2) {
    bar.hidden = true;
    bar.textContent = "";
    return;
  }
  bar.hidden = false;
  bar.textContent = `Recording · ${during.elapsed} · paused once, ${during.pausedFor} · Live transcript (${during.liveTurns.length} turns)`;
}

// ---------- fixture tag (synthetic content only: states 2 and 3) ----------
function renderFixtureTag() {
  const stage = $("#stage");
  const existing = stage.querySelector(".fixture-tag");
  if (existing) existing.remove();
  const label = state === 2 ? during.fixture : state === 3 ? noteMeeting.fixture : null;
  if (!label) return;
  stage.appendChild(el(`<div class="fixture-tag">${label}</div>`));
}

// ---------- assemble ----------
$("#meetingsCount").textContent = String(meetings.length);
renderPopover();
setPopoverOpen(state === 1);
renderRecordControl();
renderNoteSurface();
renderTranscriptSplit();
renderStatusbar();
renderFixtureTag();
