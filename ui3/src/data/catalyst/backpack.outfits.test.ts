import { describe, it, expect } from "vitest";

import { buildOutfitsMetadata, hasOutfitsEntity, loadOutfits, type OutfitInput } from "./backpack";

const BODY = "urn:decentraland:off-chain:base-avatars:BaseMale";

describe("buildOutfitsMetadata", () => {
  it("shapes an outfit into the catalyst entity metadata (Color3 + forceRender)", () => {
    const md = buildOutfitsMetadata([
      {
        slot: 0,
        bodyShape: BODY,
        wearables: ["urn:decentraland:off-chain:base-avatars:eyes_00"],
        skinColor: "#ffffff",
        hairColor: "#000000",
        eyeColor: "#3a6ea5",
      },
    ]);

    expect(md.namesForExtraSlots).toEqual([]);
    expect(md.outfits).toHaveLength(1);
    const entry = md.outfits[0]!;
    expect(entry.slot).toBe(0);
    expect(entry.outfit.bodyShape).toBe(BODY);
    expect(entry.outfit.wearables).toEqual([
      "urn:decentraland:off-chain:base-avatars:eyes_00",
    ]);
    expect(entry.outfit.forceRender).toEqual([]);
    expect(entry.outfit.skin.color).toEqual({ r: 1, g: 1, b: 1 });
    expect(entry.outfit.hair.color).toEqual({ r: 0, g: 0, b: 0 });
    expect(entry.outfit.eyes.color.r).toBeCloseTo(0.0423114, 5);
    expect(entry.outfit.eyes.color.g).toBeCloseTo(0.155926, 5);
    expect(entry.outfit.eyes.color.b).toBeCloseTo(0.376262, 5);
  });

  it("drops entries without a bodyShape or out-of-range slots, keeps the first of a duplicated slot, and tolerates nullish input", () => {
    const input: OutfitInput[] = [
      { slot: 0, bodyShape: BODY, wearables: [] },
      { slot: 1, wearables: [] },
      { slot: 9, bodyShape: BODY, wearables: [] },
      { slot: -1, bodyShape: BODY, wearables: [] },
    ];
    expect(buildOutfitsMetadata(input).outfits.map((o) => o.slot)).toEqual([0]);
    const dup = buildOutfitsMetadata([
      { slot: 2, bodyShape: BODY, wearables: ["a"] },
      { slot: 2, bodyShape: BODY, wearables: ["b"] },
    ]);
    expect(dup.outfits).toHaveLength(1);
    expect(dup.outfits[0]!.outfit.wearables).toEqual(["a"]);
    expect(buildOutfitsMetadata([]).outfits).toEqual([]);
    // @ts-expect-error exercising the runtime nullish guard
    expect(buildOutfitsMetadata(undefined).outfits).toEqual([]);
  });
});

const ADDR = "0x92de52247aeae00fcfb18072c8564f3549b64f9c";
const BASE = "https://cat.test";

type Route = (init?: RequestInit) => unknown;

function fakeFetch(routes: Record<string, Route>) {
  const calls: string[] = [];
  const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push(url);
    const route = routes[url];
    if (!route) return new Response("not found", { status: 404 });
    return new Response(JSON.stringify(route(init)), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as typeof fetch;
  return { fetchImpl, calls };
}

const ACTIVE = `${BASE}/content/entities/active`;

const ENTITY = {
  id: "bafyoutfits",
  pointers: [`${ADDR}:outfits`],
  metadata: {
    outfits: [
      {
        slot: 1,
        outfit: {
          bodyShape: BODY,
          wearables: ["urn:decentraland:off-chain:base-avatars:eyes_00"],
          skin: { color: { r: 1, g: 1, b: 1 } },
          hair: { color: { r: 0, g: 0, b: 0 } },
          eyes: { color: { r: 0, g: 0, b: 1 } },
        },
      },
    ],
  },
};

describe("loadOutfits", () => {
  it("loads saved outfits in one request and distinguishes missing entities from failures", async () => {
    const full = fakeFetch({
      [ACTIVE]: (init) => {
        expect(JSON.parse(String(init?.body))).toEqual({ pointers: [`${ADDR}:outfits`] });
        return [ENTITY];
      },
    });
    const outfits = await loadOutfits(ADDR, { base: BASE, fetchImpl: full.fetchImpl });
    expect(full.calls).toEqual([ACTIVE]);
    expect(outfits).toEqual([
      {
        slot: 1,
        bodyShape: BODY,
        wearables: ["urn:decentraland:off-chain:base-avatars:eyes_00"],
        skinColor: "#ffffff",
        hairColor: "#000000",
        eyeColor: "#0000ff",
      },
    ]);

    const noEntity = fakeFetch({ [ACTIVE]: () => [] });
    expect(await loadOutfits(ADDR, { base: BASE, fetchImpl: noEntity.fetchImpl })).toEqual([]);
    expect(noEntity.calls).toEqual([ACTIVE]);

    const emptyEntity = fakeFetch({ [ACTIVE]: () => [{ ...ENTITY, metadata: { outfits: [] } }] });
    expect(await loadOutfits(ADDR, { base: BASE, fetchImpl: emptyEntity.fetchImpl })).toEqual([]);
    expect(emptyEntity.calls).toEqual([ACTIVE]);

    const failed = fakeFetch({});
    await expect(hasOutfitsEntity(ADDR, { base: BASE, fetchImpl: failed.fetchImpl })).rejects.toThrow("Could not load saved outfits");
    await expect(loadOutfits(ADDR, { base: BASE, fetchImpl: failed.fetchImpl })).rejects.toThrow("Could not load saved outfits");
    expect(failed.calls).toEqual([ACTIVE, ACTIVE]);
    const malformed = fakeFetch({ [ACTIVE]: () => ({ error: "unavailable" }) });
    await expect(loadOutfits(ADDR, { base: BASE, fetchImpl: malformed.fetchImpl })).rejects.toThrow("unexpected response");
  });
});
