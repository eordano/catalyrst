import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test } from "vitest";

import SmartWearablesPanel from "./SmartWearables.route";
import { FakeBridge } from "../../test/fakeBridge";

const JETPACK = { pid: "urn:decentraland:entity:jetpack", name: "Jetpack" };
const RADAR = { pid: "urn:decentraland:entity:radar", name: "Radar" };

function setup() {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  render(<SmartWearablesPanel />);
  return { bridge, user: userEvent.setup() };
}

const row = (name: string) =>
  within(screen.getByText(name).closest("li") as HTMLElement);

afterEach(async () => {
  delete window.dclBridge;
  await new Promise((r) => setTimeout(r, 0));
});

describe("portables stop pending state", () => {
  test("Stop goes pending per row until the next portables push reconciles", async () => {
    const { bridge, user } = setup();
    act(() => {
      bridge.push({ kind: "portables", portables: [JETPACK, RADAR] });
    });

    await user.click(row("Jetpack").getByRole("button", { name: "Stop" }));
    expect(bridge.expectSent("KillPortable")).toEqual({ pid: JETPACK.pid });

    const pendingBtn = row("Jetpack").getByRole("button", { name: "Stopping\u{2026}" });
    expect(pendingBtn).toBeDisabled();
    expect(row("Radar").getByRole("button", { name: "Stop" })).toBeEnabled();

    act(() => {
      bridge.push({ kind: "portables", portables: [RADAR] });
    });
    expect(screen.queryByText("Jetpack")).toBeNull();
    expect(row("Radar").getByRole("button", { name: "Stop" })).toBeEnabled();
  });

  test("a survivor row gets its Stop button back after the push", async () => {
    const { bridge, user } = setup();
    act(() => {
      bridge.push({ kind: "portables", portables: [JETPACK] });
    });

    await user.click(row("Jetpack").getByRole("button", { name: "Stop" }));
    expect(row("Jetpack").getByRole("button", { name: "Stopping\u{2026}" })).toBeDisabled();

    act(() => {
      bridge.push({ kind: "portables", portables: [JETPACK] });
    });
    expect(row("Jetpack").getByRole("button", { name: "Stop" })).toBeEnabled();
  });
});
