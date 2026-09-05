import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });

describe("sidebar toggles", () => {
  test("chat button opens the chat panel; second click collapses it back to the bare bar", async () => {
    const { user } = renderHud();
    // The translucent input bar is always mounted (Explorer 2.0's collapsed state) -- the
    // open panel (header + message list) is what the sidebar button actually toggles.
    expect(screen.getByLabelText("Send a message to Nearby chat")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close chat" })).toBeNull();

    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    expect(screen.getByRole("button", { name: "Close chat" })).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    expect(screen.queryByRole("button", { name: "Close chat" })).toBeNull();
    expect(screen.getByLabelText("Send a message to Nearby chat")).toBeInTheDocument();
  });

  test("Enter opens the chat widget when closed", async () => {
    const { user } = renderHud();
    expect(screen.queryByRole("button", { name: "Close chat" })).toBeNull();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Close chat" })).toBeInTheDocument();
  });

  test("voice chat toggles the NEARBY VOICE panel", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByText("NEARBY VOICE")).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.queryByText("NEARBY VOICE")).toBeNull();
  });

  test("skybox toggles the NIGHT/DAY panel", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Skybox" }));
    expect(screen.getByText("NIGHT/DAY")).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Skybox" }));
    expect(screen.queryByText("NIGHT/DAY")).toBeNull();
  });

  test("portable experiences toggles the honest explainer panel (no fake permission dialog)", async () => {
    const { user } = renderHud();
    await user.click(
      within(sidebar()).getByRole("button", { name: "Portable Experiences" }),
    );
    expect(await screen.findByText("Portable experiences")).toBeInTheDocument();
    expect(screen.getByText(/Nothing is running right now/i)).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByText(/Magic Sneakers/)).toBeNull();

    await user.click(
      within(sidebar()).getByRole("button", { name: "Portable Experiences" }),
    );
    expect(screen.queryByText(/Nothing is running right now/i)).toBeNull();
  });

  test("friends toggles the Friends panel (Friends/Requests/Blocked tabs)", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    expect(await screen.findByRole("tab", { name: "Friends" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Requests" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Blocked" })).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    expect(screen.queryByRole("tab", { name: "Friends" })).toBeNull();
  });

  test("notifications bell toggles the notifications widget", async () => {
    const { user, container } = renderHud();
    await user.click(
      within(sidebar()).getByRole("button", { name: "Notifications" }),
    );
    expect(container.querySelector(".ui3-overlay__notifications")).not.toBeNull();

    await user.click(
      within(sidebar()).getByRole("button", { name: "Notifications" }),
    );
    expect(container.querySelector(".ui3-overlay__notifications")).toBeNull();
  });

  test("profile button toggles the profile widget", async () => {
    const { user } = renderHud();
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();

    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.getByText("VIEW PROFILE")).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();
  });

  test("left panels are exclusive \u{2014} opening one closes the other", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByText("NEARBY VOICE")).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    expect(screen.queryByText("NEARBY VOICE")).toBeNull();
    expect(screen.getByRole("tab", { name: "Friends" })).toBeInTheDocument();
  });

  test("ESC closes an open left panel", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Skybox" }));
    expect(screen.getByText("NIGHT/DAY")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByText("NIGHT/DAY")).toBeNull();
  });

  test("ESC closes the chat and profile widgets", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    // Opening Profile afterwards moves focus off chat's input, collapsing chat's solid
    // "active" chrome (the header) back to idle -- but the message panel (gated on `open`
    // alone) stays up, so check that rather than the header's Close button.
    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.getByText(/No messages yet|Connecting to Nearby chat/)).toBeInTheDocument();
    expect(screen.getByText("VIEW PROFILE")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    // Closing collapses chat back to the persistent bare bar rather than unmounting it.
    expect(screen.queryByText(/No messages yet|Connecting to Nearby chat/)).toBeNull();
    expect(screen.getByLabelText("Send a message to Nearby chat")).toBeInTheDocument();
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();
  });

  test("minimap hides while a left panel is open and returns on close", async () => {
    const { user, bridge } = renderHud();
    bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    expect(screen.queryByText("Test Plaza")).toBeNull();

    await user.keyboard("{Escape}");
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();
  });

  test("minimap defaults to hidden; the restore pin opts it back on", async () => {
    const { user, bridge } = renderHud({ minimapShown: false });
    bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
    expect(screen.queryByText("Test Plaza")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Show map" }));
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();
    expect(localStorage.getItem("dcl.minimap.userHidden")).toBe("0");

    await user.click(screen.getByRole("button", { name: "Hide map" }));
    expect(screen.queryByText("Test Plaza")).toBeNull();
    expect(screen.getByRole("button", { name: "Show map" })).toBeInTheDocument();
  });

  test("SIGN OUT sends a bridge Logout and closes the profile widget", async () => {
    const { user, bridge } = renderHud();
    bridge.pushIdentity();

    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.getByText("VIEW PROFILE")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "SIGN OUT" }));
    bridge.expectSent("Logout", {});
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();
  });

  test("emotes button toggles the emote wheel", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Emotes" }));
    expect(screen.getByRole("button", { name: "Wave" })).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Emotes" }));
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();
  });

});
