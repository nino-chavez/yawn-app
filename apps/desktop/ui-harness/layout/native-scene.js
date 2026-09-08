// Native runner body: wait for the real renderer, then inspect layout in CSS pixels.
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const settings = location.pathname.includes('settings');
const ready = () => settings
  ? document.querySelector('#models')?.textContent.length > 50
  : window.__layoutReady;
for (let attempt = 0; attempt < 100 && !ready(); attempt++) await sleep(50);
if (!ready()) return { error: 'Scene did not become ready', errors: window.__errors };
const params = new URLSearchParams(location.search);
if (window.__layoutAppliedScene !== params.get('scene')) return { error: 'Fixture scene identity mismatch' };
const provenance = await fetch('/layout-provenance.json').then(response => response.json());
if (!provenance.synthetic || !provenance.sourceCommit || !Object.keys(provenance.sha256 || {}).length) return { error: 'Missing fixture provenance' };
if (params.get('section')) document.querySelector('#' + params.get('section'))?.scrollIntoView();
if (params.get('bottom')) {
  for (const selector of ['.start-sheet', '.doc-main', '.trash-pane', '.canvas-wrap']) {
    const element = document.querySelector(selector);
    if (element) element.scrollTop = element.scrollHeight;
  }
}
await sleep(120);
const overflows = [...document.querySelectorAll('*')].filter(element => {
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  // Single-line fields scroll their editable value intentionally. The startup
  // orb is a decorative animation, not overflowing text or a control.
  return rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight
    && element.scrollWidth > element.clientWidth + 2 && style.overflowX === 'visible'
    && !['SVG', 'INPUT', 'TEXTAREA'].includes(element.tagName)
    && !element.classList.contains('startup-orb');
}).map(element => ({ tag: element.tagName, cls: String(element.className),
  extra: element.scrollWidth - element.clientWidth, text: element.textContent.slice(0, 100) }));
const failures = [];
for (const selector of ['.sidebar-scroll', '.doc-main', '.trash-pane', '.transcript-scroll', '.inspector', '.start-sheet']) {
  const element = document.querySelector(selector);
  if (element?.clientWidth && element.scrollWidth > element.clientWidth + 2) failures.push(selector + ' scrolls horizontally');
}
const popup = document.querySelector('.evidence-popover')?.getBoundingClientRect();
if (popup && (popup.top < 11 || popup.left < 11 || popup.bottom > innerHeight - 11 || popup.right > innerWidth - 11)) failures.push('Source preview leaves the window');
const title = document.querySelector('.doc-heading')?.getBoundingClientRect();
const actions = document.querySelector('.doc-head-actions')?.getBoundingClientRect();
if (title && actions && Math.min(title.right, actions.right) > Math.max(title.left, actions.left) + 1 && Math.min(title.bottom, actions.bottom) > Math.max(title.top, actions.top) + 1) failures.push('Meeting heading overlaps its actions');
if (!settings && document.documentElement.scrollHeight > innerHeight + 2) failures.push('Main window scrolls outside its content panes');
return { pass: overflows.length === 0 && failures.length === 0, scene: params.get('scene'),
  width: innerWidth, height: innerHeight, dark: matchMedia('(prefers-color-scheme: dark)').matches,
  provenance, appliedScene: window.__layoutAppliedScene, overflows, failures, errors: window.__errors || [] };
