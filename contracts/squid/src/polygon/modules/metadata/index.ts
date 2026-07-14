import * as itemTypes from "./itemTypes";
import {
  Emote,
  Item,
  ItemType,
  Metadata,
  NFT,
  Network,
  Wearable,
} from "../../../model";
import {
  buildWearableItem,
  setItemWearableSearchFields,
  setNFTWearableSearchFields,
} from "./wearable";
import {
  buildEmoteItem,
  setItemEmoteSearchFields,
  setNFTEmoteSearchFields,
} from "./emote";

/**
 * @notice the item's metadata must follow: version:item_type:representation_id:data
 */
export function buildItemMetadata(
  item: Item,
  metadatas: Map<string, Metadata>,
  wearables: Map<string, Wearable>,
  emotes: Map<string, Emote>
): Metadata {
  const id = item.id;
  let metadata = metadatas.get(id);

  if (!metadata) {
    metadata = new Metadata({ id });
    metadata.network = Network.POLYGON;
    metadatas.set(id, metadata);
  }

  const data = item.rawMetadata.split(":");
  if (data.length >= 2) {
    const type = data[1];

    if (type === itemTypes.WEARABLE_TYPE_SHORT) {
      const wearable = buildWearableItem(item, wearables);
      if (wearable) {
        metadata.itemType = ItemType.wearable_v2;
        metadata.wearable = wearable;
        wearables.set(item.id, wearable);
      } else {
        metadata.itemType = ItemType.undefined;
      }
    } else if (type === itemTypes.SMART_WEARABLE_TYPE_SHORT) {
      const wearable = buildWearableItem(item, wearables);
      if (wearable) {
        metadata.itemType = ItemType.smart_wearable_v1;
        metadata.wearable = wearable;
        wearables.set(item.id, wearable);
      } else {
        metadata.itemType = ItemType.undefined;
      }
    } else if (type === itemTypes.EMOTE_TYPE_SHORT) {
      const emote = buildEmoteItem(item, emotes);
      if (emote) {
        metadata.itemType = ItemType.emote_v1;
        metadata.emote = emote;
        emotes.set(item.id, emote);
      } else {
        metadata.itemType = ItemType.undefined;
      }
    } else {
      metadata.itemType = ItemType.undefined;
    }
  } else {
    metadata.itemType = ItemType.undefined;
  }

  return metadata;
}

export function setItemSearchFields(
  item: Item,
  metadatas: Map<string, Metadata>,
  wearables: Map<string, Wearable>,
  emotes?: Map<string, Emote>
): void {
  if (
    item.itemType === itemTypes.WEARABLE_V2 ||
    item.itemType === itemTypes.WEARABLE_V1 ||
    item.itemType === itemTypes.SMART_WEARABLE_V1
  ) {
    setItemWearableSearchFields(item, metadatas, wearables);
  }
  if (item.itemType === itemTypes.EMOTE_V1 && emotes) {
    setItemEmoteSearchFields(item, metadatas, emotes);
  }
}

export function setNFTSearchFields(
  nft: NFT,
  metadatas: Map<string, Metadata>
): NFT {
  if (
    nft.itemType == itemTypes.WEARABLE_V2 ||
    nft.itemType == itemTypes.WEARABLE_V1 ||
    nft.itemType == itemTypes.SMART_WEARABLE_V1
  ) {
    return setNFTWearableSearchFields(nft, metadatas);
  }

  if (nft.itemType == itemTypes.EMOTE_V1) {
    return setNFTEmoteSearchFields(nft, metadatas);
  }

  return nft;
}
