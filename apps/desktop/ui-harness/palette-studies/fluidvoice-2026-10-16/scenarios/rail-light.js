// Palette study scenario (rail, light); generated. Synthetic stub data only.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokens = {};
for (const [k, v] of Object.entries(tokens)) document.documentElement.style.setProperty('--' + k, v);
await sleep(900);
document.getAnimations().forEach((a) => a.finish());
return { ok: true, palette: 'rail', theme: document.documentElement.dataset.theme, errors: window.__errors || [] };
