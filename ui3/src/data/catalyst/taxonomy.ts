// The fixed vocabularies the wearable and emote readers are written against,
// and the one rule that reads one of them.
//
// Separate from schemas/backpack.ts, which a perf build aliases to an accepting
// stub: a vocabulary put there would come back as a shim, and `bucketEmoteCategory`
// decides output rather than acceptance, so it has to run in both modes.

export const RARITIES = [
  "unique",
  "mythic",
  "exotic",
  "legendary",
  "epic",
  "rare",
  "uncommon",
  "common",
];

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
];

export const EMOTE_CATEGORIES = [
  "dance",
  "stunt",
  "greetings",
  "fun",
  "poses",
  "reactions",
  "horror",
  "miscellaneous",
];

/**
 * A category the UI has no tab for is still a stated category, so it lands in
 * the catch-all bucket. One the metadata never stated stays unstated.
 */
export function bucketEmoteCategory(category: string | null): string | null {
  if (category == null || EMOTE_CATEGORIES.includes(category)) return category;
  return "miscellaneous";
}

export const SLOT_ORDER = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
