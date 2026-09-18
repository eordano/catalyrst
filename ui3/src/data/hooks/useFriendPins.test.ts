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
  test("pins only friends who are nearby and not blocked, joined case-insensitively and ordered by address", () => {
    const pins = joinFriendPins([player()], [friend()], []);
    expect(pins).toHaveLength(1);
    expect(pins[0]).toMatchObject({ name: "Ada", coords: "10,-20", x: 10, y: -20 });
    expect(pins[0]?.left).toBeGreaterThan(0);
    expect(pins[0]?.top).toBeGreaterThan(0);

    const upper = "0xABC0000000000000000000000000000000000001";
    expect(joinFriendPins([player({ address: upper })], [friend()], [])[0]?.address).toBe(upper);
    expect(joinFriendPins([player()], [friend({ address: "0xdef0000000000000000000000000000000000002" })], [])).toHaveLength(0);
    expect(joinFriendPins([player()], [], [])).toHaveLength(0);
    expect(joinFriendPins([player()], [friend()], [upper])).toHaveLength(0);

    const ordered = joinFriendPins(
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
    expect(ordered.map((p) => p.name)).toEqual(["A", "B"]);
  });

  test("prefers the live player picture and name, falling back to the friend profile", () => {
    const withLive = joinFriendPins([player({ picture: "https://peer.example/live.png" })], [friend()], []);
    expect(withLive[0]?.picture).toBe("https://peer.example/live.png");
    const withProfile = joinFriendPins([player()], [friend()], []);
    expect(withProfile[0]?.picture).toBe("https://peer.example/face.png");
    const withNeither = joinFriendPins([player()], [friend({ profilePictureUrl: "" })], []);
    expect(withNeither[0]?.picture).toBeNull();
    expect(joinFriendPins([player()], [friend({ name: "" })], [])[0]?.name).toBe("nearby");
  });
});
