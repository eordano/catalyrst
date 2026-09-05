import type { NativeHostMessage } from "../generated/bridge/NativeHostMessage";

export type { NativeHostMessage };

export type Rect = Extract<NativeHostMessage, { t: "pointerRegions" }>["rects"][number];

export function isNativeHost(): boolean {
  return typeof window !== "undefined" && !!window.__dclNativeHost;
}

// The native host appends no preview/editor flags to the shell query; the
// guard makes that structural -- a native shell must always mount the HUD.
export function isEditorShell(search: string): boolean {
  return !isNativeHost() && /[?&](editorUi=1|preview=true)(?:&|$)/.test(search);
}

function post(msg: NativeHostMessage): void {
  try {
    window.__dclNativeHost?.post(msg);
  } catch {
  }
}

function contains(a: Rect, b: Rect): boolean {
  return (
    b[0] >= a[0] &&
    b[1] >= a[1] &&
    b[0] + b[2] <= a[0] + a[2] &&
    b[1] + b[3] <= a[1] + a[3]
  );
}

function dropContained(rects: Rect[]): Rect[] {
  return rects.filter(
    (r, i) =>
      !rects.some(
        (o, j) => j !== i && contains(o, r) && (j < i || !contains(r, o)),
      ),
  );
}

// The `.ui3-overlay{pointer-events:none}` + per-widget `pointer-events:auto`
// convention (overlay.css) makes computed pointer-events the single source of
// truth for interactivity, so the walk stops at the first `auto` ancestor --
// its whole subtree is inside the recorded rect.
function visit(el: Element, rects: Rect[]): void {
  const style = getComputedStyle(el);
  if (
    style.display === "none" ||
    style.visibility === "hidden" ||
    style.opacity === "0"
  ) {
    return;
  }
  // Zero-area elements still descend: `display:contents` wrappers and
  // sizeless mount points report a zero rect while their absolutely
  // positioned children paint (and hit-test) with real boxes.
  const box = el.getBoundingClientRect();
  if (style.pointerEvents === "auto" && box.width > 0 && box.height > 0) {
    const x = Math.floor(box.left);
    const y = Math.floor(box.top);
    rects.push([x, y, Math.ceil(box.right) - x, Math.ceil(box.bottom) - y]);
    return;
  }
  for (const child of el.children) visit(child, rects);
}

export function computeInteractiveRects(root: HTMLElement): Rect[] {
  const rects: Rect[] = [];
  for (const child of root.children) visit(child, rects);
  return dropContained(rects);
}

const NON_RENDERED = new Set(["SCRIPT", "STYLE", "LINK", "META", "TEMPLATE"]);

// Modals/popovers portal to document.body (components/Modal.tsx), outside
// #ui3-overlay: measuring only the root would leave them click-transparent.
export function computePageRects(root: HTMLElement): Rect[] {
  const rects: Rect[] = [];
  for (const child of root.children) visit(child, rects);
  for (const sibling of document.body.children) {
    if (sibling === root || sibling.contains(root)) continue;
    if (NON_RENDERED.has(sibling.tagName)) continue;
    visit(sibling, rects);
  }
  return dropContained(rects);
}

// FNV-1a over the quantised rect list: the emit gate, so identical geometry
// never re-crosses the IPC boundary.
export function hashRects(rects: readonly Rect[]): number {
  const s = rects.map((r) => r.join(",")).join(";");
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

const EDITABLE = 'input, textarea, [contenteditable=""], [contenteditable="true"]';

export function startNativeHostBridge(): () => void {
  if (typeof window === "undefined") return () => {};
  const root = document.getElementById("ui3-overlay");
  if (!root) return () => {};

  let lastHash = -1;
  let rafId: number | null = null;
  const measure = (): void => {
    rafId = null;
    const rects = computePageRects(root);
    const h = hashRects(rects);
    if (h === lastHash) return;
    lastHash = h;
    post({
      t: "pointerRegions",
      w: window.innerWidth,
      h: window.innerHeight,
      rects,
    });
  };
  const schedule = (): void => {
    if (rafId !== null) return;
    rafId = window.requestAnimationFrame(measure);
  };

  // body, not root: portals mount their panels as body children.
  const mo = new MutationObserver(schedule);
  mo.observe(document.body, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["class", "style"],
  });
  let ro: ResizeObserver | null = null;
  if (typeof ResizeObserver !== "undefined") {
    ro = new ResizeObserver(schedule);
    ro.observe(document.body);
  }
  window.addEventListener("resize", schedule);
  // 8 Hz safety tick: CSS transitions move rects without mutating anything.
  const tick = setInterval(schedule, 125);

  // Debounced so focus hopping between two fields emits nothing: the X focus
  // transfer on the host side is expensive to thrash.
  let focusTimer: ReturnType<typeof setTimeout> | null = null;
  let lastWant = false;
  const emitFocus = (want: boolean): void => {
    if (focusTimer !== null) clearTimeout(focusTimer);
    focusTimer = setTimeout(() => {
      focusTimer = null;
      if (want === lastWant) return;
      lastWant = want;
      post({ t: "keyboardFocus", want });
    }, 16);
  };
  const editable = (t: EventTarget | null): boolean =>
    t instanceof Element && t.matches(EDITABLE);
  const onFocusIn = (e: FocusEvent): void => {
    if (editable(e.target)) emitFocus(true);
  };
  const onFocusOut = (e: FocusEvent): void => {
    if (editable(e.target)) emitFocus(false);
  };
  // document, not root: fields inside body-portaled modals must also park
  // engine input while focused.
  document.addEventListener("focusin", onFocusIn);
  document.addEventListener("focusout", onFocusOut);

  schedule();

  return () => {
    mo.disconnect();
    ro?.disconnect();
    window.removeEventListener("resize", schedule);
    clearInterval(tick);
    if (rafId !== null) window.cancelAnimationFrame(rafId);
    if (focusTimer !== null) clearTimeout(focusTimer);
    document.removeEventListener("focusin", onFocusIn);
    document.removeEventListener("focusout", onFocusOut);
  };
}
