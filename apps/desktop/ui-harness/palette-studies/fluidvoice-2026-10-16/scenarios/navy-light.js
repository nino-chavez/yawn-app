// Palette study scenario (navy, light); generated. Synthetic stub data only.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokens = {"window": "#e7eaf0", "sidebar": "#e1e5ed", "moment": "#0f6b6d", "on-moment": "#ffffff", "moment-soft": "rgba(15,107,109,0.12)", "evidence": "rgba(15,107,109,0.12)"};
for (const [k, v] of Object.entries(tokens)) document.documentElement.style.setProperty('--' + k, v);
await sleep(900);
document.getAnimations().forEach((a) => a.finish());
return { ok: true, palette: 'navy', theme: document.documentElement.dataset.theme, errors: window.__errors || [] };
