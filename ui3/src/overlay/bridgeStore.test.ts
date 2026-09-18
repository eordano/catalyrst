import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useBridgeState } from "./bridge";
import { FakeBridge } from "../test/fakeBridge";
import { lifecycleFixture } from "../test/lifecycleFixture";

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
  it("keeps the live state across an unmount/remount pair (StrictMode) but resets to the offline snapshot once zero listeners survive a tick", async () => {
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
    await tick();
    expect(bridge.subscriberCount).toBe(0);
    const third = renderHook(() => useBridgeState());
    expect(third.result.current.identity.name).toBe("Guest");
    expect(bridge.subscriberCount).toBe(1);
    third.unmount();
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


describe("lifecycle replication", () => {
  it("retries attaching when the bridge subscription initially throws", async () => {
    const bridge = new FakeBridge();
    const subscribe = bridge.onState;
    bridge.onState = () => { throw new Error("not ready"); };
    window.dclBridge = bridge;
    const hook = renderHook(() => useBridgeState());
    expect(hook.result.current.live).toBe(false);
    bridge.onState = subscribe;
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 300)); });
    expect(bridge.subscriberCount).toBe(1);
    act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() }));
    expect(hook.result.current.lifecycle?.session).toBe("engine-a");
    hook.unmount();
  });
  it("requests a full snapshot, orders updates, and rejects a retired engine session", () => {
    const bridge = new FakeBridge();
    window.dclBridge = bridge;
    const hook = renderHook(() => useBridgeState());
    expect(bridge.sent.some((command) => command.action === "GetLifecycleSnapshot")).toBe(true);
    act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 4 }) }));
    act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2 }) }));
    expect(hook.result.current.lifecycle?.revision).toBe(4);
    act(() => bridge.push({ ...IDENTITY }));
    act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ session: "engine-b" }) }));
    expect(hook.result.current.lifecycle?.session).toBe("engine-b");
    expect(hook.result.current.identity.name).toBe("Guest");
    act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 100 }) }));
    expect(hook.result.current.lifecycle?.session).toBe("engine-b");
    hook.unmount();
  });

  it("reattaches and requests replay while the page remains mounted", async () => {
    const first = new FakeBridge();
    const second = new FakeBridge();
    window.dclBridge = first;
    const hook = renderHook(() => useBridgeState());
    act(() => first.push({ kind: "lifecycle", snapshot: lifecycleFixture() }));
    window.dclBridge = second;
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 300)); });
    expect(first.subscriberCount).toBe(0);
    expect(second.subscriberCount).toBe(1);
    expect(second.sent.some((command) => command.action === "GetLifecycleSnapshot")).toBe(true);
    expect(hook.result.current.lifecycle).toBeNull();
    act(() => second.push({ kind: "lifecycle", snapshot: lifecycleFixture({ session: "engine-b" }) }));
    expect(hook.result.current.lifecycle?.session).toBe("engine-b");
    hook.unmount();
  });
});
