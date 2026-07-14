import { z } from "zod";

import { ETH_ADDRESS_RE } from "../format/address";

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

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
export type Rarity = (typeof RARITIES)[number];

export const WEARABLE_CATEGORIES = [
  "body_shape",
  "hair",
  "eyebrows",
  "eyes",
  "mouth",
  "facial_hair",
  "upper_body",
  "hands_wear",
  "lower_body",
  "feet",
  "hat",
  "eyewear",
  "earring",
  "mask",
  "tiara",
  "helmet",
  "top_head",
  "skin",
] as const;

/**
 * What a wearable definition actually carries.
 *
 * `rarity` is a StandardProps field in `@dcl/schemas`: on-chain collection
 * items have one and base avatars are *prohibited* from carrying it, so an
 * absent rarity is a fact about the item, not a read that failed. `common` --
 * the old default -- is a real tier with its own colour and price band, so
 * every base wearable used to render as a common drop.
 *
 * `category` and `bodyShapes` get no fallback of any kind. `category` picks
 * the slot an item equips into, so a defaulted `upper_body` puts a hat where
 * the shirt goes; `bodyShapes` decides whether the item can be worn at all.
 * An item whose definition does not say is dropped from the catalog rather
 * than mis-slotted, which is what `parseCatalog` below does with a rejection.
 *
 * `name` is required because `mapWearable` always supplies one -- from the URN
 * slug when the definition omits it, which is a fact about the URN.
 */
export const WearableSchema = z.object({
  urn: z.string().min(1),
  name: z.string(),
  thumbnail: z.string().nullish().transform((v) => v ?? null),
  rarity: z.string().nullish().transform((v) => v ?? null),
  category: z.string(),
  bodyShapes: z.array(z.string()),
  description: z.string().nullish().transform((v) => v ?? null),
  isSmart: z.boolean(),
  creator: z.string().nullish().transform((v) => v ?? null),
  network: z.string().nullish().transform((v) => v ?? null),
});
export type Wearable = z.infer<typeof WearableSchema>;

export const CategorySchema = z.object({
  id: z.string(),
  label: z.string(),
  slot: z.string(),
});
export type Category = z.infer<typeof CategorySchema>;

/**
 * `bodyShape`, `eyes`, `hair` and `skin` are required on every `AvatarInfo` in
 * `@dcl/schemas`, so a profile that does not carry them is not a profile.
 * Defaulting them produced a plausible stranger -- BaseMale, a skin tone, no
 * wearables -- that the panel then invited the wearer to save back over the
 * avatar we had merely failed to read. A rejection reaches `fetchEquipped`,
 * which returns null, and the panel says so.
 */
export const EquippedSchema = z.object({
  bodyShape: z.string(),
  skinColor: z.string(),
  hairColor: z.string(),
  eyeColor: z.string(),
  wearables: z.array(z.string()),
});
export type Equipped = z.infer<typeof EquippedSchema>;

/** `amount` is how many copies the wallet owns. An absent amount is not "one" --
 *  nothing here reads it, and inventing a quantity is how a broken read becomes
 *  a claim about someone's holdings. */
export const OwnedElementSchema = z
  .object({
    urn: z.string(),
    amount: z.number().nullish().transform((v) => v ?? null),
  })
  .passthrough();
export type OwnedElement = z.infer<typeof OwnedElementSchema>;

export function parseCatalog(raw: unknown): Wearable[] {
  if (!Array.isArray(raw)) return [];
  const out: Wearable[] = [];
  for (const item of raw) {
    const r = WearableSchema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  return out;
}

export function parseOwned(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  for (const item of raw) {
    const r = OwnedElementSchema.safeParse(item);
    if (r.success) out.push(r.data.urn);
  }
  return out;
}

/** A label read off the URN. Not a name the definition gave us, but a fact
 *  about the item, which an empty tile is not. */
export function urnLabel(urn: string): string {
  const last = urn.split(":").pop() ?? "";
  if (!last || /^\d+$/.test(last)) return urn;
  return last
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (m) => m.toUpperCase())
    .trim();
}

export function findWearable(catalog: Wearable[], urn: string): Wearable | undefined {
  return catalog.find((w) => w.urn === urn);
}

export function byCategory(catalog: Wearable[]): Record<string, Wearable[]> {
  const out: Record<string, Wearable[]> = {};
  for (const w of catalog) {
    (out[w.category] ??= []).push(w);
  }
  return out;
}

export function equipInto(
  wearables: string[],
  next: Wearable,
  catalog: Wearable[],
): string[] {
  const kept = wearables.filter((urn) => {
    const occ = findWearable(catalog, urn);
    if (!occ) return !urnSlotMatches(urn, next.category);
    return occ.category !== next.category;
  });
  return [...kept, next.urn];
}

function urnSlotMatches(urn: string, category: string): boolean {
  const tail = urn.split(":").pop()?.toLowerCase() ?? "";
  const key = category.toLowerCase();
  if (key === "hair") return tail.includes("hair");
  if (key === "upper_body") return /hoodie|tshirt|shirt|jacket|upper/.test(tail);
  if (key === "lower_body") return /pants|trousers|shorts|lower/.test(tail);
  if (key === "feet") return /shoe|sneaker|boot|feet/.test(tail);
  if (key === "eyes") return tail.includes("eyes");
  if (key === "mouth") return tail.includes("mouth");
  if (key === "eyebrows") return tail.includes("eyebrows");
  return false;
}

export function rarityLabel(rarity: string): string {
  return rarity.charAt(0).toUpperCase() + rarity.slice(1);
}

/**
 * What was actually read about the player's own items.
 *
 * "loaded" is the only arm that carries a claim about how many items exist.
 * A failed read used to arrive here as `[]`, which the panel rendered as
 * "your inventory is empty" -- an answer nobody measured.
 */
export type InventoryState =
  | { status: "loaded"; empty: boolean }
  | { status: "not-connected" }
  | { status: "unavailable"; reason: string };

/** Whether the browsable catalog is all of it, some of it, or none of it. */
export type CatalogState =
  | { status: "complete" }
  | { status: "partial"; reason: string }
  | { status: "unavailable"; reason: string };
