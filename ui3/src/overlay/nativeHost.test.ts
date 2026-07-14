import { afterEach, describe, expect, it, vi } from "vitest";

import type { NativeHostMessage } from "./nativeHost";
import {
  computeInteractiveRects,
  computePageRects,
  hashRects,
  isEditorShell,
  isNativeHost,
  startNativeHostBridge,
} from "./nativeHost";

function box(el: HTMLElement, x: number, y: number, w: number, h: number): void {
  el.getBoundingClientRect = () =>
    ({
      x,
      y,
      left: x,
      top: y,
      right: x + w,
      bottom: y + h,
      width: w,
      height: h,
      toJSON: () => ({}),
    }) as DOMRect;
}

function el(
  pe: "auto" | "none",
  rect?: [number, number, number, number],
  tag = "div",
): HTMLElement {
  const node = document.createElement(tag);
  node.style.pointerEvents = pe;
  if (rect) box(node, ...rect);
  return node;
}

function overlayRoot(): HTMLElement {
  const root = el("none", [0, 0, 1280, 720]);
  root.id = "ui3-overlay";
  document.body.appendChild(root);
  return root;
}

afterEach(() => {
  document.body.innerHTML = "";
  delete window.__dclNativeHost;
  vi.useRealTimers();
});

describe("computeInteractiveRects", () => {
  it("records the first auto ancestor only, rounded outward", () => {
    const root = overlayRoot();
    const stage = el("none", [0, 0, 1280, 720]);
    const widget = el("auto", [10.2, 20.7, 99.5, 40.1]);
    const inner = el("auto", [20, 30, 50, 20], "button");
    widget.appendChild(inner);
    stage.appendChild(widget);
    root.appendChild(stage);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 41]]);
  });

  it("drops rects contained in a sibling rect", () => {
    const root = overlayRoot();
    const big = el("auto", [10, 20, 100, 41]);
    const small = el("auto", [20, 30, 10, 10]);
    root.append(big, small);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 41]]);
  });

  it("skips hidden and zero-area subtrees", () => {
    const root = overlayRoot();
    const hidden = el("none", [0, 0, 300, 300]);
    hidden.style.display = "none";
    hidden.appendChild(el("auto", [5, 5, 50, 50]));
    const ghost = el("auto", [0, 0, 40, 40]);
    ghost.style.visibility = "hidden";
    const faded = el("auto", [0, 0, 40, 40]);
    faded.style.opacity = "0";
    const flat = el("auto", [0, 0, 0, 40]);
    root.append(hidden, ghost, faded, flat);
    expect(computeInteractiveRects(root)).toEqual([]);
  });

  it("descends through zero-area wrappers to overflowing children", () => {
    const root = overlayRoot();
    const wrapper = el("none", [0, 0, 0, 0]);
    wrapper.style.display = "contents";
    wrapper.appendChild(el("auto", [10, 20, 100, 40]));
    root.appendChild(wrapper);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 40]]);
  });

  it("captures portal subtrees mounted outside the overlay root", () => {
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    // components/Modal.tsx portals straight to document.body.
    const portal = el("none", [0, 0, 0, 0]);
    portal.appendChild(el("auto", [300, 200, 400, 300]));
    document.body.appendChild(portal);
    const script = document.createElement("script");
    document.body.appendChild(script);
    expect(computePageRects(root)).toEqual([
      [10, 20, 100, 40],
      [300, 200, 400, 300],
    ]);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 40]]);
  });

  it("hashes identically across a no-op mutation", () => {
    const root = overlayRoot();
    const stage = el("none", [0, 0, 1280, 720]);
    stage.appendChild(el("auto", [10, 20, 100, 40]));
    root.appendChild(stage);
    const before = hashRects(computeInteractiveRects(root));
    stage.dataset.tick = "1";
    expect(hashRects(computeInteractiveRects(root))).toBe(before);
    stage.appendChild(el("auto", [500, 20, 40, 40]));
    expect(hashRects(computeInteractiveRects(root))).not.toBe(before);
  });
});

const FAKED = [
  "setTimeout",
  "clearTimeout",
  "setInterval",
  "clearInterval",
  "requestAnimationFrame",
  "cancelAnimationFrame",
] as const;

describe("startNativeHostBridge", () => {
  it("is a no-op without a host: nothing emitted, nothing thrown", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    const stop = startNativeHostBridge();
    vi.advanceTimersByTime(300);
    const post = vi.fn();
    window.__dclNativeHost = { post };
    vi.advanceTimersByTime(300);
    stop();
    // hash gate: geometry already measured while the host was absent, so a
    // late-arriving host sees no re-emit until geometry actually changes.
    expect(post).not.toHaveBeenCalled();
  });

  it("posts pointerRegions once per geometry, keyboardFocus on editables", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    const posts: NativeHostMessage[] = [];
    window.__dclNativeHost = { post: (m) => posts.push(m) };
    const root = overlayRoot();
    const widget = el("auto", [10, 20, 100, 40]);
    root.appendChild(widget);
    const stop = startNativeHostBridge();
    vi.advanceTimersByTime(300);
    expect(posts).toEqual([
      { t: "pointerRegions", w: 1024, h: 768, rects: [[10, 20, 100, 40]] },
    ]);
    vi.advanceTimersByTime(300);
    expect(posts).toHaveLength(1);

    const input = el("auto", undefined, "input");
    widget.appendChild(input);
    input.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    input.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    input.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    vi.advanceTimersByTime(50);
    expect(posts.filter((m) => m.t === "keyboardFocus")).toEqual([
      { t: "keyboardFocus", want: true },
    ]);
    stop();
  });

  it("re-emits when a modal portals into document.body", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    const posts: NativeHostMessage[] = [];
    window.__dclNativeHost = { post: (m) => posts.push(m) };
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    const stop = startNativeHostBridge();
    vi.advanceTimersByTime(300);
    expect(posts).toHaveLength(1);

    document.body.appendChild(el("auto", [300, 200, 400, 300]));
    vi.advanceTimersByTime(300);
    const last = posts.at(-1);
    expect(last).toEqual({
      t: "pointerRegions",
      w: 1024,
      h: 768,
      rects: [
        [10, 20, 100, 40],
        [300, 200, 400, 300],
      ],
    });
    stop();
  });

  it("survives a rejecting host", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    window.__dclNativeHost = {
      post: () => {
        throw new Error("gone");
      },
    };
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    const stop = startNativeHostBridge();
    expect(() => vi.advanceTimersByTime(300)).not.toThrow();
    stop();
  });
});

describe("isNativeHost", () => {
  it("reflects the global", () => {
    expect(isNativeHost()).toBe(false);
    window.__dclNativeHost = { post: () => {} };
    expect(isNativeHost()).toBe(true);
  });
});

describe("isEditorShell", () => {
  it("matches editor/preview queries on the web", () => {
    expect(isEditorShell("?editorUi=1")).toBe(true);
    expect(isEditorShell("?preview=true")).toBe(true);
    expect(isEditorShell("?realm=x&preview=true&y=1")).toBe(true);
    expect(isEditorShell("?realm=x")).toBe(false);
  });

  it("never treats a native host as the editor shell", () => {
    // Regression: --preview + --hud must still mount the HUD (the native
    // host carries preview via the Ready event, not the query).
    window.__dclNativeHost = { post: () => {} };
    expect(isEditorShell("?preview=true")).toBe(false);
    expect(isEditorShell("?editorUi=1")).toBe(false);
  });
});
