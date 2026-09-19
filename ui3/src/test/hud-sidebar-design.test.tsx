import { afterEach, beforeEach, expect, test } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import SidebarDesign from "../explorer/frames/SidebarDesign";
import { makeFriend, renderHud } from "./harness";
import { SIDEBAR_DESIGN_STORAGE } from "../data/sidebarDesignFlag";
import { setMobileOverride } from "../explorer/mobile/detect";

beforeEach(() => localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1"));
afterEach(() => { localStorage.removeItem(SIDEBAR_DESIGN_STORAGE); setMobileOverride(null); });
const dock = (name: string) => within(screen.getByRole("navigation", { name: "Main menu" })).getByRole("button", { name });

test("System exposes settings and support, with Escape returning focus", async () => {
  const { user } = renderHud();
  expect(screen.queryByRole("button", { name: "Portable experiences" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Sidebar options" })).toBeNull();
  await user.click(dock("System"));
  expect(screen.getByRole("region", { name: "System" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Discord" })).toHaveAttribute("href", "https://decentraland.org/discord");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("region", { name: "System" })).toBeNull();
  expect(dock("System")).toHaveFocus();
});

test("Me opens the real backpack, and the flag survives in-app navigation", async () => {
  const { user, path } = renderHud();
  await user.click(dock("Me"));
  await user.click(screen.getByRole("button", { name: "Backpack" }));
  expect(path()).toBe("/backpack");
  await user.keyboard("{Escape}");
  expect(dock("Me")).toBeInTheDocument();
});

test("drawers and existing chat/voice popups are mutually exclusive", async () => {
  const { user } = renderHud();
  await user.click(dock("Me"));
  await user.click(screen.getByRole("button", { name: "Chat" }));
  expect(screen.queryByRole("region", { name: "ME" })).toBeNull();
  expect(screen.getByLabelText("Send a message to Nearby chat")).toBeInTheDocument();
  await user.click(dock("System"));
  expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
  await user.click(dock("Voice Chat"));
  expect(screen.queryByRole("region", { name: "System" })).toBeNull();
  expect(screen.getByText("Nearby voice")).toBeInTheDocument();
});

test("discovery, marketplace and scene location use real destinations", async () => {
  const { user, path, bridge } = renderHud();
  bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
  expect(screen.queryByRole("button", { name: "Open map: Test Plaza" })).toBeNull();
  await user.click(dock("Location"));
  expect(screen.getByText("Test Plaza")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Open map: Test Plaza" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Open full map" }));
  expect(path()).toBe("/map");
  await user.keyboard("{Escape}");
  await user.click(dock("Discover"));
  expect(screen.getByRole("region", { name: "Discover" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Places" }));
  expect(path()).toBe("/places");
  await user.keyboard("{Escape}");
  await user.click(dock("Discover"));
  await user.click(screen.getByRole("button", { name: "Marketplace" }));
  expect(path()).toBe("/marketplace");
});

test("Location chevron controls the minimap and keeps scene details visible", async () => {
  const { user, bridge } = renderHud();
  bridge.pushScene({ title: "Test Plaza", coords: "5,5" });
  await user.click(dock("Location"));
  await user.click(screen.getByRole("button", { name: "Hide minimap" }));
  expect(localStorage.getItem("dcl.minimap.userHidden")).toBe("1");
  expect(screen.getByText("5,5")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Open full map" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Show minimap" }));
  expect(screen.getByRole("button", { name: "Open full map" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  expect(screen.queryByRole("menuitem", { name: "Sidebar display settings" })).toBeNull();
  await user.click(screen.getByRole("menuitem", { name: "Time of day" }));
  expect(screen.getByRole("region", { name: "Time of day" })).toBeInTheDocument();
});

test("Enter activates a focused dock control instead of hijacking it for chat", async () => {
  const { user } = renderHud();
  dock("Me").focus();
  await user.keyboard("{Enter}");
  expect(screen.getByRole("region", { name: "ME" })).toBeInTheDocument();
  expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
  expect(screen.getByRole("button", { name: "Backpack" })).toHaveFocus();
});

test("outside clicks dismiss the drawer without opening another panel", async () => {
  const { user } = renderHud();
  await user.click(dock("Me"));
  await user.click(document.body);
  expect(screen.queryByRole("region", { name: "ME" })).toBeNull();
  expect(dock("Me")).toHaveAttribute("aria-expanded", "false");
});

test("menu shortcuts match their labels without replacing world shortcuts", async () => {
  const { user, path } = renderHud();
  await user.click(dock("Me"));
  await user.keyboard("p");
  expect(path()).toBe("/passport");
  await user.keyboard("{Escape}");
  await user.click(dock("Me"));
  await user.keyboard("b");
  expect(path()).toBe("/passport");
  await user.keyboard("{Escape}");
  await user.click(dock("System"));
  await user.keyboard("h");
  expect(path()).toBe("/help");
  await user.keyboard("{Escape}");
  await user.keyboard("p");
  expect(path()).toBe("/settings");
});

test("the desktop redesign leaves the existing touch HUD intact", () => {
  setMobileOverride("mobile");
  renderHud();
  expect(document.querySelector(".sd")).toBeNull();
  expect(document.documentElement).toHaveAttribute("data-mobile-chrome");
});


test("Location owns the scene info and owner actions; Me owns notifications", async () => {
  const { user } = renderHud();
  expect(screen.queryByRole("button", { name: "Scene options" })).toBeNull();
  expect(screen.queryByText("Press Enter to chat")).toBeNull();
  await user.click(dock("Location"));
  expect(screen.getByRole("button", { name: "Open full map" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  expect(screen.getByRole("menuitem", { name: "Send feedback to scene owner" })).toBeInTheDocument();
  expect(screen.getByRole("menuitem", { name: "Send tip to scene owner" })).toBeInTheDocument();
  await user.click(dock("Me"));
  await user.click(screen.getByRole("button", { name: "Notifications" }));
  expect(await screen.findByRole("region", { name: "Notifications" })).toBeInTheDocument();
});

test("System links directly to flags and consolidates support", async () => {
  const { user, router } = renderHud();
  await user.click(dock("System"));
  expect(screen.getByRole("link", { name: "Report a bug / Contact support" })).toHaveAttribute("href", "https://catalyst.example.com/support");
  await user.click(screen.getByRole("button", { name: "Feature Flags" }));
  expect(router.state.location.pathname).toBe("/settings");
  expect(router.state.location.search).toBe("?section=flags");
  expect(await screen.findByLabelText("Connection status", { selector: "select" })).toHaveValue("default");
});

test("connection status is restored by default and can be disabled persistently", async () => {
  const first = renderHud();
  await first.user.click(screen.getByRole("button", { name: "Connection status" }));
  expect(screen.getByRole("tab", { name: "Performance" })).toBeInTheDocument();
  await first.user.keyboard("{Escape}");
  expect(screen.queryByRole("tab", { name: "Performance" })).toBeNull();
  first.unmount();
  localStorage.setItem("dcl.feature.2026-09-connection-status", "0");
  try {
    const second = renderHud();
    expect(screen.queryByRole("button", { name: "Connection status" })).toBeNull();
    await second.user.click(dock("System"));
    expect(screen.queryByRole("button", { name: "Connection & performance" })).toBeNull();
  } finally { localStorage.removeItem("dcl.feature.2026-09-connection-status"); }
});

test("obsolete sidebar display preferences are not offered or applied", async () => {
  localStorage.setItem("dcl.sidebar.large", "1");
  const { user } = renderHud();
  expect(document.querySelector('.sd[data-large="true"]')).toBeNull();
  await user.click(dock("System"));
  expect(screen.queryByRole("button", { name: "Sidebar display settings" })).toBeNull();
  expect(screen.queryByRole("switch", { name: /sidebar/i })).toBeNull();
  localStorage.removeItem("dcl.sidebar.large");
});

test("camera opens without flashing full-screen Explore chrome", async () => {
  const { user } = renderHud();
  await user.click(dock("Camera"));
  expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();
  expect(await screen.findByRole("button", { name: "Take photo" })).toBeInTheDocument();
});


test("Me carries an unread indicator only while notifications are pending", () => {
  const { rerender } = render(<SidebarDesign drawer={null} onDrawerChange={() => {}} unread={3} />);
  expect(within(dock("Me")).getByLabelText("Unread notifications")).toBeInTheDocument();
  rerender(<SidebarDesign drawer={null} onDrawerChange={() => {}} unread={0} />);
  expect(within(dock("Me")).queryByLabelText("Unread notifications")).toBeNull();
});


test("running experiences gain a shortcut without sidebar display settings", async () => {
  const { user, bridge } = renderHud();
  expect(screen.queryByRole("button", { name: "Portable experiences" })).toBeNull();
  bridge.push({ kind: "portables", portables: [{ pid: "test", name: "Test portable" }] });
  await user.click(dock("Portable experiences"));
  expect(await screen.findByLabelText("Activate an experience")).toBeInTheDocument();
});

test("Friends shortcut follows the actual friends list", async () => {
  const { bridge } = renderHud();
  expect(screen.queryByRole("button", { name: "Friends" })).toBeNull();
  bridge.pushFriends({ friends: [makeFriend({ status: "offline" })] });
  expect(dock("Friends")).toBeInTheDocument();
  bridge.pushFriends({ friends: [] });
  expect(screen.queryByRole("button", { name: "Friends" })).toBeNull();
});

test("quick audio changes the same engine settings as the Settings page", async () => {
  const { user, bridge } = renderHud();
  await user.click(dock("Audio"));
  bridge.expectSent("GetSettings", {});
  bridge.push({ kind: "settings", settings: [{ name: "Master Volume", category: "Audio", description: "Master volume", value: 80, default: 100, minValue: 0, maxValue: 100, stepSize: 1, namedVariants: [] }] });
  const slider = screen.getByRole("slider", { name: "Master" });
  fireEvent.change(slider, { target: { value: "79" } });
  bridge.expectSent("SetSetting", { name: "Master Volume", value: 79 });
  expect(screen.getAllByRole("slider")).toHaveLength(5);
});
