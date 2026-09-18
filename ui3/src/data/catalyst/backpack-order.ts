export const BACKPACK_PAGE_SIZE = 24;
export const RARITY_RANK: Record<string, number> = {
  unique: 7, mythic: 6, legendary: 5, exotic: 4, epic: 3, rare: 2, uncommon: 1, common: 0, base: 0,
};

export function firstBackpackPage<T extends { rarity?: string | null }>(items: T[]) {
  return [...items].sort((a, b) =>
    (RARITY_RANK[(b.rarity ?? "").toLowerCase()] ?? 0) - (RARITY_RANK[(a.rarity ?? "").toLowerCase()] ?? 0),
  ).slice(0, BACKPACK_PAGE_SIZE);
}
