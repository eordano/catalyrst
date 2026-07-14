import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { BidsEnvelopeSchema } from "../generated-schemas/market";
import type { Bid as WireBid } from "./bid";
import { formatMana } from "./money";
import { warnInvalid } from "../warn";

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * UI view-model for a received bid. The wire row (`WireBid`, the generated
 * schema in ./bid) carries none of the display fields below; `normalizeBid`
 * derives them explicitly.
 */
export type BidAsset = {
  id: string;
  name: string;
  issuedId: number;
  category: string;
  rarity: string;
  bodyShape: string;
  isSmart: boolean;
  network: "ethereum" | "polygon";
  description: string;
  thumbnail: string | null;
  owner: { address: string; name: string };
  collection: { name: string; address: string };
  order: null;
};

export type Bid = {
  id: string;
  bidder: string;
  bidderName: string;
  seller: string;
  price: string;
  priceMana: string;
  status: string;
  expiresAt: number;
  createdAt: number;
  contractAddress: string;
  tokenId: string;
  network: string;
  createdRelative: string;
  timeLeft: string;
  asset: BidAsset;
};

function shortHex(v: string): string {
  return v.length > 12 ? `${v.slice(0, 6)}\u{2026}${v.slice(-4)}` : v;
}

function createdRelativeLabel(createdAtMs: number, now: number): string {
  const days = Math.max(0, Math.round((now - createdAtMs) / 86_400_000));
  if (days <= 0) return "today";
  if (days === 1) return "1 day ago";
  return `${days} days ago`;
}

function timeLeftLabel(expiresAtMs: number, now: number): string {
  if (expiresAtMs <= now) return "expired";
  const days = Math.ceil((expiresAtMs - now) / 86_400_000);
  return `${days} ${days === 1 ? "day" : "days"}`;
}

/**
 * Wire row -> UI view-model. The bid names its target only as
 * contract + token/item id, so the asset block is a placeholder card
 * (label from ids, neutral rarity) -- the wire has no name or thumbnail
 * for it.
 */
export function normalizeBid(row: WireBid, now = Date.now()): Bid {
  const token = row.tokenId ?? row.itemId ?? null;
  const assetName = token ? `Token #${shortHex(token)}` : shortHex(row.contractAddress);
  return {
    id: row.id,
    bidder: row.bidder,
    bidderName: "",
    seller: row.seller,
    price: row.price,
    priceMana: formatMana(row.price) ?? "0",
    status: row.status,
    expiresAt: row.expiresAt,
    createdAt: row.createdAt,
    contractAddress: row.contractAddress,
    tokenId: row.tokenId ?? "",
    network: row.network,
    createdRelative: createdRelativeLabel(row.createdAt, now),
    timeLeft: timeLeftLabel(row.expiresAt, now),
    asset: {
      id: `${row.contractAddress}-${token ?? "unknown"}`,
      name: assetName,
      issuedId: 0,
      category: "wearable",
      rarity: "common",
      bodyShape: "Unisex",
      isSmart: false,
      network: row.network === "ETHEREUM" ? "ethereum" : "polygon",
      description: "",
      thumbnail: null,
      owner: { address: row.seller, name: "" },
      collection: { name: "", address: row.contractAddress },
      order: null,
    },
  };
}

/**
 * "live" -- these are the seller's open bids.
 * "empty" -- the node answered and nobody has bid.
 * "unavailable" -- the read failed, so we do not know whether anyone has bid.
 *   Telling a seller "nobody has bid" off the back of this hides real money.
 */
export type ReceivedBids = {
  bids: Bid[];
  source: "live" | "empty" | "unavailable";
  reason?: string;
};

export async function fetchReceivedBids(
  seller: string,
  opts: GetOptions = {},
): Promise<ReceivedBids> {
  let env: unknown;
  try {
    env = await getJSON<unknown>("/market/v1/bids", {
      ...opts,
      query: { seller, status: "open", first: 24 },
    });
  } catch (error) {
    return { bids: [], source: "unavailable", reason: message(error) };
  }

  const parsed = BidsEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    warnInvalid("BidsEnvelope", parsed.error.issues);
    return {
      bids: [],
      source: "unavailable",
      reason: "the bids response did not match the bids-page shape",
    };
  }

  const bids = parsed.data.data.results.map((row) => normalizeBid(row));
  return { bids, source: bids.length ? "live" : "empty" };
}

export type AcceptResult = { txHash: string; bidId: string };

export type AcceptFn = (args: {
  bid: Pick<Bid, "id" | "contractAddress" | "tokenId" | "price"> & {
    tradeId?: string;
  };
  signal?: AbortSignal;
}) => Promise<AcceptResult>;

export const simulateAccept: AcceptFn = async ({ bid, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const hex = bid.id.replace(/[^0-9a-f]/gi, "").toLowerCase();
  const body = `acce97${hex}`.padEnd(64, "0").slice(0, 64);
  return { txHash: `0x${body}`, bidId: bid.id };
};
