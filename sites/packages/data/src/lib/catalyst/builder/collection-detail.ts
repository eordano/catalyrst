import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";

export const RARITIES = [
  "unique",
  "mythic",
  "exotic",
  "legendary",
  "epic",
  "rare",
  "uncommon",
  "common",
] as const;

export const ITEM_STATUSES = [
  "ready",
  "not_ready",
  "published",
  "under_review",
  "unsynced",
] as const;

export const COLLECTION_STATUSES = [
  "synced",
  "under_review",
  "unsynced",
  "loading",
] as const;

const nullableStr = z.string().nullish().transform((v) => v ?? null);

/**
 * These three describe the shape the page renders; nothing is parsed with them.
 * Every value reaching them is assembled below from `LiveItemSchema` /
 * `OnchainItemRowSchema` or from `emptyCollection`, which is an explicit,
 * labelled empty -- not a parse result wearing one.
 *
 * They are still written strictly, `.catch()` included. A rarity that falls
 * back to "common" or a status that falls back to "not_ready" is a claim about
 * an item a creator is about to publish and price; the assemblers below already
 * pick those values from the enums, so anything else reaching here is a payload
 * we did not understand and must fail rather than downgrade.
 */
export const WearableItemSchema = z.object({
  id: z.string(),
  name: z.string(),
  rarity: z.enum(RARITIES),
  category: z.string(),
  price: nullableStr,
  supply: nullableStr,
  status: z.enum(ITEM_STATUSES),
  smart: z.boolean(),
  hue: z.number(),
});
export type WearableItem = z.infer<typeof WearableItemSchema>;

export const EmoteItemSchema = z.object({
  id: z.string(),
  name: z.string(),
  rarity: z.enum(RARITIES),
  category: z.string(),
  playMode: z.enum(["loop", "simple"]),
  price: nullableStr,
  supply: nullableStr,
  status: z.enum(ITEM_STATUSES),
  hue: z.number(),
});
export type EmoteItem = z.infer<typeof EmoteItemSchema>;

export const CollectionSchema = z.object({
  id: z.string(),
  name: z.string(),
  status: z.enum(COLLECTION_STATUSES),
  isPublished: z.boolean(),
  isApproved: z.boolean(),
  isOnSale: z.boolean(),
  isLocked: z.boolean(),
  contractAddress: nullableStr,
  urn: nullableStr,
  createdAt: z.number().nullish().transform((v) => v ?? null),
  updatedAt: z.number().nullish().transform((v) => v ?? null),
  reviewedAt: z.number().nullish().transform((v) => v ?? null),
  wearables: z.array(WearableItemSchema),
  emotes: z.array(EmoteItemSchema),
});
export type CollectionDetail = z.infer<typeof CollectionSchema>;

function simCollectionName(id: string): string {
  const words = id.slice("sim-".length).split("-").filter(Boolean);
  if (!words.length) return "Collection";
  return words.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}

export function emptyCollection(id: string): CollectionDetail {
  return {
    id,
    name: isSimulatedCollectionId(id) ? simCollectionName(id) : "Collection",
    status: "unsynced",
    isPublished: false,
    isApproved: false,
    isOnSale: false,
    isLocked: false,
    contractAddress: null,
    urn: null,
    createdAt: null,
    updatedAt: null,
    reviewedAt: null,
    wearables: [],
    emotes: [],
  };
}

export function isSimulatedCollectionId(id: string): boolean {
  return id.startsWith("sim-");
}

const COLLECTION_UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const CONTRACT_ADDRESS_RE = /^0x[0-9a-f]{40}$/i;

export function isContractCollectionId(id: string): boolean {
  return CONTRACT_ADDRESS_RE.test(id.trim());
}

export function collectionIdCanExist(id: string): boolean {
  return (
    COLLECTION_UUID_RE.test(id.trim()) ||
    isContractCollectionId(id) ||
    isSimulatedCollectionId(id)
  );
}

export type ItemTab = "wearables" | "emotes";

export function readTab(raw: string | null | undefined): ItemTab {
  return raw === "emotes" ? "emotes" : "wearables";
}

const LiveItemSchema = z.object({
  id: z.string(),
  name: z.string(),
  rarity: z.string().nullish(),
  type: z.string().nullish(),
  price: z.union([z.string(), z.number()]).nullish(),
  total_supply: z.number().nullish(),
  is_published: z.boolean().nullish(),
  is_approved: z.boolean().nullish(),
  data: z
    .object({
      category: z.string().nullish(),
      loop: z.boolean().nullish(),
    })
    .nullish(),
});
type LiveItem = z.infer<typeof LiveItemSchema>;

function liveStatus(it: LiveItem): (typeof ITEM_STATUSES)[number] {
  if (it.is_published && it.is_approved) return "published";
  if (it.is_published) return "under_review";
  return it.price != null ? "ready" : "not_ready";
}

export async function fetchCollectionItems(
  id: string,
  opts: GetOptions = {},
): Promise<{ wearables: WearableItem[]; emotes: EmoteItem[] }> {
  const raw = await getJSON<{ data?: unknown }>(
    `/v1/collections/${encodeURIComponent(id)}/items`,
    opts,
  );
  // No `.catch([])`: a body that is not a list of items is a failed read, and
  // every caller separates that from a collection that holds nothing. Swallowed
  // here it told a creator their published collection was empty.
  const list = z
    .array(LiveItemSchema)
    .parse((raw as { data?: unknown })?.data ?? raw);

  const wearables: WearableItem[] = [];
  const emotes: EmoteItem[] = [];
  let i = 0;
  for (const it of list) {
    const rarity = (RARITIES as readonly string[]).includes(it.rarity ?? "")
      ? (it.rarity as (typeof RARITIES)[number])
      : "common";
    const price = it.price == null ? null : String(it.price);
    const supply =
      it.is_published && it.is_approved && it.total_supply != null
        ? String(it.total_supply)
        : null;
    const status = liveStatus(it);
    const hue = (i * 47) % 360;
    if ((it.type ?? "wearable") === "emote") {
      emotes.push({
        id: it.id,
        name: it.name,
        rarity,
        category: it.data?.category ?? "dance",
        playMode: it.data?.loop ? "loop" : "simple",
        price,
        supply,
        status,
        hue,
      });
    } else {
      wearables.push({
        id: it.id,
        name: it.name,
        rarity,
        category: it.data?.category ?? "upper_body",
        price,
        supply,
        status,
        smart: (it.type ?? "") === "smart_wearable",
        hue,
      });
    }
    i++;
  }
  return { wearables, emotes };
}

export function itemCount(c: CollectionDetail): number {
  return c.wearables.length + c.emotes.length;
}

const CollectionMetaSchema = z.object({
  id: z.string(),
  name: z.string(),
  is_published: z.boolean().nullish(),
  is_approved: z.boolean().nullish(),
  contract_address: nullableStr,
  urn: nullableStr,
  created_at: z.number().nullish(),
  updated_at: z.number().nullish(),
});

export type CollectionMeta = {
  name: string;
  status: (typeof COLLECTION_STATUSES)[number];
  isPublished: boolean;
  isApproved: boolean;
  contractAddress: string | null;
  urn: string | null;
  createdAt: number | null;
  updatedAt: number | null;
};

export async function fetchCollectionMeta(
  id: string,
  opts: GetOptions = {},
): Promise<CollectionMeta | null> {
  const raw = await getJSON<{ data?: unknown }>(
    `/v1/collections/${encodeURIComponent(id)}`,
    opts,
  );
  const parsed = CollectionMetaSchema.safeParse(
    (raw as { data?: unknown })?.data ?? raw,
  );
  if (!parsed.success) return null;
  const c = parsed.data;
  const isPublished = c.is_published ?? false;
  const isApproved = c.is_approved ?? false;
  const status: (typeof COLLECTION_STATUSES)[number] = isApproved
    ? "synced"
    : isPublished
      ? "under_review"
      : "unsynced";
  return {
    name: c.name,
    status,
    isPublished,
    isApproved,
    contractAddress: c.contract_address,
    urn: c.urn,
    createdAt: c.created_at ?? null,
    updatedAt: c.updated_at ?? null,
  };
}

export function mergeCollectionMeta(
  base: CollectionDetail,
  meta: CollectionMeta,
): CollectionDetail {
  return {
    ...base,
    name: meta.name,
    status: meta.status,
    isPublished: meta.isPublished,
    isApproved: meta.isApproved,
    contractAddress: meta.contractAddress,
    urn: meta.urn,
    createdAt: meta.createdAt,
    updatedAt: meta.updatedAt,
  };
}

const OnchainCollectionRowSchema = z.object({
  id: z.string(),
  name: z.string(),
  status: z.string().nullish(),
  is_published: z.boolean().nullish(),
  is_approved: z.boolean().nullish(),
  contract_address: nullableStr,
  urn: nullableStr,
  created_at: z.number().nullish(),
  updated_at: z.number().nullish(),
  reviewed_at: z.number().nullish(),
});

const OnchainItemRowSchema = z.object({
  id: z.string(),
  collection_id: nullableStr,
  name: z.string(),
  type: z.string().nullish(),
  category: nullableStr,
  rarity: z.string().nullish(),
  price: nullableStr,
  total_supply: z.number().nullish(),
  is_published: z.boolean().nullish(),
});

const PRICE_SENTINEL_WEI = 2n ** 248n;

function onchainPriceMana(wei: string | null): string | null {
  if (!wei) return null;
  try {
    const v = BigInt(wei);
    if (v <= 0n || v >= PRICE_SENTINEL_WEI) return null;
    const mana = Number(v) / 1e18;
    return mana.toLocaleString(undefined, { maximumFractionDigits: 2 });
  } catch {
    return null;
  }
}

function onchainRarity(r: string | null | undefined): (typeof RARITIES)[number] {
  return (RARITIES as readonly string[]).includes(r ?? "")
    ? (r as (typeof RARITIES)[number])
    : "common";
}

export type OnchainCollectionDetail = {
  found: boolean;
  meta: CollectionMeta | null;
  wearables: WearableItem[];
  emotes: EmoteItem[];
};

export async function fetchOnchainCollectionDetail(
  address: string,
  contractId: string,
  opts: GetOptions = {},
): Promise<OnchainCollectionDetail> {
  const wanted = contractId.trim().toLowerCase();
  const addr = address.trim().toLowerCase();

  const [colsRaw, itemsRaw] = await Promise.all([
    getJSON<{ data?: unknown }>(
      `/v1/${encodeURIComponent(addr)}/collections`,
      opts,
    ),
    getJSON<{ data?: unknown }>(`/v1/${encodeURIComponent(addr)}/items`, opts),
  ]);

  const cols = z
    .array(z.unknown())
    .parse((colsRaw as { data?: unknown })?.data ?? colsRaw);
  const row = cols
    .map((c) => OnchainCollectionRowSchema.safeParse(c))
    .filter((r) => r.success)
    .map((r) => r.data)
    .find(
      (c) =>
        c.id.toLowerCase() === wanted ||
        (c.contract_address ?? "").toLowerCase() === wanted,
    );
  if (!row) return { found: false, meta: null, wearables: [], emotes: [] };

  const isPublished = row.is_published ?? true;
  const isApproved = row.is_approved ?? false;
  const status: (typeof COLLECTION_STATUSES)[number] = (
    COLLECTION_STATUSES as readonly string[]
  ).includes(row.status ?? "")
    ? (row.status as (typeof COLLECTION_STATUSES)[number])
    : isApproved
      ? "synced"
      : isPublished
        ? "under_review"
        : "unsynced";

  const meta: CollectionMeta = {
    name: row.name,
    status,
    isPublished,
    isApproved,
    contractAddress: row.contract_address ?? wanted,
    urn: row.urn,
    createdAt: row.created_at ?? null,
    updatedAt: row.updated_at ?? null,
  };

  const items = z
    .array(z.unknown())
    .parse((itemsRaw as { data?: unknown })?.data ?? itemsRaw)
    .map((it) => OnchainItemRowSchema.safeParse(it))
    .filter((r) => r.success)
    .map((r) => r.data)
    .filter((it) => (it.collection_id ?? "").toLowerCase() === wanted);

  const wearables: WearableItem[] = [];
  const emotes: EmoteItem[] = [];
  let i = 0;
  for (const it of items) {
    const price = onchainPriceMana(it.price);
    const supply =
      it.is_published && it.total_supply != null ? String(it.total_supply) : null;
    const itemStatus: (typeof ITEM_STATUSES)[number] = it.is_published
      ? isApproved
        ? "published"
        : "under_review"
      : "not_ready";
    const hue = (i * 47) % 360;
    if ((it.type ?? "wearable") === "emote") {
      emotes.push({
        id: it.id,
        name: it.name,
        rarity: onchainRarity(it.rarity),
        category: it.category ?? "dance",
        playMode: "simple",
        price,
        supply,
        status: itemStatus,
        hue,
      });
    } else {
      wearables.push({
        id: it.id,
        name: it.name,
        rarity: onchainRarity(it.rarity),
        category: it.category ?? "upper_body",
        price,
        supply,
        status: itemStatus,
        smart: (it.type ?? "") === "smart_wearable",
        hue,
      });
    }
    i++;
  }

  return { found: true, meta, wearables, emotes };
}
