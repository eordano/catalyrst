import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import JumpLoading, { JumpCompleteContext, useJump } from "./JumpLoading";
import { EMPTY_TRANSFERS, LOADING_TRANSFER_EVENT } from "../../overlay/loadingTransferProtocol";
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
  delete window.__dclLoadingTransfers;
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
  it("enters the world after completing a panel jump, but not when cancelled", () => {
    const bridge = withBridge();
    const enterWorld = vi.fn();
    const hook = renderHook(() => useJump(), {
      wrapper: ({ children }) => <JumpCompleteContext value={enterWorld}>{children}</JumpCompleteContext>,
    });
    act(() => hook.result.current.beginJump("Plaza"));
    act(() => hook.result.current.cancelJump());
    expect(enterWorld).not.toHaveBeenCalled();
    act(() => hook.result.current.beginJump("Plaza"));
    act(() => bridge.pushLoading({ ready: false, percent: 20 }));
    expect(enterWorld).not.toHaveBeenCalled();
    act(() => bridge.pushLoading({ ready: true, percent: 100 }));
    expect(enterWorld).toHaveBeenCalledOnce();
  });
  it("accepts an already-loaded destination only after the scene changes, or when already on its parcel", () => {
    const bridge = withBridge();
    const onDone = vi.fn();
    const hook = renderHook(() => useJump(onDone));
    act(() => {
      bridge.pushScene({ coords: "0,0" });
      bridge.push({ kind: "loading", percent: 100, ready: true, avatarLoaded: true });
    });
    act(() => hook.result.current.beginJump("Cached scene", "3,4"));
    expect(onDone).not.toHaveBeenCalled();
    act(() => { bridge.pushScene({ coords: "3,4" }); });
    expect(onDone).toHaveBeenCalledOnce();
    act(() => hook.result.current.beginJump("Here", "3,4"));
    expect(onDone).toHaveBeenCalledTimes(2);
    expect(hook.result.current.jumping).toBeNull();
  });
  it("keeps the destination covered without a fresh loading acknowledgement, and remains cancellable", () => {
    vi.useFakeTimers();
    const onDone = vi.fn();
    const fallback = renderHook(() => useJump(onDone));
    act(() => fallback.result.current.beginJump("Plaza"));
    expect(fallback.result.current.jumping).toBe("Plaza");
    act(() => {
      vi.advanceTimersByTime(INSTANT_FALLBACK_MS);
    });
    expect(onDone).not.toHaveBeenCalled();
    expect(fallback.result.current.jumping).toBe("Plaza");
    act(() => { vi.advanceTimersByTime(CEILING_MS); });
    expect(fallback.result.current.stalled).toBe(true);
    fallback.unmount();

    const onCancelled = vi.fn();
    const cancelled = renderHook(() => useJump(onCancelled));
    act(() => cancelled.result.current.beginJump("Plaza"));
    act(() => cancelled.result.current.cancelJump());
    expect(onCancelled).not.toHaveBeenCalled();
    expect(cancelled.result.current.jumping).toBeNull();
  });

  it("does not label an active long download stalled, but warns after 30 seconds without progress", () => {
    vi.useFakeTimers();
    const bridge = withBridge();
    const hook = renderHook(() => useJump());
    act(() => hook.result.current.beginJump("Large scene"));
    act(() => bridge.pushLoading({ ready: false, percent: 10 }));
    act(() => vi.advanceTimersByTime(20000));
    act(() => {
      window.__dclLoadingTransfers = { ...EMPTY_TRANSFERS, started: 1, active: 1, receivedBytes: 100000 };
      window.dispatchEvent(new Event(LOADING_TRANSFER_EVENT));
    });
    act(() => vi.advanceTimersByTime(20000));
    expect(hook.result.current.stalled).toBe(false);
    act(() => vi.advanceTimersByTime(10000));
    expect(hook.result.current.stalled).toBe(true);
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
    expect(screen.getAllByRole("status")[0]).toHaveTextContent("Teleporting to Plaza");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);

    rerender(
      <JumpLoading name="Plaza" stalled onCancel={onCancel} onEnterAnyway={onEnterAnyway} />,
    );
    expect(screen.getByText(/No loading progress/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Enter anyway" }));
    expect(onEnterAnyway).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(3);
  });
});
