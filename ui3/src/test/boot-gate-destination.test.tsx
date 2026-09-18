import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { MemoryRouter } from "react-router";
import AppLayout from "../app/AppLayout";
import { act, fireEvent, screen } from "@testing-library/react";

import { destinationFromSearch } from "../app/BootGate";
import { PlaceSchema } from "../data/catalyst/placesSchema";
import { normalizePlace, toPlaceView } from "../data/catalyst/places";
import { qk } from "../data/queryKeys";
import { renderBoot } from "./harness";
import { lifecycleFixture } from "./lifecycleFixture";
import { IDENTITY_STORAGE_KEY, signOutEngineAuth } from "../data/auth/engineLogin";

const placeDefaults = {
  positions: [],
  categories: [],
  user_visits: 0,
  favorites: 0,
  likes: 0,
  highlighted: false,
  world: false,
};

const parcelPlace = toPlaceView(
  normalizePlace(
    PlaceSchema.parse({
      ...placeDefaults,
      id: "p1",
      title: "Plaza Party",
      base_position: "10,-20",
      positions: ["10,-20"],
      user_count: 9,
    }),
  ),
);

const worldPlace = toPlaceView(
  normalizePlace(
    PlaceSchema.parse({
      ...placeDefaults,
      id: "w1",
      title: "Kickoff World",
      world: true,
      world_name: "kickoff.dcl.eth",
      base_position: "0,0",
    }),
  ),
);

function acceptGuestTerms() {
  fireEvent.click(screen.getByRole("button", { name: "Play as a guest" }));
  fireEvent.click(screen.getByRole("checkbox"));
}

function letsGo() {
  fireEvent.click(screen.getByRole("button", { name: "Let\u2019s go" }));
}

function continueAsGuest() {
  acceptGuestTerms();
  letsGo();
}

beforeEach(() => {
  signOutEngineAuth();
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  window.dclEngineReady = true;
  window.dclEngineStart = vi.fn();
  window.dclEngineConnectLobby = vi.fn();
});
afterEach(() => {
  signOutEngineAuth();
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
  delete window.dclEngineReady;
  delete window.dclEngineStart;
  delete window.dclEngineConnectLobby;
  delete window.__dclNativeHost;
  window.history.replaceState({}, "", "/");
});

describe("one lobby entry path", () => {
  test("a guest selected before engine startup signs in once when the lobby engine becomes alive", () => {
    const { bridge } = renderBoot();
    continueAsGuest();
    bridge.expectNotSent("LoginGuest");
    const snapshot = lifecycleFixture();
    snapshot.readiness.scene = null;
    snapshot.readiness.canExplore = false;
    bridge.push({ kind: "lifecycle", snapshot });
    bridge.expectSent("LoginGuest", {});
    bridge.push({ kind: "lifecycle", snapshot });
    bridge.pushLoading({ ready: false, percent: 0 });
    expect(bridge.sent.filter(command => command.action === "LoginGuest")).toHaveLength(1);
    expect(window.dclEngineStart).not.toHaveBeenCalled();
  });

  test.each([
    ["", undefined],
    ["?realm=kickoff.dcl.eth", { realm: "kickoff.dcl.eth", parcel: undefined }],
    ["?realm=%2F_project&preview=true", { realm: `${window.location.origin}/_project`, parcel: undefined }],
    ["?2026-09-lobby=0", undefined],
  ])("connects without entering a scene until Jump In: %s", (search, destination) => {
    window.history.replaceState({}, "", `/play/${search}`);
    const harness = renderBoot();
    continueAsGuest();
    expect(screen.queryByText("Where do you want to go?")).toBeNull();
    expect(screen.getByText("Enter linked destination")).toBeInTheDocument();
    expect(window.dclEngineConnectLobby).toHaveBeenCalledTimes(1);
    expect(window.dclEngineStart).not.toHaveBeenCalled();
    harness.bridge.pushIdentity();
    expect(window.dclEngineStart).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("Enter linked destination"));
    expect(window.dclEngineStart).toHaveBeenCalledExactlyOnceWith(destination);
    harness.bridge.expectNotSent("Travel");
    harness.bridge.expectNotSent("ChangeRealm");
    harness.bridge.expectNotSent("Teleport");
  });

  test("the native HUD automatically enters its linked position", () => {
    window.__dclNativeHost = { post: vi.fn() };
    window.history.replaceState({}, "", "/?realm=https://sdk.example&position=4,5");
    const harness = renderBoot();
    continueAsGuest();
    expect(window.dclEngineConnectLobby).not.toHaveBeenCalled();
    expect(window.dclEngineStart).toHaveBeenCalledExactlyOnceWith({ realm: "https://sdk.example", parcel: [4, 5] });
    harness.bridge.expectNotSent("Travel");
  });

  test("a failed lobby connection is visible without starting a world", async () => {
    window.dclEngineConnectLobby = vi.fn(() => Promise.reject(new Error("Connection unavailable")));
    renderBoot();
    acceptGuestTerms();
    await act(async () => letsGo());
    expect(screen.getByRole("alert")).toHaveTextContent("Connection unavailable");
    expect(window.dclEngineStart).not.toHaveBeenCalled();
  });

  test("world entry waits for acknowledgement and displays rejection", async () => {
    let reject!: (error: Error) => void;
    window.dclEngineStart = vi.fn(() => new Promise<void>((_, fail) => { reject = fail; }));
    const harness = renderBoot();
    continueAsGuest();
    fireEvent.click(screen.getByText("Enter linked destination"));
    harness.bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    expect(screen.queryByTestId("world-content")).toBeNull();
    await act(async () => reject(new Error("Access denied")));
    expect(screen.getByRole("alert")).toHaveTextContent("Access denied");
    expect(screen.queryByTestId("world-content")).toBeNull();
    harness.bridge.expectNotSent("Travel");
  });
});

describe("lobby destination selection", () => {
  test.each([
    ["Plaza Party", { realm: "", parcel: [10, -20] }],
    ["Kickoff World", { realm: "kickoff.dcl.eth", parcel: undefined }],
  ])("enters %s once from the real lobby", async (title, destination) => {
    const harness = renderBoot({ children: <MemoryRouter><AppLayout /></MemoryRouter> });
    harness.queryClient.setQueryDefaults(qk.places().slice(0, -1), { initialData: [parcelPlace, worldPlace] });
    continueAsGuest();
    expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
    expect(window.dclEngineStart).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: `Jump in to ${title}` }));
    expect(window.dclEngineStart).toHaveBeenCalledExactlyOnceWith(destination);
    harness.bridge.expectNotSent("Travel");
    harness.bridge.expectNotSent("SendChat");
    harness.bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
  });

  test("position links enter after onboarding without a second lobby choice", () => {
    window.history.replaceState({}, "", "/?realm=kickoff.dcl.eth&position=4,5");
    renderBoot({ children: <MemoryRouter><AppLayout /></MemoryRouter> });
    continueAsGuest();
    expect(screen.queryByRole("main", { name: "Decentraland lobby" })).toBeNull();
    expect(window.dclEngineConnectLobby).not.toHaveBeenCalled();
    expect(window.dclEngineStart).toHaveBeenCalledExactlyOnceWith({ realm: "kickoff.dcl.eth", parcel: [4, 5] });
  });
});

describe("destinationFromSearch", () => {
  test("preserves a realm and its optional parcel; resolves origin paths and rejects malformed parcels", () => {
    const cases: [string, ReturnType<typeof destinationFromSearch>][] = [
      ["?realm=%2F_project", { kind: "world", realm: `${window.location.origin}/_project` }],
      ["?realm=%2F%2Fevil.example%2Fx", { kind: "world", realm: "//evil.example/x" }],
      ["?realm=flagtag.dcl.eth", { kind: "world", realm: "flagtag.dcl.eth" }],
      ["?position=-29,55", { kind: "parcel", x: -29, y: 55 }],
      ["?position=1,2&realm=a.dcl.eth", { kind: "world", realm: "a.dcl.eth", parcel: [1, 2] }],
      ["?realm=my%20world.dcl.eth", { kind: "world", realm: "my world.dcl.eth" }],
      ["", null],
      ["?other=1", null],
      ["?realm=", null],
      ["?realm=%20%20", null],
      ["?position=", null],
      ["?position=nope", null],
      ["?position=1", null],
      ["?position=1,2,3", null],
    ];
    expect(cases.map(([search]) => [search, destinationFromSearch(search)])).toEqual(cases);
  });
});
