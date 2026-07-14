import { fetchCollectionItems } from "../builder/collection-detail";
import type { GetOptions } from "../client";
import type { PublishCollection } from "@features/stories/creator-hub/wearable-publish-collection/machine";
import type { SummaryView } from "@features/stories/creator-hub/wearable-publish-collection/PublishCollectionWizard";
import {
  MANA_PER_ITEM,
  buildPublishCollection,
  buildSummary,
  toPublishItems,
} from "./wearable-publish-collection";

export { MANA_PER_ITEM };

export type PublishCollectionData = {
  collection: PublishCollection;
  summary: SummaryView;
  manaPerItem: number;
  id: string;
  fallback: boolean;
};

function emptyData(id: string): PublishCollectionData {
  return {
    collection: { id, name: "", items: [] },
    summary: buildSummary(id, [], []),
    manaPerItem: MANA_PER_ITEM,
    id,
    fallback: true,
  };
}

export async function loadPublishCollection(
  id?: string | null,
  opts: GetOptions = {},
): Promise<PublishCollectionData> {
  const wanted = (id ?? "").trim();
  if (!wanted) return emptyData(wanted);

  try {
    const { wearables, emotes } = await fetchCollectionItems(wanted, opts);
    const items = toPublishItems(wearables, emotes);
    if (items.length === 0) return emptyData(wanted);

    const collection: PublishCollection = buildPublishCollection(wanted, items);
    const summary: SummaryView = {
      collection: {
        name: collection.name,
        status: "unsynced",
        isPublished: false,
        isApproved: false,
        isOnSale: false,
        isLocked: false,
      },
      wearables,
      emotes,
    };

    return {
      collection,
      summary,
      manaPerItem: MANA_PER_ITEM,
      id: wanted,
      fallback: false,
    };
  } catch {
    return emptyData(wanted);
  }
}
