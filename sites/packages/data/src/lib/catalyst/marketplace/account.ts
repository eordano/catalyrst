import { CatalystError, catalystBase, getJSON } from "../client";
import type { GetOptions } from "../client";
import { z } from "zod";
import { formatMana } from "./money";

export { formatMana };

import {
  AssetsHttpResponseSchema,
  OrderSchema,
  ProfileEmoteSchema,
  ProfileNameSchema,
  ProfileWearableSchema,
} from "../generated-schemas/market";
import {
  parseMarketEnvelope,
  parseCollection,
  parseCatalogItem,
  normalizeOrder,
  type Collection,
  type Order,
} from "./schema";

export { OrderSchema, type Order };
import { shortAddress } from "../format/address";
import { warnInvalid } from "../warn";

// `/users/{addr}/wearables` and `/emotes` rows are distinct DTOs with the same
// fields; one parser serves both, and this assert fails the build if they drift
// apart.
type Mutual<A, B> = A extends B ? (B extends A ? true : false) : false;
type Assert<T extends true> = T;
export type _AssertProfileItemShapesMatch = Assert<
  Mutual<z.infer<typeof ProfileWearableSchema>, z.infer<typeof ProfileEmoteSchema>>
>;

export type OwnedItem = {
  id: string;
  urn: string | null;
  tokenId: string | null;
  category: string | null;
  name: string | null;
  rarity: string | null;
  transferredAt: string | null;
  price: string | null;
};

export type OwnedName = {
  name: string;
  contractAddress: string | null;
  tokenId: string | null;
  price: string | null;
};

export function parseOwnedItem(raw: unknown): OwnedItem | null {
  const r = ProfileWearableSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("OwnedItem", r.error.issues);
    return null;
  }
  const w = r.data;
  return {
    id: w.id,
    urn: w.urn,
    tokenId: w.tokenId,
    category: w.category,
    name: w.name,
    rarity: w.rarity,
    transferredAt: w.transferredAt,
    price: w.price ?? null,
  };
}

export function parseOwnedName(raw: unknown): OwnedName | null {
  const r = ProfileNameSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("OwnedName", r.error.issues);
    return null;
  }
  const n = r.data;
  return {
    name: n.name,
    contractAddress: n.contractAddress,
    tokenId: n.tokenId,
    price: n.price ?? null,
  };
}

export function parseOrder(raw: unknown): Order | null {
  const r = OrderSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("Order", r.error.issues);
    return null;
  }
  return normalizeOrder(r.data);
}

const AssetsEnvelopeSchema = AssetsHttpResponseSchema(z.unknown());

export type AssetsPage = {
  elements: unknown[];
  total: number;
  totalItems?: number | null;
};

/**
 * `null` when the envelope did not parse. Every caller turns that into a thrown
 * read: an owned-asset page is a claim about what a wallet holds, and an empty
 * one for an unreadable response reads as "you own nothing".
 */
export function parseAssetsEnvelope(raw: unknown): AssetsPage | null {
  const r = AssetsEnvelopeSchema.safeParse(raw);
  if (r.success) return r.data.data;
  warnInvalid("AssetsEnvelope", r.error.issues);
  return null;
}

export function requireAssetsEnvelope(raw: unknown, path: string): AssetsPage {
  const page = parseAssetsEnvelope(raw);
  if (page === null) {
    throw new CatalystError("assets response did not match the paged shape", path);
  }
  return page;
}

export type OwnedList<T> = { elements: T[]; total: number };

const USERS_BASE = "/market/v1/users";

function collectValid<T>(rows: unknown[], parse: (raw: unknown) => T | null): T[] {
  const out: T[] = [];
  for (const raw of rows) {
    const row = parse(raw);
    if (row) out.push(row);
  }
  return out;
}

export async function fetchOwnedWearables(
  address: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedList<OwnedItem>> {
  const path = `${USERS_BASE}/${encodeURIComponent(address)}/wearables`;
  const d = requireAssetsEnvelope(
    await getJSON<unknown>(path, {
      ...opts,
      query: { first: params.first, skip: params.skip },
    }),
    path,
  );
  return {
    elements: collectValid(d.elements, parseOwnedItem),
    total: d.totalItems ?? d.total,
  };
}

export async function fetchOwnedEmotes(
  address: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedList<OwnedItem>> {
  const path = `${USERS_BASE}/${encodeURIComponent(address)}/emotes`;
  const d = requireAssetsEnvelope(
    await getJSON<unknown>(path, {
      ...opts,
      query: { first: params.first, skip: params.skip },
    }),
    path,
  );
  return {
    elements: collectValid(d.elements, parseOwnedItem),
    total: d.totalItems ?? d.total,
  };
}

export async function fetchOwnedNames(
  address: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedList<OwnedName>> {
  const path = `${USERS_BASE}/${encodeURIComponent(address)}/names`;
  const d = requireAssetsEnvelope(
    await getJSON<unknown>(path, {
      ...opts,
      query: { first: params.first, skip: params.skip },
    }),
    path,
  );
  return {
    elements: collectValid(d.elements, parseOwnedName),
    total: d.total,
  };
}

export async function fetchOwnerOrders(
  address: string,
  params: { first?: number; skip?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedList<Order>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/orders", {
      ...opts,
      query: { owner: address, first: params.first, skip: params.skip },
    }),
  );
  return {
    elements: collectValid(env.data, parseOrder),
    total: env.total,
  };
}

export async function fetchCreatorCollections(
  address: string,
  params: { first?: number; skip?: number; sortBy?: string } = {},
  opts: GetOptions = {},
): Promise<OwnedList<Collection>> {
  const env = parseMarketEnvelope(
    await getJSON<unknown>("/market/v1/collections", {
      ...opts,
      query: {
        creator: address,
        first: params.first,
        skip: params.skip,
        sortBy: params.sortBy,
      },
    }),
  );
  return {
    elements: collectValid(env.data, parseCollection),
    total: env.total,
  };
}

/** `null` means the read failed -- "no tiles known", not "no rarities". */
export async function fetchCollectionTiles(
  contractAddress: string,
  opts: GetOptions = {},
): Promise<string[] | null> {
  try {
    const env = parseMarketEnvelope(
      await getJSON<unknown>("/market/v1/catalog", {
        ...opts,
        query: { contractAddress, first: 4 },
      }),
    );
    return env.data
      .map((raw) => parseCatalogItem(raw)?.rarity)
      .filter((r): r is string => typeof r === "string" && r.length > 0);
  } catch {
    return null;
  }
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

export function toCardNetwork(
  network: string | null | undefined,
): "ethereum" | "polygon" {
  return network === "ETHEREUM" ? "ethereum" : "polygon";
}

export function shortenAddress(addr: string | null | undefined): string {
  return addr ? shortAddress(addr) : "";
}

export function thumbnailFromUrn(urn: string | null | undefined): string | undefined {
  return urn
    ? `${catalystBase()}/lambdas/collections/contents/${urn}/thumbnail`
    : undefined;
}

export type OwnedCard = {
  id: string;
  name: string;
  collection: string | undefined;
  price: string | null;
  rarity: string;
  network: "ethereum" | "polygon";
  image: string | undefined;
};

export function toOwnedCard(item: OwnedItem): OwnedCard {
  return {
    id: item.id,
    name: item.name ?? "Untitled",
    collection: item.category ?? undefined,
    price: formatMana(item.price),
    rarity: safeRarity(item.rarity),
    network: "polygon",
    image: thumbnailFromUrn(item.urn),
  };
}

export type OnSaleRow = {
  id: string;
  name: string;
  sub: string;
  category: string;
  rarity: string;
  saleType: "primary" | "secondary";
  price: string;
};

export function toOnSaleRow(order: Order): OnSaleRow {
  const tok = order.tokenId ?? order.issuedId ?? "";
  const shortTok = tok.length > 10 ? `${tok.slice(0, 6)}\u{2026}${tok.slice(-4)}` : tok;
  return {
    id: order.id,
    name: `Listing #${order.issuedId ?? shortTok}`,
    sub: shortenAddress(order.contractAddress),
    category: "wearable",
    rarity: "rare",
    saleType: "secondary",
    price: formatMana(order.price) ?? "0",
  };
}
