import { fetchOrders, toCancelListing, type Order, type CancelListing } from "./orders";
import type { GetOptions } from "../client";

export type LoadCancelArgs = {
  owner?: string;
  orderId?: string;
  opts?: GetOptions;
};

/**
 * "catalyst" -- these are the seller's open listings.
 * "empty" -- we asked and the seller has none open.
 * "unavailable" -- the read failed, so we do not know. "No active listing to
 *   cancel" would tell a seller their item is already off the market when it
 *   may still be listed and selling.
 */
export type LoadCancelResult = {
  listing: CancelListing | null;
  orders: CancelListing[];
  source: "catalyst" | "empty" | "unavailable";
  reason?: string;
  owner: string | null;
};

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function loadCancelListing(args: LoadCancelArgs = {}): Promise<LoadCancelResult> {
  const owner = args.owner?.trim().toLowerCase() || null;
  if (!owner) return { listing: null, orders: [], source: "empty", owner: null };

  let rows: Order[] = [];
  try {
    const env = await fetchOrders({ owner, status: "open", first: 24 }, args.opts);
    rows = env.data;
  } catch (error) {
    return {
      listing: null,
      orders: [],
      source: "unavailable",
      reason: message(error),
      owner,
    };
  }

  const orders = rows.map(toCancelListing);
  const listing =
    (args.orderId
      ? orders.find((o) => o.orderId.toLowerCase() === args.orderId!.toLowerCase())
      : undefined) ??
    orders[0] ??
    null;

  return { listing, orders, source: rows.length ? "catalyst" : "empty", owner };
}
