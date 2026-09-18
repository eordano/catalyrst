import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DclEditorChrome from "./DclEditorChrome";
import { BOOT_LEAVE_MS, BOOT_PROGRESS_POLL_MS } from "../editor-config";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("DclEditorChrome boot curtain", () => {
  const boot = (): HTMLElement | null => document.querySelector(".eui-boot");

  it("waits for the running engine instead of the pre-start Ready signal and observes bootstrap failure", () => {
    vi.useFakeTimers();
    const onBootstrap = vi.fn();
    const onEngineStatus = vi.fn();
    const viewportRef = { current: null as HTMLIFrameElement | null };
    render(<DclEditorChrome viewportSrc="/_play/?editorUi=1" viewportRef={viewportRef} onBootstrap={onBootstrap} onEngineStatus={onEngineStatus} />);
    const host = { dclEngineReady: true, dclBootstrapSnapshot: { phase: "Ready" } };
    Object.defineProperty(viewportRef.current!, "contentWindow", { configurable: true, value: host });
    act(() => { vi.advanceTimersByTime(BOOT_PROGRESS_POLL_MS); });
    expect(onBootstrap).toHaveBeenLastCalledWith(expect.objectContaining({ ready: false }));
    host.dclBootstrapSnapshot.phase = "World";
    act(() => { vi.advanceTimersByTime(BOOT_PROGRESS_POLL_MS); });
    expect(onBootstrap).toHaveBeenLastCalledWith(expect.objectContaining({ ready: true }));
    host.dclBootstrapSnapshot.phase = "Failed";
    act(() => { vi.advanceTimersByTime(BOOT_PROGRESS_POLL_MS); });
    expect(onEngineStatus).toHaveBeenLastCalledWith("offline");
  });

  it("treats iframe readiness as an engine fact when no scene readiness prop is supplied", () => {
    vi.useFakeTimers();
    const onBootstrap = vi.fn();
    const onEngineStatus = vi.fn();
    const viewportRef = { current: null as HTMLIFrameElement | null };
    render(
      <DclEditorChrome
        viewportSrc="/_play/?editorSession=00000000-0000-4000-8000-000000000001"
        viewportRef={viewportRef}
        onBootstrap={onBootstrap}
        onEngineStatus={onEngineStatus}
      />,
    );
    Object.defineProperty(viewportRef.current!, "contentWindow", {
      configurable: true,
      value: { dclEngineReady: true },
    });

    act(() => {
      vi.advanceTimersByTime(BOOT_PROGRESS_POLL_MS);
    });

    expect(onBootstrap).toHaveBeenLastCalledWith(expect.objectContaining({ ready: true }));
    expect(onEngineStatus).toHaveBeenLastCalledWith("connecting");
    expect(boot()?.textContent).toContain("Starting scene");
  });

  it("shows the curtain while loading, plays a leave animation once loaded, then unmounts; a curtain never shown never leaves", () => {
    vi.useFakeTimers();
    const fresh = render(<DclEditorChrome loading={false} viewportSrc={null} />);
    expect(boot()?.getAttribute("aria-hidden")).not.toBe("true");
    fresh.unmount();

    const { rerender } = render(<DclEditorChrome loading viewportSrc={null} />);
    const shown = boot();
    expect(shown).not.toBeNull();
    expect(shown?.getAttribute("role")).toBe("status");

    rerender(<DclEditorChrome loading={false} viewportSrc={null} />);
    const leaving = boot();
    expect(leaving?.getAttribute("aria-hidden")).toBe("true");
    expect(leaving?.getAttribute("role")).toBeNull();

    act(() => {
      vi.advanceTimersByTime(BOOT_LEAVE_MS + 20);
    });
    expect(boot()).toBeNull();
  });
});
