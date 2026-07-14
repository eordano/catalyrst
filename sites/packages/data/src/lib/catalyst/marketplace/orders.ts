import { getJSON } from "../client";
import type { GetOptions } from "../client";

import { OrderSchema } from "../generated-schemas/market";
import { parseMarketEnvelope } from "./schema";
import { z } from "zod";
import { warnInvalid } from "../warn";

export { OrderSchema };

export type MarketEnvelope<T> = { data: T; total: number };

/**
 * The wire row, straight from catalyrst-market's `Order`. Every field it sends
 * is required there, so a row missing a price or a chain is not an order with
 * blanks -- it is not an order, and `parseOrder` drops it.
 */
export type Order = z.infer<typeof OrderSchema>;

export function parseOrder(raw: unknown): Order | null {
  const r = OrderSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("Order", r.error.issues);
  return null;
}

export function parseOrders(raw: unknown[]): Order[] {
  const out: Order[] = [];
  for (const row of raw ?? []) {
    const order = parseOrder(row);
    if (order) out.push(order);
  }
  return out;
}

export type FetchOrdersParams = {
  owner?: string;
  status?: string;
  contractAddress?: string;
  tokenId?: string;
  itemId?: string;
  first?: number;
  skip?: number;
};

export async function fetchOrders(
  params: FetchOrdersParams = {},
  opts: GetOptions = {},
): Promise<MarketEnvelope<Order[]>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/orders", {
      ...opts,
      query: {
        owner: params.owner,
        status: params.status,
        contractAddress: params.contractAddress,
        tokenId: params.tokenId,
        itemId: params.itemId,
        first: params.first,
        skip: params.skip,
      },
    }),
  );
  return { data: parseOrders(env.data), total: env.total };
}

const WEI = 1e18;

/**
 * `null` when the wei string cannot be read. "0" would render as **Free** in
 * the asset view, which is what an unreadable price must never be mistaken for.
 */
export function formatOrderMana(wei: string | null | undefined): string | null {
  if (!wei) return null;
  let n: number;
  try {
    n = Number(BigInt(wei)) / WEI;
  } catch {
    const parsed = Number(wei);
    if (!Number.isFinite(parsed)) return null;
    n = parsed / WEI;
  }
  return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

export function orderNetwork(network: string | null | undefined): "ethereum" | "polygon" {
  return network === "ETHEREUM" ? "ethereum" : "polygon";
}

export function shortHex(value: string | null | undefined): string {
  if (!value) return "";
  return value.length > 12 ? `${value.slice(0, 6)}\u{2026}${value.slice(-4)}` : value;
}

export type CancelListing = {
  orderId: string;
  owner: string;
  price: string | null;
  network: "ethereum" | "polygon";
  contractAddress: string;
  tokenId: string;
  issuedId: string | null;
  name: string;
};

export function toCancelListing(order: Order): CancelListing {
  const issued = order.issuedId ?? null;
  const name = issued
    ? `Listing #${issued}`
    : `Listing ${shortHex(order.id)}`;
  return {
    orderId: order.id,
    owner: order.owner,
    price: formatOrderMana(order.price),
    network: orderNetwork(order.network),
    contractAddress: order.contractAddress,
    tokenId: order.tokenId ?? "",
    issuedId: issued,
    name,
  };
}

export type AssetListing = {
  owner: string;
  name: string;
  published: string;
  expires: string;
  issued: number;
  price: string | null;
  credits: string | null;
  listed: boolean;
  contractAddress: string;
  tokenId: string | null;
};

function listingDate(ms: number | null | undefined): string {
  if (!ms) return "";
  const d = new Date(ms);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString();
}

export function toAssetListing(order: Order, credits: string | null = null): AssetListing {
  const issuedNum = Number(order.issuedId ?? "");
  return {
    owner: order.owner,
    name: order.issuedId ? `#${order.issuedId}` : `Listing ${shortHex(order.id)}`,
    published: listingDate(order.createdAt),
    expires: listingDate(order.expiresAt ? order.expiresAt * 1000 : 0),
    issued: Number.isFinite(issuedNum) ? issuedNum : 0,
    price: formatOrderMana(order.price),
    credits,
    listed: order.status === "open",
    contractAddress: order.contractAddress,
    tokenId: order.tokenId,
  };
}
