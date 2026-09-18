import { useEffect } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { renderHud as renderExplorer, makeFriend } from "./harness";
const LOBBY_STORAGE = "dcl.feature.2026-09-lobby";
const renderHud = (options: Parameters<typeof renderExplorer>[0] = {}) => {
  const harness = renderExplorer({ ...options, initialView: "lobby" });
  harness.bridge.push({ kind: "avatar", bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale", wearables: [], emotes: [] });
  return harness;
};
import { SIDEBAR_DESIGN_STORAGE } from "../data/sidebarDesignFlag";
import { setMobileOverride } from "../explorer/mobile/detect";
import { pushRecent } from "../data/recentPlaces";
import { normalizePlace, toPlaceView } from "../data/catalyst/places";
import { PlaceSchema } from "../data/catalyst/placesSchema";
import { IDENTITY_STORAGE_KEY } from "../data/auth/engineLogin";

vi.mock("../wearable-preview/WearablePreview", () => ({ default: ({ onStatus }: { onStatus?: (status: "ready") => void }) => { useEffect(() => onStatus?.("ready"), [onStatus]); return <div data-testid="avatar-preview" />; } }));
beforeEach(() => { localStorage.setItem(LOBBY_STORAGE, "1"); vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} }); });
afterEach(() => { vi.unstubAllGlobals(); localStorage.removeItem(LOBBY_STORAGE); localStorage.removeItem(SIDEBAR_DESIGN_STORAGE); localStorage.removeItem("dcl.recentPlaces"); history.replaceState(null, "", "/"); setMobileOverride(null); });

test("Escape shows the current scene, coordinates and a fresh screenshot; Resume returns without travel", async () => {
  const { bridge, user } = renderExplorer();
  bridge.pushScene({ title: "Cyberpunk Disparity", coords: "10,-20" });
  await user.keyboard("{Escape}");
  expect(await screen.findByRole("heading", { name: "You are in Cyberpunk Disparity \u00b7 10,-20" })).toBeInTheDocument();
  expect(screen.queryByText(/^Welcome /)).toBeNull();
  bridge.expectSent("CapturePhoto");
  bridge.push({ kind: "photo", dataUrl: "data:image/png;base64,c2NlbmU=" });
  expect(screen.getByRole("img", { name: "Current view of Cyberpunk Disparity" })).toHaveAttribute("src", "data:image/png;base64,c2NlbmU=");
  await user.click(screen.getByRole("button", { name: "Resume Cyberpunk Disparity" }));
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  bridge.expectNotSent("Travel");
  bridge.expectNotSent("Teleport");
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
});

test("the live scene canvas moves into the Resume card and returns intact on resume", async () => {
  const host = document.createElement("div");
  const canvas = document.createElement("canvas");
  canvas.id = "mygame-canvas";
  canvas.tabIndex = 0;
  host.appendChild(canvas);
  document.body.appendChild(host);
  try {
    const { bridge, user } = renderExplorer();
    bridge.pushScene({ title: "Live Plaza", coords: "0,0" });
    await user.keyboard("{Escape}");
    const live = await screen.findByRole("img", { name: "Live view of Live Plaza" });
    expect(live).toContainElement(canvas);
    expect(canvas.tabIndex).toBe(-1);
    bridge.expectNotSent("CapturePhoto");
    await user.click(screen.getByRole("button", { name: "Resume Live Plaza" }));
    expect(host).toContainElement(canvas);
    expect(canvas.tabIndex).toBe(0);
    bridge.expectNotSent("Travel");
  } finally { host.remove(); }
});

test("flagged lobby is the default view and locks world input until Enter", async () => {
  const { bridge, user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  bridge.expectSent("SetExplorerUiOpen", { ui: "lobby" });
  expect(screen.queryByRole("heading", { name: /^Friends/ })).toBeNull();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
  expect(screen.queryByRole("button", { name: "Lobby" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Open lobby" }));
  expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});

test("customization opens Backpack and Escape returns to the lobby", async () => {
  const { user, path } = renderHud();
  (await screen.findByRole("button", { name: "Edit avatar" })).focus();
  await user.keyboard("{Enter}");
  expect(path()).toBe("/backpack");
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  await user.keyboard("{Escape}");
  expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});

test("Escape exits the flagged lobby even while searching and releases world input", async () => {
  history.replaceState(null, "", "/?2026-09-lobby=1&position=-101%2C102");
  const { user, bridge } = renderHud();
  await user.click(await screen.findByRole("button", { name: "Search places and worlds" }));
  await user.type(await screen.findByRole("searchbox"), "Genesis");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
  expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Open lobby" }));
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});

test("keyboard activation in lobby is not hijacked by world chat shortcuts", async () => {
  const { user, path } = renderHud();
  (await screen.findByRole("button", { name: "Edit avatar" })).focus();
  await user.keyboard("{Enter}");
  expect(path()).toBe("/backpack");
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
});

test("notifications stay above the lobby and Escape closes only the popup", async () => {
  const { user, path, bridge } = renderHud();
  await user.click(await screen.findByRole("button", { name: /^Notifications/ }));
  expect(path()).toBe("/");
  expect(screen.getByRole("dialog", { name: "Notifications" })).toBeInTheDocument();
  bridge.expectSent("SetExplorerUiOpen", { ui: "lobby" });
  await user.keyboard("{Escape}");
  expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
});

test("unknown friend locations disable Join; known locations dispatch a real parcel teleport", async () => {
  const { bridge, user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  const friend = makeFriend();
  bridge.pushFriends({ friends: [friend] });
  expect(screen.getByRole("button", { name: "Join Ripley" })).toBeDisabled();
  bridge.push({ kind: "players", players: [{ address: friend.address, name: friend.name, coords: "10,-20", wearables: [] }] });
  await user.click(screen.getByRole("button", { name: "Join Ripley" }));
  bridge.expectSent("Teleport", { x: 168, z: -312 });
  bridge.pushLoading({ ready: false, percent: 20 });
  bridge.pushScene({ coords: "10,-20" });
  bridge.pushLoading({ ready: true, percent: 100 });
  await waitFor(() => expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull());
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
});

test("Genesis places use the realm-aware goto command when visiting from a World", async () => {
  pushRecent(toPlaceView(normalizePlace(PlaceSchema.parse({
    id: "plaza", title: "Plaza", base_position: "10,-20", positions: ["10,-20"],
    categories: [], user_visits: 0, favorites: 0, likes: 0, highlighted: true, world: false,
  }))));
  const { bridge, user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  bridge.pushScene({ realm: "my-world.dcl.eth", coords: "0,0" });
  await user.click(screen.getByRole("button", { name: "Jump in to Plaza" }));
  expect(screen.queryByRole("region", { name: "Place details" })).toBeNull();
  bridge.expectSent("SendChat", { channel: "Nearby", message: "/goto 10,-20" });
});

test("credits use the authenticated engine when browser wallet credentials are absent", async () => {
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
  const { bridge } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  const identity = bridge.pushIdentity();
  await waitFor(() => bridge.expectSent("SignedFetch", (request) =>
    request.method === "GET" && request.url.endsWith(`/credits/wallet/${identity.address}/balance`)));
  const request = bridge.expectSent("SignedFetch", (request) =>
    request.url.endsWith(`/credits/wallet/${identity.address}/balance`));
  bridge.push({ kind: "signedFetchResult", id: request.id, status: 200, body: JSON.stringify({ available: "42" }) });
  expect(await screen.findByRole("button", { name: "42 credits \u2014 open marketplace" })).toBeInTheDocument();
});

test("search accepts text and exposes honest network errors with recovery", async () => {
  const { user } = renderHud();
  await user.click(await screen.findByRole("button", { name: "Search places and worlds" }));
  await user.type(await screen.findByRole("searchbox", { name: "Search places and worlds" }), "music");
  expect(screen.getByRole("heading", { name: "Search results" })).toBeInTheDocument();
  expect((await screen.findAllByRole("button", { name: "Retry" })).length).toBeGreaterThan(0);
  await user.click(screen.getByRole("button", { name: "Clear search" }));
  expect(screen.getByRole("button", { name: "Edit avatar" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^Back to scene/ })).toBeNull();
});

test("the lobby supports narrow screens without the touch HUD over it", async () => {
  setMobileOverride("mobile");
  localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1");
  renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  expect(screen.queryByRole("navigation", { name: "Main menu" })).toBeNull();
});

test("plain and destination URLs open the lobby by default without a stored opt-in", async () => {
  localStorage.removeItem(LOBBY_STORAGE);
  history.replaceState(null, "", "/?position=10,-20");
  renderHud({ legacyHud: false });
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  expect(localStorage.getItem(LOBBY_STORAGE)).toBeNull();
});

test("retired opt-outs leave the lobby and desktop sidebar enabled", async () => {
  localStorage.setItem(LOBBY_STORAGE, "0");
  localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "0");
  history.replaceState(null, "", "/?2026-09-lobby=0&2026-09-sidebar-design=0");
  const { user } = renderHud({ legacyHud: false });
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.getByRole("button", { name: "Me" })).toBeInTheDocument();
});

test("the redesigned Me menu can return home to the lobby", async () => {
  localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1");
  const { user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Me" }));
  await user.click(screen.getByRole("button", { name: "Home" }));
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "ME" })).toBeNull();
});

test("keyboard activation of the world profile picture opens the lobby, not chat", async () => {
  const { user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  await user.keyboard("{Escape}");
  screen.getByRole("button", { name: "Open lobby" }).focus();
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  expect(screen.queryByLabelText("Send a message to Nearby chat")).toBeNull();
});

test("the mobile profile picture returns to the lobby", async () => {
  setMobileOverride("mobile");
  const { user } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  await user.keyboard("{Escape}");
  await waitFor(() => expect(document.querySelector(".mtb")).not.toBeNull());
  const topbar = document.querySelector(".mtb") as HTMLElement;
  await user.click(within(topbar).getByRole("button", { name: "Open lobby" }));
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});

test("clicking the central avatar opens the existing Backpack without sending avatar changes", async () => {
  const { user, bridge, path } = renderHud();
  await user.click(await screen.findByRole("button", { name: "Edit avatar" }));
  expect(path()).toBe("/backpack");
  expect(await screen.findByRole("button", { name: /Wearables/, pressed: true })).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Edit avatar" })).toBeNull();
  expect(bridge.sentOf("SetAvatar")).toHaveLength(0);
});

test("SDK realm links enter their existing scene without replacing the destination", async () => {
  history.replaceState(null, "", "/?realm=http%3A%2F%2Flocalhost%3A8000&preview=true");
  const { user, bridge } = renderHud();
  await user.click(await screen.findByRole("button", { name: "Jump in to localhost" }));
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  expect(bridge.sentOf("ChangeRealm")).toHaveLength(0);
  expect(bridge.sentOf("Teleport")).toHaveLength(0);
  expect(bridge.sentOf("SendChat")).toHaveLength(0);
});


test("account actions stay in a lobby popup", async () => {
  const { user, path } = renderHud();
  await user.click(await screen.findByRole("button", { name: "Account menu" }));
  expect(screen.getByRole("dialog", { name: "Your account" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  expect(path()).toBe("/");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});


test("Escape closes sidebar panels first, then toggles between scene and lobby without key-repeat bouncing", async () => {
  localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1");
  const { user, bridge } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "System" }));
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("region", { name: "System" })).toBeNull();
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  await user.keyboard("{Escape}");
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  bridge.expectSent("SetExplorerUiOpen", { ui: "lobby" });
  fireEvent.keyDown(window, { key: "Escape", repeat: true });
  expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
});


test("an event opened from the lobby shows details and entering it returns to the world", async () => {
  const { user, router, bridge, path } = renderHud();
  await screen.findByRole("main", { name: "Decentraland lobby" });
  await act(() => router.navigate("/events", { state: { event: {
    id: "test-event", name: "Cyberpunk Disparity", description: "Meet at the plaza",
    live: true, x: 10, y: -20, world: false,
  } } }));
  expect(await screen.findByText("Meet at the plaza")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /JUMP IN/i }));
  bridge.pushLoading({ ready: false, percent: 10 });
  expect(path()).toBe("/events");
  bridge.pushScene({ coords: "10,-20" });
  bridge.pushLoading({ ready: true, percent: 100 });
  await waitFor(() => expect(path()).toBe("/"));
  expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  bridge.expectSent("SetExplorerUiOpen", { ui: null });
});
