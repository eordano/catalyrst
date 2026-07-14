import { describe, it, expect } from "vitest";

import { buildOutfitsMetadata, type OutfitInput } from "./backpack";

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
    expect(entry.outfit.eyes.color.r).toBeCloseTo(0x3a / 255, 5);
    expect(entry.outfit.eyes.color.g).toBeCloseTo(0x6e / 255, 5);
    expect(entry.outfit.eyes.color.b).toBeCloseTo(0xa5 / 255, 5);
  });

  it("drops entries without a bodyShape or with out-of-range slots", () => {
    const input: OutfitInput[] = [
      { slot: 0, bodyShape: BODY, wearables: [] },
      { slot: 1, wearables: [] },
      { slot: 9, bodyShape: BODY, wearables: [] },
      { slot: -1, bodyShape: BODY, wearables: [] },
    ];
    const md = buildOutfitsMetadata(input);
    expect(md.outfits.map((o) => o.slot)).toEqual([0]);
  });

  it("keeps the first entry when a slot is duplicated", () => {
    const md = buildOutfitsMetadata([
      { slot: 2, bodyShape: BODY, wearables: ["a"] },
      { slot: 2, bodyShape: BODY, wearables: ["b"] },
    ]);
    expect(md.outfits).toHaveLength(1);
    expect(md.outfits[0]!.outfit.wearables).toEqual(["a"]);
  });

  it("tolerates empty / nullish input", () => {
    expect(buildOutfitsMetadata([]).outfits).toEqual([]);
    // @ts-expect-error exercising the runtime nullish guard
    expect(buildOutfitsMetadata(undefined).outfits).toEqual([]);
  });
});
