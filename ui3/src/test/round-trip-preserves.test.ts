import { expect, test } from "vitest";

import { RecentPlacesSchema, StoredAuthIdentitySchema } from "../data/persisted-schemas";
import { ThirdwebAuthResultSchema } from "../data/auth/thirdwebSchema";
import { check } from "../validate";

test("round-trip boundaries preserve keys written by a newer build or added upstream", () => {
  const stored = {
    ephemeralIdentity: { address: "0x1", privateKey: "0xpk", publicKey: "0xpub" },
    expiration: new Date(Date.now() + 3600_000).toISOString(),
    authChain: [],
    fieldFromANewerBuild: { nested: true },
  };
  const identity = check(StoredAuthIdentitySchema, stored, "test/persisted") as typeof stored;
  expect(identity.fieldFromANewerBuild).toEqual({ nested: true });

  const recents = [{
    id: "p1", title: "t", description: "d", coords: "0,0", x: 0, y: 0, left: 0, top: 0,
    players: null, live: false, featured: false, rating: 0, favorites: 0, likes: 0,
    visits: 0, parcels: 1, categories: [], creator: "c", world: false, worldName: null,
    updated: "2026-01-01", hue: 0, kind: "place",
    pinnedByNewerBuild: true,
  }];
  const places = check(RecentPlacesSchema, recents, "test/recents") as typeof recents;
  expect(places[0]?.pinnedByNewerBuild).toBe(true);

  const raw = {
    isNewUser: false, token: "t", userId: "u", walletAddress: "0x1", type: "email",
    newUpstreamField: "keep me",
  };
  const external = check(ThirdwebAuthResultSchema, raw, "test/external") as Record<string, unknown>;
  expect(external.newUpstreamField).toBe("keep me");
});
