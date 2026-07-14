import { describe, expect, test } from "vitest";

import type { FriendEntry } from "../../generated/bridge/FriendEntry";
import type { NearbyPlayer } from "../../generated/bridge/NearbyPlayer";
import { joinFriendPins } from "./useFriendPins";

function player(over: Partial<NearbyPlayer> = {}): NearbyPlayer {
  return {
    address: "0xabc0000000000000000000000000000000000001",
    name: "nearby",
    wearables: [],
    coords: "10,-20",
    ...over,
  };
}

function friend(over: Partial<FriendEntry> = {}): FriendEntry {
  return {
    address: "0xabc0000000000000000000000000000000000001",
    name: "Ada",
    hasClaimedName: true,
    profilePictureUrl: "https://peer.example/face.png",
    status: "online",
    ...over,
  };
}

describe("joinFriendPins", () => {
  test("a friend present in the players push gets a pin at the player coords", () => {
    const pins = joinFriendPins([player()], [friend()], []);
    expect(pins).toHaveLength(1);
    expect(pins[0]).toMatchObject({ name: "Ada", coords: "10,-20", x: 10, y: -20 });
    expect(pins[0]?.left).toBeGreaterThan(0);
    expect(pins[0]?.top).toBeGreaterThan(0);
  });

  test("joins addresses case-insensitively", () => {
    const pins = joinFriendPins(
      [player({ address: "0xABC0000000000000000000000000000000000001" })],
      [friend()],
      [],
    );
    expect(pins).toHaveLength(1);
    expect(pins[0]?.address).toBe("0xABC0000000000000000000000000000000000001");
  });

  test("a friend with no nearby player has no position and no pin", () => {
    const pins = joinFriendPins(
      [player()],
      [friend({ address: "0xdef0000000000000000000000000000000000002" })],
      [],
    );
    expect(pins).toHaveLength(0);
  });

  test("a nearby stranger is not a friend pin", () => {
    const pins = joinFriendPins([player()], [], []);
    expect(pins).toHaveLength(0);
  });

  test("prefers the live player picture, falls back to the friend profile url", () => {
    const withLive = joinFriendPins([player({ picture: "https://peer.example/live.png" })], [friend()], []);
    expect(withLive[0]?.picture).toBe("https://peer.example/live.png");
    const withProfile = joinFriendPins([player()], [friend()], []);
    expect(withProfile[0]?.picture).toBe("https://peer.example/face.png");
    const withNeither = joinFriendPins([player()], [friend({ profilePictureUrl: "" })], []);
    expect(withNeither[0]?.picture).toBeNull();
  });

  test("falls back to the player name when the friend entry has none", () => {
    const pins = joinFriendPins([player()], [friend({ name: "" })], []);
    expect(pins[0]?.name).toBe("nearby");
  });

  test("a blocked friend gets no pin, case-insensitively", () => {
    const pins = joinFriendPins(
      [player()],
      [friend()],
      ["0xABC0000000000000000000000000000000000001"],
    );
    expect(pins).toHaveLength(0);
  });

  test("pins are ordered by address for stable rendering", () => {
    const pins = joinFriendPins(
      [
        player({ address: "0xbbb0000000000000000000000000000000000002", coords: "1,1" }),
        player({ address: "0xaaa0000000000000000000000000000000000001", coords: "2,2" }),
      ],
      [
        friend({ address: "0xbbb0000000000000000000000000000000000002", name: "B" }),
        friend({ address: "0xaaa0000000000000000000000000000000000001", name: "A" }),
      ],
      [],
    );
    expect(pins.map((p) => p.name)).toEqual(["A", "B"]);
  });
});
