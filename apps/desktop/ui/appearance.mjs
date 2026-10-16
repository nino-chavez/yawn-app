// Appearance (operator decision, 2026-10-16): Yawn follows the Mac by default
// and Settings offers Light or Dark instead. tokens.css keys every dark value
// on `html[data-theme="dark"]` and carries no `prefers-color-scheme` fallback,
// so this module is the one place that sets that attribute.
//
// The choice is a per-Mac display preference, not meeting data, so it lives in
// the webview's own storage. The native window theme follows it too, so the
// title bar, scroll bars and menus match the page.

const STORAGE_KEY = "yawn.appearance";

export const APPEARANCES = ["system", "light", "dark"];

export function getAppearance() {
  try {
    const stored = globalThis.localStorage?.getItem(STORAGE_KEY);
    return APPEARANCES.includes(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

export function resolveTheme(choice, systemDark) {
  if (choice === "light" || choice === "dark") return choice;
  return systemDark ? "dark" : "light";
}

function systemPrefersDark() {
  return typeof globalThis.matchMedia === "function"
    && globalThis.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyAppearance(choice = getAppearance()) {
  if (typeof document === "undefined") return;
  const nativeWindow = globalThis.__TAURI__?.window?.getCurrentWindow?.();
  // Hand the native window back to the system first, so "system" reads the
  // Mac's real setting rather than a theme an earlier choice forced.
  const setNative = nativeWindow?.setTheme?.(choice === "system" ? null : choice);
  const apply = () => {
    document.documentElement.dataset.theme = resolveTheme(choice, systemPrefersDark());
  };
  apply();
  if (setNative && typeof setNative.then === "function") setNative.then(apply, () => {});
}

export function setAppearance(choice) {
  if (!APPEARANCES.includes(choice)) return;
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, choice);
  } catch {
    // Storage refused (private mode, a full disk): the choice still applies
    // for this session; it just will not survive a relaunch.
  }
  applyAppearance(choice);
}

// Follow the Mac while the choice is System; an explicit Light or Dark holds.
export function watchSystemAppearance() {
  if (typeof globalThis.matchMedia !== "function") return;
  globalThis.matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
    if (getAppearance() === "system") applyAppearance("system");
  });
}
