import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  parseOwned,
  normalizeAddress,
  urnLabel,
  EquippedSchema,
  WEARABLE_CATEGORIES,
  type Wearable,
  type Category,
  type Equipped,
  type InventoryState,
  type CatalogState,
} from "./backpack";

const BASE_AVATARS_COLLECTION = "urn:decentraland:off-chain:base-avatars";

export type BackpackData = {
  address: string;
  owned: Wearable[];
  catalog: Wearable[];
  categories: Category[];
  equipped: Equipped | null;
  /** Only `{ status: "loaded" }` asserts anything about what the player owns. */
  inventory: InventoryState;
  /** Says whether `catalog` is everything this node could offer. */
  catalogState: CatalogState;
};

type RawWearable = {
  id?: string;
  name?: string;
  thumbnail?: string;
  rarity?: string | null;
  data?: {
    category?: string;
    tags?: string[];
    representations?: { bodyShapes?: string[] }[];
  };
};

function extractWearables(raw: unknown): RawWearable[] {
  if (Array.isArray(raw)) return raw as RawWearable[];
  const d = (raw ?? {}) as { wearables?: unknown[]; data?: unknown[] };
  return (d.wearables ?? d.data ?? []) as RawWearable[];
}

/**
 * null when the definition does not say which slot the item occupies.
 *
 * `category` used to fall back to `upper_body`, which silently moved a hat
 * into the shirt slot on equip; `rarity` used to fall back to `base`, which
 * labelled an on-chain drop as a free item. Both now pass through as read, and
 * an item with no category is left out of the catalog instead of mis-slotted.
 */
function mapWearable(w: RawWearable): Wearable | null {
  if (!w.id) return null;
  const category = w.data?.category?.trim();
  if (!category) return null;
  const bodyShapes = [
    ...new Set((w.data?.representations ?? []).flatMap((r) => r.bodyShapes ?? [])),
  ];
  const tags = w.data?.tags ?? [];
  const network = w.id.includes(":matic:")
    ? "polygon"
    : w.id.includes(":ethereum:")
      ? "ethereum"
      : null;
  return {
    urn: w.id,
    name: w.name?.trim() || urnLabel(w.id),
    thumbnail: w.thumbnail ?? null,
    rarity: w.rarity ?? null,
    category,
    bodyShapes,
    description: null,
    isSmart: tags.includes("smart"),
    creator: null,
    network,
  };
}

function mapWearables(raw: unknown): Wearable[] {
  return extractWearables(raw)
    .map(mapWearable)
    .filter((w): w is Wearable => w != null);
}

function buildCategories(): Category[] {
  return WEARABLE_CATEGORIES.map((c) => ({
    id: c,
    label: c.replace(/_/g, " ").replace(/\b\w/g, (m) => m.toUpperCase()),
    slot: c,
  }));
}

/** null when the base catalog could not be read -- not an empty catalog. */
async function fetchBaseCatalog(opts: GetOptions): Promise<Wearable[] | null> {
  try {
    const raw = await getJSON<unknown>(
      `/lambdas/collections/wearables?collectionId=${encodeURIComponent(
        BASE_AVATARS_COLLECTION,
      )}`,
      opts,
    );
    return mapWearables(raw);
  } catch {
    return null;
  }
}

/**
 * `failed` counts the chunks that did not answer. A dropped chunk silently
 * removes items from the grid, so the count travels with the definitions and
 * the panel says the list is incomplete.
 */
async function fetchWearableDefs(
  urns: string[],
  opts: GetOptions,
): Promise<{ defs: Wearable[]; failed: number }> {
  const onChain = [...new Set(urns)].filter(
    (u) => u && !u.startsWith(BASE_AVATARS_COLLECTION),
  );
  if (onChain.length === 0) return { defs: [], failed: 0 };
  const chunks: string[][] = [];
  for (let i = 0; i < onChain.length; i += 40) chunks.push(onChain.slice(i, i + 40));
  const results = await Promise.all(
    chunks.map(async (chunk) => {
      const qs = chunk.map((u) => `wearableId=${encodeURIComponent(u)}`).join("&");
      try {
        const raw = await getJSON<unknown>(`/lambdas/collections/wearables?${qs}`, opts);
        return mapWearables(raw);
      } catch {
        return null;
      }
    }),
  );
  return {
    defs: results.filter((r): r is Wearable[] => r !== null).flat(),
    failed: results.filter((r) => r === null).length,
  };
}

/** null when the ownership read failed -- never an empty inventory. */
async function fetchOwnedUrns(
  address: string,
  opts: GetOptions,
): Promise<string[] | null> {
  try {
    const raw = await getJSON<unknown>(
      `/lambdas/collections/wearables-by-owner/${encodeURIComponent(
        normalizeAddress(address),
      )}`,
      opts,
    );
    return parseOwned(raw);
  } catch {
    return null;
  }
}

function colorHex(c: unknown): string | undefined {
  if (typeof c === "string") return c;
  if (c && typeof c === "object") {
    const { r, g, b } = c as { r?: number; g?: number; b?: number };
    if (typeof r === "number" && typeof g === "number" && typeof b === "number") {
      const h = (n: number) =>
        Math.max(0, Math.min(255, Math.round(n * 255)))
          .toString(16)
          .padStart(2, "0");
      return `#${h(r)}${h(g)}${h(b)}`;
    }
  }
  return undefined;
}

async function fetchEquipped(address: string, opts: GetOptions): Promise<Equipped | null> {
  try {
    const prof = await getJSON<{ avatars?: { avatar?: Record<string, unknown> }[] }>(
      `/lambdas/profiles/${encodeURIComponent(normalizeAddress(address))}`,
      opts,
    );
    const av = prof.avatars?.[0]?.avatar as
      | {
          bodyShape?: string;
          skin?: { color?: unknown };
          hair?: { color?: unknown };
          eyes?: { color?: unknown };
          wearables?: unknown;
        }
      | undefined;
    if (av) {
      return EquippedSchema.parse({
        bodyShape: av.bodyShape,
        skinColor: colorHex(av.skin?.color),
        hairColor: colorHex(av.hair?.color),
        eyeColor: colorHex(av.eyes?.color),
        wearables: Array.isArray(av.wearables) ? av.wearables : [],
      });
    }
  } catch {
  }
  // A defaulted avatar is a real-looking one: BaseMale, a skin tone, no
  // wearables. Rendering that for a failed profile read invites the wearer to
  // save it back over the avatar we merely failed to fetch.
  return null;
}

export async function loadBackpack(
  address: string | null,
  signal?: AbortSignal,
): Promise<BackpackData> {
  const opts: GetOptions = { signal };
  const categories = buildCategories();

  if (!address) {
    const base = await fetchBaseCatalog(opts);
    return {
      address: "",
      owned: [],
      catalog: base ?? [],
      categories,
      equipped: null,
      inventory: { status: "not-connected" },
      catalogState:
        base === null
          ? { status: "unavailable", reason: "the wearables catalog did not answer" }
          : { status: "complete" },
    };
  }

  const [base, ownedUrns, equipped] = await Promise.all([
    fetchBaseCatalog(opts),
    fetchOwnedUrns(address, opts),
    fetchEquipped(address, opts),
  ]);

  const { defs: extraDefs, failed } = await fetchWearableDefs(
    [...(ownedUrns ?? []), ...(equipped?.wearables ?? [])],
    opts,
  );

  const byUrn = new Map<string, Wearable>();
  for (const w of [...(base ?? []), ...extraDefs]) byUrn.set(w.urn, w);
  const catalog = [...byUrn.values()];

  const ownedSet = new Set(ownedUrns ?? []);
  const owned = ownedUrns === null ? [] : catalog.filter((w) => ownedSet.has(w.urn));

  const catalogReason =
    base === null
      ? "the base wearables catalog did not answer"
      : `${failed} item lookup${failed === 1 ? "" : "s"} did not answer`;
  const catalogState: CatalogState =
    base === null && catalog.length === 0
      ? { status: "unavailable", reason: catalogReason }
      : base === null || failed > 0
        ? { status: "partial", reason: catalogReason }
        : { status: "complete" };

  return {
    address,
    owned,
    catalog,
    categories,
    equipped,
    inventory:
      ownedUrns === null
        ? {
            status: "unavailable",
            reason: "the ownership read did not answer",
          }
        : { status: "loaded", empty: owned.length === 0 },
    catalogState,
  };
}
