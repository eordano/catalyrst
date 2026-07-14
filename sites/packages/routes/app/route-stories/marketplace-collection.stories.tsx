import MarketplaceCollection from "../routes/marketplace.collection";
import collectionFx from "@data/fixtures/route-collection.json";
import { routeStory } from "./lib";

const base = collectionFx;

const VALID_SORTS = new Set([
  "recently_listed",
  "cheapest",
  "most_expensive",
  "recently_sold",
]);

function collectionStubLoader({ request }: { request: Request }) {
  const raw = new URL(request.url).searchParams.get("sortBy")?.trim() ?? "";
  return { ...base, sortBy: VALID_SORTS.has(raw) ? raw : "recently_listed" };
}

const EMPTY_STATS = {
  floor: null,
  floorCredits: null,
  creator: "",
  creatorShort: "",
  itemCount: 0,
  network: "polygon",
};

export default {
  title: "Routes/MarketplaceCollection",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Default = {
  render: routeStory({
    Component: MarketplaceCollection,
    path: "/marketplace/collection",
    url: `/marketplace/collection?contract=${base.contract}`,
    loaderData: base,
    loader: collectionStubLoader,
  }),
};

export const NotFound = {
  render: routeStory({
    Component: MarketplaceCollection,
    path: "/marketplace/collection",
    url: "/marketplace/collection?contract=0x0000000000000000000000000000000000000000",
    loaderData: {
      sid: "story-sid",
      contract: "0x0000000000000000000000000000000000000000",
      sortBy: "recently_listed",
      header: null,
      items: [],
      stats: EMPTY_STATS,
      fallback: false,
    },
  }),
};
