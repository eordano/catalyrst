import MarketplaceAssetRoute from "../routes/marketplace.$id";
import assetDetail from "@data/fixtures/route-asset-detail.json";
import { catalystGet, routeStory } from "./lib";

const base = assetDetail;

export default {
  title: "Routes/MarketplaceItem",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const EmoteForSale = {
  render: routeStory({
    Component: MarketplaceAssetRoute,
    path: "/marketplace/:id",
    url: `/marketplace/${encodeURIComponent(base.id)}`,
    loaderData: base,
  }),
  parameters: {
    msw: {
      handlers: [
        catalystGet("/credits/cart", { address: "0x0", items: [], totalCredits: "0" }),
      ],
    },
  },
};

export const NotFound = {
  render: routeStory({
    Component: MarketplaceAssetRoute,
    path: "/marketplace/:id",
    url: "/marketplace/0x0000000000000000000000000000000000000000-999",
    loaderData: {
      ...base,
      id: "0x0000000000000000000000000000000000000000-999",
      nft: null,
      listings: [],
    },
  }),
};
