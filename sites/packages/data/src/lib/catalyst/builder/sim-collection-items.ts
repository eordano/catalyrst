import { check } from "@ui/validate";

import { SimCollectionItemsStoreSchema } from "../../persisted-schemas";

import type { EmoteItem, WearableItem } from "./collection-detail";

export const SIM_COLLECTION_ITEMS_KEY = "dcl:ch:sim-collection-items:v1";

const SIM_ITEMS_TTL_MS = 24 * 60 * 60 * 1000;
const SIM_ITEMS_MAX_COLLECTIONS = 8;

export type SimDraftFile = { name: string; size: number; fileType: string };

export type SimEntry = { ts: number; files: SimDraftFile[] };
export type SimStore = Record<string, SimEntry>;

/**
 * `null` when storage could not be read or held something else.
 *
 * Only the read and the parse are inside the try. `check` throws in dev by
 * design, and a catch wide enough to cover it would collapse a drifted store
 * back into the same `null` an unreadable one produces -- detection wired in and
 * never firing. The `typeof` guard stays as the production fallback, because
 * `check` returns the ORIGINAL value when it rejects outside dev.
 */
function readStore(): SimStore | null {
  if (typeof window === "undefined") return null;
  let parsed: unknown;
  try {
    const raw = window.localStorage.getItem(SIM_COLLECTION_ITEMS_KEY);
    if (!raw) return {};
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  const store = check(
    SimCollectionItemsStoreSchema,
    parsed,
    "persisted/sim-collection-items",
  );
  return store && typeof store === "object" ? store : null;
}

function writeStore(store: SimStore): void {
  if (typeof window === "undefined") return;
  try {
    if (Object.keys(store).length === 0) {
      window.localStorage.removeItem(SIM_COLLECTION_ITEMS_KEY);
    } else {
      window.localStorage.setItem(SIM_COLLECTION_ITEMS_KEY, JSON.stringify(store));
    }
  } catch {
  }
}

function prune(store: SimStore): SimStore {
  const now = Date.now();
  const live = Object.entries(store).filter(
    ([, entry]) =>
      entry &&
      Array.isArray(entry.files) &&
      typeof entry.ts === "number" &&
      now - entry.ts <= SIM_ITEMS_TTL_MS,
  );
  live.sort(([, a], [, b]) => b.ts - a.ts);
  return Object.fromEntries(live.slice(0, SIM_ITEMS_MAX_COLLECTIONS));
}

export function saveSimCollectionItems(
  collectionId: string,
  files: SimDraftFile[],
): void {
  // Unreadable storage cannot be merged into, so this starts a fresh one.
  const store = prune(readStore() ?? {});
  store[collectionId] = {
    ts: Date.now(),
    files: files.map((f) => ({ name: f.name, size: f.size, fileType: f.fileType })),
  };
  writeStore(store);
}

function baseName(file: string): string {
  const dot = file.lastIndexOf(".");
  return dot > 0 ? file.slice(0, dot) : file;
}

export function readSimCollectionItems(
  collectionId: string,
): { wearables: WearableItem[]; emotes: EmoteItem[] } | null {
  const store = readStore();
  if (!store) return null;
  const entry = prune(store)[collectionId];
  if (!entry || entry.files.length === 0) return null;
  const wearables: WearableItem[] = entry.files.map((f, i) => ({
    id: `sim-item-${i}`,
    name: baseName(f.name),
    rarity: "common",
    category: "upper_body",
    price: null,
    supply: null,
    status: "not_ready",
    smart: false,
    hue: (i * 47) % 360,
  }));
  return { wearables, emotes: [] };
}
