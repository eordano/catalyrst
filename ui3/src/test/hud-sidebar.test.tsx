import { describe, test, expect, vi } from "vitest";
import { screen, within } from "@testing-library/react";

import { renderHud } from "./harness";
import { siteUrl } from "../data/site";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const sidebarButton = (name: string) => within(sidebar()).getByRole("button", { name });

const PANELS: { button: string; shown: () => HTMLElement | null }[] = [
  { button: "Voice Chat", shown: () => screen.queryByText("Nearby voice") },
  { button: "Skybox", shown: () => screen.queryByText("Time of day") },
  { button: "Portable Experiences", shown: () => screen.queryByText(/Nothing is running right now/i) },
  { button: "Friends", shown: () => screen.queryByRole("tab", { name: "Friends" }) },
  { button: "Notifications", shown: () => document.querySelector(".ui3-overlay__notifications") },
  { button: "Profile", shown: () => screen.queryByText("VIEW PROFILE") },
  { button: "Emotes", shown: () => screen.queryByRole("button", { name: "Wave" }) },
];

describe("sidebar toggles", () => {
  test("chat renders nothing while closed; click or Enter opens it with the input focused", async () => {
    const { user } = renderHud();
    expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
    expect(screen.queryByRole("button", { name: "Close chat" })).toBeNull();

    await user.click(sidebarButton("Chat"));
    expect(screen.getByRole("button", { name: "Close chat" })).toBeInTheDocument();
    expect(screen.getByLabelText("Send a message to Nearby chat")).toHaveFocus();
    await user.click(sidebarButton("Chat"));
    expect(screen.queryByRole("button", { name: "Close chat" })).toBeNull();
    expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();

    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Close chat" })).toBeInTheDocument();
    expect(screen.getByLabelText("Send a message to Nearby chat")).toHaveFocus();

    await user.click(document.body);
    expect(screen.getByLabelText("Send a message to Nearby chat")).not.toHaveFocus();
    await user.keyboard("{Enter}");
    expect(screen.getByLabelText("Send a message to Nearby chat")).toHaveFocus();
    expect(screen.getByRole("button", { name: "Close chat" })).toBeInTheDocument();
  });

  test("every sidebar button toggles its own panel", async () => {
    const { user } = renderHud();
    for (const { button, shown } of PANELS) {
      expect(shown(), `${button} closed before`).toBeNull();
      await user.click(sidebarButton(button));
      await vi.waitFor(() => expect(shown(), `${button} open`).not.toBeNull());
      await user.click(sidebarButton(button));
      expect(shown(), `${button} closed after`).toBeNull();
    }
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByText(/Magic Sneakers/)).toBeNull();
    await user.click(sidebarButton("Friends"));
    expect(screen.getByRole("tab", { name: "Requests" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Blocked" })).toBeInTheDocument();
  });

  test("left panels are exclusive and Escape closes panels, chat and profile widgets", async () => {
    const { user } = renderHud();
    await user.click(sidebarButton("Voice Chat"));
    expect(screen.getByText("Nearby voice")).toBeInTheDocument();
    await user.click(sidebarButton("Friends"));
    expect(screen.queryByText("Nearby voice")).toBeNull();
    expect(await screen.findByRole("tab", { name: "Friends" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("tab", { name: "Friends" })).toBeNull();

    await user.click(sidebarButton("Chat"));
    expect(screen.getByLabelText("Send a message to Nearby chat")).toBeInTheDocument();
    await user.click(sidebarButton("Profile"));
    expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
    expect(screen.getByText("VIEW PROFILE")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByText(/No messages yet|Connecting to Nearby chat/)).toBeNull();
    expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();
  });

  test("chat replaces every left popup and the popup replaces chat", async () => {
    const { user } = renderHud();
    for (const { button, shown } of PANELS) {
      await user.click(sidebarButton(button));
      await vi.waitFor(() => expect(shown()).not.toBeNull());
      await user.click(sidebarButton("Chat"));
      expect(shown()).toBeNull();
      expect(screen.getByLabelText("Send a message to Nearby chat")).toHaveFocus();
      await user.click(sidebarButton(button));
      expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
      await user.keyboard("{Escape}");
    }
  });

  test("More options changes persisted sidebar size without opening Settings", async () => {
    const { user, path } = renderHud();
    await user.click(sidebarButton("More options"));
    await user.click(screen.getByRole("menuitemcheckbox", { name: "Larger sidebar (150%)" }));
    expect(localStorage.getItem("dcl.sidebar.large")).toBe("1");
    expect(document.documentElement.style.getPropertyValue("--sidebar-width")).toBe("69px");
    expect(screen.queryByRole("menuitemcheckbox", { name: "Auto-hide sidebar" })).toBeNull();
    expect(path()).toBe("/");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    localStorage.removeItem("dcl.sidebar.large");
    localStorage.removeItem("dcl.sidebar.autoHide");
  });

  test("the minimap hides behind left panels, defaults to hidden, and Hide map leaves no restore pin", async () => {
    const shown = renderHud({ minimapShown: true });
    shown.bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();
    await shown.user.click(sidebarButton("Friends"));
    expect(screen.queryByText("Test Plaza")).toBeNull();
    await shown.user.keyboard("{Escape}");
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();

    await shown.user.click(screen.getByRole("button", { name: "Hide map" }));
    expect(screen.queryByText("Test Plaza")).toBeNull();
    expect(localStorage.getItem("dcl.minimap.userHidden")).toBe("1");
    expect(screen.queryByRole("button", { name: "Show map" })).toBeNull();
    expect(document.querySelector(".mm__restore")).toBeNull();
    shown.unmount();

    const hidden = renderHud({ minimapShown: false });
    hidden.bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
    expect(screen.queryByText("Test Plaza")).toBeNull();
    expect(screen.queryByRole("button", { name: "Show map" })).toBeNull();
    expect(document.querySelector(".mm__restore")).toBeNull();
  });

  test("SIGN OUT sends a bridge Logout and closes the profile widget", async () => {
    const { user, bridge } = renderHud();
    bridge.pushIdentity();
    await user.click(sidebarButton("Profile"));
    expect(screen.getByText("VIEW PROFILE")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "SIGN OUT" }));
    bridge.expectSent("Logout", {});
    expect(screen.queryByText("VIEW PROFILE")).toBeNull();
  });
});

describe("sidebar nav", () => {
  test("Events and Help & Support open in-screen pages; help links stay same-origin", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    const { user, path } = renderHud();
    await user.click(sidebarButton("Events"));
    expect(path()).toBe("/events");
    expect(await screen.findByRole("heading", { name: "Events" })).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await user.click(sidebarButton("Help & Support"));
    expect(path()).toBe("/help");
    expect(await screen.findByRole("heading", { name: "Help & Support" })).toBeInTheDocument();
    expect(document.getElementById("xc-page")).toContainElement(screen.getByText("Open chat"));
    expect(screen.getByText("Take a photo")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "What's in the sidebar" })).toBeInTheDocument();
    expect(screen.getByText("Portable Experiences")).toBeInTheDocument();
    const origin = new URL(siteUrl()).origin;
    for (const name of ["Support center", "Documentation", "Shortcuts & chat commands"]) {
      const href = screen.getByRole("link", { name }).getAttribute("href") ?? "";
      expect(new URL(href).origin).toBe(origin);
    }
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });

  test("Marketplace opens inside the explorer shell instead of a new tab", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    const { user, path } = renderHud();
    await user.click(sidebarButton("Marketplace"));
    expect(path()).toBe("/marketplace");
    const frame = await screen.findByTitle("Marketplace");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("src")).toMatch(/\/shop$/);
    expect(document.getElementById("xc-page")).toContainElement(frame);
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });

  test("Places opens directly while the minimap is shown", async () => {
    const { user, path } = renderHud({ minimapShown: true });
    const places = sidebarButton("Places");
    expect(places).not.toHaveAttribute("aria-haspopup");
    await user.hover(places);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Places[Z]");
    await user.click(places);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(path()).toBe("/places");
  });

  test("with the minimap hidden Places shows a menu: Show minimap restores it, Open Places opens the page, Escape and outside clicks close it", async () => {
    const { user, path, bridge } = renderHud({ minimapShown: false });
    bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
    expect(screen.queryByText("Test Plaza")).toBeNull();
    const places = sidebarButton("Places");
    expect(places).toHaveAttribute("aria-haspopup", "menu");
    await user.hover(places);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Places[Z]");

    await user.click(places);
    expect(path()).toBe("/");
    expect(places).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();

    await user.click(places);
    await user.click(sidebarButton("Profile"));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(path()).toBe("/");

    await user.click(places);
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Open Places" }));
    expect(path()).toBe("/places");
    expect(localStorage.getItem("dcl.minimap.userHidden")).toBeNull();
    await user.keyboard("{Escape}");

    await user.click(sidebarButton("Places"));
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Show minimap" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByText("Test Plaza")).toBeInTheDocument();
    expect(localStorage.getItem("dcl.minimap.userHidden")).toBe("0");
    expect(path()).toBe("/");

    await user.click(sidebarButton("Places"));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(path()).toBe("/places");
  });
});
