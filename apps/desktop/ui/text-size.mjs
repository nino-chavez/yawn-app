// Text size (2026-10-16): Settings offers Default and three larger steps, and
// View > Bigger / Smaller / Actual Size step through them. macOS 27's
// Accessibility > Display > Text Size applies to the apps it lists, which on
// this Mac are Apple's own (com.apple.universalaccess FontSizeCategory), and
// Apple's apps change text in the app with Cmd-+ and Cmd--; Yawn does the same. tokens.css multiplies every type size by --text-scale; this
// module is the one place that sets it.
//
// Like the theme, the choice is a per-Mac display preference, not meeting
// data, so it lives in the webview's own storage.

const STORAGE_KEY = "yawn.textSize";

export const TEXT_SIZES = ["default", "large", "larger", "largest"];

export const TEXT_SCALES = { default: 1, large: 1.15, larger: 1.3, largest: 1.5 };

export const TEXT_SIZE_EVENT = "yawn:text-size";

export function getTextSize() {
  try {
    const stored = globalThis.localStorage?.getItem(STORAGE_KEY);
    return TEXT_SIZES.includes(stored) ? stored : "default";
  } catch {
    return "default";
  }
}

export function textScale(choice) {
  return TEXT_SCALES[choice] ?? 1;
}

export function applyTextSize(choice = getTextSize()) {
  if (typeof document === "undefined") return;
  const scale = textScale(choice);
  document.documentElement.style.setProperty("--text-scale", String(scale));
  document.documentElement.dataset.textSize = TEXT_SIZES.includes(choice) ? choice : "default";
  // Layout that is measured in script (the ledger's rail marks and turns)
  // has to measure again once the new sizes take effect.
  globalThis.dispatchEvent?.(new CustomEvent(TEXT_SIZE_EVENT, { detail: { choice, scale } }));
}

export function setTextSize(choice) {
  if (!TEXT_SIZES.includes(choice)) return;
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, choice);
  } catch {
    // Storage refused: the size still applies for this session; it just will
    // not survive a relaunch.
  }
  applyTextSize(choice);
}

// View > Bigger / Smaller / Actual Size (Cmd-+, Cmd--, Cmd-0) step through the
// same choices as Settings. Stepping past either end stays put.
export function stepTextSize(direction, current = getTextSize()) {
  if (direction === 0) return "default";
  const index = Math.max(0, TEXT_SIZES.indexOf(current));
  const next = Math.min(TEXT_SIZES.length - 1, Math.max(0, index + Math.sign(direction)));
  return TEXT_SIZES[next];
}
