import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { parseCatalogItem, parseCatalogItems, parseCollection, parseMarketEnvelope, parseNftResult, parseOwnedAsset, parseOwnedAssets, type CatalogItem, type Collection, type EnsResult, type MarketEnvelope, type NftResult, type OwnedAsset } from "./schema";

import { formatMana } from "./money";

export type { CatalogItem, Collection, NftResult, Nft, OwnedAsset, Order, EnsResult } from "./schema";
export { formatMana };

export type FetchCatalogParams = {
  first?: number;
  skip?: number;
  category?: string;
  rarity?: string;
  isOnSale?: boolean;
  sortBy?: string;
  search?: string;
  network?: string;
};

export async function fetchCatalog(
  params: FetchCatalogParams = {},
  opts: GetOptions = {},
): Promise<MarketEnvelope<CatalogItem[]>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/catalog", {
      ...opts,
      query: {
        first: params.first,
        skip: params.skip,
        category: params.category,
        rarity: params.rarity,
        isOnSale: params.isOnSale ? "true" : undefined,
        sortBy: params.sortBy,
        search: params.search,
        network: params.network,
      },
    }),
  );
  return { data: parseCatalogItems(env.data), total: env.total };
}

export async function fetchCatalogItem(
  contractAddress: string,
  itemId: string,
  opts: GetOptions = {},
): Promise<CatalogItem | null> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/catalog", {
      ...opts,
      query: { contractAddress, itemId, first: 1 },
    }),
  );
  const row = env.data[0];
  return row ? parseCatalogItem(row) : null;
}

export async function fetchNft(
  contractAddress: string,
  tokenId: string,
  opts: GetOptions = {},
): Promise<NftResult | null> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/nfts", {
      ...opts,
      query: { contractAddress, tokenId, first: 1 },
    }),
  );
  const row = env.data[0];
  return row ? parseNftResult(row) : null;
}

export type FetchEnsParams = { first?: number; skip?: number; onSale?: boolean };

export async function fetchOwnedAsset(
  owner: string,
  contractAddress: string,
  tokenId: string,
  opts: GetOptions = {},
): Promise<OwnedAsset | null> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/nfts", {
      ...opts,
      query: { owner, contractAddress, tokenId, first: 1 },
    }),
  );
  const row = env.data[0];
  return row ? parseOwnedAsset(row) : null;
}

export async function fetchOwnedAssets(
  owner: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<MarketEnvelope<OwnedAsset[]>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/nfts", {
      ...opts,
      query: { owner, first: params.first, skip: params.skip },
    }),
  );
  return { data: parseOwnedAssets(env.data), total: env.total };
}

export async function fetchCollection(
  contractAddress: string,
  opts: GetOptions = {},
): Promise<Collection | null> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/collections", {
      ...opts,
      query: { contractAddress, first: 1 },
    }),
  );
  const row = env.data[0];
  return row ? parseCollection(row) : null;
}

export type FetchCollectionItemsParams = {
  first?: number;
  skip?: number;
  sortBy?: string;
};

export async function fetchCollectionItems(
  contractAddress: string,
  params: FetchCollectionItemsParams = {},
  opts: GetOptions = {},
): Promise<MarketEnvelope<CatalogItem[]>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/items", {
      ...opts,
      query: {
        contractAddress,
        first: params.first,
        skip: params.skip,
        sortBy: params.sortBy,
      },
    }),
  );
  return { data: parseCatalogItems(env.data), total: env.total };
}

export function parseItemId(
  id: string,
): { contractAddress: string; itemId: string } | null {
  const dash = id.lastIndexOf("-");
  if (dash <= 0 || dash === id.length - 1) return null;
  return { contractAddress: id.slice(0, dash), itemId: id.slice(dash + 1) };
}

export function toCardNetwork(network: string | null | undefined): "ethereum" | "polygon" {
  return network === "ETHEREUM" ? "ethereum" : "polygon";
}

export function catalogManaPrice(item: CatalogItem): string | null {
  const listing = formatMana(item.minListingPrice);
  if (listing) return listing;
  if (!item.isOnSale) return null;
  return formatMana(item.price) ?? "0";
}

function weiGtZero(wei: string | null | undefined): boolean {
  if (!wei) return false;
  try {
    return BigInt(wei) > 0n;
  } catch {
    const n = Number(wei);
    return Number.isFinite(n) && n > 0;
  }
}

export function isCatalogItemBuyable(item: CatalogItem): boolean {
  return weiGtZero(item.minListingPrice) || (item.isOnSale && weiGtZero(item.price));
}

export function isEnsBuyable(result: EnsResult): boolean {
  return ensOnSale(result) && weiGtZero(result.order?.price);
}

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

export type CollectibleCard = {
  id: string;
  name: string;
  collection: string | undefined;
  price: string | null;
  credits: string | null;
  rarity: string;
  network: "ethereum" | "polygon";
  image: string | undefined;
};

export function toCollectibleCard(
  item: CatalogItem,
  credits: string | null = null,
): CollectibleCard {
  return {
    id: item.id,
    name: item.name ?? "Untitled",
    collection: item.category ?? undefined,
    price: catalogManaPrice(item),
    credits,
    rarity: safeRarity(item.rarity),
    network: toCardNetwork(item.network),
    image: item.thumbnail ?? undefined,
  };
}

export type CreationItem = {
  id: string;
  name: string;
  creator: string;
  price: string | null;
  rarity: string;
  category: string | null;
  body: string | null;
  smart: boolean;
};

export type Creations = { wearables: CreationItem[]; emotes: CreationItem[] };

function wearableBodyShape(item: CatalogItem): string | null {
  const data = item.data as Record<string, unknown>;
  const w = data?.wearable as Record<string, unknown> | undefined;
  const shapes = w?.bodyShapes;
  if (!Array.isArray(shapes) || shapes.length === 0) return null;
  const strs = shapes.filter((s): s is string => typeof s === "string");
  const female = strs.some((s) => /female/i.test(s));
  const male = strs.some((s) => /male/i.test(s) && !/female/i.test(s));
  if (male && female) return "unisex";
  if (female) return "female";
  if (male) return "male";
  return "unisex";
}

export function toCreationItem(item: CatalogItem, creatorName?: string): CreationItem {
  const isEmote = item.category === "emote";
  const data = item.data as Record<string, unknown>;
  const wearable = data?.wearable as Record<string, unknown> | undefined;
  const subCategory =
    typeof wearable?.category === "string" ? wearable.category : item.category;
  return {
    id: item.id,
    name: item.name ?? "Untitled",
    creator: creatorName?.trim() || shortAddress(item.creator),
    price: catalogManaPrice(item),
    rarity: safeRarity(item.rarity),
    category: isEmote ? null : subCategory ?? null,
    body: isEmote ? null : wearableBodyShape(item),
    smart: wearable?.isSmart === true,
  };
}

export function toCreations(items: CatalogItem[], creatorName?: string): Creations {
  const wearables: CreationItem[] = [];
  const emotes: CreationItem[] = [];
  for (const it of items) {
    const card = toCreationItem(it, creatorName);
    if (it.category === "emote") emotes.push(card);
    else wearables.push(card);
  }
  return { wearables, emotes };
}

export async function fetchCreations(
  creator: string,
  params: { first?: number; creatorName?: string } = {},
  opts: GetOptions = {},
): Promise<Creations> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/items", {
      ...opts,
      query: { creator: creator.toLowerCase(), first: params.first ?? 48 },
    }),
  );
  return toCreations(parseCatalogItems(env.data), params.creatorName);
}

function parcelCoords(nft: NftResult["nft"]): { x: number; y: number } | null {
  const data = nft.data as Record<string, unknown>;
  const parcel = data?.parcel as Record<string, unknown> | undefined;
  if (!parcel) return null;
  const x = Number(parcel.x);
  const y = Number(parcel.y);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  return { x, y };
}

function estateSize(nft: NftResult["nft"]): number | null {
  const data = nft.data as Record<string, unknown>;
  const estate = data?.estate as Record<string, unknown> | undefined;
  if (!estate) return null;
  const parcels = estate.parcels;
  if (Array.isArray(parcels)) return parcels.length;
  const size = Number(estate.size);
  return Number.isFinite(size) ? size : null;
}

export type LandCard = {
  id: string;
  name: string;
  network: "ethereum" | "polygon";
  image: string | undefined;
  metaRight: string;
  price: null;
};

export function ensSubdomain(result: EnsResult): string {
  const data = result.nft.data as Record<string, unknown>;
  const ens = data?.ens as Record<string, unknown> | undefined;
  const sub = ens?.subdomain;
  if (typeof sub === "string" && sub.trim()) return sub.trim();
  return result.nft.name ?? "Unnamed";
}

export function ensOnSale(result: EnsResult): boolean {
  const o = result.order;
  return !!o && (o.status == null || o.status === "open");
}

export type NameCard = {
  id: string;
  name: string;
  price: string | null;
};

export function catalogDescription(item: CatalogItem): string {
  const data = item.data as Record<string, unknown>;
  for (const key of ["wearable", "emote"]) {
    const sub = data?.[key] as Record<string, unknown> | undefined;
    const d = sub?.description;
    if (typeof d === "string" && d.trim()) return d.trim();
  }
  return "";
}

export type AssetDetail = {
  name: string;
  image: string | undefined;
  issuedId: number;
  category: string;
  kind: "wearable" | "emote";
  rarity: string;
  bodyShape: string;
  isSmart: boolean;
  network: "ethereum" | "polygon";
  description: string;
  owner: { address: string; name: string };
  collection: { name: string; address: string };
  order: {
    price: string;
    credits: string | null;
    source: "mint" | "listing";
    issuedId: number;
    expiresLabel: string;
  } | null;
};

export function toAssetDetail(item: CatalogItem): AssetDetail {
  const data = item.data as Record<string, unknown>;
  const wearable = data?.wearable as Record<string, unknown> | undefined;
  const emote = data?.emote as Record<string, unknown> | undefined;
  const sub = wearable ?? emote;
  const subCategory = typeof sub?.category === "string" ? sub.category : item.category ?? "";
  const isSmart = wearable?.isSmart === true;
  const price = catalogManaPrice(item);
  const mint = item.isOnSale && weiGtZero(item.price);

  return {
    name: item.name ?? "Untitled",
    image: item.thumbnail ?? undefined,
    issuedId: 0,
    category: subCategory,
    kind: emote ? "emote" : "wearable",
    rarity: safeRarity(item.rarity),
    bodyShape: "Unisex",
    isSmart,
    network: toCardNetwork(item.network),
    description: catalogDescription(item),
    owner: { address: item.creator ?? "", name: "" },
    collection: { name: item.category ?? "Decentraland", address: item.contractAddress },
    order: price
      ? {
          price,
          credits: null,
          source: mint ? "mint" : "listing",
          issuedId: 0,
          expiresLabel: "Listed on the marketplace",
        }
      : null,
  };
}

export function nftToAssetDetail(result: NftResult): AssetDetail {
  const nft = result.nft;
  const isEstate = nft.category === "estate";
  let description = "";
  const data = nft.data as Record<string, unknown>;
  const inner = (data?.[isEstate ? "estate" : "parcel"] as Record<string, unknown>) ?? {};
  if (typeof inner.description === "string") description = inner.description.trim();
  if (!description) {
    const c = parcelCoords(nft);
    description = isEstate
      ? `A Decentraland estate of ${estateSize(nft) ?? "several"} parcels.`
      : c
        ? `A Decentraland LAND parcel at ${c.x},${c.y}.`
        : "A Decentraland LAND parcel.";
  }
  return {
    name: nft.name ?? (isEstate ? "Estate" : "Parcel"),
    image: nft.image ?? undefined,
    issuedId: 0,
    category: nft.category ?? "parcel",
    kind: "wearable",
    rarity: "common",
    bodyShape: isEstate ? "Estate" : "Parcel",
    isSmart: false,
    network: toCardNetwork(nft.network),
    description,
    owner: { address: nft.owner ?? "", name: "" },
    collection: { name: "Decentraland LAND", address: nft.contractAddress },
    order: null,
  };
}

function networkLabel(network: string | null | undefined): string {
  return network === "ETHEREUM" ? "Ethereum" : "Polygon";
}

function shortAddress(addr: string | null | undefined): string {
  if (!addr) return "";
  return addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;
}

function formatDate(epochSeconds: number | null | undefined): string {
  if (!epochSeconds) return "";
  const d = new Date(epochSeconds * 1000);
  if (Number.isNaN(d.getTime())) return "";
  const months = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  return `${months[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`;
}

function wearableMeta(nft: OwnedAsset["nft"]): Record<string, unknown> | undefined {
  const data = nft.data as Record<string, unknown>;
  const w = data?.wearable as Record<string, unknown> | undefined;
  const e = data?.emote as Record<string, unknown> | undefined;
  return w ?? e;
}

export type ManageAsset = {
  id: string;
  contractAddress: string;
  tokenId: string;
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
  networkLabel: string;
  ownerAddress: string;
  ownerShort: string;
  image: string | null;
  description: string;
  order: { id: string; price: string; expiresAt: string; status: string } | null;
  rentalActive: boolean;
};

export function toManageAsset(row: OwnedAsset): ManageAsset {
  const nft = row.nft;
  const meta = wearableMeta(nft);
  const rarity = safeRarity(
    (typeof meta?.rarity === "string" ? meta.rarity : null) ?? null,
  );
  const description =
    (typeof meta?.description === "string" && meta.description.trim()) || "";
  const price = row.order ? formatMana(row.order.price) : null;

  return {
    id: nft.id,
    contractAddress: nft.contractAddress,
    tokenId: nft.tokenId ?? "",
    name: nft.name ?? "Untitled",
    category: nft.category ?? "collectible",
    rarity,
    network: toCardNetwork(nft.network),
    networkLabel: networkLabel(nft.network),
    ownerAddress: nft.owner ?? "",
    ownerShort: shortAddress(nft.owner),
    image: nft.image ?? null,
    description,
    order:
      row.order && price
        ? {
            id: row.order.id,
            price,
            expiresAt: formatDate(row.order.expiresAt),
            status: row.order.status ?? "open",
          }
        : null,
    rentalActive: row.rental != null,
  };
}

function itemSubcategory(item: CatalogItem): string {
  const data = item.data as Record<string, unknown>;
  for (const key of ["wearable", "emote"]) {
    const sub = data?.[key] as Record<string, unknown> | undefined;
    if (typeof sub?.category === "string" && sub.category.trim()) {
      return sub.category.trim();
    }
  }
  return item.category ?? "";
}

function collectionItemPrice(item: CatalogItem): string {
  return formatMana(item.price) ?? formatMana(item.minListingPrice) ?? "\u{2014}";
}

export type CollectionItemRow = {
  id: string;
  name: string;
  category: string;
  sub: string;
  rarity: string;
  available: number;
  price: string;
  credits: string | null;
  image: string | null;
};

export function toCollectionItemRow(
  item: CatalogItem,
  credits: string | null = null,
): CollectionItemRow {
  return {
    id: item.id,
    name: item.name ?? "Untitled",
    category: item.category === "emote" ? "emote" : "wearable",
    sub: itemSubcategory(item),
    rarity: safeRarity(item.rarity),
    available: typeof item.available === "number" ? item.available : 0,
    price: collectionItemPrice(item),
    credits,
    image: item.thumbnail ?? null,
  };
}

export type CollectionHeader = { name: string; isOnSale: boolean };

export function toCollectionHeader(collection: Collection): CollectionHeader {
  return {
    name: (collection.name ?? "Untitled Collection").trim() || "Untitled Collection",
    isOnSale: collection.isOnSale === true,
  };
}

export type CollectionStats = {
  floor: string | null;
  floorCredits: string | null;
  creator: string;
  creatorShort: string;
  itemCount: number;
  network: "ethereum" | "polygon";
};

export function toCollectionStats(
  collection: Collection | null,
  items: CatalogItem[],
  creditsFor: (item: CatalogItem) => string | null = () => null,
): CollectionStats {
  let floorWei: bigint | null = null;
  let floorItem: CatalogItem | null = null;
  for (const it of items) {
    if (!it.price) continue;
    let wei: bigint;
    try {
      wei = BigInt(it.price);
    } catch {
      continue;
    }
    if (wei <= 0n) continue;
    if (floorWei == null || wei < floorWei) {
      floorWei = wei;
      floorItem = it;
    }
  }
  const creator = collection?.creator ?? "";
  return {
    floor: floorWei != null ? formatMana(floorWei.toString()) : null,
    floorCredits: floorItem ? creditsFor(floorItem) : null,
    creator,
    creatorShort: shortAddress(creator),
    itemCount: items.length,
    network: toCardNetwork(collection?.network),
  };
}
