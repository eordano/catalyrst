import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import JumpLoading, { useJump } from "./JumpLoading";
import { FakeBridge } from "../../test/fakeBridge";

const INSTANT_FALLBACK_MS = 3500;
const CEILING_MS = 30000;

function withBridge() {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  return bridge;
}

afterEach(() => {
  delete window.dclBridge;
  vi.useRealTimers();
});

describe("useJump", () => {
  it("finishes through the fallback when the engine never reports loading", () => {
    vi.useFakeTimers();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    expect(result.current.jumping).toBe("Plaza");
    act(() => {
      vi.advanceTimersByTime(INSTANT_FALLBACK_MS);
    });
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(result.current.jumping).toBeNull();
  });

  it("shows the warning at the ceiling instead of faking success", () => {
    vi.useFakeTimers();
    const bridge = withBridge();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    act(() => {
      bridge.push({ kind: "loading", percent: 10, ready: false, avatarLoaded: false });
    });
    act(() => {
      vi.advanceTimersByTime(CEILING_MS);
    });
    expect(result.current.jumping).toBe("Plaza");
    expect(result.current.stalled).toBe(true);
    expect(onDone).not.toHaveBeenCalled();
  });

  it("Enter anyway proceeds through the old success path", () => {
    vi.useFakeTimers();
    const bridge = withBridge();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    act(() => {
      bridge.push({ kind: "loading", percent: 10, ready: false, avatarLoaded: false });
    });
    act(() => {
      vi.advanceTimersByTime(CEILING_MS);
    });
    act(() => result.current.confirmJump());
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(result.current.jumping).toBeNull();
    expect(result.current.stalled).toBe(false);
  });

  it("Cancel dismisses without running the done path", () => {
    vi.useFakeTimers();
    const bridge = withBridge();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    act(() => {
      bridge.push({ kind: "loading", percent: 10, ready: false, avatarLoaded: false });
    });
    act(() => {
      vi.advanceTimersByTime(CEILING_MS);
    });
    act(() => result.current.cancelJump());
    expect(onDone).not.toHaveBeenCalled();
    expect(result.current.jumping).toBeNull();
    expect(result.current.stalled).toBe(false);
  });

  it("is cancellable before the ceiling too", () => {
    vi.useFakeTimers();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    act(() => result.current.cancelJump());
    expect(onDone).not.toHaveBeenCalled();
    expect(result.current.jumping).toBeNull();
  });

  it("a world that becomes ready while stalled still finishes", () => {
    vi.useFakeTimers();
    const bridge = withBridge();
    const onDone = vi.fn();
    const { result } = renderHook(() => useJump(onDone));
    act(() => result.current.beginJump("Plaza"));
    act(() => {
      bridge.push({ kind: "loading", percent: 10, ready: false, avatarLoaded: false });
    });
    act(() => {
      vi.advanceTimersByTime(CEILING_MS);
    });
    expect(result.current.stalled).toBe(true);
    act(() => {
      bridge.push({ kind: "loading", percent: 100, ready: true, avatarLoaded: true });
    });
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(result.current.jumping).toBeNull();
  });
});

describe("JumpLoading", () => {
  it("offers Cancel from the start and cancels on Escape", () => {
    const onCancel = vi.fn();
    render(<JumpLoading name="Plaza" onCancel={onCancel} />);
    expect(screen.getByRole("status")).toHaveTextContent("Teleporting to Plaza");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("renders the too-long warning with both choices when stalled", () => {
    const onCancel = vi.fn();
    const onEnterAnyway = vi.fn();
    render(
      <JumpLoading name="Plaza" stalled onCancel={onCancel} onEnterAnyway={onEnterAnyway} />,
    );
    expect(
      screen.getByText("This scene is taking too long\u{2026} enter anyway?"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Enter anyway" }));
    expect(onEnterAnyway).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
