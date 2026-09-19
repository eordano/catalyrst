import type { CollectibleCard } from "../catalyst/marketplace/index";

export const SHOP_PAGE_SIZE = 40;
export const SHOP_OVERVIEW_SIZE = 8;

export type ShopFilters = {
  tab: "overview" | "all-assets" | "my-assets" | "my-favorites";
  category: string;
  rarity: string;
  sortBy: string;
  search: string;
  page: number;
};

export type ShopSectionState = "ready" | "unavailable" | "skipped";

export type ShopScreen = {
  filters: ShopFilters;
  cards: CollectibleCard[];
  topCards: CollectibleCard[];
  trendingCards: CollectibleCard[];
  total: number;
  fallback: boolean;
  sections: Record<"catalog" | "top" | "trending", ShopSectionState>;
};

export function readShopFilters(params: URLSearchParams): ShopFilters {
  const rawTab = params.get("tab")?.trim();
  const tab = rawTab === "all-assets" || rawTab === "my-assets" || rawTab === "my-favorites"
    ? rawTab
    : "overview";
  const page = Number.parseInt(params.get("page") ?? "0", 10);
  return {
    tab,
    category: params.get("category")?.trim() ?? "",
    rarity: params.get("rarity")?.trim() ?? "",
    sortBy: params.get("sortBy")?.trim() || "recently_listed",
    search: params.get("search")?.trim() ?? "",
    page: tab === "all-assets" && Number.isSafeInteger(page) && page > 0
      && Number.isSafeInteger(page * SHOP_PAGE_SIZE) ? page : 0,
  };
}
