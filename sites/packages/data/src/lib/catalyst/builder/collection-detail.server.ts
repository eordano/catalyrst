import { CatalystError } from "../client";
import type { GetOptions } from "../client";
import {
  collectionIdCanExist,
  emptyCollection,
  fetchCollectionItems,
  fetchOnchainCollectionDetail,
  isContractCollectionId,
  isSimulatedCollectionId,
  itemCount,
  mergeCollectionMeta,
  type CollectionDetail,
} from "./collection-detail";

export type CollectionSource = "catalyst" | "empty";

export type LoadCollectionResult = {
  collection: CollectionDetail;
  source: CollectionSource;
  itemCount: number;
  fallback: boolean;
  missing: boolean;
};

function empty(id: string, fallback: boolean, missing: boolean): LoadCollectionResult {
  return {
    collection: emptyCollection(id),
    source: "empty",
    itemCount: 0,
    fallback,
    missing,
  };
}

export async function loadCollectionDetail(
  id: string,
  opts: GetOptions = {},
  ownerAddress?: string | null,
): Promise<LoadCollectionResult> {
  if (!collectionIdCanExist(id)) return empty(id, false, true);

  if (isSimulatedCollectionId(id)) return empty(id, true, false);

  if (isContractCollectionId(id)) {
    if (!ownerAddress) return empty(id, true, false);
    try {
      const onchain = await fetchOnchainCollectionDetail(ownerAddress, id, opts);
      if (!onchain.found) return empty(id, false, true);
      const collection: CollectionDetail = {
        ...mergeCollectionMeta(emptyCollection(id), onchain.meta!),
        wearables: onchain.wearables,
        emotes: onchain.emotes,
      };
      return {
        collection,
        source: "catalyst",
        itemCount: itemCount(collection),
        fallback: false,
        missing: false,
      };
    } catch {
      return empty(id, true, false);
    }
  }

  try {
    const live = await fetchCollectionItems(id, opts);
    const collection: CollectionDetail = {
      ...emptyCollection(id),
      wearables: live.wearables,
      emotes: live.emotes,
    };
    return {
      collection,
      source: "catalyst",
      itemCount: itemCount(collection),
      fallback: false,
      missing: false,
    };
  } catch (err) {
    return empty(id, true, err instanceof CatalystError && err.status === 404);
  }
}
