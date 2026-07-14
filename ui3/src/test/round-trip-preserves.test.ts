import { describe, expect, test } from "vitest";

import { RecentPlacesSchema, StoredAuthIdentitySchema } from "../data/persisted-schemas";
import { ThirdwebAuthResultSchema } from "../data/auth/thirdwebSchema";
import { check } from "../validate";

// The bug this pins: `check` returns result.data, and a plain z.object DELETES
// keys the shape does not mention. Persisted values are read, modified and
// written back, so a stripping parse erases a newer build's field permanently
// the first time an older build touches the store. External responses are
// forwarded onward, so the same loss applies to anything upstream adds.
//
// Asserting on a field no schema mentions is the whole point -- if these
// schemas are ever tightened back to z.object, every case here fails.
describe("round-trip boundaries preserve unknown keys", () => {
  test("persisted identity keeps a field written by a newer build", () => {
    const stored = {
      ephemeralIdentity: { address: "0x1", privateKey: "0xpk", publicKey: "0xpub" },
      expiration: new Date(Date.now() + 3600_000).toISOString(),
      authChain: [],
      fieldFromANewerBuild: { nested: true },
    };
    const out = check(StoredAuthIdentitySchema, stored, "test/persisted") as typeof stored;
    expect(out.fieldFromANewerBuild).toEqual({ nested: true });
  });

  test("recent places keeps unknown per-entry fields", () => {
    const value = [{
      id: "p1", title: "t", description: "d", coords: "0,0", x: 0, y: 0, left: 0, top: 0,
      players: null, live: false, featured: false, rating: 0, favorites: 0, likes: 0,
      visits: 0, parcels: 1, categories: [], creator: "c", world: false, worldName: null,
      updated: "2026-01-01", hue: 0, kind: "place",
      pinnedByNewerBuild: true,
    }];
    const out = check(RecentPlacesSchema, value, "test/recents") as typeof value;
    expect(out[0]?.pinnedByNewerBuild).toBe(true);
  });

  test("an external response keeps a field the upstream added", () => {
    const raw = {
      isNewUser: false, token: "t", userId: "u", walletAddress: "0x1", type: "email",
      newUpstreamField: "keep me",
    };
    const out = check(ThirdwebAuthResultSchema, raw, "test/external") as Record<string, unknown>;
    expect(out.newUpstreamField).toBe("keep me");
  });
});
