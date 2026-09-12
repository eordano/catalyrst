import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DclEditorChrome from "./DclEditorChrome";
import { BOOT_LEAVE_MS } from "../editor-config";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

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
    expect(leaving?.getAttribute("aria-hidden")).toBe("true");
    expect(leaving?.getAttribute("role")).toBeNull();

    act(() => {
      vi.advanceTimersByTime(BOOT_LEAVE_MS + 20);
    });
    expect(boot()).toBeNull();
  });

  it("never plays a leave animation for a curtain that was never shown", () => {
    render(<DclEditorChrome loading={false} viewportSrc={null} />);
    const el = boot();
    expect(el === null || !el.classList.contains("is-leaving")).toBe(true);
  });
});
