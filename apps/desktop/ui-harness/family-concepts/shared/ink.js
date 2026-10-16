// Handwriting stand-in for the iPad frames. The 2026-10-14 SwiftUI concepts
// used the same font (Bradley Hand) for the same reason: the comparison is
// about the page around the writing, and real finger strokes come from the
// device build of the selected concept.

export function inkLine({ text, indent, tilt, t, dim = false, tapped = false }, opts = {}) {
  const size = fittedSize(text, opts.room || 760);
  const d = document.createElement("div");
  d.className = "ink" + (dim ? " ink-dim" : "") + (tapped ? " ink-tapped" : "") + (t == null ? " ink-after" : "");
  d.style.marginLeft = `${indent}px`;
  d.style.transform = `rotate(${tilt}deg)`;
  d.style.fontSize = `${size}px`;
  d.textContent = text;
  return d;
}

export function fittedSize(text, room) {
  // A short line writes large; a long one shrinks to fit the room.
  const base = 44;
  const estimate = text.length * base * 0.52;
  return estimate > room ? Math.max(28, Math.floor(base * room / estimate)) : base;
}
