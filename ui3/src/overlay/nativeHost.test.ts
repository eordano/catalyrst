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

const FAKED = [
  "setTimeout",
  "clearTimeout",
  "setInterval",
  "clearInterval",
  "requestAnimationFrame",
  "cancelAnimationFrame",
] as const;

afterEach(() => {
  document.body.innerHTML = "";
  delete window.__dclNativeHost;
  vi.useRealTimers();
});

describe("computeInteractiveRects", () => {
  it("records the first auto ancestor only, rounded outward, drops rects inside a sibling, and hashes stably across no-op mutations", () => {
    const root = overlayRoot();
    const stage = el("none", [0, 0, 1280, 720]);
    const widget = el("auto", [10.2, 20.7, 99.5, 40.1]);
    widget.appendChild(el("auto", [20, 30, 50, 20], "button"));
    stage.appendChild(widget);
    stage.appendChild(el("auto", [20, 30, 10, 10]));
    root.appendChild(stage);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 41]]);

    const before = hashRects(computeInteractiveRects(root));
    stage.dataset.tick = "1";
    expect(hashRects(computeInteractiveRects(root))).toBe(before);
    stage.appendChild(el("auto", [500, 20, 40, 40]));
    expect(hashRects(computeInteractiveRects(root))).not.toBe(before);
  });

  it("skips hidden and zero-area subtrees but descends through zero-area wrappers to overflowing children", () => {
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

    const wrapper = el("none", [0, 0, 0, 0]);
    wrapper.style.display = "contents";
    wrapper.appendChild(el("auto", [10, 20, 100, 40]));
    root.appendChild(wrapper);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 40]]);
  });

  it("captures portal subtrees mounted outside the overlay root in the page rects only", () => {
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    const portal = el("none", [0, 0, 0, 0]);
    portal.appendChild(el("auto", [300, 200, 400, 300]));
    document.body.appendChild(portal);
    document.body.appendChild(document.createElement("script"));
    expect(computePageRects(root)).toEqual([
      [10, 20, 100, 40],
      [300, 200, 400, 300],
    ]);
    expect(computeInteractiveRects(root)).toEqual([[10, 20, 100, 40]]);
  });
});

describe("startNativeHostBridge", () => {
  it("is a no-op without a host and survives a host that throws", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    const root = overlayRoot();
    root.appendChild(el("auto", [10, 20, 100, 40]));
    const stop = startNativeHostBridge();
    vi.advanceTimersByTime(300);
    const post = vi.fn();
    window.__dclNativeHost = { post };
    vi.advanceTimersByTime(300);
    stop();
    expect(post).not.toHaveBeenCalled();

    window.__dclNativeHost = {
      post: () => {
        throw new Error("gone");
      },
    };
    const stop2 = startNativeHostBridge();
    expect(() => vi.advanceTimersByTime(300)).not.toThrow();
    stop2();
  });

  it("posts pointerRegions once per geometry, again when a modal portals into document.body, and keyboardFocus on editables", () => {
    vi.useFakeTimers({ toFake: [...FAKED] });
    const posts: NativeHostMessage[] = [];
    window.__dclNativeHost = { post: (m) => posts.push(m) };
    const root = overlayRoot();
    const widget = el("auto", [10, 20, 100, 40]);
    root.appendChild(widget);
    const stop = startNativeHostBridge();
    vi.advanceTimersByTime(300);
    expect(posts).toEqual([{ t: "pointerRegions", w: 1024, h: 768, rects: [[10, 20, 100, 40]] }]);
    vi.advanceTimersByTime(300);
    expect(posts).toHaveLength(1);

    document.body.appendChild(el("auto", [300, 200, 400, 300]));
    vi.advanceTimersByTime(300);
    expect(posts.at(-1)).toEqual({
      t: "pointerRegions",
      w: 1024,
      h: 768,
      rects: [
        [10, 20, 100, 40],
        [300, 200, 400, 300],
      ],
    });

    const input = el("auto", undefined, "input");
    widget.appendChild(input);
    input.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    input.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    input.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    vi.advanceTimersByTime(50);
    expect(posts.filter((m) => m.t === "keyboardFocus")).toEqual([{ t: "keyboardFocus", want: true }]);
    stop();
  });
});

describe("host detection", () => {
  it("isNativeHost reflects the global and a native host is never the editor shell", () => {
    expect(isNativeHost()).toBe(false);
    expect(isEditorShell("?editorUi=1")).toBe(true);
    expect(isEditorShell("?preview=true")).toBe(true);
    expect(isEditorShell("?realm=x&preview=true&y=1")).toBe(true);
    expect(isEditorShell("?realm=x")).toBe(false);

    window.__dclNativeHost = { post: () => {} };
    expect(isNativeHost()).toBe(true);
    expect(isEditorShell("?preview=true")).toBe(false);
    expect(isEditorShell("?editorUi=1")).toBe(false);
  });
});
