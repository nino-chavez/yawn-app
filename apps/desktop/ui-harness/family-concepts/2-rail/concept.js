// Concept 2 — Rail. Mac: the time rail from the iPad runs down the Mac page.
// Each line typed during the recording carries the moment it was first
// typed; after the meeting the transcript turn nearest each note sits level
// with it and the rest fold into a count. The rail is the scrubber. This
// depends on a capability the product does not have (a moment per typed
// line); every Mac frame that uses it says so. iPad: Concept C on paper.
import { state } from "../shared/state.js";
import { mac, ipad, clock, states } from "../shared/fixtures.js";
import { el, svgIcon, macFrame, pushButton, fixtureLabel } from "../shared/chrome.js";
import { renderIpadC } from "../shared/ipad-c.js";

const root = document.getElementById("root");
const platform = document.body.dataset.platform;
const NAME = "Concept 2 Rail";
const label = (s) => `${NAME} · ${states[s]} · invented content`;

function toolbarMenu() {
  const wrap = el("div", "tools-h");
  wrap.style.gap = "6px";
  const share = el("button", "icon-btn"); share.setAttribute("aria-label", "Export"); share.append(svgIcon("share"));
  const more = el("button", "icon-btn"); more.setAttribute("aria-label", "More"); more.append(svgIcon("ellipsis"));
  wrap.append(share, more);
  return wrap;
}

function header(doc, { title, meta, retention }) {
  const inner = el("div", "doc-inner");
  const m = el("div", "doc-meta");
  m.append(el("span", null, meta));
  if (retention) m.append(el("span", null, "·"), el("span", null, retention));
  inner.append(m, el("h1", "doc-title", title));
  doc.append(inner);
  return inner;
}

function transport(inner) {
  const tr = el("div", "transport");
  const play = el("button", "play"); play.setAttribute("aria-label", "Play"); play.append(svgIcon("play"));
  const scrub = el("div", "scrub"); scrub.append(el("i"), el("b"));
  tr.append(play, el("span", "time", `${clock(690)} / ${mac.meeting.length}`), scrub);
  const full = el("button", "full"); full.append(el("span", null, "Full transcript"), svgIcon("chevron"));
  tr.append(full);
  inner.append(tr);
}

function nearest(turns, t) { return turns.reduce((a, b) => (Math.abs(b.t - t) < Math.abs(a.t - t) ? b : a)); }

/** rail | note | nearest turn. `turns` null renders the one-column ledger (During). */
function ledger(inner, notes, { turns = null, current = null, caret = false, playhead = null, stopAt = null } = {}) {
  const L = el("div", "ledger" + (turns ? "" : " ledger-one"));
  let prevT = notes.length ? notes[0].t - 60 : 0;
  notes.forEach((n, i) => {
    let near = null;
    if (turns) {
      near = nearest(turns, n.t);
      const folded = ipad.foldedBetween(prevT, near.t);
      if (folded > 0) { const fr = el("div", "fold-row"); fr.append(el("div", "fold", `+ ${folded} turns`)); L.append(fr); }
    }
    const row = el("div", "ledger-row");
    const rc = el("div", "rail-cell");
    const mark = el("div", "mark" + (playhead === n.t ? " head" : "") + (caret && i === notes.length - 1 ? " live" : ""));
    mark.append(el("span", null, clock(n.t)), el("i"));
    rc.append(mark);
    const nc = el("div", "note-cell");
    nc.append(el("p", caret && i === notes.length - 1 ? "caret" : "", n.text));
    row.append(rc, nc);
    if (turns) {
      const tc = el("div", "turn-cell");
      const t = el("div", "turn" + (near.t === current ? " turn-current" : ""));
      t.append(el("span", "turn-time", `${clock(near.t)} · ${near.who}`), el("div", "turn-text", near.text));
      tc.append(t); row.append(tc);
      prevT = near.t;
    }
    L.append(row);
  });
  if (stopAt) {
    const row = el("div", "ledger-row");
    const rc = el("div", "rail-cell"); const mark = el("div", "mark stop"); mark.append(el("span", null, stopAt), el("i")); rc.append(mark);
    const nc = el("div", "note-cell"); nc.append(el("p", "stopped", "Recording stopped here"));
    row.append(rc, nc); L.append(row);
  }
  inner.append(L);
}

function renderMac() {
  if (state === 4) return renderSettings();
  const m = mac.meeting;
  let frame;
  if (state === 1) {
    frame = macFrame(root, { title: m.title, rows: mac.library, selected: "m0", toolbarExtra: toolbarMenu() });
    const inner = header(frame.doc, { title: m.title, meta: `${m.date} · ${m.time} · ${m.length}`, retention: m.retention });
    transport(inner);
    ledger(inner, m.notes, { turns: m.transcript, current: 686, playhead: 690 });
    frame.doc.append(el("div", "capability", "Depends on a moment per typed line (not built)"));
  }
  if (state === 2) {
    const d = mac.during;
    const rows = [{ id: "live", title: "Recording now", time: clock(d.elapsed), status: "live", statusText: "", group: "Today" }, ...mac.library.slice(1)];
    frame = macFrame(root, { title: "Meeting · Oct 15, 2026", record: { mode: "live", elapsed: d.elapsed }, rows, selected: "live" });
    const inner = header(frame.doc, { title: "Meeting · Oct 15, 2026", meta: "Started 10:05 AM" });
    ledger(inner, d.typed, { caret: true });
    frame.doc.append(el("div", "capability", "Depends on a moment per typed line (not built)"));
  }
  if (state === 3) {
    const a = mac.attention;
    frame = macFrame(root, { title: a.title, rows: mac.library, selected: "m1", toolbarExtra: toolbarMenu() });
    const inner = header(frame.doc, { title: a.title, meta: `${a.date} · ${a.time} · ${a.captured} captured`, retention: a.retention });
    const b = el("div", "attention-block");
    b.append(el("h3", null, a.headline), el("p", null, a.detail));
    const acts = el("div", "actions"); acts.append(pushButton(a.primary, "push-primary"), pushButton(a.secondary)); b.append(acts);
    inner.append(b);
    ledger(inner, a.notes, { stopAt: a.captured });
  }
  if (state === 5) {
    frame = macFrame(root, { title: "Yawn", rows: [] });
    frame.sidebar.querySelector(".sb-scroll").append(el("div", "sb-empty", "Meetings you record appear here, newest first."));
    frame.doc.append(el("div", "empty-rail"));
    const e = el("div", "empty-doc");
    e.append(el("h1", null, "Record a meeting"),
      el("p", null, "Yawn records the call and your own notes, keeps them on this Mac, and makes a transcript when you stop."),
      el("div", "kbd", "⌘R, or the Record button"));
    frame.doc.append(e);
  }
  fixtureLabel(frame.app, label(state));
}

function renderSettings() {
  const d = el("div", "desktop");
  const w = el("div", "settings-win");
  const side = el("div", "settings-side");
  const lights = el("span", "lights"); lights.innerHTML = "<i></i><i></i><i></i>"; side.append(lights);
  for (const g of mac.settings.groups) {
    const it = el("div", "side-item" + (g.id === "recording" ? " on" : ""));
    it.append(el("i", "glyph"), el("span", null, g.name)); side.append(it);
  }
  w.append(side);
  const main = el("div", "settings-main");
  main.append(el("div", "win-title", "Recording"));
  const g = mac.settings.groups[0];
  const grp = el("div", "group");
  for (const r of g.rows) {
    const row = el("div", "group-row");
    const l = el("div"); l.append(el("div", "l", r.label), el("div", "d", r.detail));
    row.append(l, el("span", "state-ok", r.state)); grp.append(row);
  }
  main.append(grp);
  const foot = el("div", "group-foot"); foot.append(pushButton(g.action)); main.append(foot);
  main.append(el("div", "privacy", mac.settings.privacy));
  w.append(main); d.append(w); root.append(d);
  fixtureLabel(d, label(4));
}

if (platform === "mac") renderMac(); else renderIpadC(root, state, { conceptName: NAME });
