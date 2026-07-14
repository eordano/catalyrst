import { z } from "zod";

import { dataTotalOf } from "../envelope";
import {
  CatalogItemSchema,
  CollectionSchema,
  ItemSchema,
  NftResultSchema,
  NftSchema,
  OrderSchema,
} from "../generated-schemas/market";
import { warnInvalid } from "../warn";

// The generated schemas are the wire truth; everything below them here is
// explicit post-parse normalization into the UI-facing shapes.
export {
  CatalogItemSchema,
  CollectionSchema,
  ItemSchema,
  NftResultSchema,
  NftSchema,
  OrderSchema,
};

export type MarketEnvelope<T> = { data: T; total: number };

type WireCatalogItem = z.infer<typeof CatalogItemSchema>;
type WireItem = z.infer<typeof ItemSchema>;
type WireNft = z.infer<typeof NftSchema>;
type WireNftResult = z.infer<typeof NftResultSchema>;
type WireCollection = z.infer<typeof CollectionSchema>;
type WireOrder = z.infer<typeof OrderSchema>;

export type CatalogItem = {
  id: string;
  itemId: string;
  name: string | null;
  thumbnail: string | null;
  url: string | null;
  urn: string | null;
  category: string | null;
  contractAddress: string;
  rarity: string | null;
  available: number | null;
  isOnSale: boolean;
  creator: string | null;
  network: string | null;
  chainId: number | null;
  price: string | null;
  minPrice?: string | null;
  minListingPrice?: string | null;
  maxListingPrice?: string | null;
  listings?: number | null;
  createdAt: number | null;
  firstListedAt: number | null;
  data: Record<string, unknown>;
};

export function normalizeCatalogItem(w: WireCatalogItem): CatalogItem {
  return {
    id: w.id,
    itemId: w.itemId,
    name: w.name,
    thumbnail: w.thumbnail,
    url: w.url,
    urn: w.urn,
    category: w.category,
    contractAddress: w.contractAddress,
    rarity: w.rarity,
    available: w.available,
    isOnSale: w.isOnSale,
    creator: w.creator,
    network: w.network,
    chainId: w.chainId,
    price: w.price,
    minPrice: w.minPrice,
    minListingPrice: w.minListingPrice,
    maxListingPrice: w.maxListingPrice,
    listings: w.listings,
    createdAt: w.createdAt,
    firstListedAt: w.firstListedAt,
    data: w.data,
  };
}

/** An `/items` row carries no listing aggregates; those keys stay absent. */
export function normalizeItem(w: WireItem): CatalogItem {
  return {
    id: w.id,
    itemId: w.itemId,
    name: w.name,
    thumbnail: w.thumbnail,
    url: w.url,
    urn: w.urn,
    category: w.category,
    contractAddress: w.contractAddress,
    rarity: w.rarity,
    available: w.available,
    isOnSale: w.isOnSale,
    creator: w.creator,
    network: w.network,
    chainId: w.chainId,
    price: w.price,
    createdAt: w.createdAt,
    firstListedAt: w.firstListedAt ?? null,
    data: w.data,
  };
}

export type Nft = {
  id: string;
  category: string | null;
  name: string | null;
  image: string | null;
  contractAddress: string;
  tokenId: string | null;
  itemId: string | null;
  issuedId: string | null;
  urn: string | null;
  network: string | null;
  chainId: number | null;
  owner: string | null;
  url: string | null;
  activeOrderId: string | null;
  openRentalId: string | null;
  createdAt: number | null;
  updatedAt: number | null;
  data: Record<string, unknown>;
};

export type OwnedNft = Nft;

export function normalizeNft(w: WireNft): Nft {
  return {
    id: w.id,
    category: w.category,
    name: w.name,
    image: w.image,
    contractAddress: w.contractAddress,
    tokenId: w.tokenId,
    itemId: w.itemId,
    issuedId: w.issuedId,
    urn: w.urn ?? null,
    network: w.network,
    chainId: w.chainId,
    owner: w.owner,
    url: w.url,
    activeOrderId: w.activeOrderId,
    openRentalId: w.openRentalId,
    createdAt: w.createdAt,
    updatedAt: w.updatedAt,
    data: w.data as Record<string, unknown>,
  };
}

export type Order = {
  id: string;
  contractAddress: string;
  tokenId: string | null;
  owner: string | null;
  buyer: string | null;
  price: string | null;
  status: string | null;
  expiresAt: number | null;
  createdAt: number | null;
  updatedAt: number | null;
  network: string | null;
  chainId: number | null;
  issuedId: string | null;
};

export function normalizeOrder(w: WireOrder): Order {
  return {
    id: w.id,
    contractAddress: w.contractAddress,
    tokenId: w.tokenId,
    owner: w.owner,
    buyer: w.buyer,
    price: w.price,
    status: w.status,
    expiresAt: w.expiresAt,
    createdAt: w.createdAt,
    updatedAt: w.updatedAt,
    network: w.network,
    chainId: w.chainId,
    issuedId: w.issuedId,
  };
}

export type NftResult = {
  nft: Nft;
  order: Order | null;
  rental: unknown;
};

/** `/nfts` rows are one wire shape; these views differ only in what the UI reads. */
export type EnsResult = NftResult;
export type OwnedAsset = NftResult;

export function normalizeNftResult(w: WireNftResult): NftResult {
  return {
    nft: normalizeNft(w.nft),
    order: w.order ? normalizeOrder(w.order) : null,
    rental: w.rental,
  };
}

export type Collection = {
  urn: string;
  contractAddress: string;
  name: string | null;
  creator: string | null;
  size: number | null;
  isOnSale: boolean;
  network: string | null;
  chainId: number | null;
  createdAt: number | null;
  updatedAt: number | null;
  reviewedAt: number | null;
  firstListedAt: number | null;
};

export function normalizeCollection(w: WireCollection): Collection {
  return {
    urn: w.urn,
    contractAddress: w.contractAddress,
    name: w.name,
    creator: w.creator,
    size: w.size,
    isOnSale: w.isOnSale,
    network: w.network,
    chainId: w.chainId,
    createdAt: w.createdAt,
    updatedAt: w.updatedAt,
    reviewedAt: w.reviewedAt,
    firstListedAt: w.firstListedAt,
  };
}

export const MarketEnvelopeSchema = dataTotalOf(z.unknown());

export function parseMarketEnvelope(raw: unknown): MarketEnvelope<unknown[]> {
  const r = MarketEnvelopeSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("MarketEnvelope", r.error.issues);
  const o = (typeof raw === "object" && raw !== null ? raw : {}) as {
    data?: unknown;
    total?: unknown;
  };
  return {
    data: Array.isArray(o.data) ? o.data : [],
    total: typeof o.total === "number" ? o.total : 0,
  };
}

/**
 * `null` means the row failed validation against the wire schema; callers must
 * drop it (and decide whether "everything dropped" is an error), never render
 * an unvalidated row as if it were real.
 */
export function parseCatalogItem(raw: unknown): CatalogItem | null {
  const cat = CatalogItemSchema.safeParse(raw);
  if (cat.success) return normalizeCatalogItem(cat.data);
  const item = ItemSchema.safeParse(raw);
  if (item.success) return normalizeItem(item.data);
  warnInvalid("CatalogItem", cat.error.issues);
  return null;
}

export function parseCatalogItems(raw: unknown[]): CatalogItem[] {
  const out: CatalogItem[] = [];
  for (const r of raw ?? []) {
    const item = parseCatalogItem(r);
    if (item) out.push(item);
  }
  return out;
}

function parseNftRow(kind: string, raw: unknown): NftResult | null {
  const r = NftResultSchema.safeParse(raw);
  if (r.success) return normalizeNftResult(r.data);
  warnInvalid(kind, r.error.issues);
  return null;
}

export function parseNftResult(raw: unknown): NftResult | null {
  return parseNftRow("NftResult", raw);
}

export function parseNftResults(raw: unknown[]): NftResult[] {
  const out: NftResult[] = [];
  for (const r of raw ?? []) {
    const row = parseNftResult(r);
    if (row) out.push(row);
  }
  return out;
}

export function parseEnsResult(raw: unknown): EnsResult | null {
  return parseNftRow("EnsResult", raw);
}

export function parseEnsResults(raw: unknown[]): EnsResult[] {
  const out: EnsResult[] = [];
  for (const r of raw ?? []) {
    const row = parseEnsResult(r);
    if (row) out.push(row);
  }
  return out;
}

export function parseCollection(raw: unknown): Collection | null {
  const r = CollectionSchema.safeParse(raw);
  if (r.success) return normalizeCollection(r.data);
  warnInvalid("Collection", r.error.issues);
  return null;
}

export function parseOwnedAsset(raw: unknown): OwnedAsset | null {
  return parseNftRow("OwnedAsset", raw);
}

export function parseOwnedAssets(raw: unknown[]): OwnedAsset[] {
  const out: OwnedAsset[] = [];
  for (const r of raw ?? []) {
    const row = parseOwnedAsset(r);
    if (row) out.push(row);
  }
  return out;
}
