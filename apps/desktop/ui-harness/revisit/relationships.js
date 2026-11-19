/* Review-only cues on existing relationships; no new seek control or command. */
(() => {
  if (document.documentElement.dataset.yawnStudy !== 'refined') return;
  const style = document.createElement('style');
  style.id = 'review-relationships';
  document.head.append(style);
  let signature = '';
  function refresh() {
    const lines = [...new Set([...document.querySelectorAll('.ledger [data-line]')]
      .map(node => Number(node.dataset.line))
      .filter(line => Number.isInteger(line) && line >= 0))].sort((a, b) => a - b);
    const next = lines.join(',');
    if (next === signature) return;
    signature = next;
    style.textContent = lines.map(line => {
      const interactive = `html[data-yawn-study="refined"] .ledger:has(.ledger-turn[data-line="${line}"]:hover, .ledger-fold[data-line="${line}"]:hover, .ledger-fold[data-line="${line}"]:focus-visible)`;
      // A native offscreen frame cannot receive physical hover/keyboard focus.
      // This explicit visual pose is used only by capture-state.js; real browser
      // interaction uses the separate :has selector above.
      const capture = `html[data-yawn-study="refined"][data-yawn-review-line="${line}"] .ledger`;
      return [interactive, capture].map(scope => `
        ${scope} .ledger-mirror-line:nth-child(${line + 1}),
        ${scope} .ledger-turn[data-line="${line}"] { background:var(--moment-soft); }
        ${scope} .ledger-mark[data-line="${line}"] span { color:var(--moment); }
        ${scope} .ledger-mark[data-line="${line}"] i { background:var(--moment); }
        ${scope} .ledger-turn[data-line="${line}"] { box-shadow:inset 2px 0 0 var(--moment); }
      `).join('\n');
    }).join('\n');
  }
  const root = document.querySelector('#app');
  if (root) {
    new MutationObserver(refresh).observe(root, { childList:true, subtree:true });
    refresh();
  }
})();
