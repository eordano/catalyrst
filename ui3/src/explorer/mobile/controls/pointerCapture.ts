export function capturePointer(el: Element | null, pointerId: number): void {
  if (!el || typeof el.setPointerCapture !== "function") return;
  try {
    el.setPointerCapture(pointerId);
  } catch {
  }
}

export function releasePointer(el: Element | null, pointerId: number): void {
  if (!el || typeof el.releasePointerCapture !== "function") return;
  if (typeof el.hasPointerCapture === "function" && !el.hasPointerCapture(pointerId)) return;
  try {
    el.releasePointerCapture(pointerId);
  } catch {
  }
}
