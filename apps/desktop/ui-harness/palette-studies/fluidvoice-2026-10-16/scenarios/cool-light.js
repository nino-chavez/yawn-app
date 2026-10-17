// Palette study scenario (cool, light); generated. Synthetic stub data only.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokens = {"window": "#e9eaee", "sidebar": "#e2e4ea", "content": "#fafbfc", "panel": "#f1f2f6", "notes": "#fafbfc", "notes-border": "rgba(28,34,50,0.12)", "separator": "rgba(28,34,50,0.12)", "separator-strong": "rgba(28,34,50,0.22)", "label": "#1c1e24", "label-2": "#5c616c", "label-3": "#5d626c", "label-4": "#c8ccd4", "control": "#ffffff", "control-border": "rgba(28,34,50,0.18)", "selection": "rgba(28,34,50,0.07)", "selection-active": "rgba(28,34,50,0.07)", "selection-line": "#363a44", "on-selection": "#1c1e24", "accent": "#363a44", "on-accent": "#fafbfc", "rail": "#c9cdd6", "rail-mark": "#1c1e24"};
for (const [k, v] of Object.entries(tokens)) document.documentElement.style.setProperty('--' + k, v);
await sleep(900);
document.getAnimations().forEach((a) => a.finish());
return { ok: true, palette: 'cool', theme: document.documentElement.dataset.theme, errors: window.__errors || [] };
