// Palette study scenario (navy, dark); generated. Synthetic stub data only.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokens = {"window": "#1b1d26", "sidebar": "#1f212e", "moment": "#4fcfcf", "on-moment": "#0b2626", "moment-soft": "rgba(79,207,207,0.16)", "evidence": "rgba(79,207,207,0.16)"};
for (const [k, v] of Object.entries(tokens)) document.documentElement.style.setProperty('--' + k, v);
await sleep(900);
document.getAnimations().forEach((a) => a.finish());
return { ok: true, palette: 'navy', theme: document.documentElement.dataset.theme, errors: window.__errors || [] };
