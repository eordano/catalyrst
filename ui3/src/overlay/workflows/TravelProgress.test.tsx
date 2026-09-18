import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { TravelStatus } from "../../generated/bridge/TravelStatus";
import { FakeBridge } from "../../test/fakeBridge";
import { lifecycleFixture } from "../../test/lifecycleFixture";
import TravelProgress from "./TravelProgress";

afterEach(async () => { cleanup(); delete window.dclBridge; await new Promise((r) => setTimeout(r, 0)); });

const travel: TravelStatus = {
  operation: { session: "engine-a", realm: 1, instance: 1, requestId: "travel-a", attempt: 0 },
  realmOperation: null, phase: "preparing", realm: "world.dcl.eth", parcel: [4, 5],
  blockingReason: "Starting destination scene", canCancel: true,
};

it("renders the engine's reason and only acknowledges cancellation after the engine responds", async () => {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  render(<TravelProgress travel={travel} />);
  expect(screen.getByText("Starting destination scene")).toBeTruthy();
  const button = screen.getByRole("button", { name: "Cancel teleport" });
  fireEvent.click(button);
  fireEvent.click(button);
  expect(bridge.sentOf("CancelTravel")).toEqual([{ expectedSession: "engine-a", requestId: "travel-a" }]);
  expect(screen.getByRole("button", { name: "Cancelling\u2026" })).toBeDisabled();
  act(() => bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ commandResults: [{ requestId: "travel-a", action: "cancelTravel", accepted: false, error: "Destination is already committed" }] }) }));
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Destination is already committed"));
});

it("hides cancellation when the engine has committed the destination", () => {
  render(<TravelProgress travel={{ ...travel, canCancel: false }} />);
  expect(screen.queryByRole("button", { name: "Cancel teleport" })).toBeNull();
});
