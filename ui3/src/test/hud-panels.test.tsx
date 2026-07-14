import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const chrome = () => screen.getByRole("dialog", { name: "Explore" });

describe("fullscreen panels", () => {
  test("sidebar Settings opens /settings inside ExploreChrome", async () => {
    const { user, path } = renderHud();
    expect(path()).toBe("/");
    expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();

    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    expect(path()).toBe("/settings");
    expect(
      within(chrome()).getByRole("button", { name: /Settings/ }),
    ).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("navigation", { name: "Main menu" })).toBeNull();
    expect(
      await screen.findByRole("tablist", { name: "Settings sections" }),
    ).toBeInTheDocument();
  });

  test("sidebar Backpack routes to /backpack", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Backpack" }));
    expect(path()).toBe("/backpack");
    expect(chrome()).toBeInTheDocument();
  });

  test("sidebar Places routes to /places", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));
    expect(path()).toBe("/places");
  });

  test("ESC leaves the panel and restores the world HUD", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    expect(path()).toBe("/settings");

    await user.keyboard("{Escape}");
    expect(path()).toBe("/");
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();
  });

  test("the chrome close button returns to the world", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));
    await user.click(screen.getByRole("button", { name: "Back to world" }));
    expect(path()).toBe("/");
  });

  test("chrome tabs switch panels; clicking the active tab exits to world", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));

    await user.click(within(chrome()).getByRole("button", { name: /Places/ }));
    expect(path()).toBe("/places");

    await user.click(within(chrome()).getByRole("button", { name: /Places/ }));
    expect(path()).toBe("/");
  });

  test("hotkeys route to panels (P -> settings, M -> map)", async () => {
    const { user, path } = renderHud();
    await user.keyboard("p");
    expect(path()).toBe("/settings");

    await user.keyboard("m");
    expect(path()).toBe("/map");

    await user.keyboard("m");
    expect(path()).toBe("/");
  });

  test("hotkeys are ignored while typing in the chat input", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    const input = screen.getByLabelText("Send a message to Nearby chat");
    await user.type(input, "m");
    expect(path()).toBe("/");
    expect(input).toHaveValue("m");
  });

  test("unknown routes redirect back to the world HUD", async () => {
    const { path, navigate } = renderHud();
    await navigate("/definitely-not-a-panel");
    expect(path()).toBe("/");
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
  });

  test("entering a fullscreen panel closes any open left panel", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByText("NEARBY VOICE")).toBeInTheDocument();

    await user.keyboard("p");
    await user.keyboard("{Escape}");
    expect(screen.queryByText("NEARBY VOICE")).toBeNull();
  });

  test("the ExploreChrome user chip opens the profile menu, which routes to passport", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));

    const chip = within(chrome()).getByRole("button", { name: /Guest/ });
    await user.click(chip);
    expect(chip).toHaveAttribute("aria-expanded", "true");

    await user.click(screen.getByRole("button", { name: "VIEW PROFILE" }));
    expect(path()).toBe("/passport");
  });
});
