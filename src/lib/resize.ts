import type { Accessor, Setter } from "solid-js";

/// Starts a pointer drag that resizes a pane horizontally. The width is
/// stored in em (relative to the root font size) and clamped to [minEm,
/// maxEm]. With `invert`, dragging left increases the width (for a pane on
/// the right side of the divider).
export function startPaneResize(
  e: PointerEvent,
  width: { get: Accessor<number>; set: Setter<number> },
  opts: { minEm: number; maxEm: number; invert?: boolean },
) {
  e.preventDefault();
  const startX = e.clientX;
  const startWidth = width.get();
  // Convert pixel deltas to em using the actual root font size, so the
  // resize stays correct if the base font size differs from 16px.
  const pxPerEm =
    parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;

  function onMove(ev: PointerEvent) {
    const deltaEm = (ev.clientX - startX) / pxPerEm;
    const signed = opts.invert ? -deltaEm : deltaEm;
    width.set(Math.min(opts.maxEm, Math.max(opts.minEm, startWidth + signed)));
  }

  function onUp() {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onUp);
    document.body.style.cursor = "";
  }

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onUp);
  document.body.style.cursor = "col-resize";
}
