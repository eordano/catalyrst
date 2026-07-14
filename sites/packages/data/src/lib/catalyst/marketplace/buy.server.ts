import type { GetOptions } from "../client";
import {
  loadBuyableListing,
  loadBuyableNftListing,
  fetchOrders,
  type BuyableListing,
} from "./buy";

export type LoadBuyArgs = {
  itemId?: string;
  nftId?: string;
  opts?: GetOptions;
};

/**
 * "unavailable" means the buy page could not read the listing, so it must not
 * claim the item is sold or gone. `reason` is safe to show to a visitor.
 */
export type LoadBuyResult = {
  listing: BuyableListing | null;
  source: "catalyst" | "empty" | "unavailable";
  reason?: string;
  itemId: string;
};

type DefaultItem = { itemId: string; failed: boolean; reason?: string };

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function defaultBuyableItemId(opts?: GetOptions): Promise<DefaultItem> {
  try {
    const { data, invalid } = await fetchOrders(
      { status: "open", sortBy: "cheapest", first: 1 },
      opts ?? {},
    );
    const order = data[0];
    if (order) return { itemId: `${order.contractAddress}-0`, failed: false };
    if (invalid > 0) {
      return {
        itemId: "",
        failed: true,
        reason: "the marketplace returned listings we could not validate",
      };
    }
    return { itemId: "", failed: false };
  } catch (error) {
    return { itemId: "", failed: true, reason: message(error) };
  }
}

function splitAssetId(id: string): { contractAddress: string; inner: string } {
  const dash = id.lastIndexOf("-");
  return {
    contractAddress: dash > 0 ? id.slice(0, dash) : "",
    inner: dash > 0 ? id.slice(dash + 1) : "",
  };
}

export async function loadBuyListing(args: LoadBuyArgs = {}): Promise<LoadBuyResult> {
  const nftId = args.nftId?.trim() || "";
  if (nftId) {
    const { contractAddress, inner: tokenId } = splitAssetId(nftId);
    if (!contractAddress || !tokenId) {
      return { listing: null, source: "empty", itemId: nftId };
    }
    try {
      const found = await loadBuyableNftListing(contractAddress, tokenId, args.opts);
      return { ...found, itemId: nftId };
    } catch (error) {
      return {
        listing: null,
        source: "unavailable",
        reason: message(error),
        itemId: nftId,
      };
    }
  }

  let itemId = args.itemId?.trim() || "";
  if (!itemId) {
    const fallback = await defaultBuyableItemId(args.opts);
    if (fallback.failed) {
      return {
        listing: null,
        source: "unavailable",
        reason: fallback.reason,
        itemId: "",
      };
    }
    itemId = fallback.itemId;
  }

  const { contractAddress, inner: innerItemId } = splitAssetId(itemId);
  if (!contractAddress || !innerItemId) {
    return { listing: null, source: "empty", itemId };
  }

  try {
    const found = await loadBuyableListing(contractAddress, innerItemId, args.opts);
    return { ...found, itemId };
  } catch (error) {
    return { listing: null, source: "unavailable", reason: message(error), itemId };
  }
}
