import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, screen } from "@testing-library/react";

import { destinationFromSearch, primeBootPosition } from "../app/BootGate";
import { PlaceSchema } from "../data/catalyst/placesSchema";
import { normalizePlace, toPlaceView } from "../data/catalyst/places";
import { qk } from "../data/queryKeys";
import { renderBoot } from "./harness";
import type { BootHarness } from "./harness";

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

const PICKER_PARAMS = { limit: 48, order_by: "most_active", order: "desc" };
const PICKER_TITLE = "Where do you want to go?";

function jumpInFromLobby(): void {
  fireEvent.click(screen.getByRole("checkbox"));
  const jump = screen.getByText("Continue as guest");
  fireEvent.click(jump.closest("button") ?? jump);
}

function openPicker(harness: BootHarness): void {
  harness.queryClient.setQueryData(qk.places(PICKER_PARAMS), [parcelPlace, worldPlace]);
  jumpInFromLobby();
}

function pickCard(title: string): void {
  fireEvent.click(screen.getByText(title));
}

beforeEach(() => {
  window.dclEngineReady = true;
  window.dclEngineStart = vi.fn();
});
afterEach(() => {
  delete window.dclEngineReady;
  delete window.dclEngineStart;
  document.getElementById("position")?.remove();
  window.history.replaceState({}, "", "/");
});

function installPositionInput(): HTMLInputElement {
  const input = document.createElement("input");
  input.id = "position";
  document.body.appendChild(input);
  return input;
}

describe("destination picker jump", () => {
  test("without a deep link the picker shows cached place cards (no entity fetches); a parcel pick primes the engine boot position and skips the Teleport", () => {
    const input = installPositionInput();
    const harness = renderBoot();
    openPicker(harness);
    expect(screen.getByText(PICKER_TITLE)).toBeInTheDocument();
    expect(screen.getByText("Plaza Party")).toBeInTheDocument();
    expect(screen.getByText("Kickoff World")).toBeInTheDocument();
    expect(fetch).not.toHaveBeenCalledWith(expect.stringContaining("entities/active"), expect.anything());

    pickCard("Plaza Party");
    expect(input.value).toBe("10,-20");
    expect(window.dclEngineStart).toHaveBeenCalledTimes(1);

    harness.bridge.pushIdentity({ isGuest: true, name: "guest" });
    harness.bridge.expectNotSent("Teleport");
    harness.bridge.expectNotSent("ChangeRealm");
  });

  test("without the host position input the parcel pick falls back to Teleport", () => {
    const harness = renderBoot();
    openPicker(harness);
    pickCard("Plaza Party");

    expect(window.dclEngineStart).toHaveBeenCalledTimes(1);
    harness.bridge.expectNotSent("Teleport");

    harness.bridge.pushIdentity({ isGuest: true, name: "guest" });
    harness.bridge.expectSent("Teleport", { x: 10 * 16 + 8, z: -20 * 16 + 8 });
  });

  test("a world pick keeps the deferred ChangeRealm and never touches the position input", () => {
    const input = installPositionInput();
    const harness = renderBoot();
    openPicker(harness);
    pickCard("Kickoff World");

    expect(input.value).toBe("");
    expect(window.dclEngineStart).toHaveBeenCalledTimes(1);

    harness.bridge.pushIdentity({ isGuest: true, name: "guest" });
    harness.bridge.expectSent("ChangeRealm", { realm: "kickoff.dcl.eth" });
    harness.bridge.expectNotSent("Teleport");
  });
});

describe("deep-linked destination", () => {
  test("?realm= skips the picker and lands the ChangeRealm", () => {
    window.history.replaceState({}, "", "/play/?realm=flagtag.dcl.eth");
    const harness = renderBoot();
    jumpInFromLobby();

    expect(screen.queryByText(PICKER_TITLE)).not.toBeInTheDocument();
    expect(window.dclEngineStart).toHaveBeenCalledTimes(1);

    harness.bridge.pushIdentity({ isGuest: true, name: "guest" });
    harness.bridge.expectSent("ChangeRealm", { realm: "flagtag.dcl.eth" });
    harness.bridge.expectNotSent("Teleport");
  });

  test("?position= skips the picker and primes the engine boot position", () => {
    window.history.replaceState({}, "", "/play/?position=10,-20");
    const input = installPositionInput();
    const harness = renderBoot();
    jumpInFromLobby();

    expect(screen.queryByText(PICKER_TITLE)).not.toBeInTheDocument();
    expect(input.value).toBe("10,-20");
    expect(window.dclEngineStart).toHaveBeenCalledTimes(1);

    harness.bridge.pushIdentity({ isGuest: true, name: "guest" });
    harness.bridge.expectNotSent("ChangeRealm");
    harness.bridge.expectNotSent("Teleport");
  });
});

describe("destinationFromSearch", () => {
  test("reads realm (decoded, resolving a single leading slash against this origin) and position, realm winning; rejects blank and malformed values", () => {
    const cases: [string, ReturnType<typeof destinationFromSearch>][] = [
      ["?realm=%2F_project", { kind: "world", realm: `${window.location.origin}/_project` }],
      ["?realm=%2F%2Fevil.example%2Fx", { kind: "world", realm: "//evil.example/x" }],
      ["?realm=flagtag.dcl.eth", { kind: "world", realm: "flagtag.dcl.eth" }],
      ["?position=-29,55", { kind: "parcel", x: -29, y: 55 }],
      ["?position=1,2&realm=a.dcl.eth", { kind: "world", realm: "a.dcl.eth" }],
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

describe("primeBootPosition", () => {
  test("only a parcel destination writes x,y into the host input and reports success", () => {
    const input = installPositionInput();
    expect(primeBootPosition(null)).toBe(false);
    expect(primeBootPosition({ kind: "world", realm: "a.dcl.eth" })).toBe(false);
    expect(input.value).toBe("");
    expect(primeBootPosition({ kind: "parcel", x: -29, y: 55 })).toBe(true);
    expect(input.value).toBe("-29,55");
  });
});
