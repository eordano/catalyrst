

const EMOTE_CATEGORIES = [
  "dance",
  "stunt",
  "greetings",
  "fun",
  "poses",
  "reactions",
  "horror",
  "miscellaneous",
];

export function bucketEmoteCategory(category: string | null): string | null {
  if (category == null || EMOTE_CATEGORIES.includes(category)) return category;
  return "miscellaneous";
}

export const SLOT_ORDER = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
