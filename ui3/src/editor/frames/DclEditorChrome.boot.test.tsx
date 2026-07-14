import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DclEditorChrome from "./DclEditorChrome";
import { BOOT_LEAVE_MS } from "../editor-config";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

// The boot curtain is a full-bleed blurred overlay over the engine canvas, and it
// used to unmount on the frame the engine reported ready -- the most jarring cut
// in the editor. It now outlives its own condition by one animation. That is a
// timing behaviour, so it needs a test: a stray `is-leaving` on first paint, or a
// curtain that never unmounts, are both invisible to a type check.
describe("DclEditorChrome boot curtain", () => {
  const boot = (): HTMLElement | null => document.querySelector(".eui-boot");

  it("shows the curtain while loading, with no leave state", () => {
    render(<DclEditorChrome loading viewportSrc={null} />);
    const el = boot();
    expect(el).not.toBeNull();
    expect(el?.classList.contains("is-leaving")).toBe(false);
    expect(el?.getAttribute("role")).toBe("status");
  });

  it("plays a leave animation, then unmounts", () => {
    vi.useFakeTimers();
    const { rerender } = render(<DclEditorChrome loading viewportSrc={null} />);
    expect(boot()).not.toBeNull();

    rerender(<DclEditorChrome loading={false} viewportSrc={null} />);
    const leaving = boot();
    expect(leaving?.classList.contains("is-leaving")).toBe(true);
    // Announced only while it is really saying something; on the way out it is
    // decoration and must not be read again.
    expect(leaving?.getAttribute("aria-hidden")).toBe("true");
    expect(leaving?.getAttribute("role")).toBeNull();

    act(() => {
      vi.advanceTimersByTime(BOOT_LEAVE_MS + 20);
    });
    expect(boot()).toBeNull();
  });

  // The guard that makes the leave state conditional on having actually shown:
  // without it every mount would play a fade-out for a curtain nobody saw.
  it("never plays a leave animation for a curtain that was never shown", () => {
    render(<DclEditorChrome loading={false} viewportSrc={null} />);
    const el = boot();
    expect(el === null || !el.classList.contains("is-leaving")).toBe(true);
  });
});
