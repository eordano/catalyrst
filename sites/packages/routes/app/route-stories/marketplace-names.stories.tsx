import MarketplaceNames from "../routes/marketplace.names";
import {
  classifyName,
  type NameAvailability,
} from "@data/lib/catalyst/marketplace/names";
import { routeStory } from "./lib";

const LIVE_RESULTS: Record<string, NameAvailability> = {
  spam: {
    kind: "listed",
    name: "Spam",
    contractAddress: "0x2a187453064356c898cae034eaed119e1663acb8",
    tokenId:
      "25148460884184534751006484895236384418939915233395890336266209114444888566",
    priceWei: "4225000000000000000000",
    priceMana: "4,225",
  },
  maria: { kind: "taken", name: "Maria" },
  job: { kind: "taken", name: "Job" },
  decentraland: { kind: "taken", name: "Decentraland" },
};

const NO_TAKEN: ReadonlySet<string> = new Set();

function readQuery(request: Request): string {
  const p = new URL(request.url).searchParams;
  return (p.get("search") ?? p.get("name") ?? "").trim();
}

function namesStubLoader({ request }: { request: Request }) {
  const query = readQuery(request);
  let result: NameAvailability | null = null;
  if (query && classifyName(query, NO_TAKEN).kind === "available") {
    result = LIVE_RESULTS[query.toLowerCase()] ?? { kind: "claimable", name: query };
  }
  return { sid: "story-sid", query, result, checkFailed: false };
}

function checkDownStubLoader({ request }: { request: Request }) {
  const query = readQuery(request);
  const available = !!query && classifyName(query, NO_TAKEN).kind === "available";
  return { sid: "story-sid", query, result: null, checkFailed: available };
}

function loaderDataFor(
  url: string,
  loader: typeof namesStubLoader = namesStubLoader,
) {
  return loader({ request: new Request(`https://catalyst.example.com${url}`) });
}

export default {
  title: "Routes/MarketplaceNames",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Idle = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    loaderData: loaderDataFor("/marketplace/names"),
    loader: namesStubLoader,
  }),
};

export const Claimable = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    url: "/marketplace/names?search=storybooksmoke",
    loaderData: loaderDataFor("/marketplace/names?search=storybooksmoke"),
    loader: namesStubLoader,
  }),
};

export const ListedForResale = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    url: "/marketplace/names?search=spam",
    loaderData: loaderDataFor("/marketplace/names?search=spam"),
    loader: namesStubLoader,
  }),
};

export const Taken = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    url: "/marketplace/names?search=maria",
    loaderData: loaderDataFor("/marketplace/names?search=maria"),
    loader: namesStubLoader,
  }),
};

export const InvalidTooShort = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    url: "/marketplace/names?search=a",
    loaderData: loaderDataFor("/marketplace/names?search=a"),
    loader: namesStubLoader,
  }),
};

export const CheckFailed = {
  render: routeStory({
    Component: MarketplaceNames,
    path: "/marketplace/names",
    url: "/marketplace/names?search=palermo",
    loaderData: loaderDataFor("/marketplace/names?search=palermo", checkDownStubLoader),
    loader: checkDownStubLoader,
  }),
};
