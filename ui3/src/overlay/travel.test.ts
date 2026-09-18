import { afterEach, describe, expect, it } from "vitest";
import { lifecycleFixture } from "../test/lifecycleFixture";
import { FakeBridge } from "../test/fakeBridge";
import { travelInExplorer } from "./travel";

afterEach(async () => {
  delete window.dclBridge;
  await new Promise((resolve) => setTimeout(resolve, 0));
});


function outcome(phase: "arrived" | "degraded" | "superseded" = "arrived") {
  return { operation: { session: "engine-a", realm: 1, instance: 1, requestId: "travel-a", attempt: 0 }, phase, reason: phase === "degraded" ? "Scene unavailable" : null };
}

describe("explorer travel completion", () => {
  it("continues observing the same engine after the bridge is replaced", async () => {
    const first = new FakeBridge();
    const replacement = new FakeBridge();
    window.dclBridge = first;
    const result = travelInExplorer({ parcel: [4, 5] }, { requestId: "travel-a" });
    first.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    window.dclBridge = replacement;
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(first.subscriberCount).toBe(0);
    expect(replacement.sent.some((command) => command.action === "GetLifecycleSnapshot")).toBe(true);
    replacement.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2, outcomes: [outcome()] }) });
    await expect(result).resolves.toMatchObject({ phase: "arrived" });
    expect(replacement.sent.some((command) => command.action === "Travel")).toBe(false);
  });

  it("rejects failed subscriptions and an already aborted observation", async () => {
    const bridge = new FakeBridge();
    const signal = AbortSignal.abort();
    await expect(travelInExplorer({}, { bridge, signal })).rejects.toMatchObject({ name: "AbortError" });
    expect(bridge.sent).toHaveLength(0);
    bridge.onState = () => { throw new Error("bridge closed"); };
    await expect(travelInExplorer({}, { bridge })).rejects.toThrow("bridge closed");
  });
  it("waits for arrival after acceptance and ignores an older snapshot", async () => {
    const bridge = new FakeBridge();
    const result = travelInExplorer({ parcel: [4, 5] }, { bridge, requestId: "travel-a" });
    let settled = false;
    void result.then(() => { settled = true; });
    expect(bridge.sent[0]?.action).toBe("GetLifecycleSnapshot");
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    expect(bridge.sent[1]).toMatchObject({ action: "Travel", payload: { requestId: "travel-a", expectedSession: "engine-a", parcel: [4, 5] } });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 3, commandResults: [{ requestId: "travel-a", action: "travel", accepted: true, error: null }] }) });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2, outcomes: [outcome()] }) });
    await Promise.resolve();
    expect(settled).toBe(false);
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 4, outcomes: [outcome()] }) });
    await expect(result).resolves.toMatchObject({ phase: "arrived" });
    expect(bridge.subscriberCount).toBe(0);
  });

  it("preserves degraded arrival in the result", async () => {
    const bridge = new FakeBridge();
    const result = travelInExplorer({ realm: "world.dcl.eth" }, { bridge, requestId: "travel-a" });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2, outcomes: [outcome("degraded")] }) });
    await expect(result).resolves.toMatchObject({ phase: "degraded", reason: "Scene unavailable" });
  });

  it("reports a replaced engine session and cleans up", async () => {
    const bridge = new FakeBridge();
    const result = travelInExplorer({ parcel: [0, 0] }, { bridge, requestId: "travel-a" });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ session: "engine-b", revision: 1 }) });
    await expect(result).rejects.toThrow("Explorer restarted during travel");
    expect(bridge.subscriberCount).toBe(0);
  });

  it("unmounting stops observation without cancelling engine work", async () => {
    const bridge = new FakeBridge();
    const controller = new AbortController();
    const result = travelInExplorer({ parcel: [0, 0] }, { bridge, signal: controller.signal, requestId: "travel-a" });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    controller.abort();
    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(bridge.sent.some((command) => command.action === "CancelTravel")).toBe(false);
    expect(bridge.subscriberCount).toBe(0);
  });
});
