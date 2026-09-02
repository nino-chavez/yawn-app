// Reads ?state=1..4&theme=dark|light, applies the theme, exposes the state.
const q = new URLSearchParams(location.search);
export const state = Number(q.get("state") || 1);
export const theme = q.get("theme") || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.documentElement.dataset.theme = theme;
document.documentElement.dataset.state = String(state);
