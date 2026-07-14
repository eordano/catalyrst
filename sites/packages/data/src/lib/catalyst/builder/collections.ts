import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  parseCollections,
  parseOrphanItems,
  COLLECTION_SORTS,
  type BuilderCollection,
  type CollectionSort,
  type CollectionType,
  type OrphanItem,
} from "./collections-schema";

export type {
  BuilderCollection,
  CollectionSort,
  CollectionType,
  CollectionStatus,
  OrphanItem,
} from "./collections-schema";
export { COLLECTION_SORTS, COLLECTION_TYPES } from "./collections-schema";

export const COLLECTION_TABS = {
  standard: "standard_collections",
  third_party: "third_party_collections",
  items: "orphan_items",
} as const;
export type CollectionTab = keyof typeof COLLECTION_TABS;

export function readTab(raw: string | null | undefined): CollectionTab {
  if (raw === "third_party") return "third_party";
  if (raw === "items") return "items";
  return "standard";
}

export function readView(raw: string | null | undefined): "grid" | "list" {
  return raw === "list" ? "list" : "grid";
}

export function readSort(raw: string | null | undefined): CollectionSort {
  const v = (raw ?? "").trim() as CollectionSort;
  return (COLLECTION_SORTS as readonly string[]).includes(v) ? v : "MOST_RELEVANT";
}

export function tabToUiId(tab: CollectionTab): string {
  return COLLECTION_TABS[tab];
}

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export async function fetchCollections(
  address: string,
  opts: GetOptions = {},
): Promise<BuilderCollection[]> {
  const raw = await getJSON<{ data?: unknown }>(
    `/v1/${encodeURIComponent(normalizeAddress(address))}/collections`,
    opts,
  );
  return parseCollections(raw?.data ?? raw);
}

export async function fetchCreatorItems(
  address: string,
  opts: GetOptions = {},
): Promise<OrphanItem[]> {
  const raw = await getJSON<{ data?: unknown }>(
    `/v1/${encodeURIComponent(normalizeAddress(address))}/items`,
    opts,
  );
  return parseOrphanItems(raw?.data ?? raw);
}

const MAX_COLLECTION_THUMBS = 4;

export function collectionThumbsFrom(items: OrphanItem[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const it of items) {
    if (!it.collection_id || !it.image) continue;
    const arr = map.get(it.collection_id) ?? [];
    if (arr.length >= MAX_COLLECTION_THUMBS) continue;
    arr.push(it.image);
    map.set(it.collection_id, arr);
  }
  return map;
}

export function mergeCollectionThumbs(
  collections: BuilderCollection[],
  items: OrphanItem[],
): BuilderCollection[] {
  const thumbs = collectionThumbsFrom(items);
  return collections.map((c) => {
    if (c.thumbs.length > 0) return c;
    const found = thumbs.get(c.id);
    return found && found.length > 0 ? { ...c, thumbs: found } : c;
  });
}

export function sortCollections(
  rows: BuilderCollection[],
  sort: CollectionSort,
): BuilderCollection[] {
  const out = [...rows];
  const num = (v: number | null) => v ?? 0;
  switch (sort) {
    case "CREATED_AT_DESC":
      return out.sort((a, b) => num(b.created_at) - num(a.created_at));
    case "CREATED_AT_ASC":
      return out.sort((a, b) => num(a.created_at) - num(b.created_at));
    case "UPDATED_AT_DESC":
      return out.sort((a, b) => num(b.updated_at) - num(a.updated_at));
    case "UPDATED_AT_ASC":
      return out.sort((a, b) => num(a.updated_at) - num(b.updated_at));
    case "NAME_ASC":
      return out.sort((a, b) => a.name.localeCompare(b.name));
    case "NAME_DESC":
      return out.sort((a, b) => b.name.localeCompare(a.name));
    case "MOST_RELEVANT":
    default:
      return out;
  }
}

export type CollectionCardVM = {
  id: string;
  name: string;
  type: "collection" | "third_party";
  status: BuilderCollection["status"];
  count: number;
  thumbs: string[];
  pending?: boolean;
};

export function toCollectionCard(c: BuilderCollection): CollectionCardVM {
  return {
    id: c.id,
    name: c.name,
    type: c.type === "third_party" ? "third_party" : "collection",
    status: c.status,
    count: c.count,
    thumbs: c.thumbs,
    pending: c.pending || undefined,
  };
}

export type OrphanItemVM = {
  id: string;
  name: string;
  type: OrphanItem["type"];
  status: OrphanItem["status"];
  createdAt: string;
  updatedAt: string;
  grad: string;
};

const DEFAULT_GRAD = "linear-gradient(135deg,#438fff,#2f004d)";

export function relativeTime(ms: number | null): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  if (diff < 0) return "just now";
  const day = 86_400_000;
  const days = Math.floor(diff / day);
  if (days < 1) return "today";
  if (days < 7) return `${days} day${days === 1 ? "" : "s"} ago`;
  if (days < 30) {
    const w = Math.floor(days / 7);
    return `${w} week${w === 1 ? "" : "s"} ago`;
  }
  if (days < 365) {
    const m = Math.floor(days / 30);
    return `${m} month${m === 1 ? "" : "s"} ago`;
  }
  const y = Math.floor(days / 365);
  return `${y} year${y === 1 ? "" : "s"} ago`;
}

export function toOrphanItem(it: OrphanItem): OrphanItemVM {
  return {
    id: it.id,
    name: it.name,
    type: it.type,
    status: it.status,
    createdAt: relativeTime(it.createdAt),
    updatedAt: relativeTime(it.updatedAt),
    grad: it.grad ?? DEFAULT_GRAD,
  };
}
