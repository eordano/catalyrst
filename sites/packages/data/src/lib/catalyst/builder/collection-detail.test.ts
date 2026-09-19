import { describe, expect, it } from "vitest";
import { fetchCollectionItems } from "./collection-detail";

const item = {
  id: "draft", name: "Coat", rarity: "rare", type: "wearable",
  price: "2125000000000000001", contents: { "coat.glb": "model-hash" },
  data: { category: "upper_body", representations: [{ mainFile: "coat.glb", contents: ["coat.glb"] }] },
};
async function read(items: unknown[]) {
  return fetchCollectionItems("collection", { fetchImpl: async () => Response.json({ data: items }) });
}

describe("collection item readiness", () => {
  it("displays exact MANA rather than wei, including free items", async () => {
    const { wearables } = await read([item, { ...item, price: "0" }]);
    expect(wearables.map(i => [i.price, i.status])).toEqual([["2.125000000000000001", "ready"], ["0", "ready"]]);
  });
  it("does not mark a priced draft ready when metadata or resources are missing", async () => {
    const { wearables } = await read([
      { ...item, data: null }, { ...item, data: { category: "upper_body" } },
      { ...item, contents: {} }, { ...item, price: null },
      { ...item, data: { ...item.data, representations: [{ mainFile: "coat.glb", contents: [] }] } },
      { ...item, data: { representations: item.data.representations } },
    ]);
    expect(wearables.map(i => i.status)).toEqual(Array(6).fill("not_ready"));
  });
  it("preserves on-chain review status even when draft resources are unavailable", async () => {
    const { wearables } = await read([
      { ...item, contents: {}, is_published: true },
      { ...item, contents: {}, is_published: true, is_approved: true },
    ]);
    expect(wearables.map(i => i.status)).toEqual(["under_review", "published"]);
  });
});
