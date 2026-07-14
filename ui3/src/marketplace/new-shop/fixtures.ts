import type { ShopBanner, ShopCard, FeaturedSection } from "./NewShopHome";
import type { RankRow } from "./NewShopRankTable";
import type { FilterGroup } from "./NewShopFilterSidebar";

const RARITIES = ["common", "uncommon", "rare", "epic", "legendary", "mythic", "unique", "exotic"];
const NAMES = [
  "Golden Sneakers", "Neon Visor", "Astro Jacket", "Pixel Crown", "Cyber Cap",
  "Wave Rider", "Solar Cloak", "Retro Shades", "Void Gloves", "Aurora Boots",
  "Glass Katana", "Meadow Parcel",
];

export function makeCards(count: number, seed = 0): ShopCard[] {
  return Array.from({ length: count }, (_, i) => {
    const k = i + seed;
    return {
      id: "asset-" + k,
      name: NAMES[k % NAMES.length],
      meta: (1 + (k % 9)) + "d " + (1 + (k % 23)) + "h ago",
      price: String(50 * (1 + (k % 40))),
      rarity: RARITIES[k % RARITIES.length],
      network: k % 5 === 0 ? "ethereum" : "polygon",
    };
  });
}

export const banners: ShopBanner[] = [
  {
    id: "emotes",
    eyebrow: "Trending now",
    title: "Best Rated Emotes",
    subtitle: "The community's top-voted moves this week.",
    cta: "Shop emotes",
    tone: "purple",
  },
  {
    id: "outfits",
    eyebrow: "Curated",
    title: "Week Selected Outfits",
    subtitle: "Hand-picked wearables to refresh your look.",
    cta: "Shop outfits",
    tone: "magenta",
  },
];

export const featured: FeaturedSection[] = [
  { id: "trending", title: "Featured", cards: makeCards(8, 0) },
  { id: "fresh", title: "New Creations", cards: makeCards(8, 8) },
];

export const rankRows: RankRow[] = Array.from({ length: 6 }, (_, i) => ({
  id: "rank-" + i,
  name: NAMES[i % NAMES.length],
  verified: i % 2 === 0,
  floor: String(120 * (i + 1)),
  volume: String(4300 - i * 500),
  network: "polygon",
}));

export const filterGroups: FilterGroup[] = [
  {
    id: "category",
    label: "Category",
    options: [
      { id: "wearables", label: "Wearables", count: 3120 },
      { id: "emotes", label: "Emotes", count: 842 },
      { id: "names", label: "Names", count: 511 },
      { id: "lands", label: "Lands", count: 640 },
      { id: "parcels", label: "Parcels", count: 260 },
      { id: "estates", label: "Estates", count: 90 },
    ],
  },
  {
    id: "rarity",
    label: "Rarity",
    options: [
      { id: "legendary", label: "Legendary" },
      { id: "epic", label: "Epic" },
      { id: "rare", label: "Rare" },
      { id: "common", label: "Common" },
    ],
  },
];
