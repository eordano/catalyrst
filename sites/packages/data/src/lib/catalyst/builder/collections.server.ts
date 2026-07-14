import { CatalystError } from "../client";
import {
  fetchCollections,
  fetchCreatorItems,
  mergeCollectionThumbs,
  type BuilderCollection,
  type OrphanItem,
} from "./collections";

export type CollectionsData = {
  collections: BuilderCollection[];
  items: OrphanItem[];
  usedFallback: boolean;
  error: boolean;
};

function isNotFound(r: PromiseSettledResult<unknown>): boolean {
  return (
    r.status === "rejected" &&
    r.reason instanceof CatalystError &&
    r.reason.status === 404
  );
}

export async function loadCollections(
  address: string,
  signal?: AbortSignal,
): Promise<CollectionsData> {
  const [colRes, itemRes] = await Promise.allSettled([
    fetchCollections(address, { signal }),
    fetchCreatorItems(address, { signal }),
  ]);

  let collections =
    colRes.status === "fulfilled" ? colRes.value : ([] as BuilderCollection[]);
  const allItems =
    itemRes.status === "fulfilled" ? itemRes.value : ([] as OrphanItem[]);
  collections = mergeCollectionThumbs(collections, allItems);
  const items = allItems.filter((it) => !it.collection_id);
  const error =
    (colRes.status === "rejected" && !isNotFound(colRes)) ||
    (itemRes.status === "rejected" && !isNotFound(itemRes));

  const usedFallback =
    error || (collections.length === 0 && items.length === 0);

  return { collections, items, usedFallback, error };
}
