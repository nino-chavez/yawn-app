// Concept 3 — Record. Mac: the transcript is the record and the document.
// The operator's notes are a pinned strip above it, a full-width timeline
// with a tick per turn is the scrubber, and while recording the live
// transcript streams under the notes. iPad: Concept C in the instrument
// language with the transcript given equal width after the meeting.
import { state } from "../shared/state.js";
import { mac, clock, states } from "../shared/fixtures.js";
import { el, svgIcon, macFrame, pushButton, fixtureLabel } from "../shared/chrome.js";
import { renderIpadC } from "../shared/ipad-c.js";

const root = document.getElementById("root");
const platform = document.body.dataset.platform;
const NAME = "Concept 3 Record";
const label = (s) => `${NAME} · ${states[s]} · invented content`;

function toolbarMenu() {
  const wrap = el("div", "tools-h"); wrap.style.gap = "6px";
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

/** Full-width timeline: a tick per turn (You above, Them below), a playhead. */
function timeline(inner, { total, head = null, live = false, turns = [], stopAt = null, stopLabel = null }) {
  const tl = el("div", "timeline");
  const play = el("button", "play"); play.setAttribute("aria-label", live ? "Pause" : "Play"); play.append(svgIcon(live ? "pause" : "play"));
  tl.append(play);
  tl.append(el("span", "t", head != null ? clock(head) : "0:00"));
  const track = el("div", "track");
  const span = total || 1;
  for (const t of turns) {
    const k = el("i", "tick " + (t.who === "You" ? "you" : "them"));
    k.style.left = `${(t.t / span) * 100}%`; track.append(k);
  }
  if (head != null) {
    const done = el("i", "done"); done.style.width = `${(head / span) * 100}%`; track.append(done);
    const h = el("i", "head" + (live ? " live" : "")); h.style.left = `${(head / span) * 100}%`; track.append(h);
  }
  if (stopAt != null) {
    const s = el("i", "stop"); s.style.left = `${(stopAt / span) * 100}%`; track.append(s);
    const sl = el("span", "stop-label", stopLabel || ""); sl.style.left = `${(stopAt / span) * 100}%`; track.append(sl);
  }
  tl.append(track);
  tl.append(el("span", "t right", total ? clock(total) : "—"));
  inner.append(tl);
}

function notesStrip(inner, lines, { caret = false } = {}) {
  const s = el("div", "notes-strip");
  s.append(el("div", "strip-label", "Your notes"));
  const g = el("div", "lines" + (lines.length <= 3 ? " one" : ""));
  lines.forEach((l, i) => g.append(el("p", caret && i === lines.length - 1 ? "caret" : "", l.text)));
  s.append(g);
  inner.append(s);
}

function transcript(inner, turns, { current = null, limit = 13, live = false, labelText = "Transcript" } = {}) {
  const head = el("div", "strip-label");
  head.append(el("span", null, labelText));
  if (!live) { const find = el("div", "find"); find.append(svgIcon("search"), el("span", null, "Find in transcript")); head.append(find); }
  inner.append(head);
  const list = el("div", "turns");
  turns.slice(0, limit).forEach((t) => {
    const r = el("div", "turn-row" + (t.t === current ? " current" : ""));
    r.append(el("span", "t", clock(t.t)), el("span", "who " + (t.who === "You" ? "you" : "them"), t.who), el("span", "text", t.text));
    list.append(r);
  });
  if (live) {
    const r = el("div", "turn-row listening");
    r.append(el("span", "t", clock(mac.during.elapsed)), el("span", "who them", "Them"), el("span", "text", "Listening"));
    list.append(r);
  }
  inner.append(list);
}

function renderMac() {
  if (state === 4) return renderSettings();
  const m = mac.meeting;
  let frame;
  if (state === 1) {
    frame = macFrame(root, { title: m.title, rows: mac.library, selected: "m0", toolbarExtra: toolbarMenu() });
    const inner = header(frame.doc, { title: m.title, meta: `${m.date} · ${m.time} · ${m.length}`, retention: m.retention });
    timeline(inner, { total: m.lengthSeconds, head: 686, turns: m.transcript });
    notesStrip(inner, m.notes);
    transcript(inner, m.transcript, { current: 686, limit: 13 });
  }
  if (state === 2) {
    const d = mac.during;
    const rows = [{ id: "live", title: "Recording now", time: clock(d.elapsed), status: "live", statusText: "", group: "Today" }, ...mac.library.slice(1)];
    frame = macFrame(root, { title: "Meeting · Oct 15, 2026", record: { mode: "live", elapsed: d.elapsed }, rows, selected: "live" });
    const inner = header(frame.doc, { title: "Meeting · Oct 15, 2026", meta: "Started 10:05 AM" });
    timeline(inner, { total: null, head: d.elapsed, live: true, turns: m.transcript.slice(0, d.liveTurns).map((t) => ({ ...t })) });
    // while recording the track scales to the elapsed time
    const track = inner.querySelector(".track");
    track.querySelectorAll(".tick").forEach((k, i) => { k.style.left = `${(m.transcript[i].t / d.elapsed) * 100}%`; });
    track.querySelector(".head").style.left = "100%"; track.querySelector(".done").style.width = "100%";
    notesStrip(inner, d.typed, { caret: true });
    transcript(inner, m.transcript.slice(0, d.liveTurns), { live: true, labelText: "Live transcript" });
  }
  if (state === 3) {
    const a = mac.attention;
    frame = macFrame(root, { title: a.title, rows: mac.library, selected: "m1", toolbarExtra: toolbarMenu() });
    const inner = header(frame.doc, { title: a.title, meta: `${a.date} · ${a.time} · ${a.captured} captured`, retention: a.retention });
    timeline(inner, { total: 401, head: 0, turns: [], stopAt: 401, stopLabel: "Stopped 3:36 PM" });
    const s = el("div", "attention-strip");
    s.append(el("h3", null, a.headline), el("p", null, a.detail));
    const acts = el("div", "actions"); acts.append(pushButton(a.primary, "push-primary"), pushButton(a.secondary)); s.append(acts);
    inner.append(s);
    notesStrip(inner, a.notes);
    inner.append(el("div", "strip-label", "Transcript"));
    inner.append(el("div", "empty-transcript", "No transcript was made."));
  }
  if (state === 5) {
    frame = macFrame(root, { title: "Yawn", rows: [] });
    frame.sidebar.querySelector(".sb-scroll").append(el("div", "sb-empty", "Meetings you record appear here, newest first."));
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
  const tb = el("div", "settings-tb");
  const lights = el("span", "lights"); lights.innerHTML = "<i></i><i></i><i></i>";
  tb.append(lights, el("span", null, "Settings"));
  w.append(tb);
  const body = el("div", "settings-body");
  for (const g of mac.settings.groups) {
    const sg = el("div", "sgroup");
    sg.append(el("div", "strip-label", g.name));
    for (const r of g.rows) {
      const row = el("div", "srow");
      const l = el("div"); l.append(el("div", "l", r.label), el("div", "d", r.detail));
      row.append(l);
      if (r.kind === "toggle") row.append(el("span", "toggle"));
      else if (r.kind === "action") row.append(el("span", "sstate action", r.state));
      else if (r.kind === "none") row.append(el("span"));
      else row.append(el("span", "sstate", r.state));
      sg.append(row);
    }
    if (g.action) { const row = el("div", "srow"); row.append(el("span"), el("span", "sstate action", g.action)); sg.append(row); }
    body.append(sg);
  }
  body.append(el("div", "privacy", mac.settings.privacy));
  w.append(body); d.append(w); root.append(d);
  fixtureLabel(d, label(4));
}

if (platform === "mac") renderMac(); else renderIpadC(root, state, { conceptName: NAME, inkShare: 0.5 });
