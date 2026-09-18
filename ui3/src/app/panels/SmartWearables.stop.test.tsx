import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";

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
  delete window.engine_console_command;
  await new Promise((r) => setTimeout(r, 0));
});

describe("portables stop pending state", () => {
  test("activates only a valid source and reports engine lookup failures", async () => {
    const { user } = setup();
    const command = vi.fn().mockRejectedValue(new Error("World does not exist"));
    window.engine_console_command = command;
    await user.type(screen.getByLabelText("Activate an experience"), "demo.dcl.eth");
    await user.click(screen.getByRole("button", { name: "Activate" }));
    expect(command).toHaveBeenCalledWith("/spawn demo.dcl.eth");
    expect(await screen.findByRole("status")).toHaveTextContent("World does not exist");
    command.mockResolvedValue("");
    await user.click(screen.getByRole("button", { name: "Activate" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Experience requested");
    expect(screen.getByLabelText("Activate an experience")).toHaveValue("");
  });
  test("Stop goes pending per row until the next portables push reconciles, and a survivor gets its Stop back", async () => {
    const { bridge, user } = setup();
    act(() => {
      bridge.push({ kind: "portables", portables: [JETPACK, RADAR] });
    });

    await user.click(row("Jetpack").getByRole("button", { name: "Stop" }));
    expect(bridge.expectSent("KillPortable")).toEqual({ pid: JETPACK.pid });
    expect(row("Jetpack").getByRole("button", { name: "Stopping\u{2026}" })).toBeDisabled();
    expect(row("Radar").getByRole("button", { name: "Stop" })).toBeEnabled();

    act(() => {
      bridge.push({ kind: "portables", portables: [JETPACK, RADAR] });
    });
    expect(row("Jetpack").getByRole("button", { name: "Stop" })).toBeEnabled();

    await user.click(row("Jetpack").getByRole("button", { name: "Stop" }));
    expect(row("Jetpack").getByRole("button", { name: "Stopping\u{2026}" })).toBeDisabled();
    act(() => {
      bridge.push({ kind: "portables", portables: [RADAR] });
    });
    expect(screen.queryByText("Jetpack")).toBeNull();
    expect(row("Radar").getByRole("button", { name: "Stop" })).toBeEnabled();
  });
});
