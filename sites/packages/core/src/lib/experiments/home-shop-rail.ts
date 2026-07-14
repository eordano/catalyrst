
export const HOME_SHOP_RAIL_EXPERIMENT_KEY = "lp_home_shop_rail";

export const HOME_SHOP_RAIL_STORY_DIR = "landings/home-shop-rail";

export const HOME_SHOP_RAIL_ARMS = ["base", "cta", "rail"] as const;

export type HomeShopRailArm = (typeof HOME_SHOP_RAIL_ARMS)[number];

export function activeHomeShopRailExperiment(
  raw: string | undefined | null,
): typeof HOME_SHOP_RAIL_STORY_DIR | null {
  const v = raw?.trim();
  if (!v) return null;
  return v === "home-shop-rail" ||
    v === HOME_SHOP_RAIL_STORY_DIR ||
    v === HOME_SHOP_RAIL_EXPERIMENT_KEY
    ? HOME_SHOP_RAIL_STORY_DIR
    : null;
}

export function homeShopRailFromFlags(
  flags: Record<string, unknown>,
): HomeShopRailArm | null {
  const arm = flags["shopEntry"];
  if (typeof arm !== "string") return null;
  return (HOME_SHOP_RAIL_ARMS as readonly string[]).includes(arm)
    ? (arm as HomeShopRailArm)
    : null;
}

export { armOverride } from "./create-entry";

export const HOME_SHOP_RAIL_TARGETS = {
  shop: "/shop?from=home-shop-rail",
} as const;

export function homeShopItemPath(itemId: string): string {
  return `/marketplace/${encodeURIComponent(itemId)}?from=home-shop-rail`;
}
