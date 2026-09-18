import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const dialog = () => screen.getByRole("alertdialog", { name: "Scene permission request" });

describe("permission dialog", () => {
  test("shows the scene name and the per-type request clause, falling back to the raw scene id", () => {
    const { bridge } = renderHud();
    bridge.pushPermissionRequest({
      id: 1,
      sceneName: "Genesis Plaza",
      additional: "Jump to DCL Kickoff Challenge?",
    });
    expect(within(dialog()).getByText("Genesis Plaza")).toBeInTheDocument();
    expect(within(dialog()).getByText(/move you to a new realm/)).toBeInTheDocument();
    expect(within(dialog()).getByText("Jump to DCL Kickoff Challenge?")).toBeInTheDocument();

    bridge.pushPermissionWithdrawn({ id: 1 });
    bridge.pushPermissionRequest({ id: 3, sceneName: "", scene: "bafkfallback" });
    expect(within(dialog()).getByText("bafkfallback")).toBeInTheDocument();
  });

  test("Allow resolves once, Deny carries the picked scope, Escape denies once ignoring the scope; queued requests follow in order without double-queuing", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 7, sceneName: "First Scene" });
    bridge.pushPermissionRequest({ id: 7, sceneName: "First Scene" });
    bridge.pushPermissionRequest({ id: 8, sceneName: "Second Scene" });
    bridge.pushPermissionRequest({ id: 9, sceneName: "Third Scene" });
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    expect(within(dialog()).getByText("First Scene")).toBeInTheDocument();

    await user.click(within(dialog()).getByRole("button", { name: "Allow" }));
    bridge.expectSent("ResolvePermission", { id: 7, allow: true, level: "once" });
    expect(within(dialog()).getByText("Second Scene")).toBeInTheDocument();

    await user.click(within(dialog()).getByRole("radio", { name: "Always for Realm" }));
    await user.click(within(dialog()).getByRole("button", { name: "Deny" }));
    bridge.expectSent("ResolvePermission", { id: 8, allow: false, level: "realm" });
    expect(within(dialog()).getByText("Third Scene")).toBeInTheDocument();

    await user.click(within(dialog()).getByRole("radio", { name: "Always for Global" }));
    await user.keyboard("{Escape}");
    bridge.expectSent("ResolvePermission", { id: 9, allow: false, level: "once" });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(bridge.sentOf("ResolvePermission")).toHaveLength(3);
  });

  test("withdrawing the shown request closes it unresolved and advances; withdrawing a queued one skips it later; unknown ids change nothing", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 1, sceneName: "First Scene" });
    bridge.pushPermissionRequest({ id: 2, sceneName: "Second Scene" });
    bridge.pushPermissionRequest({ id: 3, sceneName: "Third Scene" });
    bridge.pushPermissionRequest({ id: 4, sceneName: "Fourth Scene" });

    bridge.pushPermissionWithdrawn({ id: 99 });
    expect(within(dialog()).getByText("First Scene")).toBeInTheDocument();

    bridge.pushPermissionWithdrawn({ id: 1 });
    expect(within(dialog()).getByText("Second Scene")).toBeInTheDocument();
    expect(screen.queryByText("First Scene")).toBeNull();
    bridge.expectNotSent("ResolvePermission");

    bridge.pushPermissionWithdrawn({ id: 3 });
    await user.click(within(dialog()).getByRole("button", { name: "Allow" }));
    expect(within(dialog()).getByText("Fourth Scene")).toBeInTheDocument();
    bridge.expectNotSent("ResolvePermission", { id: 3 });

    bridge.pushPermissionWithdrawn({ id: 4 });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(bridge.sentOf("ResolvePermission")).toHaveLength(1);
  });

  test("the engine's withdrawal echo after the user's own answer is a no-op", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 7 });
    await user.click(within(dialog()).getByRole("button", { name: "Allow" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();

    bridge.pushPermissionWithdrawn({ id: 7 });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(bridge.sentOf("ResolvePermission")).toHaveLength(1);
  });
});
