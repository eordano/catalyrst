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

function stalledJump(bridge: FakeBridge) {
  const onDone = vi.fn();
  const hook = renderHook(() => useJump(onDone));
  act(() => hook.result.current.beginJump("Plaza"));
  act(() => {
    bridge.push({ kind: "loading", percent: 10, ready: false, avatarLoaded: false });
  });
  act(() => {
    vi.advanceTimersByTime(CEILING_MS);
  });
  expect(hook.result.current.jumping).toBe("Plaza");
  expect(hook.result.current.stalled).toBe(true);
  expect(onDone).not.toHaveBeenCalled();
  return { ...hook, onDone };
}

describe("useJump", () => {
  it("finishes through the fallback when the engine never reports loading, and is cancellable before the ceiling", () => {
    vi.useFakeTimers();
    const onDone = vi.fn();
    const fallback = renderHook(() => useJump(onDone));
    act(() => fallback.result.current.beginJump("Plaza"));
    expect(fallback.result.current.jumping).toBe("Plaza");
    act(() => {
      vi.advanceTimersByTime(INSTANT_FALLBACK_MS);
    });
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(fallback.result.current.jumping).toBeNull();
    fallback.unmount();

    const onCancelled = vi.fn();
    const cancelled = renderHook(() => useJump(onCancelled));
    act(() => cancelled.result.current.beginJump("Plaza"));
    act(() => cancelled.result.current.cancelJump());
    expect(onCancelled).not.toHaveBeenCalled();
    expect(cancelled.result.current.jumping).toBeNull();
  });

  it("warns at the ceiling instead of faking success; from there Enter anyway finishes, Cancel dismisses, and a late ready still finishes", () => {
    vi.useFakeTimers();
    const bridge = withBridge();

    const confirmed = stalledJump(bridge);
    act(() => confirmed.result.current.confirmJump());
    expect(confirmed.onDone).toHaveBeenCalledTimes(1);
    expect(confirmed.result.current.jumping).toBeNull();
    expect(confirmed.result.current.stalled).toBe(false);
    confirmed.unmount();

    const cancelled = stalledJump(bridge);
    act(() => cancelled.result.current.cancelJump());
    expect(cancelled.onDone).not.toHaveBeenCalled();
    expect(cancelled.result.current.jumping).toBeNull();
    expect(cancelled.result.current.stalled).toBe(false);
    cancelled.unmount();

    const late = stalledJump(bridge);
    act(() => {
      bridge.push({ kind: "loading", percent: 100, ready: true, avatarLoaded: true });
    });
    expect(late.onDone).toHaveBeenCalledTimes(1);
    expect(late.result.current.jumping).toBeNull();
  });
});

describe("JumpLoading", () => {
  it("offers Cancel from the start (also on Escape) and shows both choices once stalled", () => {
    const onCancel = vi.fn();
    const onEnterAnyway = vi.fn();
    const { rerender } = render(<JumpLoading name="Plaza" onCancel={onCancel} />);
    expect(screen.getByRole("status")).toHaveTextContent("Teleporting to Plaza");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);

    rerender(
      <JumpLoading name="Plaza" stalled onCancel={onCancel} onEnterAnyway={onEnterAnyway} />,
    );
    expect(
      screen.getByText("This scene is taking too long\u{2026} enter anyway?"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Enter anyway" }));
    expect(onEnterAnyway).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(3);
  });
});
