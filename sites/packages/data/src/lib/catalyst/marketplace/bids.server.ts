import { fetchReceivedBids, type Bid } from "./bids";
import type { GetOptions } from "../client";

/**
 * "live" -- `bids` are the seller's open bids.
 * "empty" -- we asked and nobody has bid.
 * "unavailable" -- the read failed, so we do not know. A seller shown "nobody
 *   has bid" here would walk away from offers that may exist; `reason` is safe
 *   to show them instead.
 */
export type LoadedBids = {
  bids: Bid[];
  source: "live" | "empty" | "unavailable";
  reason?: string;
};

export async function loadReceivedBids(
  owner: string | null,
  opts: GetOptions = {},
): Promise<LoadedBids> {
  if (!owner) return { bids: [], source: "empty" };
  return fetchReceivedBids(owner, opts);
}

export function findBid(bids: Bid[], bidId: string | null | undefined): Bid | null {
  if (!bidId) return null;
  return bids.find((b) => b.id === bidId) ?? null;
}
