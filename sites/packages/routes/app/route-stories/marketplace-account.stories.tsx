import MarketplaceAccount from "../routes/marketplace.account";
import accountLoader from "@data/fixtures/route-account-loader.json";
import collectionsLoader from "@data/fixtures/route-account-collections.json";
import { routeStory } from "./lib";

const TABS = ["overview", "on-sale", "on-rent", "collections", "bids"];

function accountStubLoader(base: typeof accountLoader | typeof collectionsLoader) {
  return ({ request }: { request: Request }) => {
    const raw = new URL(request.url).searchParams.get("tab")?.trim() ?? "";
    const tab = TABS.includes(raw) ? raw : "overview";
    return {
      ...base,
      tab,
      collections: tab === "collections" ? base.collections : [],
      collectionsTotal: tab === "collections" ? base.collectionsTotal : 0,
    };
  };
}

export default {
  title: "Routes/MarketplaceAccount",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Overview = {
  render: routeStory({
    Component: MarketplaceAccount,
    path: "/marketplace/account",
    loaderData: accountLoader,
    loader: accountStubLoader(accountLoader),
  }),
};

export const OnSale = {
  render: routeStory({
    Component: MarketplaceAccount,
    path: "/marketplace/account",
    url: "/marketplace/account?tab=on-sale",
    loaderData: { ...accountLoader, tab: "on-sale" },
    loader: accountStubLoader(accountLoader),
  }),
};

export const CollectionsTab = {
  render: routeStory({
    Component: MarketplaceAccount,
    path: "/marketplace/account",
    url: "/marketplace/account?tab=collections",
    loaderData: collectionsLoader,
    loader: accountStubLoader(collectionsLoader),
  }),
};

export const SignedOutEmpty = {
  render: routeStory({
    Component: MarketplaceAccount,
    path: "/marketplace/account",
    loaderData: {
      sid: "story-sid",
      tab: "overview",
      address: "",
      source: "empty",
      fallback: true,
      ownedCards: [],
      ownedTotal: 0,
      names: [],
      namesTotal: 0,
      onSaleRows: [],
      onSaleTotal: 0,
      collections: [],
      collectionsTotal: 0,
    },
  }),
};
