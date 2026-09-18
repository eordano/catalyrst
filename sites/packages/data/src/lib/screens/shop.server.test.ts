import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { resetCatalogRailCache } from "../catalyst/marketplace/catalog-rails.server";
import { tryQuoteCreditItems } from "../catalyst/marketplace/credit-quotes";
import { fetchCatalog, type CatalogItem } from "../catalyst/marketplace/index";
import { PUBLIC_FEED_MAX_AGE_MS } from "../public-feed.server";
import { loadShopScreen, SHOP_CATALOG_TIMEOUT_MS, SHOP_QUOTES_TIMEOUT_MS, SHOP_RAIL_TIMEOUT_MS } from "./shop.server";

vi.mock("../catalyst/marketplace/index", async (original) => ({
  ...await original<typeof import("../catalyst/marketplace/index")>(),
  fetchCatalog: vi.fn(),
}));
vi.mock("../catalyst/marketplace/credit-quotes", () => ({ tryQuoteCreditItems: vi.fn() }));

const collection = "0x1111111111111111111111111111111111111111";
function item(id: number, overrides: Partial<CatalogItem> = {}): CatalogItem {
  return {
    id: `${collection}-${id}`, name: `Item ${id}`, category: "wearable",
    price: "2000000000000000000", minListingPrice: null, isOnSale: true,
    network: "matic", rarity: "common", thumbnail: `/image/${id}`,
    ...overrides,
  } as CatalogItem;
}
const read = (query = "", signal?: AbortSignal) => loadShopScreen(new URLSearchParams(query), signal);

beforeEach(() => {
  vi.useFakeTimers();
  resetCatalogRailCache();
  vi.mocked(fetchCatalog).mockReset().mockImplementation(async (params = {}) => {
    const offset = params.sortBy === "cheapest" ? 200 : params.sortBy === "most_expensive" ? 100 : 0;
    return { data: Array.from({ length: params.first ?? 0 }, (_, i) => item(offset + i)), total: 200 };
  });
  vi.mocked(tryQuoteCreditItems).mockReset().mockImplementation(async (refs) => refs.map(() => "20"));
});

afterEach(() => {
  resetCatalogRailCache();
  vi.useRealTimers();
});

describe("shop screen data", () => {
  it("shares all three default overview catalogs across visitors, but refreshes quotes", async () => {
    await read();
    await read();
    expect(fetchCatalog).toHaveBeenCalledTimes(3);
    expect(tryQuoteCreditItems).toHaveBeenCalledTimes(2);
  });

  it("reuses public searches and refreshes the default overview in the background after its TTL", async () => {
    await read();
    await read("search=hat");
    await read("search=hat");
    expect(fetchCatalog).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(PUBLIC_FEED_MAX_AGE_MS - 1);
    const stale = await read();
    expect(stale.data.freshness?.refreshing).toBe(true);
    expect(fetchCatalog).toHaveBeenCalledTimes(7);
  });

  it("loads and prices only the overview's visible cards, while preserving card price units", async () => {
    const { data, serverTiming } = await read();
    expect(fetchCatalog).toHaveBeenCalledTimes(3);
    expect(vi.mocked(fetchCatalog).mock.calls.map(([p]) => p?.first).sort()).toEqual([6, 8, 8]);
    expect(data.cards).toHaveLength(8);
    expect(data.topCards).toHaveLength(6);
    expect(data.trendingCards).toHaveLength(8);
    expect(vi.mocked(tryQuoteCreditItems).mock.calls[0][0]).toHaveLength(
      data.cards.length + data.topCards.length + data.trendingCards.length,
    );
    expect(data.cards[0]).toMatchObject({ price: "2", credits: "20" });
    expect(data.fallback).toBe(false);
    expect(serverTiming).toMatch(/shop_catalog;dur=[\d.]+;desc="ready"/);
    expect(serverTiming).toContain("shop_total;dur=");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("retains browse pagination and filters without fetching overview rails", async () => {
    const { data } = await read("tab=all-assets&page=2&category=emote&rarity=rare&search=dance&sortBy=cheapest");
    expect(fetchCatalog).toHaveBeenCalledTimes(1);
    expect(fetchCatalog).toHaveBeenCalledWith(expect.objectContaining({
      first: 40, skip: 80, category: "emote", rarity: "rare", search: "dance",
      sortBy: "cheapest",
    }), expect.anything());
    expect(data.cards).toHaveLength(40);
    expect(data.total).toBe(200);
    expect(data.sections).toEqual({ catalog: "ready", top: "skipped", trending: "skipped" });
  });

  it.each(["my-assets", "my-favorites"])("does no catalog or quote work for %s", async (tab) => {
    const { data } = await read(`tab=${tab}`);
    expect(fetchCatalog).not.toHaveBeenCalled();
    expect(tryQuoteCreditItems).not.toHaveBeenCalled();
    expect(data.fallback).toBe(false);
    expect(data.cards).toEqual([]);
  });

  it("keeps successful sections when a rail fails", async () => {
    vi.mocked(fetchCatalog).mockImplementation(async (params) => {
      if (params?.sortBy === "most_expensive") throw new Error("rail unavailable");
      return { data: [item(1)], total: 1 };
    });
    const { data } = await read();
    expect(data.cards).toHaveLength(1);
    expect(data.trendingCards).toHaveLength(1);
    expect(data.topCards).toEqual([]);
    expect(data.sections.top).toBe("unavailable");
    expect(data.fallback).toBe(true);
    expect(tryQuoteCreditItems).toHaveBeenCalledWith([
      { collection, itemId: "1" },
    ], { signal: expect.any(AbortSignal) });
  });

  it("distinguishes a successful empty catalog from a failed catalog", async () => {
    vi.mocked(fetchCatalog).mockResolvedValue({ data: [], total: 0 });
    const empty = await read("tab=all-assets");
    expect(empty.data.sections.catalog).toBe("ready");
    expect(empty.data.fallback).toBe(false);
    vi.mocked(fetchCatalog).mockRejectedValue(new Error("offline"));
    resetCatalogRailCache();
    const failed = await read("tab=all-assets");
    expect(failed.data.sections.catalog).toBe("unavailable");
    expect(failed.data.fallback).toBe(true);
  });

  it("does not price unbuyable items or more cards than the section displays", async () => {
    vi.mocked(fetchCatalog).mockResolvedValue({
      data: [item(99, { price: "0" }), ...Array.from({ length: 50 }, (_, i) => item(i))], total: 51,
    });
    const { data } = await read();
    expect(data.cards).toHaveLength(8);
    expect(data.topCards).toHaveLength(6);
    expect(data.cards.every((c) => c.id !== item(99).id)).toBe(true);
    expect(vi.mocked(tryQuoteCreditItems).mock.calls[0][0]).toHaveLength(8);
  });

  it("renders MANA prices when quotes exceed their deadline and aborts their request", async () => {
    let quoteSignal: AbortSignal | undefined;
    vi.mocked(tryQuoteCreditItems).mockImplementation((_refs, opts) => {
      quoteSignal = opts?.signal;
      return new Promise(() => {});
    });
    const pending = read("tab=all-assets");
    await vi.advanceTimersByTimeAsync(SHOP_QUOTES_TIMEOUT_MS);
    const { data } = await pending;
    expect(quoteSignal?.aborted).toBe(true);
    expect(data.cards[0]).toMatchObject({ price: "2", credits: null });
    expect(data.sections.catalog).toBe("ready");
  });

  it("bounds stalled catalog work even if a dependency ignores cancellation", async () => {
    let requestSignal: AbortSignal | undefined;
    vi.mocked(fetchCatalog).mockImplementation((_params, opts) => {
      requestSignal = opts?.signal;
      return new Promise(() => {});
    });
    const pending = read("tab=all-assets");
    await vi.advanceTimersByTimeAsync(SHOP_CATALOG_TIMEOUT_MS);
    expect((await pending).data.fallback).toBe(true);
    expect(requestSignal?.aborted).toBe(true);
    expect(tryQuoteCreditItems).not.toHaveBeenCalled();
  });

  it("lets a shared rail refresh finish after an individual screen's deadline", async () => {
    vi.mocked(fetchCatalog).mockImplementation(async (params) => {
      if (params?.sortBy !== "recently_listed") {
        await new Promise((resolve) => setTimeout(resolve, SHOP_RAIL_TIMEOUT_MS + 100));
      }
      return { data: [item(1)], total: 1 };
    });
    const pending = read();
    await vi.advanceTimersByTimeAsync(SHOP_RAIL_TIMEOUT_MS);
    const first = await pending;
    expect(first.data.cards).toHaveLength(1);
    expect(first.data.sections.top).toBe("unavailable");
    await vi.advanceTimersByTimeAsync(100);
    const second = await read();
    expect(second.data.sections.top).toBe("ready");
    expect(fetchCatalog).toHaveBeenCalledTimes(3);
  });

  it("cancels one visitor promptly without aborting a shared rail used by another", async () => {
    const signals: AbortSignal[] = [];
    vi.mocked(fetchCatalog).mockImplementation(async (_params, opts) => {
      signals.push(opts!.signal!);
      await new Promise((resolve) => setTimeout(resolve, 100));
      return { data: [item(1)], total: 1 };
    });
    const cancel = new AbortController();
    const first = read("", cancel.signal);
    const rejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = read();
    await vi.advanceTimersByTimeAsync(0);
    cancel.abort();
    await rejected;
    expect(signals.filter((s) => s.aborted)).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(100);
    expect((await second).data.fallback).toBe(false);
    expect(fetchCatalog).toHaveBeenCalledTimes(3);
  });

  it("does not start work for an already cancelled request", async () => {
    await expect(read("", AbortSignal.abort())).rejects.toMatchObject({ name: "AbortError" });
    expect(fetchCatalog).not.toHaveBeenCalled();
  });

  it("normalizes unknown tabs and ignores hidden overview pagination", async () => {
    const { data } = await read("tab=unknown&page=5");
    expect(data.filters).toMatchObject({ tab: "overview", page: 0 });
    expect(data.trendingCards.length).toBeGreaterThan(0);
    expect(fetchCatalog).toHaveBeenCalledWith(expect.objectContaining({ skip: 0 }), expect.anything());
  });
});
