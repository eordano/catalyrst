import { describe, expect, test } from "vitest";

import type { OverlayPush } from "../../generated/bridge/OverlayPush";
import { adaptBridgeFriends, normalizeFriends } from "./useFriends";

type FriendsPush = Extract<OverlayPush, { kind: "friends" }>;

function friendsPush(over: Partial<FriendsPush> = {}): FriendsPush {
  return {
    kind: "friends",
    onlineCount: 1,
    friends: [
      {
        address: "0xabc0000000000000000000000000000000000001",
        name: "Ada",
        hasClaimedName: true,
        profilePictureUrl: "https://peer.example/face.png",
        status: "online",
      },
    ],
    received: [],
    sent: [],
    ...over,
  };
}

describe("adaptBridgeFriends", () => {
  test("the blocked list holds only blockedByMe addresses, with the full friend shape when known and address-only otherwise", () => {
    const stranger = "0xdef0000000000000000000000000000000000002";
    const data = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: [stranger], blockedByMe: [stranger] })),
    );
    expect(data.blocked).toHaveLength(1);
    expect(data.blocked[0]).toMatchObject({ address: stranger, tag: "#0002", name: "unknown", profilePictureUrl: "" });
    expect(data.friends).toHaveLength(1);

    const address = "0xABC0000000000000000000000000000000000001";
    const known = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: [address], blockedByMe: [address] })),
    );
    expect(known.blocked).toHaveLength(1);
    expect(known.blocked[0]).toMatchObject({
      address,
      name: "Ada",
      hasClaimedName: true,
      profilePictureUrl: "https://peer.example/face.png",
    });

    const byThemOnly = "0xdef0000000000000000000000000000000000003";
    expect(
      normalizeFriends(adaptBridgeFriends(friendsPush({ blocked: [byThemOnly], blockedByMe: [] }))).blocked,
    ).toEqual([]);
  });

  test("a push without the optional blocked fields yields an empty blocked list and a null push adapts to null", () => {
    expect(normalizeFriends(adaptBridgeFriends(friendsPush())).blocked).toEqual([]);
    expect(adaptBridgeFriends(null)).toBeNull();
  });
});
