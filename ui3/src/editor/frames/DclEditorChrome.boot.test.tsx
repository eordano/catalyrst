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

  it("shows the curtain while loading, plays a leave animation once loaded, then unmounts; a curtain never shown never leaves", () => {
    vi.useFakeTimers();
    const fresh = render(<DclEditorChrome loading={false} viewportSrc={null} />);
    const never = boot();
    expect(never === null || !never.classList.contains("is-leaving")).toBe(true);
    fresh.unmount();

    const { rerender } = render(<DclEditorChrome loading viewportSrc={null} />);
    const shown = boot();
    expect(shown).not.toBeNull();
    expect(shown?.classList.contains("is-leaving")).toBe(false);
    expect(shown?.getAttribute("role")).toBe("status");

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
});
