
export const PLACES_SHOP_ENTRY_EXPERIMENT_KEY = "places_shop_entry";

export const PLACES_SHOP_ENTRY_STORY_DIR = "misc/places-shop-entry";

export const PLACES_SHOP_ENTRY_ARMS = ["base", "pill", "rail"] as const;

export type PlacesShopEntryArm = (typeof PLACES_SHOP_ENTRY_ARMS)[number];

export function activePlacesShopEntryExperiment(
  raw: string | undefined | null,
): typeof PLACES_SHOP_ENTRY_STORY_DIR | null {
  const v = raw?.trim();
  if (!v) return null;
  return v === "places-shop-entry" ||
    v === PLACES_SHOP_ENTRY_STORY_DIR ||
    v === PLACES_SHOP_ENTRY_EXPERIMENT_KEY
    ? PLACES_SHOP_ENTRY_STORY_DIR
    : null;
}

export function shopEntryFromFlags(
  flags: Record<string, unknown>,
): PlacesShopEntryArm | null {
  const arm = flags["shopEntry"];
  if (typeof arm !== "string") return null;
  return (PLACES_SHOP_ENTRY_ARMS as readonly string[]).includes(arm)
    ? (arm as PlacesShopEntryArm)
    : null;
}

export { armOverride } from "./create-entry";

export const PLACES_SHOP_ENTRY_TARGETS = {
  shop: "/shop?from=places-shop-entry",
} as const;

export function shopItemPath(itemId: string): string {
  return `/marketplace/${encodeURIComponent(itemId)}?from=places-shop-entry`;
}
