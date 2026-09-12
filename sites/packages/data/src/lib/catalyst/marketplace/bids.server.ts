import { fetchReceivedBids, type Bid } from "./bids";
import type { GetOptions } from "../client";

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
