
export const BLOG_SHOP_ENTRY_EXPERIMENT_KEY = "lp_blog_shop_entry";

export const BLOG_SHOP_ENTRY_STORY_DIR = "misc/blog-shop-entry";

export const BLOG_SHOP_ENTRY_ARMS = ["base", "card", "rail"] as const;

export type BlogShopEntryArm = (typeof BLOG_SHOP_ENTRY_ARMS)[number];

export function activeBlogShopEntryExperiment(
  raw: string | undefined | null,
): typeof BLOG_SHOP_ENTRY_STORY_DIR | null {
  const v = raw?.trim();
  if (!v) return null;
  return v === "blog-shop-entry" ||
    v === BLOG_SHOP_ENTRY_STORY_DIR ||
    v === BLOG_SHOP_ENTRY_EXPERIMENT_KEY
    ? BLOG_SHOP_ENTRY_STORY_DIR
    : null;
}

export function blogShopEntryFromFlags(
  flags: Record<string, unknown>,
): BlogShopEntryArm | null {
  const arm = flags["shopEntry"];
  if (typeof arm !== "string") return null;
  return (BLOG_SHOP_ENTRY_ARMS as readonly string[]).includes(arm)
    ? (arm as BlogShopEntryArm)
    : null;
}

export { armOverride } from "./create-entry";

export const BLOG_SHOP_ENTRY_TARGETS = {
  shop: "/shop?from=blog-shop-entry",
} as const;

export function blogShopItemPath(itemId: string): string {
  return `/marketplace/${encodeURIComponent(itemId)}?from=blog-shop-entry`;
}
