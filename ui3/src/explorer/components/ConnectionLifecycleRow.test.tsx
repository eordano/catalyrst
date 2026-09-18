import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { ConnectionStatus } from "../../generated/bridge/ConnectionStatus";
import { FakeBridge } from "../../test/fakeBridge";
import { lifecycleFixture } from "../../test/lifecycleFixture";
import ConnectionLifecycleRow from "./ConnectionLifecycleRow";

afterEach(async () => { cleanup(); delete window.dclBridge; await new Promise((r) => setTimeout(r, 0)); });
const connection: ConnectionStatus = { id: "transport:12", protocol: "websocket", scene: null, control: false, phase: "down", attempt: 2, error: "Connection closed", canRetry: true };

it("retries the exact disconnected instance and reports a rejected retry", async () => {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  render(<ConnectionLifecycleRow connection={connection} session="engine-a" />);
  fireEvent.click(screen.getByRole("button", { name: "Retry nearby players" }));
  const request = bridge.lastSent("RetryConnection")!;
  expect(request).toMatchObject({ expectedSession: "engine-a", connectionId: "transport:12" });
  expect(screen.getByRole("button", { name: "Retry nearby players" })).toBeDisabled();
  act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ commandResults: [{ requestId: request.requestId, action: "retryConnection", accepted: false, error: "Connection was replaced" }] }) }));
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Connection was replaced"));
});

it("shows reconnecting progress without offering an unavailable action", () => {
  render(<ConnectionLifecycleRow connection={{ ...connection, phase: "connecting", canRetry: false, error: null }} session="engine-a" />);
  expect(screen.getByText("Connecting")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});
