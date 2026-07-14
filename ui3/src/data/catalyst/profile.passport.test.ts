import { describe, it, expect } from "vitest";

import { AvatarSchema, fetchUserPhotos, mapProfile } from "./profile";
import { baseItemUrn } from "./backpack";

const ADDR = "0xe2b6024873d218b2e83b462d3658d8d7c3f55a18";

const TOKEN_URN =
  "urn:decentraland:matic:collections-v2:0xf16e015c31b9902014e7cbf049872899c5fdbc61:0:23";
const CATALOG_URN =
  "urn:decentraland:matic:collections-v2:0xf16e015c31b9902014e7cbf049872899c5fdbc61:0";

describe("mapProfile name fallback", () => {
  it("shortens a bare address instead of rendering all 40 hex chars", () => {
    const vm = mapProfile(AvatarSchema.parse({}), ADDR);
    expect(vm.name).toBe("0xe2b…5a18");
    expect(vm.tag).toBe("#5a18");
  });

  it("keeps a real profile name untouched", () => {
    const vm = mapProfile(AvatarSchema.parse({ name: "NicoE" }), ADDR);
    expect(vm.name).toBe("NicoE");
  });

  it("yields an empty name when there is no name and no address", () => {
    const vm = mapProfile(AvatarSchema.parse({}), null);
    expect(vm.name).toBe("");
  });
});

describe("equipped URN matching via baseItemUrn", () => {
  it("strips the token suffix from collections-v2 URNs", () => {
    expect(baseItemUrn(TOKEN_URN)).toBe(CATALOG_URN);
  });

  it("resolves a token-suffixed profile URN against a 6-segment catalog key", () => {
    const byUrn = new Map([[baseItemUrn(CATALOG_URN), { urn: CATALOG_URN }]]);
    expect(byUrn.get(baseItemUrn(TOKEN_URN))).toEqual({ urn: CATALOG_URN });
  });

  it("leaves base-avatar URNs untouched", () => {
    const base = "urn:decentraland:off-chain:base-avatars:dcl_watch";
    expect(baseItemUrn(base)).toBe(base);
  });
});

describe("fetchUserPhotos", () => {
  it("requests the camera-reel /api/users/... path", async () => {
    const seen: string[] = [];
    const fetchImpl = (async (url: RequestInfo | URL) => {
      seen.push(String(url));
      return new Response(JSON.stringify({ images: [] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as typeof fetch;

    const images = await fetchUserPhotos(ADDR.toUpperCase(), {
      base: "https://catalyst.example",
      fetchImpl,
    });

    expect(images).toEqual([]);
    expect(seen).toEqual([
      `https://catalyst.example/api/users/${ADDR}/images`,
    ]);
  });
});
