import { loadCatalogRail } from "../catalyst/marketplace/catalog-rails.server";
import { tryQuoteCreditItems } from "../catalyst/marketplace/credit-quotes";
import {
  fetchCatalog,
  isCatalogItemBuyable,
  parseItemId,
  toCollectibleCard,
  type CatalogItem,
} from "../catalyst/marketplace/index";
import type { MarketEnvelope } from "../catalyst/marketplace/schema";
import { withDeadline } from "../request-deadline";
import {
  readShopFilters, SHOP_OVERVIEW_SIZE, SHOP_PAGE_SIZE,
  type ShopScreen, type ShopSectionState,
} from "./shop";

export const SHOP_CATALOG_TIMEOUT_MS = 3_000;
export const SHOP_RAIL_TIMEOUT_MS = 1_000;
export const SHOP_QUOTES_TIMEOUT_MS = 1_000;

type CatalogSection = MarketEnvelope<CatalogItem[]> & { state: ShopSectionState };

export async function loadShopScreen(params: URLSearchParams, signal?: AbortSignal) {
  signal?.throwIfAborted();
  const started = performance.now();
  const timings: string[] = [];
  const filters = readShopFilters(params);
  const overview = filters.tab === "overview";
  const catalog = overview || filters.tab === "all-assets";
  const limit = overview ? SHOP_OVERVIEW_SIZE : SHOP_PAGE_SIZE;

  async function section(
    name: string,
    enabled: boolean,
    size: number,
    timeout: number,
    load: (signal: AbortSignal) => Promise<MarketEnvelope<CatalogItem[]>>,
  ): Promise<CatalogSection> {
    if (!enabled) return { data: [], total: 0, state: "skipped" };
    const at = performance.now();
    let state: ShopSectionState = "unavailable";
    try {
      const result = await withDeadline(load, timeout, signal);
      state = "ready";
      return { data: result.data.filter(isCatalogItemBuyable).slice(0, size), total: result.total, state };
    } catch {
      signal?.throwIfAborted();
      return { data: [], total: 0, state };
    } finally {
      timings.push(`shop_${name};dur=${(performance.now() - at).toFixed(1)};desc="${state}"`);
    }
  }

  const [items, top, trending] = await Promise.all([
    section("catalog", catalog, limit, SHOP_CATALOG_TIMEOUT_MS, (requestSignal) =>
      fetchCatalog({
        first: limit,
        skip: filters.page * SHOP_PAGE_SIZE,
        category: filters.category || undefined,
        rarity: filters.rarity || undefined,
        isOnSale: true,
        minPrice: "1",
        sortBy: filters.sortBy,
        search: filters.search || undefined,
      }, { signal: requestSignal }),
    ),
    section("top", overview, 6, SHOP_RAIL_TIMEOUT_MS, () =>
      loadCatalogRail({ first: 6, isOnSale: true, minPrice: "1", sortBy: "most_expensive" }),
    ),
    section("trending", overview, 8, SHOP_RAIL_TIMEOUT_MS, () =>
      loadCatalogRail({ first: 8, isOnSale: true, minPrice: "1", sortBy: "cheapest" }),
    ),
  ]);
  signal?.throwIfAborted();

  const unique = new Map<string, CatalogItem>();
  for (const item of [...items.data, ...top.data, ...trending.data]) unique.set(item.id, item);
  const quotables = [...unique.values()];
  const refs = quotables.map((item) => {
    const ref = parseItemId(item.id);
    return ref ? { itemId: ref.itemId, collection: ref.contractAddress } : null;
  });
  let credits: (string | null)[] = [];
  if (refs.some(Boolean)) {
    const at = performance.now();
    try {
      credits = await withDeadline(
        (requestSignal) => tryQuoteCreditItems(refs, { signal: requestSignal }),
        SHOP_QUOTES_TIMEOUT_MS,
        signal,
      );
    } catch {
      signal?.throwIfAborted();
    }
    timings.push(`shop_quotes;dur=${(performance.now() - at).toFixed(1)}`);
  }
  signal?.throwIfAborted();
  const creditsById = new Map(quotables.map((item, index) => [item.id, credits[index] ?? null]));
  const card = (item: CatalogItem) => toCollectibleCard(item, creditsById.get(item.id) ?? null);
  const data: ShopScreen = {
    filters,
    cards: items.data.map(card),
    topCards: top.data.map(card),
    trendingCards: trending.data.map(card),
    total: items.total,
    fallback: [items, top, trending].some((s) => s.state === "unavailable"),
    sections: { catalog: items.state, top: top.state, trending: trending.state },
  };
  timings.push(`shop_total;dur=${(performance.now() - started).toFixed(1)}`);
  return { data, serverTiming: timings.join(", ") };
}
