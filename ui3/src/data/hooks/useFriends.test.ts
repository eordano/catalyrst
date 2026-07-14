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
  test("maps a friends push carrying blockedByMe addresses into the blocked list", () => {
    const blockedByMe = ["0xdef0000000000000000000000000000000000002"];
    const data = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: blockedByMe, blockedByMe })),
    );
    expect(data.blocked).toHaveLength(1);
    expect(data.blocked[0]).toMatchObject({
      address: "0xdef0000000000000000000000000000000000002",
      tag: "#0002",
    });
    expect(data.friends).toHaveLength(1);
  });

  test("a blocked address still present in the friends push keeps its full shape", () => {
    const address = "0xABC0000000000000000000000000000000000001";
    const data = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: [address], blockedByMe: [address] })),
    );
    expect(data.blocked).toHaveLength(1);
    expect(data.blocked[0]).toMatchObject({
      address,
      name: "Ada",
      hasClaimedName: true,
      profilePictureUrl: "https://peer.example/face.png",
    });
  });

  test("a blocked address with no friend entry degrades to address-only", () => {
    const stranger = "0xdef0000000000000000000000000000000000009";
    const data = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: [stranger], blockedByMe: [stranger] })),
    );
    expect(data.blocked[0]).toMatchObject({
      address: stranger,
      name: "unknown",
      profilePictureUrl: "",
    });
  });

  test("a by-them-only address in the union blocked field never reaches the panel list", () => {
    const byThemOnly = "0xdef0000000000000000000000000000000000003";
    const data = normalizeFriends(
      adaptBridgeFriends(friendsPush({ blocked: [byThemOnly], blockedByMe: [] })),
    );
    expect(data.blocked).toEqual([]);
  });

  test("a push without the optional blocked fields yields an empty blocked list", () => {
    const data = normalizeFriends(adaptBridgeFriends(friendsPush()));
    expect(data.blocked).toEqual([]);
  });

  test("a null push adapts to null", () => {
    expect(adaptBridgeFriends(null)).toBeNull();
  });
});
