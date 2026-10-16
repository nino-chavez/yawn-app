// Concept 1 — Notebook. Mac: the operator's notes are the document; the
// transcript is a labeled section under them with one slim transport;
// secondary actions live in the toolbar menu. iPad: Concept C in the system
// language. Not product code.
import { state, theme } from "../shared/state.js";
import { mac, ipad, clock, states } from "../shared/fixtures.js";
import { el, svgIcon, macFrame, ipadFrame, pushButton, capsule, fixtureLabel } from "../shared/chrome.js";
import { renderIpadC } from "../shared/ipad-c.js";

const root = document.getElementById("root");
const platform = document.body.dataset.platform;
const label = (s) => `Concept 1 Notebook · ${states[s]} · invented content`;

function toolbarMenu() {
  const wrap = el("div", "tb-actions");
  wrap.style.display = "flex"; wrap.style.gap = "6px";
  const share = el("button", "icon-btn"); share.setAttribute("aria-label", "Export"); share.append(svgIcon("share"));
  const more = el("button", "icon-btn"); more.setAttribute("aria-label", "More"); more.append(svgIcon("ellipsis"));
  wrap.append(share, more);
  return wrap;
}

function header(doc, { title, meta, retention }) {
  const inner = el("div", "doc-inner");
  const m = el("div", "doc-meta");
  m.append(el("span", null, meta));
  if (retention) m.append(el("span", null, "·"), el("span", "retention", retention));
  inner.append(m, el("h1", "doc-title", title));
  doc.append(inner);
  return inner;
}

function notesBlock(lines, { caret = false } = {}) {
  const n = el("div", "notes");
  lines.forEach((l, i) => {
    const p = el("p", caret && i === lines.length - 1 ? "caret" : "", l.text);
    n.append(p);
  });
  return n;
}

function transcriptSection(inner, turns, { current = null, limit = 14 } = {}) {
  const tr = el("div", "transport");
  const play = el("button", "play"); play.setAttribute("aria-label", "Play"); play.append(svgIcon("play"));
  const scrub = el("div", "scrub"); scrub.append(el("i"), el("b"));
  tr.append(play, el("span", "time", clock(686)), scrub, el("span", "time", mac.meeting.length));
  inner.append(tr);
  const head = el("div", "section-head");
  head.append(el("h2", null, "Transcript"));
  const find = el("div", "find"); find.append(svgIcon("search"), el("span", null, "Find in transcript"));
  head.append(find);
  inner.append(head);
  const list = el("div", "turns");
  turns.slice(0, limit).forEach((t, i) => {
    const r = el("div", "turn-row" + (t.t === current ? " current" : ""));
    r.append(el("span", "t", clock(t.t)), el("span", "who", t.who), el("span", "text", t.text));
    list.append(r);
  });
  inner.append(list);
}

// ------------------------------------------------------------------ Mac --
function renderMac() {
  if (state === 4) return renderSettings();
  const m = mac.meeting;
  if (state === 1) {
    const { doc } = macFrame(root, { title: m.title, rows: mac.library, selected: "m0", toolbarExtra: toolbarMenu() });
    const inner = header(doc, { title: m.title, meta: `${m.date} · ${m.time} · ${m.length}`, retention: m.retention });
    inner.append(notesBlock(m.notes));
    transcriptSection(inner, m.transcript, { current: 686 });
  }
  if (state === 2) {
    const d = mac.during;
    const rows = [{ id: "live", title: "Recording now", time: clock(d.elapsed), status: "live", statusText: "", group: "Today" }, ...mac.library.slice(1)];
    const { doc } = macFrame(root, { title: "Meeting · Oct 15, 2026", record: { mode: "live", elapsed: d.elapsed }, rows, selected: "live" });
    const inner = header(doc, { title: "Meeting · Oct 15, 2026", meta: `Started 10:05 AM` });
    inner.append(notesBlock(d.typed, { caret: true }));
    const foot = el("div", "live-footer");
    foot.append(svgIcon("chevron"), el("span", null, `Live transcript · ${d.liveTurns} turns`));
    doc.append(foot);
  }
  if (state === 3) {
    const a = mac.attention;
    const { doc } = macFrame(root, { title: a.title, rows: mac.library, selected: "m1", toolbarExtra: toolbarMenu() });
    const inner = header(doc, { title: a.title, meta: `${a.date} · ${a.time} · ${a.captured} captured`, retention: a.retention });
    const c = el("div", "callout");
    c.append(el("div", "glyph", "!"));
    const body = el("div");
    body.append(el("h3", null, a.headline), el("p", null, a.detail));
    const acts = el("div", "actions");
    acts.append(pushButton(a.primary, "push-primary"), pushButton(a.secondary));
    body.append(acts);
    c.append(body);
    inner.append(c);
    inner.append(notesBlock(a.notes));
  }
  if (state === 5) {
    const { doc, sidebar } = macFrame(root, { title: "Yawn", rows: [] });
    sidebar.querySelector(".sb-scroll").append(el("div", "sb-empty", "Meetings you record appear here, newest first."));
    const e = el("div", "empty-doc");
    e.append(el("h1", null, "Record a meeting"),
      el("p", null, "Yawn records the call and your own notes, keeps them on this Mac, and makes a transcript when you stop."),
      el("div", "kbd", "⌘R, or the Record button"));
    doc.append(e);
  }
  fixtureLabel(root.firstChild, label(state));
}

function renderSettings() {
  const d = el("div", "desktop");
  const w = el("div", "settings-win");
  const tb = el("div", "settings-tb");
  const lights = el("span", "lights"); lights.innerHTML = "<i></i><i></i><i></i>";
  tb.append(lights, el("div", "win-title", "Recording"));
  for (const g of mac.settings.groups) {
    const t = el("div", "tab" + (g.id === "recording" ? " on" : ""));
    t.append(el("i", "glyph"), el("span", null, g.name));
    tb.append(t);
  }
  w.append(tb);
  const body = el("div", "settings-body");
  const g = mac.settings.groups[0];
  const grp = el("div", "group");
  for (const r of g.rows) {
    const row = el("div", "group-row");
    const l = el("div"); l.append(el("div", "l", r.label), el("div", "d", r.detail));
    row.append(l, el("span", "state-ok", r.state));
    grp.append(row);
  }
  body.append(grp);
  const foot = el("div", "group-foot"); foot.append(pushButton(g.action)); body.append(foot);
  // the one privacy sentence, once
  body.append(el("div", "privacy", mac.settings.privacy));
  w.append(body);
  d.append(w);
  root.append(d);
  fixtureLabel(d, label(4));
}

if (platform === "mac") renderMac(); else renderIpadC(root, state, { conceptName: "Concept 1 Notebook" });
