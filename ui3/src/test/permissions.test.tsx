import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const dialog = () => screen.getByRole("alertdialog", { name: "Scene permission request" });

describe("permission dialog", () => {
  test("shows the scene name and the per-type request clause", () => {
    const { bridge } = renderHud();
    bridge.pushPermissionRequest({
      sceneName: "Genesis Plaza",
      additional: "Jump to DCL Kickoff Challenge?",
    });

    expect(within(dialog()).getByText("Genesis Plaza")).toBeInTheDocument();
    expect(within(dialog()).getByText(/move you to a new realm/)).toBeInTheDocument();
    expect(within(dialog()).getByText("Jump to DCL Kickoff Challenge?")).toBeInTheDocument();
  });

  test("a re-sent request with the same id does not double-queue", () => {
    const { bridge } = renderHud();
    bridge.pushPermissionRequest({ id: 9 });
    bridge.pushPermissionRequest({ id: 9 });
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
  });

  test("Allow resolves with the default once scope", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 7 });
    await user.click(within(dialog()).getByRole("button", { name: "Allow" }));
    bridge.expectSent("ResolvePermission", { id: 7, allow: true, level: "once" });
  });

  test("Deny carries the selected persistence scope", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 7 });
    await user.click(within(dialog()).getByRole("radio", { name: "Always for Realm" }));
    await user.click(within(dialog()).getByRole("button", { name: "Deny" }));
    bridge.expectSent("ResolvePermission", { id: 7, allow: false, level: "realm" });
  });

  test("Escape dismisses as a one-time deny, ignoring any scope already picked", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 7 });
    await user.click(within(dialog()).getByRole("radio", { name: "Always for Global" }));
    await user.keyboard("{Escape}");
    bridge.expectSent("ResolvePermission", { id: 7, allow: false, level: "once" });
  });

  test("resolving the current request reveals the next queued one", async () => {
    const { bridge, user } = renderHud();
    bridge.pushPermissionRequest({ id: 1, sceneName: "First Scene" });
    bridge.pushPermissionRequest({ id: 2, sceneName: "Second Scene" });
    expect(within(dialog()).getByText("First Scene")).toBeInTheDocument();

    await user.click(within(dialog()).getByRole("button", { name: "Allow" }));
    expect(within(dialog()).getByText("Second Scene")).toBeInTheDocument();
  });

  test("a missing scene name falls back to the raw scene id, then to a generic label", () => {
    const { bridge } = renderHud();
    bridge.pushPermissionRequest({ id: 3, sceneName: "", scene: "bafkfallback" });
    expect(within(dialog()).getByText("bafkfallback")).toBeInTheDocument();
  });
});
