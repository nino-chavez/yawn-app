// Platform chrome shared by the three concepts. The Mac window frame (Concept
// A: toolbar, meeting list, document pane) and the iPad frame (status bar, nav
// bar, floating sidebar, page) are decided; the concepts differ in what the
// document pane and the page contain, and in their visual language. Each
// concept's styles.css skins these elements; nothing here is product code.

import { clock } from "./fixtures.js";

export function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

export function svgIcon(name) {
  const icons = {
    sidebar: '<svg width="16" height="13" viewBox="0 0 16 13" fill="none"><rect x="0.5" y="0.5" width="15" height="12" rx="2.5" stroke="currentColor"/><line x1="5.5" y1="1" x2="5.5" y2="12" stroke="currentColor"/></svg>',
    search: '<svg width="12" height="12" viewBox="0 0 12 12" fill="none"><circle cx="5" cy="5" r="3.6" stroke="currentColor" stroke-width="1.4"/><line x1="7.8" y1="7.8" x2="11" y2="11" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>',
    trash: '<svg width="13" height="14" viewBox="0 0 13 14" fill="none"><path d="M1 3.5h11M5 3V1.5h3V3M2.5 3.5l.7 9h6.6l.7-9" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    play: '<svg width="10" height="12" viewBox="0 0 10 12"><path d="M1 1.2v9.6c0 .6.6.9 1.1.6l7.4-4.8c.5-.3.5-1 0-1.3L2.1.6C1.6.3 1 .6 1 1.2z" fill="currentColor"/></svg>',
    pause: '<svg width="10" height="12" viewBox="0 0 10 12"><rect x="1" y="1" width="3" height="10" rx="1" fill="currentColor"/><rect x="6" y="1" width="3" height="10" rx="1" fill="currentColor"/></svg>',
    stop: '<svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" rx="1.5" fill="currentColor"/></svg>',
    ellipsis: '<svg width="14" height="4" viewBox="0 0 14 4"><circle cx="2" cy="2" r="1.6" fill="currentColor"/><circle cx="7" cy="2" r="1.6" fill="currentColor"/><circle cx="12" cy="2" r="1.6" fill="currentColor"/></svg>',
    share: '<svg width="13" height="15" viewBox="0 0 13 15" fill="none"><path d="M6.5 1v9M3.5 4l3-3 3 3M2 7v6h9V7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    chevron: '<svg width="8" height="12" viewBox="0 0 8 12" fill="none"><path d="M2 2l4 4-4 4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    chevronDown: '<svg width="10" height="7" viewBox="0 0 10 7" fill="none"><path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    back: '<svg width="9" height="15" viewBox="0 0 9 15" fill="none"><path d="M8 1L2 7.5 8 14" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    pen: '<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M4 16l1.2-4.2L13.8 3.2a1.4 1.4 0 012 0l1 1a1.4 1.4 0 010 2L8.2 14.8 4 16z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/></svg>',
    eraser: '<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M3.5 13l7.5-7.5a1.5 1.5 0 012.1 0l3.4 3.4a1.5 1.5 0 010 2.1L12 15.5H7.5L3.5 13z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M7.5 15.5L11 12" stroke="currentColor" stroke-width="1.5"/></svg>',
    undo: '<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M7 5L3.5 8.5 7 12" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/><path d="M3.5 8.5h8a4.5 4.5 0 010 9H9" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>',
    transcript: '<svg width="18" height="16" viewBox="0 0 18 16" fill="none"><path d="M2 3h14M2 7h14M2 11h9" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>',
    wifi: '<svg width="16" height="12" viewBox="0 0 16 12" fill="currentColor"><path d="M8 11.2a1.3 1.3 0 110-2.6 1.3 1.3 0 010 2.6zM8 6.3c1.4 0 2.6.5 3.5 1.4l-1.2 1.2A3.3 3.3 0 008 7.9c-.9 0-1.7.4-2.3 1l-1.2-1.2A4.9 4.9 0 018 6.3zm0-3.2c2.2 0 4.2.9 5.7 2.3l-1.2 1.2A6.4 6.4 0 008 4.7c-1.8 0-3.4.7-4.5 1.9L2.3 5.4A8 8 0 018 3.1zM8 0c3.1 0 5.9 1.2 8 3.2l-1.2 1.2A9.6 9.6 0 008 1.6 9.6 9.6 0 001.2 4.4L0 3.2A11.2 11.2 0 018 0z"/></svg>',
    battery: '<svg width="27" height="13" viewBox="0 0 27 13" fill="none"><rect x="0.5" y="0.5" width="22" height="12" rx="3" stroke="currentColor" stroke-opacity="0.4"/><rect x="2" y="2" width="19" height="9" rx="1.8" fill="currentColor"/><path d="M24.5 4.5v4a2 2 0 000-4z" fill="currentColor" fill-opacity="0.4"/></svg>',
    ipad: '<svg width="14" height="11" viewBox="0 0 14 11" fill="none"><rect x="0.6" y="0.6" width="12.8" height="9.8" rx="1.6" stroke="currentColor" stroke-width="1.1"/><circle cx="12" cy="5.5" r="0.6" fill="currentColor"/></svg>',
    mic: '<svg width="11" height="14" viewBox="0 0 11 14" fill="none"><rect x="3.5" y="0.6" width="4" height="7.5" rx="2" stroke="currentColor" stroke-width="1.1"/><path d="M1 6.5a4.5 4.5 0 009 0M5.5 11v2.4M3.3 13.4h4.4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/></svg>',
    speaker: '<svg width="13" height="12" viewBox="0 0 13 12" fill="none"><path d="M1 4h2.5L7 1v10L3.5 8H1z" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/><path d="M9 3.5a3.5 3.5 0 010 5M10.8 1.8a6 6 0 010 8.4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/></svg>',
  };
  const s = el("span", "icon icon-" + name);
  s.innerHTML = icons[name] || "";
  s.setAttribute("aria-hidden", "true");
  return s;
}

// ------------------------------------------------------------- Mac frame --

/**
 * Builds the decided Mac window: unified toolbar (traffic lights, sidebar
 * toggle, title, search, Record), day-grouped sidebar with Trash last, and an
 * empty document pane. Returns { doc, toolbar, sidebar }.
 */
export function macFrame(root, { title = "Yawn", record = { mode: "idle" }, rows = [], selected = null,
  toolbarExtra = null, sidebarHidden = false } = {}) {
  const app = el("div", "mac" + (sidebarHidden ? " mac-sidebar-hidden" : ""));
  const tb = el("header", "toolbar");
  const left = el("div", "tb-left");
  const lights = el("span", "lights");
  lights.innerHTML = "<i></i><i></i><i></i>";
  lights.setAttribute("aria-hidden", "true");
  left.append(lights);
  const toggle = el("button", "icon-btn", "");
  toggle.setAttribute("aria-label", "Toggle Sidebar");
  toggle.append(svgIcon("sidebar"));
  left.append(toggle);
  tb.append(left);
  tb.append(el("div", "tb-title", title));
  const right = el("div", "tb-right");
  if (toolbarExtra) right.append(toolbarExtra);
  const search = el("div", "search");
  search.append(svgIcon("search"));
  const input = el("input");
  input.type = "text"; input.placeholder = "Search"; input.setAttribute("aria-label", "Search meetings");
  search.append(input);
  right.append(search);
  right.append(recordControl(record));
  tb.append(right);
  app.append(tb);

  const body = el("div", "mac-body");
  const sb = el("nav", "sidebar");
  sb.setAttribute("aria-label", "Meetings");
  const scroll = el("div", "sb-scroll");
  let group = null;
  for (const r of rows) {
    if (r.group !== group) { group = r.group; scroll.append(el("div", "group-label", group)); }
    const row = el("div", "row" + (r.id === selected ? " row-selected" : "") + (r.status === "attention" ? " row-attention" : "") + (r.status === "live" ? " row-live" : ""));
    row.setAttribute("role", "button");
    const t = el("div", "row-title", r.title);
    if (r.status === "attention") t.prepend(el("i", "dot"));
    if (r.status === "live") t.prepend(el("i", "dot dot-live"));
    row.append(t);
    row.append(el("div", "row-caption", [r.time, r.length, r.statusText].filter(Boolean).join(" · ")));
    if (r.excerpt) row.append(el("div", "row-excerpt", r.excerpt));
    scroll.append(row);
  }
  sb.append(scroll);
  const trash = el("div", "trash-row");
  trash.append(svgIcon("trash"), el("span", null, "Trash"));
  sb.append(trash);
  body.append(sb);
  const doc = el("main", "doc");
  body.append(doc);
  app.append(body);
  root.append(app);
  return { doc, toolbar: tb, sidebar: sb, app };
}

export function recordControl({ mode = "idle", elapsed = 0, label = "Record" }) {
  const c = el("div", "record record-" + mode);
  if (mode === "idle") {
    const b = el("button", "record-btn");
    b.append(el("i", "record-glyph"), el("span", null, label));
    c.append(b);
  } else {
    const badge = el("span", "record-badge");
    badge.append(el("i", "record-glyph"), el("span", "record-word", mode === "paused" ? "Paused" : "Recording"), el("span", "record-time", clock(elapsed)));
    c.append(badge);
    const pause = el("button", "icon-btn record-pause"); pause.setAttribute("aria-label", mode === "paused" ? "Resume" : "Pause");
    pause.append(svgIcon(mode === "paused" ? "play" : "pause"));
    const stop = el("button", "icon-btn record-stop"); stop.setAttribute("aria-label", "Stop");
    stop.append(svgIcon("stop"));
    c.append(pause, stop);
  }
  return c;
}

export function pushButton(label, cls = "") {
  const b = el("button", "push " + cls, label);
  return b;
}

export function menuButton(label) {
  const b = el("button", "push push-menu");
  b.append(el("span", null, label), svgIcon("chevronDown"));
  return b;
}

// ------------------------------------------------------------ iPad frame --

/**
 * Builds the decided iPad frame: status bar, a nav bar, an optional floating
 * sidebar, and the page. Returns { page, nav, sidebar }.
 */
export function ipadFrame(root, { nav = {}, sidebar = null, pageClass = "" } = {}) {
  const dev = el("div", "ipad");
  const status = el("div", "statusbar");
  const sl = el("span", "status-left"); sl.append(el("b", null, "11:04 AM"), el("span", null, "Wed Oct 14"));
  const sr = el("span", "status-right"); sr.append(svgIcon("wifi"), el("span", null, "96%"), svgIcon("battery"));
  status.append(sl, sr);
  dev.append(status);

  const nb = el("div", "navbar");
  const nl = el("div", "nav-left"); const nc = el("div", "nav-center"); const nr = el("div", "nav-right");
  if (nav.left) nl.append(...nav.left);
  if (nav.center) nc.append(...nav.center);
  if (nav.right) nr.append(...nav.right);
  nb.append(nl, nc, nr);
  dev.append(nb);

  const body = el("div", "ipad-body");
  if (sidebar) {
    const sb = el("div", "ipad-sidebar");
    const head = el("div", "ipad-sb-head");
    head.append(el("div", "ipad-sb-title", "Meetings"));
    const tog = el("button", "ipad-icon-btn"); tog.setAttribute("aria-label", "Hide Sidebar"); tog.append(svgIcon("sidebar"));
    head.append(tog);
    sb.append(head);
    const list = el("div", "ipad-sb-list");
    sb.append(list);
    body.append(sb);
    sidebar.list = list;
  }
  const page = el("div", "page " + pageClass);
  body.append(page);
  dev.append(body);
  root.append(dev);
  return { page, nav: nb, body, device: dev };
}

export function capsule(label, { icon = null, tint = "" } = {}) {
  const b = el("button", "capsule " + tint);
  if (icon) b.append(svgIcon(icon));
  if (label) b.append(el("span", null, label));
  return b;
}

export function fixtureLabel(root, text) {
  root.append(el("div", "fixture-label", text));
}
