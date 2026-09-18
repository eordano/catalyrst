import { describe, expect, it } from "vitest";
import { FakeBridge } from "../test/fakeBridge";
import { lifecycleFixture } from "../test/lifecycleFixture";
import { lifecycleCommand, type LifecycleCommand } from "./lifecycleCommand";

const command: LifecycleCommand = { action: "CancelTravel", payload: { requestId: "travel-a", expectedSession: "engine-a" } };

describe("lifecycle command acknowledgements", () => {
  it("matches the action as well as the request and ignores older revisions", async () => {
    const bridge = new FakeBridge();
    const result = lifecycleCommand(command, { bridge });
    let completed = false;
    void result.then(() => { completed = true; }, () => {});
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 3, commandResults: [{ requestId: "travel-a", action: "travel", accepted: true, error: null }] }) });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2, commandResults: [{ requestId: "travel-a", action: "cancelTravel", accepted: true, error: null }] }) });
    await Promise.resolve();
    expect(completed).toBe(false);
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 4, commandResults: [{ requestId: "travel-a", action: "cancelTravel", accepted: false, error: "Destination is already committed" }] }) });
    await expect(result).rejects.toThrow("Destination is already committed");
    expect(bridge.subscriberCount).toBe(0);
  });

  it("rejects a response from a replaced engine even if its request ID matches", async () => {
    const bridge = new FakeBridge();
    const result = lifecycleCommand(command, { bridge });
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ session: "engine-b", commandResults: [{ requestId: "travel-a", action: "cancelTravel", accepted: true, error: null }] }) });
    await expect(result).rejects.toThrow("Explorer restarted");
    expect(bridge.subscriberCount).toBe(0);
  });

  it("unsubscribes when the panel closes without issuing another engine command", async () => {
    const bridge = new FakeBridge();
    const controller = new AbortController();
    const result = lifecycleCommand(command, { bridge, signal: controller.signal });
    controller.abort();
    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(bridge.sentOf("CancelTravel")).toHaveLength(1);
    expect(bridge.subscriberCount).toBe(0);
  });
});
