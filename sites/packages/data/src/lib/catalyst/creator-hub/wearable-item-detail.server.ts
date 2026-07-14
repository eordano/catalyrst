import { parseItem, type BuilderItem } from "../builder/items";
import type { GetOptions } from "../client";
import { fetchCatalogItem, formatMana, parseItemId } from "../marketplace/index";
import type { CatalogItem } from "../marketplace/schema";

const RARITY_MAX_SUPPLY: Record<string, number> = {
  unique: 1,
  mythic: 10,
  exotic: 50,
  legendary: 100,
  epic: 1000,
  rare: 5000,
  uncommon: 10000,
  common: 100000,
};

function bodyShapeFrom(shapes: unknown): "male" | "female" | "both" {
  if (!Array.isArray(shapes)) return "both";
  const strs = shapes.filter((s): s is string => typeof s === "string");
  const female = strs.some((s) => /female/i.test(s));
  const male = strs.some((s) => /male/i.test(s) && !/female/i.test(s));
  if (male && female) return "both";
  if (female) return "female";
  if (male) return "male";
  return "both";
}

function hueFromId(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i += 1) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return h % 360;
}

function stringArray(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : [];
}

export function catalogItemToBuilderItem(item: CatalogItem): BuilderItem | null {
  const data = item.data as Record<string, unknown>;
  const wearable = data?.wearable as Record<string, unknown> | undefined;
  const emote = data?.emote as Record<string, unknown> | undefined;
  const sub = wearable ?? emote;
  const type = item.category === "emote" || emote ? "emote" : "wearable";

  const rarity = (typeof sub?.rarity === "string" ? sub.rarity : item.rarity) ?? "common";
  const maxSupply = RARITY_MAX_SUPPLY[rarity] ?? 0;
  const available = typeof item.available === "number" ? item.available : null;
  const totalSupply =
    available != null && maxSupply > 0 ? Math.max(0, maxSupply - available) : 0;

  const metrics =
    type === "emote"
      ? { sequences: 0, duration: 0, frames: 0, fps: 0 }
      : { triangles: 0, materials: 0, textures: 0 };

  const representations = Array.isArray(sub?.representations)
    ? (sub.representations as unknown[])
        .map((r) => {
          const rep = r as Record<string, unknown>;
          const mainFile = typeof rep?.mainFile === "string" ? rep.mainFile : null;
          return mainFile
            ? { mainFile, bodyShape: bodyShapeFrom(rep?.bodyShapes) }
            : null;
        })
        .filter(
          (x): x is { mainFile: string; bodyShape: "male" | "female" | "both" } =>
            x !== null,
        )
    : [];

  return parseItem({
    id: item.id,
    type,
    name: item.name ?? "Untitled",
    description: typeof sub?.description === "string" ? sub.description : "",
    utility: "",
    rarity,
    category: typeof sub?.category === "string" ? sub.category : "",
    bodyShape: bodyShapeFrom(sub?.bodyShapes),
    smart: wearable?.isSmart === true,
    isPublished: true,
    tokenId: item.itemId,
    totalSupply,
    maxSupply,
    collection: "",
    urn: item.urn ?? "",
    price: formatMana(item.price) ?? "",
    beneficiary: "",
    hue: hueFromId(item.id),
    tags: stringArray(sub?.tags),
    metrics,
    representations,
    requiredPermissions: stringArray(wearable?.requiredPermissions),
  });
}

export type CreatorItemResult = {
  item: BuilderItem | null;
  /** true when the catalog read failed. `item: null` alone cannot tell "no such
   *  item" from "we could not ask", and the page says different things. */
  fallback: boolean;
  /** null unless `fallback` is set. Safe to show. */
  reason: string | null;
};

export async function loadCreatorItem(
  id: string,
  opts: GetOptions = {},
): Promise<CreatorItemResult> {
  const parsed = parseItemId(id);
  if (!parsed) return { item: null, fallback: false, reason: null };

  try {
    const row = await fetchCatalogItem(parsed.contractAddress, parsed.itemId, opts);
    if (!row) return { item: null, fallback: false, reason: null };
    return { item: catalogItemToBuilderItem(row), fallback: false, reason: null };
  } catch (err) {
    return {
      item: null,
      fallback: true,
      reason: (err as Error)?.message ?? "the catalog did not answer",
    };
  }
}
