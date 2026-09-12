import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  fetchCatalogItem,
  fetchNft,
  formatMana,
  toCardNetwork,
  type CatalogItem,
} from "./index";
import { ensOrderExpired } from "./names";
import { parseMarketEnvelope } from "./schema";
import { warnInvalid } from "../warn";
import { OrderSchema } from "../generated-schemas/market";

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const BuyOrderSchema = OrderSchema.extend({ tokenId: z.string() });

export type BuyOrder = z.infer<typeof BuyOrderSchema>;

export function parseBuyOrder(raw: unknown): BuyOrder | null {
  const r = BuyOrderSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("BuyOrder", r.error.issues);
  return null;
}

export type FetchOrdersParams = {
  contractAddress?: string;
  tokenId?: string;
  itemId?: string;
  status?: string;
  sortBy?: string;
  first?: number;
  skip?: number;
};

export type FetchedOrders = {
  data: BuyOrder[];
  total: number;
  invalid: number;
};

export async function fetchOrders(
  params: FetchOrdersParams = {},
  opts: GetOptions = {},
): Promise<FetchedOrders> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/orders", {
      ...opts,
      query: {
        contractAddress: params.contractAddress,
        tokenId: params.tokenId,
        itemId: params.itemId,
        status: params.status,
        sortBy: params.sortBy,
        first: params.first,
        skip: params.skip,
      },
    }),
  );
  const data: BuyOrder[] = [];
  let invalid = 0;
  for (const row of env.data) {
    const order = parseBuyOrder(row);
    if (order) data.push(order);
    else invalid += 1;
  }
  return { data, total: env.total, invalid };
}

export type OrderLookup = {
  order: BuyOrder | null;
  source: "catalyst" | "empty" | "unavailable";
  reason?: string;
};

const DROPPED_ROWS =
  "the marketplace returned listings we could not validate, so none of them can be quoted";

export async function fetchCheapestOpenOrder(
  contractAddress: string,
  opts: GetOptions = {},
): Promise<OrderLookup> {
  let res: FetchedOrders;
  try {
    res = await fetchOrders(
      { contractAddress, status: "open", sortBy: "cheapest", first: 5 },
      opts,
    );
  } catch (error) {
    return { order: null, source: "unavailable", reason: message(error) };
  }
  const open = res.data.find((o) => o.status === "open" && !o.buyer) ?? res.data[0];
  if (open) return { order: open, source: "catalyst" };
  if (res.invalid > 0) {
    return { order: null, source: "unavailable", reason: DROPPED_ROWS };
  }
  return { order: null, source: "empty" };
}

export type BuyAsset = {
  name: string;
  rarity: string;
  category: string;
  kind: "wearable" | "emote" | "ens" | "land" | "other";
  image: string | null;
};

export type BuyableListing = {
  assetId: string;
  contractAddress: string;
  tokenId: string;
  issuedId: string | null;
  priceMana: string;
  priceWei: string;
  network: "ethereum" | "polygon";
  chainId: number | null;
  marketplaceAddress: string | null;
  seller: string;
  asset: BuyAsset;
  buyable: boolean;
};

const RARITIES = new Set([
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
]);

function safeRarity(r: string | null | undefined): string {
  return r && RARITIES.has(r) ? r : "common";
}

function kindFor(category: string | null | undefined): BuyAsset["kind"] {
  switch (category) {
    case "wearable":
      return "wearable";
    case "emote":
      return "emote";
    case "ens":
      return "ens";
    case "parcel":
    case "estate":
      return "land";
    default:
      return "other";
  }
}

function wearableDescriptionCategory(item: CatalogItem | null): string {
  if (!item) return "wearable";
  const data = item.data as Record<string, unknown>;
  const sub = (data?.wearable ?? data?.emote) as Record<string, unknown> | undefined;
  const c = sub?.category;
  return typeof c === "string" && c ? c : item.category ?? "wearable";
}

export function toBuyableListing(
  order: BuyOrder,
  item: CatalogItem | null,
): BuyableListing {
  const assetId = item ? item.id : `${order.contractAddress}-${order.tokenId}`;
  const priceMana = formatMana(order.price) ?? "0";
  const category = item?.category ?? null;
  const name = item?.name ?? `Item #${order.issuedId ?? order.tokenId.slice(0, 8)}`;
  return {
    assetId,
    contractAddress: order.contractAddress,
    tokenId: order.tokenId,
    issuedId: order.issuedId,
    priceMana,
    priceWei: order.price,
    network: toCardNetwork(order.network),
    chainId: order.chainId,
    marketplaceAddress: order.marketplaceAddress,
    seller: order.owner,
    asset: {
      name,
      rarity: safeRarity(item?.rarity),
      category: wearableDescriptionCategory(item),
      kind: kindFor(category),
      image: item?.thumbnail ?? null,
    },
    buyable: order.status === "open" && !order.buyer,
  };
}

export type ListingLookup = {
  listing: BuyableListing | null;
  source: "catalyst" | "empty" | "unavailable";
  reason?: string;
};

export async function loadBuyableListing(
  contractAddress: string,
  itemId: string,
  opts: GetOptions = {},
): Promise<ListingLookup> {
  const [item, lookup] = await Promise.all([
    fetchCatalogItem(contractAddress, itemId, opts).catch(() => null),
    fetchCheapestOpenOrder(contractAddress, opts),
  ]);
  if (!lookup.order) {
    return { listing: null, source: lookup.source, reason: lookup.reason };
  }
  return { listing: toBuyableListing(lookup.order, item), source: "catalyst" };
}

function priceGtZero(wei: string | null | undefined): boolean {
  if (!wei) return false;
  try {
    return BigInt(wei) > 0n;
  } catch {
    return false;
  }
}

export async function fetchOpenOrderForToken(
  contractAddress: string,
  tokenId: string,
  opts: GetOptions = {},
): Promise<OrderLookup> {
  let res: FetchedOrders;
  try {
    res = await fetchOrders({ contractAddress, tokenId, status: "open", first: 5 }, opts);
  } catch (error) {
    return { order: null, source: "unavailable", reason: message(error) };
  }
  const open = res.data.find(
    (o) =>
      o.status === "open" &&
      !o.buyer &&
      priceGtZero(o.price) &&
      !ensOrderExpired(o.expiresAt),
  );
  if (open) return { order: open, source: "catalyst" };
  if (res.invalid > 0) {
    return { order: null, source: "unavailable", reason: DROPPED_ROWS };
  }
  return { order: null, source: "empty" };
}

export async function loadBuyableNftListing(
  contractAddress: string,
  tokenId: string,
  opts: GetOptions = {},
): Promise<ListingLookup> {
  const [nftRes, lookup] = await Promise.all([
    fetchNft(contractAddress, tokenId, opts).catch(() => null),
    fetchOpenOrderForToken(contractAddress, tokenId, opts),
  ]);
  const order = lookup.order;
  if (!order) {
    return { listing: null, source: lookup.source, reason: lookup.reason };
  }
  const nft = nftRes?.nft ?? null;
  return {
    listing: {
      assetId: `${contractAddress}-${tokenId}`,
      contractAddress,
      tokenId,
      issuedId: order.issuedId,
      priceMana: formatMana(order.price) ?? "0",
      priceWei: order.price,
      network: toCardNetwork(order.network),
      chainId: order.chainId,
      marketplaceAddress: order.marketplaceAddress,
      seller: order.owner,
      asset: {
        name: nft?.name ?? `#${tokenId.slice(0, 8)}`,
        rarity: safeRarity(null),
        category: nft?.category ?? "collectible",
        kind: kindFor(nft?.category),
        image: nft?.image || null,
      },
      buyable: true,
    },
    source: "catalyst",
  };
}
