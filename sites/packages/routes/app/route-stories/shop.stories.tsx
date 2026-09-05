import MarketplaceShop from "../routes/shop";
import shopLoader from "@data/fixtures/route-shop-loader.json";
import { routeStory } from "./lib";

const base = shopLoader;

function shopStubLoader({ request }: { request: Request }) {
  const p = new URL(request.url).searchParams;
  const page = Number.parseInt(p.get("page") ?? "0", 10);
  return {
    ...base,
    filters: {
      tab: p.get("tab")?.trim() || "overview",
      category: p.get("category")?.trim() ?? "",
      rarity: p.get("rarity")?.trim() ?? "",
      sortBy: p.get("sortBy")?.trim() || "recently_listed",
      search: p.get("search")?.trim() ?? "",
      page: Number.isFinite(page) && page > 0 ? page : 0,
    },
  };
}

export default {
  title: "Routes/Shop",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Overview = {
  render: routeStory({
    Component: MarketplaceShop,
    path: "/shop",
    loaderData: base,
    loader: shopStubLoader,
  }),
};

export const AllAssets = {
  render: routeStory({
    Component: MarketplaceShop,
    path: "/shop",
    url: "/shop?tab=all-assets",
    loaderData: { ...base, filters: { ...base.filters, tab: "all-assets" } },
    loader: shopStubLoader,
  }),
};

export const FavoritesEmpty = {
  render: routeStory({
    Component: MarketplaceShop,
    path: "/shop",
    url: "/shop?tab=my-favorites",
    loaderData: { ...base, filters: { ...base.filters, tab: "my-favorites" } },
    loader: shopStubLoader,
  }),
};

export const CatalogDown = {
  render: routeStory({
    Component: MarketplaceShop,
    path: "/shop",
    loaderData: {
      ...base,
      cards: [],
      topCards: [],
      trendingCards: [],
      total: 0,
      fallback: true,
    },
  }),
};
