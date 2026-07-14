import type { EmoteItem, WearableItem } from "./collection-detail";

export const SIM_COLLECTION_ITEMS_KEY = "dcl:ch:sim-collection-items:v1";

const SIM_ITEMS_TTL_MS = 24 * 60 * 60 * 1000;
const SIM_ITEMS_MAX_COLLECTIONS = 8;

export type SimDraftFile = { name: string; size: number; fileType: string };

type SimEntry = { ts: number; files: SimDraftFile[] };
type SimStore = Record<string, SimEntry>;

/** `null` when storage could not be read or held something else. */
function readStore(): SimStore | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(SIM_COLLECTION_ITEMS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as SimStore;
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
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
