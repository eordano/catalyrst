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

export const CATEGORIES = [
  "eyebrows",
  "eyes",
  "facial_hair",
  "hair",
  "mouth",
  "upper_body",
  "lower_body",
  "feet",
  "earring",
  "eyewear",
  "hat",
  "helmet",
  "mask",
  "tiara",
  "top_head",
  "skin",
  "hands_wear",
  "dance",
] as const;

export const BODY_SHAPES = ["male", "female"] as const;

export const RARITY_MAX_SUPPLY: Record<(typeof RARITIES)[number], number> = {
  unique: 1,
  mythic: 10,
  exotic: 50,
  legendary: 100,
  epic: 1000,
  rare: 5000,
  uncommon: 10000,
  common: 100000,
} as const;

export function maxSupplyFor(rarity: string): number {
  return RARITY_MAX_SUPPLY[rarity as (typeof RARITIES)[number]] ?? 0;
}
