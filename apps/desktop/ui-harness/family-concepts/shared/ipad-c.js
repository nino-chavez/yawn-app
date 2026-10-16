// The iPad screens, Concept C (Time rail, selected 2026-10-14) as the fixed
// structure. Each concept renders these through its own styles.css and the
// options below; the structure does not change between concepts.
import { ipad, clock, states } from "./fixtures.js";
import { el, svgIcon, ipadFrame, capsule, fixtureLabel } from "./chrome.js";
import { inkLine } from "./ink.js";

const LINE = 64;

export function railAndInk(page, lines, { live = null, head = null, dimAfter = null, tapped = null, after = null, room = 700, top = 24 } = {}) {
  page.append(el("div", "rail"));
  const inkCol = el("div", "ink-lines");
  let y = top;
  let lastMark = null;
  for (const l of lines) {
    const dim = dimAfter != null && l.t > dimAfter;
    const line = inkLine({ ...l, dim, tapped: l.id === tapped }, { room });
    line.style.position = "absolute"; line.style.top = `${y}px`;
    inkCol.append(line);
    const mark = el("div", "rail-mark");
    mark.style.top = `${y + 22}px`;
    mark.append(el("i"), el("span", null, clock(l.t)));
    page.append(mark);
    lastMark = mark;
    y += LINE;
  }
  if (live != null && lastMark) {
    lastMark.classList.add("live");
    lastMark.querySelector("span").textContent = clock(live);
  }
  if (head != null) {
    const m = el("div", "rail-mark head"); m.style.top = `${head}px`;
    m.append(el("i"), el("span", null, clock(ipad.pausedSeconds)));
    page.append(m);
  }
  if (after) {
    const div = el("div", "after-divider", "After the meeting");
    div.style.position = "absolute"; div.style.top = `${y + 2}px`; div.style.left = "0"; div.style.right = "0";
    page.append(div);
    const line = inkLine({ ...after, t: null }, { room });
    line.style.position = "absolute"; line.style.top = `${y + 34}px`;
    inkCol.append(line);
  }
  page.append(inkCol);
  return y;
}

export function toolCluster(vertical) {
  const c = el("div", vertical ? "toolcol" : "tools-h");
  const pen = el("button", "ipad-icon-btn icon-pen"); pen.setAttribute("aria-label", "Pen"); pen.append(svgIcon("pen"));
  const er = el("button", "ipad-icon-btn"); er.setAttribute("aria-label", "Eraser"); er.append(svgIcon("eraser"));
  const un = el("button", "ipad-icon-btn"); un.setAttribute("aria-label", "Undo"); un.append(svgIcon("undo"));
  c.append(pen, er, un);
  return c;
}

/** The folded model: the nearest turn level with each visible note, the rest folded into a count. */
export function transcriptColumn(col, { current, visibleNotes }) {
  const turns = ipad.transcript;
  let prevT = visibleNotes[0].t - 60;
  for (const n of visibleNotes) {
    const nearest = turns.reduce((a, b) => (Math.abs(b.t - n.t) < Math.abs(a.t - n.t) ? b : a));
    const folded = ipad.foldedBetween(prevT, nearest.t);
    if (folded > 0) col.append(el("div", "turn-fold", `+ ${folded} turns`));
    const t = el("div", "turn" + (nearest.t === current ? " turn-current" : ""));
    t.append(el("span", "turn-time", clock(nearest.t)), el("div", "turn-text", nearest.text));
    col.append(t);
    prevT = nearest.t;
  }
}

export function sidebarRows(list, selectedId) {
  let g = null;
  for (const p of ipad.past) {
    if (p.group !== g) { g = p.group; list.append(el("div", "ipad-group", g)); }
    const r = el("div", "ipad-row" + (p.id === selectedId ? " ipad-row-selected" : ""));
    r.append(el("div", "ipad-row-day", p.day), el("div", "ipad-row-caption", `${p.time} · ${p.minutes} min`));
    const mr = el("div", "mini-rail");
    for (const m of p.marks) { const i = el("i"); i.style.left = `${(m / p.length) * 100}%`; mr.append(i); }
    r.append(mr, el("div", "ipad-row-ink", p.firstLines.join(" · ")));
    list.append(r);
  }
}

function recordingBadge() {
  const badge = el("span", "record-badge ipad-record-badge");
  badge.append(el("i", "record-glyph"), el("span", "record-word", "Recording"), el("span", "record-time", clock(ipad.duringSeconds)));
  return badge;
}

/**
 * Renders iPad state 6..9 into root.
 * opts.conceptName: label prefix. opts.inkShare: the page's share of the
 * width after the meeting (0.6 default).
 */
export function renderIpadC(root, state, opts = {}) {
  const name = opts.conceptName || "Concept";
  const label = (s) => `${name} · ${states[s]} · invented content`;
  const inkShare = opts.inkShare ?? 0.6;

  if (state === 6) {
    const stop = capsule("Stop", { icon: "stop", tint: "tint-record" });
    const { page, device } = ipadFrame(root, { nav: { center: [recordingBadge()], right: [stop] } });
    const lines = ipad.lines.filter((l) => l.t <= ipad.duringSeconds);
    railAndInk(page, lines, { live: ipad.duringSeconds, room: 820 });
    page.append(toolCluster(true));
    fixtureLabel(device, label(6));
  }
  if (state === 7) {
    const back = capsule("Meetings", { icon: "back", tint: "tint-soft" });
    const transport = capsule(`${clock(ipad.pausedSeconds)} / ${clock(ipad.meetingLength)}`, { icon: "play", tint: "tint-soft" });
    const center = el("div"); center.append(el("div", "nav-title", ipad.meetingTitle), el("div", "nav-sub", "23 min · Transcript ready"));
    const right = toolCluster(false);
    const tr = el("button", "ipad-icon-btn"); tr.setAttribute("aria-label", "Full transcript"); tr.append(svgIcon("transcript"));
    const se = el("button", "ipad-icon-btn"); se.setAttribute("aria-label", "Search"); se.append(svgIcon("search"));
    const sh = el("button", "ipad-icon-btn"); sh.setAttribute("aria-label", "Export"); sh.append(svgIcon("share"));
    right.append(tr, se, sh);
    const { page, device } = ipadFrame(root, { nav: { left: [back, transport], center: [center], right: [right] } });
    const split = el("div", "page-split");
    const inkPage = el("div", "page-ink"); inkPage.style.flex = `0 0 ${Math.round(inkShare * 100)}%`;
    const visible = ipad.lines.slice(4); // scrolled past the first four lines
    const room = Math.round(1180 * inkShare) - 170;
    railAndInk(inkPage, visible, { dimAfter: ipad.pausedSeconds, tapped: ipad.tappedLine, head: 24 + 4 * LINE + 22, after: ipad.addedAfterStop, room });
    const col = el("div", "page-transcript");
    transcriptColumn(col, { current: 678, visibleNotes: visible });
    split.append(inkPage, col);
    page.append(split);
    fixtureLabel(device, label(7));
  }
  if (state === 8) {
    const { page, device } = ipadFrame(root, { nav: {} });
    const screen = el("div", "consent-screen");
    const card = el("div", "consent consent-card");
    card.append(el("h1", null, "Before you record"),
      el("p", null, "Say out loud that you're recording, and check that everyone in the room is okay with it."));
    const tg = el("div", "consent-toggle"); tg.append(el("span", null, "Everyone who can be heard agreed to this recording"), el("span", "switch on"));
    card.append(tg);
    const keep = el("div"); keep.append(el("div", "caption", "Keep the audio for"));
    const seg = el("div", "segmented"); seg.append(el("span", null, "1 day"), el("span", "on", "7 days"), el("span", null, "30 days"));
    keep.append(seg); card.append(keep);
    const fact = el("div", "consent-fact"); fact.append(svgIcon("ipad"), el("span", null, "Stays on this iPad")); card.append(fact);
    const start = el("div", "big-btn primary"); start.append(el("i", "record-glyph"), el("span", null, "Start recording")); card.append(start);
    card.append(el("div", "link-btn", "Not now"));
    screen.append(card); page.append(screen);
    fixtureLabel(device, label(8));
  }
  if (state === 9) {
    const center = el("div"); center.append(el("div", "nav-title", ipad.meetingTitle), el("div", "nav-sub", "23 min · Transcript ready"));
    const sb = {};
    const rec = capsule("Record", { tint: "tint-soft" }); rec.prepend(el("i", "record-glyph"));
    const { page, device } = ipadFrame(root, { nav: { center: [center], right: [toolCluster(false), rec] }, sidebar: sb });
    sidebarRows(sb.list, 0);
    railAndInk(page, ipad.lines.slice(0, 11), { room: 640 });
    fixtureLabel(device, label(9));
  }
}
