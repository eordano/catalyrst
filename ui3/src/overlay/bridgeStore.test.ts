import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useBridgeState } from "./bridge";
import { FakeBridge } from "../test/fakeBridge";

const IDENTITY = {
  kind: "identity",
  address: "0x1234567890abcdef1234567890abcdef12345678",
  signerAddress: "0x1234567890abcdef1234567890abcdef12345678",
  isGuest: false,
  name: "Neo",
} as const;

const tick = () => new Promise((r) => setTimeout(r, 0));

afterEach(async () => {
  delete window.dclBridge;
  await tick();
});

describe("bridge store teardown", () => {
  it("keeps the live state across an unmount/remount pair (StrictMode)", () => {
    const bridge = new FakeBridge();
    window.dclBridge = bridge;
    const first = renderHook(() => useBridgeState());
    act(() => {
      bridge.push({ ...IDENTITY });
    });
    expect(first.result.current.identity.name).toBe("Neo");

    first.unmount();
    const second = renderHook(() => useBridgeState());
    expect(second.result.current.identity.name).toBe("Neo");
    expect(bridge.subscriberCount).toBe(1);
    second.unmount();
  });

  it("resets to the offline snapshot once zero listeners survive a tick", async () => {
    const bridge = new FakeBridge();
    window.dclBridge = bridge;
    const hook = renderHook(() => useBridgeState());
    act(() => {
      bridge.push({ ...IDENTITY });
    });
    hook.unmount();
    await tick();
    expect(bridge.subscriberCount).toBe(0);

    const again = renderHook(() => useBridgeState());
    expect(again.result.current.identity.name).toBe("Guest");
    expect(bridge.subscriberCount).toBe(1);
    again.unmount();
  });

  it("re-attaches when the bridge global is replaced", () => {
    const a = new FakeBridge();
    window.dclBridge = a;
    const first = renderHook(() => useBridgeState());
    act(() => {
      a.push({ ...IDENTITY });
    });
    expect(first.result.current.identity.name).toBe("Neo");
    first.unmount();

    const b = new FakeBridge();
    window.dclBridge = b;
    const second = renderHook(() => useBridgeState());
    expect(a.subscriberCount).toBe(0);
    expect(b.subscriberCount).toBe(1);
    expect(second.result.current.identity.name).toBe("Guest");
    second.unmount();
  });
});
