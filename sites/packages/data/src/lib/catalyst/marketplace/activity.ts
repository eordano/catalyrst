import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { formatMana } from "./money";

export { formatMana };
import {
  parseSales,
  parseTrades,
  TradesEnvelopeSchema,
  type Sale,
  type SalesEnvelope,
  type Trade,
} from "./activity-schema";
import { parseMarketEnvelope } from "./schema";
import { shortAddress } from "../format/address";

export type { Sale, Trade } from "./activity-schema";

export const ACTIVITY_TYPES = ["sale", "listing", "bid"] as const;
export type ActivityType = (typeof ACTIVITY_TYPES)[number];

export const TRADES_HARD_CAP = 100;

export type FetchSalesParams = {
  first?: number;
  skip?: number;
  type?: string;
  seller?: string;
  buyer?: string;
  contractAddress?: string;
  from?: number;
  to?: number;
};

export async function fetchSales(
  params: FetchSalesParams = {},
  opts: GetOptions = {},
): Promise<SalesEnvelope> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/sales", {
      ...opts,
      query: {
        first: params.first,
        skip: params.skip,
        type: params.type,
        seller: params.seller,
        buyer: params.buyer,
        contractAddress: params.contractAddress,
        from: params.from,
        to: params.to,
      },
    }),
  );
  return { data: parseSales(env.data), total: env.total };
}

export const SALES_CONTRACT_FANOUT_CAP = 60;
export const SALES_PAGE_MAX = 1000;

export type FetchSalesByContractsParams = {
  contractAddresses: string[];
  from?: number;
  to?: number;
  first?: number;
};

export async function fetchSalesByContracts(
  params: FetchSalesByContractsParams,
  opts: GetOptions = {},
): Promise<SalesEnvelope> {
  const contracts = [
    ...new Set(
      params.contractAddresses
        .map((c) => (c ?? "").trim().toLowerCase())
        .filter((c) => c.length > 0),
    ),
  ].slice(0, SALES_CONTRACT_FANOUT_CAP);
  if (contracts.length === 0) return { data: [], total: 0 };

  const first = Math.min(params.first ?? SALES_PAGE_MAX, SALES_PAGE_MAX);
  const results = await Promise.all(
    contracts.map((contractAddress) =>
      fetchSales(
        { first, contractAddress, from: params.from, to: params.to },
        opts,
      ),
    ),
  );
  return {
    data: results.flatMap((r) => r.data),
    total: results.reduce((sum, r) => sum + r.total, 0),
  };
}

export type FetchTradesParams = {
  first?: number;
  skip?: number;
};

export async function fetchTrades(
  params: FetchTradesParams = {},
  opts: GetOptions = {},
): Promise<{ data: Trade[]; total: number }> {
  const first = Math.min(Math.max(params.first ?? 25, 1), TRADES_HARD_CAP);
  const skip = Math.max(params.skip ?? 0, 0);
  const raw = await getJSON<unknown>("/market/v1/trades", {
    ...opts,
    query: {
      first,
      skip,
    },
  });
  const env = TradesEnvelopeSchema.safeParse(raw);
  const inner = (env.success ? env.data.data : null) ?? { data: [], count: 0 };
  const rows = inner.data;
  const total = inner.count ?? rows.length;
  const windowed = rows.length > first ? rows.slice(skip, skip + first) : rows;
  return { data: parseTrades(windowed), total };
}

export function toNetwork(network: string | null | undefined): "ethereum" | "polygon" {
  return network === "ETHEREUM" ? "ethereum" : "polygon";
}

export function shortAddr(addr: string | null | undefined): string {
  return addr ? shortAddress(addr) : "";
}

export type ActivityEntry = {
  id: string;
  kind: ActivityType;
  rawType: string;
  price: string | null;
  network: "ethereum" | "polygon";
  from: string;
  to: string;
  timestamp: number;
  txHash: string | null;
  contractAddress: string | null;
};

function saleKind(type: string | null): ActivityType {
  return type === "bid" ? "bid" : "sale";
}

function tradeKind(type: string | null): ActivityType {
  return type === "bid" ? "bid" : "listing";
}

export function saleToEntry(s: Sale): ActivityEntry {
  return {
    id: s.id,
    kind: saleKind(s.type),
    rawType: s.type ?? "sale",
    price: formatMana(s.price),
    network: toNetwork(s.network),
    from: shortAddr(s.seller),
    to: shortAddr(s.buyer),
    timestamp: s.timestamp ?? 0,
    txHash: s.txHash ?? null,
    contractAddress: s.contractAddress,
  };
}

export function tradeToEntry(t: Trade): ActivityEntry {
  const ts = t.created_at ? Date.parse(t.created_at) : NaN;
  return {
    id: t.id,
    kind: tradeKind(t.type),
    rawType: t.type ?? "listing",
    price: null,
    network: toNetwork(t.network),
    from: shortAddr(t.signer),
    to: "",
    timestamp: Number.isFinite(ts) ? ts : 0,
    txHash: null,
    contractAddress: t.contract ?? null,
  };
}

export function buildFeed(
  sales: Sale[],
  trades: Trade[],
  type?: ActivityType,
): ActivityEntry[] {
  const entries = [...sales.map(saleToEntry), ...trades.map(tradeToEntry)];
  const filtered = type ? entries.filter((e) => e.kind === type) : entries;
  return filtered.sort((a, b) => b.timestamp - a.timestamp);
}
