import { describe, expect, it } from "vitest";

import fixture from "../../../fixtures/builder-collections.json";
import {
  BuilderCollectionSchema,
  OrphanItemSchema,
  parseCollections,
  parseOrphanItems,
} from "./collections-schema";
import {
  fetchCollections,
  readSort,
  readTab,
  readView,
  relativeTime,
  sortCollections,
  tabToUiId,
  toCollectionCard,
  toOrphanItem,
} from "./collections";

const COLLECTIONS = (fixture as { collections: unknown[] }).collections;
const ITEMS = (fixture as { items: unknown[] }).items;

describe("schemas / parsers", () => {
  it("accepts the derived fixture rows and tolerates non-arrays without throwing", () => {
    for (const row of COLLECTIONS) {
      expect(BuilderCollectionSchema.safeParse(row).success).toBe(true);
    }
    for (const row of ITEMS) {
      expect(OrphanItemSchema.safeParse(row).success).toBe(true);
    }
    expect(parseCollections(COLLECTIONS).length).toBe(COLLECTIONS.length);
    expect(parseOrphanItems(ITEMS).length).toBe(ITEMS.length);
    expect(parseCollections(null)).toEqual([]);
    expect(parseOrphanItems(undefined)).toEqual([]);
  });
});

describe("URL readers", () => {
  it("normalises tab / view / sort and maps a tab to its ui3 TABS id", () => {
    expect(readTab("third_party")).toBe("third_party");
    expect(readTab("items")).toBe("items");
    expect(readTab("garbage")).toBe("standard");
    expect(readTab(null)).toBe("standard");
    expect(readView("list")).toBe("list");
    expect(readView("grid")).toBe("grid");
    expect(readView(null)).toBe("grid");
    expect(readSort("CREATED_AT_DESC")).toBe("CREATED_AT_DESC");
    expect(readSort("NAME_ASC")).toBe("NAME_ASC");
    expect(readSort("nope")).toBe("MOST_RELEVANT");
    expect(readSort(null)).toBe("MOST_RELEVANT");
    expect(tabToUiId("standard")).toBe("standard_collections");
    expect(tabToUiId("third_party")).toBe("third_party_collections");
    expect(tabToUiId("items")).toBe("orphan_items");
  });
});

describe("sortCollections", () => {
  it("orders by created_at both ways and by name, and leaves MOST_RELEVANT untouched", () => {
    const rows = parseCollections(COLLECTIONS);
    const desc = sortCollections(rows, "CREATED_AT_DESC");
    for (let i = 1; i < desc.length; i++) {
      expect(desc[i - 1].created_at ?? 0).toBeGreaterThanOrEqual(desc[i].created_at ?? 0);
    }
    const asc = sortCollections(rows, "CREATED_AT_ASC");
    for (let i = 1; i < asc.length; i++) {
      expect(asc[i - 1].created_at ?? 0).toBeLessThanOrEqual(asc[i].created_at ?? 0);
    }
    const names = sortCollections(rows, "NAME_ASC").map((c) => c.name);
    expect([...names].sort((a, b) => a.localeCompare(b))).toEqual(names);
    expect(sortCollections(rows, "MOST_RELEVANT").map((c) => c.id)).toEqual(rows.map((c) => c.id));
  });
});

describe("view-models", () => {
  it("projects collections and orphan items onto ui3 cards with coarse, null-safe relative dates", () => {
    const rows = parseCollections(COLLECTIONS);
    const tp = rows.find((c) => c.type === "third_party")!;
    const std = rows.find((c) => c.type === "standard")!;
    expect(toCollectionCard(tp).type).toBe("third_party");
    expect(toCollectionCard(std).type).toBe("collection");
    const empty = rows.find((c) => c.count === 0)!;
    expect(toCollectionCard(empty).count).toBe(0);
    expect(toCollectionCard(empty).thumbs).toEqual([]);
    const vm = toOrphanItem(parseOrphanItems(ITEMS)[0]);
    expect(vm.id).toBeTruthy();
    expect(typeof vm.createdAt).toBe("string");
    expect(typeof vm.grad).toBe("string");
    expect(relativeTime(null)).toBe("");
    expect(relativeTime(Date.now())).toBe("today");
    expect(relativeTime(Date.now() - 10 * 86_400_000)).toMatch(/week/);
  });
});

describe("fetchCollections (injected fetch, no network)", () => {
  it("unwraps a {data} envelope and validates rows", async () => {
    const stub: typeof fetch = async () =>
      new Response(JSON.stringify({ data: COLLECTIONS }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    const rows = await fetchCollections("0xabc", { fetchImpl: stub });
    expect(rows.length).toBe(COLLECTIONS.length);
    expect(rows[0].name).toBeTruthy();
  });
});
