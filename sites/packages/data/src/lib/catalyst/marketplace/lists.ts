import { z } from "zod";

import { getJSON, type GetOptions } from "../client";
import { dataOf } from "../envelope";
import {
  FavoriteListSchema,
  ListsEnvelopeSchema,
} from "../generated-schemas/market";
import { formatMana, toCardNetwork } from "./index";

export { FavoriteListSchema };

type WireFavoriteList = z.infer<typeof FavoriteListSchema>;

/**
 * UI item card source. The lists wire rows carry only `previewOfItemIds`; item
 * rows are hydrated separately (or not at all), so this is a plain UI type.
 */
export type ListItem = {
  id: string;
  itemId: string | null;
  name: string | null;
  thumbnail: string | null;
  category: string | null;
  rarity: string | null;
  contractAddress: string | null;
  network: string | null;
  isOnSale: boolean;
  price: string | null;
  creator: string | null;
};

export type List = {
  id: string;
  name: string;
  itemsCount: number;
  isPrivate: boolean;
  description: string | null;
  userAddress: string | null;
  createdAt: number | null;
  updatedAt: number | null;
  permission: string | null;
  previewOfItemIds: string[];
  pickedByUser: boolean | null;
  items: ListItem[];
};

export function normalizeList(w: WireFavoriteList): List {
  return {
    id: w.id,
    name: w.name,
    itemsCount: w.itemsCount,
    isPrivate: w.isPrivate,
    description: w.description ?? null,
    userAddress: w.userAddress,
    createdAt: w.createdAt,
    updatedAt: w.updatedAt ?? null,
    permission: w.permission ?? null,
    previewOfItemIds: w.previewOfItemIds,
    // The wire carries neither of these: pickedByUser is unknown until a
    // picks read says otherwise, and item rows come from a separate hydrate.
    pickedByUser: null,
    items: [],
  };
}

const ListEnvelopeSchema = dataOf(z.unknown());

/**
 * `null` means the read failed or did not parse, which the caller has to render
 * as a broken panel. `[]` is the real "this wallet has no lists".
 */
export async function loadLists(
  userAddress: string,
  opts: GetOptions = {},
): Promise<List[] | null> {
  if (!userAddress) return [];
  try {
    const env = ListsEnvelopeSchema.safeParse(
      await getJSON<unknown>("/market/v1/lists", {
        ...opts,
        query: { first: 100, userAddress },
      }),
    );
    if (!env.success) return null;
    return env.data.data.results.map(normalizeList);
  } catch {
    return null;
  }
}

export async function loadList(
  id: string,
  userAddress: string,
  opts: GetOptions = {},
): Promise<List | null> {
  try {
    const env = ListEnvelopeSchema.safeParse(
      await getJSON<unknown>(`/market/v1/lists/${encodeURIComponent(id)}`, opts),
    );
    const parsed = FavoriteListSchema.safeParse(env.success ? env.data.data : undefined);
    if (
      parsed.success &&
      parsed.data.userAddress.toLowerCase() === userAddress.toLowerCase()
    ) {
      return normalizeList(parsed.data);
    }
  } catch {
  }
  const all = await loadLists(userAddress, opts);
  return all?.find((l) => l.id === id) ?? null;
}

const RARITIES = new Set([
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
]);

function safeRarity(r: string | null | undefined): string {
  return r && RARITIES.has(r) ? r : "common";
}

export function listItemPrice(item: ListItem): string | null {
  if (!item.isOnSale) return null;
  return formatMana(item.price);
}

export type ListItemCard = {
  id: string;
  name: string;
  collection: string | undefined;
  price: string | null;
  credits: string | null;
  rarity: string;
  network: "ethereum" | "polygon";
  image: string | undefined;
};

export function toListItemCard(
  item: ListItem,
  credits: string | null = null,
): ListItemCard {
  return {
    id: item.id,
    name: item.name ?? "Untitled",
    collection: item.category ?? undefined,
    price: listItemPrice(item),
    credits,
    rarity: safeRarity(item.rarity),
    network: toCardNetwork(item.network),
    image: item.thumbnail ?? undefined,
  };
}

export type ListCard = {
  id: string;
  name: string;
  description: string | null;
  itemsCount: number;
  isPrivate: boolean;
  pickedByUser: boolean | null;
  previews: { id: string; image: string | undefined; rarity: string }[];
};

export function toListCard(list: List): ListCard {
  const byId = new Map(list.items.map((it) => [it.id, it]));
  const ids = list.previewOfItemIds.length
    ? list.previewOfItemIds
    : list.items.map((it) => it.id);
  const previews = ids.slice(0, 4).map((id) => {
    const it = byId.get(id);
    return {
      id,
      image: it?.thumbnail ?? undefined,
      rarity: safeRarity(it?.rarity),
    };
  });
  return {
    id: list.id,
    name: list.name,
    description: list.description,
    itemsCount: list.itemsCount,
    isPrivate: list.isPrivate,
    pickedByUser: list.pickedByUser,
    previews,
  };
}
