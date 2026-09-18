import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../builder/collections", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../builder/collections")>();
  return { ...actual, fetchCollections: vi.fn() };
});
vi.mock("../marketplace/index", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../marketplace/index")>();
  return { ...actual, fetchCreations: vi.fn() };
});
vi.mock("../marketplace/activity", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../marketplace/activity")>();
  return { ...actual, fetchSalesByContracts: vi.fn() };
});
vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, getJSON: vi.fn() };
});

import { CatalystError, getJSON } from "../client";
import {
  fetchCollections,
  type BuilderCollection,
} from "../builder/collections";
import { fetchCreations } from "../marketplace/index";
import { fetchSalesByContracts } from "../marketplace/activity";
import { loadCreatorMetrics } from "./metrics.server";

const ADDRESS = "0x797066a17f83425c1b4c7a8cca52d19095520a52";

const mockCollections = vi.mocked(fetchCollections);
const mockCreations = vi.mocked(fetchCreations);
const mockSales = vi.mocked(fetchSalesByContracts);
const mockGetJSON = vi.mocked(getJSON);

function collection(over: Partial<BuilderCollection>): BuilderCollection {
  return {
    id: "c1",
    name: "Col",
    type: "collection",
    status: "synced",
    count: 1,
    thumbs: [],
    pending: false,
    contract_address: "0x14ad733ea8e28e93160ac7d8a94cdfdedcdafdf0",
    created_at: 1,
    updated_at: 1,
    ...over,
  } as BuilderCollection;
}

type Creations = Awaited<ReturnType<typeof fetchCreations>>;
function creations(
  wearables: Array<{ price: string | null }>,
  emotes: Array<{ price: string | null }> = [],
): Creations {
  const item = (p: { price: string | null }, i: number) =>
    ({ id: `i${i}`, name: `Item ${i}`, price: p.price }) as Creations["wearables"][number];
  return {
    wearables: wearables.map(item),
    emotes: emotes.map(item),
  };
}

const NO_PLACES = { ok: true, data: [], total: 0 };

beforeEach(() => {
  vi.resetAllMocks();
});

describe("loadCreatorMetrics \u{2014} failure must not masquerade as emptiness", () => {
  it("a single source down (collections 503, items 500) \u{2192} null count, and NOT the empty state", async () => {
    mockCollections.mockRejectedValue(
      new CatalystError("Catalyst returned 503", "/v1/x/collections", 503),
    );
    mockCreations.mockResolvedValue(creations([]));
    mockGetJSON.mockResolvedValue(NO_PLACES);

    const collectionsDown = await loadCreatorMetrics(ADDRESS);
    expect(collectionsDown.summary.publishedCollections).toBeNull();
    expect(collectionsDown.summary.salesUnavailable).toBe(true);
    expect(collectionsDown.empty).toBe(false);
    expect(collectionsDown.loadError).toBe(false);

    vi.resetAllMocks();
    mockCollections.mockResolvedValue([]);
    mockCreations.mockRejectedValue(
      new CatalystError("Catalyst returned 500", "/market/v1/items", 500),
    );
    mockGetJSON.mockResolvedValue(NO_PLACES);

    const itemsDown = await loadCreatorMetrics(ADDRESS);
    expect(itemsDown.summary.onSaleItems).toBeNull();
    expect(itemsDown.empty).toBe(false);
    expect(itemsDown.loadError).toBe(false);
  });

  it("sales down while collections exist \u{2192} salesUnavailable; all sources down \u{2192} loadError, never empty", async () => {
    mockCollections.mockResolvedValue([collection({})]);
    mockCreations.mockResolvedValue(creations([{ price: "1000000000000000000" }]));
    mockGetJSON.mockResolvedValue(NO_PLACES);
    mockSales.mockRejectedValue(new Error("db down"));

    const salesDown = await loadCreatorMetrics(ADDRESS);
    expect(salesDown.summary.salesUnavailable).toBe(true);
    expect(salesDown.summary.sales7d).toBeNull();
    expect(salesDown.summary.publishedCollections).toBe(1);
    expect(salesDown.empty).toBe(false);

    vi.resetAllMocks();
    mockCollections.mockRejectedValue(new Error("down"));
    mockCreations.mockRejectedValue(new Error("down"));
    mockGetJSON.mockRejectedValue(new Error("down"));

    const allDown = await loadCreatorMetrics(ADDRESS);
    expect(allDown.loadError).toBe(true);
    expect(allDown.empty).toBe(false);
    expect(allDown.summary.publishedCollections).toBeNull();
    expect(allDown.summary.onSaleItems).toBeNull();
    expect(allDown.summary.scenes).toBeNull();
  });
});

describe("loadCreatorMetrics \u{2014} real data and genuine emptiness still work", () => {
  it("a creator with data gets real numbers; one with nothing published is empty (all sources OK)", async () => {
    mockCollections.mockResolvedValue([
      collection({ id: "a", status: "synced" }),
      collection({ id: "b", status: "unsynced", contract_address: undefined }),
    ]);
    mockCreations.mockResolvedValue(
      creations(
        [{ price: "5000000000000000000" }, { price: null }],
        [{ price: "3000000000000000000" }],
      ),
    );
    mockGetJSON.mockResolvedValue({
      ok: true,
      data: [
        { world: false, base_position: "10,10", user_visits: 10, user_count: 2 },
        { world: false, base_position: "11,11", user_visits: 5, user_count: 0 },
      ],
      total: 2,
    });
    mockSales.mockResolvedValue({
      data: [
        { timestamp: Date.now() - 1000, price: "2000000000000000000" },
      ] as Awaited<ReturnType<typeof fetchSalesByContracts>>["data"],
      total: 3,
    });

    const withData = await loadCreatorMetrics(ADDRESS);
    expect(withData.summary.publishedCollections).toBe(1);
    expect(withData.summary.onSaleItems).toBe(2);
    expect(withData.summary.sales7d).toBe(3);
    expect(withData.summary.salesVolumeMana7d).toBe(2);
    expect(withData.summary.scenes).toEqual({ places: 2, visits30d: 15, liveNow: 2 });
    expect(withData.empty).toBe(false);
    expect(withData.loadError).toBe(false);

    vi.resetAllMocks();
    mockCollections.mockResolvedValue([]);
    mockCreations.mockResolvedValue(creations([]));
    mockGetJSON.mockResolvedValue(NO_PLACES);

    const nothing = await loadCreatorMetrics(ADDRESS);
    expect(nothing.empty).toBe(true);
    expect(nothing.loadError).toBe(false);
    expect(nothing.summary.publishedCollections).toBe(0);
    expect(nothing.summary.sales7d).toBe(0);
  });
});
