import type {
  CollectionMeta,
  EmoteItem,
  WearableItem,
} from "../builder/collection-detail";
import {
  DEFAULT_MANA_PER_ITEM,
  type PublishCollection,
  type PublishItem,
} from "@features/stories/creator-hub/wearable-publish-collection/machine";
import type { SummaryView } from "@features/stories/creator-hub/wearable-publish-collection/PublishCollectionWizard";

export const MANA_PER_ITEM: number = DEFAULT_MANA_PER_ITEM;

export function collectionName(id: string): string {
  return id ? `Collection ${id.slice(0, 8)}` : "";
}

export function toPublishItems(
  wearables: WearableItem[],
  emotes: EmoteItem[],
): PublishItem[] {
  return [
    ...wearables.map(
      (w): PublishItem => ({
        id: w.id,
        name: w.name,
        rarity: w.rarity,
        kind: "wearable",
      }),
    ),
    ...emotes.map(
      (e): PublishItem => ({
        id: e.id,
        name: e.name,
        rarity: e.rarity,
        kind: "emote",
      }),
    ),
  ];
}

export function buildPublishCollection(
  id: string,
  items: PublishItem[],
  name?: string,
): PublishCollection {
  return { id, name: name ?? collectionName(id), items };
}

export function buildSummary(
  id: string,
  wearables: WearableItem[],
  emotes: EmoteItem[],
  meta?: CollectionMeta | null,
): SummaryView {
  return {
    collection: {
      name: meta?.name ?? collectionName(id),
      status: meta?.status ?? "unsynced",
      isPublished: meta?.isPublished ?? false,
      isApproved: meta?.isApproved ?? false,
      isOnSale: false,
      isLocked: meta?.isPublished ?? false,
    },
    wearables,
    emotes,
  };
}
