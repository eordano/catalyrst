const HUD_LINKS: Record<string, string> = {
  "Explorer/Pages/Marketplace": "/bevy-overlay/explore?tab=marketplace",
  "Explorer/Pages/Places": "/bevy-overlay/explore?tab=places",
  "Explorer/Pages/Reel": "/bevy-overlay/explore?tab=reel",
  "Explorer/Pages/Backpack": "/bevy-overlay/backpack-equip",
  "Explorer/Pages/BackpackEmotes": "/bevy-overlay/backpack-emotes",
  "Explorer/Pages/Passport": "/bevy-overlay/passport",
  "Explorer/Pages/Settings": "/bevy-overlay/settings",
  "Explorer/Components/Notifications": "/bevy-overlay/notifications",
  "Explorer/Pages/Map": "/bevy-overlay/map-jump",
  "Explorer/Pages/Help": "/support",
};

const ADDRESSED = new Set([
  "/bevy-overlay/backpack-equip",
  "/bevy-overlay/backpack-emotes",
  "/bevy-overlay/passport",
]);

export function hudLinkFor(linkto: string | null | undefined, address: string | null): string | null {
  const path = linkto ? HUD_LINKS[linkto] : undefined;
  if (!path) return null;
  if (!address || !ADDRESSED.has(path)) return path;
  return `${path}?address=${encodeURIComponent(address)}`;
}

export function hudLinkPath(target: EventTarget | null, address: string | null): string | null {
  const el =
    target && typeof (target as Element).closest === "function"
      ? (target as Element).closest("[data-sb-linkto]")
      : null;
  return hudLinkFor(el?.getAttribute("data-sb-linkto"), address);
}
