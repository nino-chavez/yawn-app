// Palette study scenario (cool, dark); generated. Synthetic stub data only.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokens = {"window": "#1a1b21", "sidebar": "#1f212e", "content": "#24252a", "panel": "#2a2b31", "notes": "#24252a", "notes-border": "rgba(235,240,255,0.10)", "separator": "rgba(235,240,255,0.10)", "separator-strong": "rgba(235,240,255,0.20)", "label": "#e8eaf0", "label-2": "#a0a5b0", "label-3": "#979ca7", "label-4": "#40434c", "control": "#33353d", "control-border": "rgba(235,240,255,0.14)", "selection": "rgba(235,240,255,0.07)", "selection-active": "rgba(235,240,255,0.07)", "selection-line": "#cfd3dc", "on-selection": "#e8eaf0", "accent": "#cfd3dc", "on-accent": "#1a1b21", "rail": "#464955", "rail-mark": "#e8eaf0"};
for (const [k, v] of Object.entries(tokens)) document.documentElement.style.setProperty('--' + k, v);
await sleep(900);
document.getAnimations().forEach((a) => a.finish());
return { ok: true, palette: 'cool', theme: document.documentElement.dataset.theme, errors: window.__errors || [] };
