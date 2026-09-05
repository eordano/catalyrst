
export const WHATSON_SHOP_ENTRY_EXPERIMENT_KEY = "lp_whatson_shop_entry";

export const WHATSON_SHOP_ENTRY_STORY_DIR = "landings/whatson-shop-entry";

export const WHATSON_SHOP_ENTRY_ARMS = ["base", "pill", "rail"] as const;

export type WhatsOnShopEntryArm = (typeof WHATSON_SHOP_ENTRY_ARMS)[number];

export function activeWhatsOnShopEntryExperiment(
  raw: string | undefined | null,
): typeof WHATSON_SHOP_ENTRY_STORY_DIR | null {
  const v = raw?.trim();
  if (!v) return null;
  return v === "whatson-shop-entry" ||
    v === WHATSON_SHOP_ENTRY_STORY_DIR ||
    v === WHATSON_SHOP_ENTRY_EXPERIMENT_KEY
    ? WHATSON_SHOP_ENTRY_STORY_DIR
    : null;
}

export function whatsOnShopEntryFromFlags(
  flags: Record<string, unknown>,
): WhatsOnShopEntryArm | null {
  const arm = flags["shopEntry"];
  if (typeof arm !== "string") return null;
  return (WHATSON_SHOP_ENTRY_ARMS as readonly string[]).includes(arm)
    ? (arm as WhatsOnShopEntryArm)
    : null;
}

export { armOverride } from "./create-entry";

export const WHATSON_SHOP_ENTRY_TARGETS = {
  shop: "/shop?from=whatson-shop-entry",
} as const;

export function whatsOnShopItemPath(itemId: string): string {
  return `/marketplace/${encodeURIComponent(itemId)}?from=whatson-shop-entry`;
}
