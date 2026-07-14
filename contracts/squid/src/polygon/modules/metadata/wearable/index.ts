import { ChainId, Network } from "@dcl/schemas";
import {
  Collection,
  Item,
  Metadata,
  Wearable,
  WearableBodyShape,
  WearableCategory,
  WearableRarity,
  Network as ModelNetwork,
  NFT,
} from "../../../../model";
import {
  isWearableAccessory,
  isWearableHead,
} from "../../../../common/modules/metadata/wearable";

const baseDecentralandURN = "urn:decentraland:";

/**
 * @dev The item's rawMetadata for wearables should follow: version:item_type:name:description:category:bodyshapes
 * If the item has been rescues, the metadata could be be version:item_type:name:description:category:bodyshapes:prev_hash:new_entity_timestamp
 * @param item
 */
export function buildWearableItem(
  item: Item,
  wearables: Map<string, Wearable>
): Wearable | null {
  const id = item.id;
  const data = item.rawMetadata.split(":");
  if (
    (data.length == 6 || data.length == 8) &&
    isValidWearableCategory(data[4]) &&
    isValidBodyShape(data[5].split(","))
  ) {
    let wearable = wearables.get(id);

    if (!wearable) {
      wearable = new Wearable({ id });
    }

    wearable.collection = item.collection.id;
    wearable.name = data[2];
    wearable.description = data[3];
    wearable.rarity = item.rarity as WearableRarity;
    wearable.category = data[4] as WearableCategory;
    wearable.bodyShapes = data[5].split(",") as WearableBodyShape[]; // Could be more than one
    wearable.network = ModelNetwork.POLYGON;
    return wearable;
  }

  return null;
}

function getNetwork(network: Network): string {
  const chainId =
    network === Network.ETHEREUM
      ? process.env.ETHEREUM_CHAIN_ID
      : process.env.POLYGON_CHAIN_ID;
  const chainName =
    network === Network.ETHEREUM
      ? chainId === ChainId.ETHEREUM_MAINNET.toString()
        ? "ethereum"
        : "sepolia"
      : chainId === ChainId.MATIC_MAINNET.toString()
      ? "matic"
      : "amoy";
  return chainName;
}

function isValidWearableCategory(category: string): boolean {
  if (
    category == "eyebrows" ||
    category == "eyes" ||
    category == "facial_hair" ||
    category == "hair" ||
    category == "mouth" ||
    category == "upper_body" ||
    category == "lower_body" ||
    category == "feet" ||
    category == "earring" ||
    category == "eyewear" ||
    category == "hat" ||
    category == "helmet" ||
    category == "mask" ||
    category == "tiara" ||
    category == "top_head" ||
    category == "skin" ||
    category == "hands_wear"
  ) {
    return true;
  }

  console.log("ERROR: Invalid Wearable Category {}", [category]);

  return false;
}

export function isValidBodyShape(bodyShapes: string[]): boolean {
  for (let i = 0; i++; i < bodyShapes.length) {
    let bodyShape = bodyShapes[i];
    if (bodyShape !== "BaseFemale" && bodyShape !== "BaseMale") {
      console.log("ERROR: Invalid BodyShape {}", [bodyShape]);
      return false;
    }
  }

  return true;
}

export function getURNForCollectionV1(
  collection: Collection,
  network: Network
): string {
  const collectionName = collection.name.split("dcl://");
  return (
    baseDecentralandURN +
    getNetwork(network) +
    ":collections-v1:" +
    (collectionName.length > 1 ? collectionName[1] : collectionName[0])
  );
}

export function getURNForCollectionV2(
  collectionAddress: string,
  network: Network
): string {
  return (
    baseDecentralandURN +
    getNetwork(network) +
    ":collections-v2:" +
    collectionAddress
  );
}

export function getURNForWearableV1(
  collection: Collection,
  representationId: string,
  network: Network
): string {
  return getURNForCollectionV1(collection, network) + ":" + representationId;
}

export function getURNForWearableV2(
  collectionAddress: string,
  itemId: string,
  network: Network
): string {
  return getURNForCollectionV2(collectionAddress, network) + ":" + itemId;
}

export function setItemWearableSearchFields(
  item: Item,
  metadatas: Map<string, Metadata>,
  wearables: Map<string, Wearable>
): Item {
  if (!item.metadata) {
    return item;
  }
  const metadata = metadatas.get(item.metadata.id);
  if (metadata && metadata.wearable) {
    const wearable = wearables.get(metadata.wearable.id);
    if (wearable) {
      item.searchText =
        `${wearable.name} ${wearable.description}`.toLowerCase();
      item.searchIsWearableHead = isWearableHead(wearable.category);
      item.searchIsWearableAccessory = isWearableAccessory(wearable.category);
      item.searchWearableCategory = wearable.category;
      item.searchWearableBodyShapes = wearable.bodyShapes;
      item.searchWearableRarity = wearable.rarity;
    } else {
      console.log(`ERROR: Wearable not found for item ${item.id}`);
    }
    item.searchItemType = item.itemType;
  } else if (!metadata) {
    console.log(`ERROR: Metadata not found for item ${item.id}`);
  } else if (!metadata.wearable) {
    console.log(`ERROR: Wearable not found for metadata ${metadata.id}`);
  }

  return item;
}

export function setNFTWearableSearchFields(
  nft: NFT,
  metadatas: Map<string, Metadata>
): NFT {
  if (!nft.metadata) {
    return nft;
  }
  const metadata = metadatas.get(nft.metadata.id);
  if (metadata && metadata.wearable) {
    const wearable = metadata.wearable;

    if (wearable) {
      nft.searchText = `${wearable.name} ${wearable.description}`;
      nft.searchItemType = nft.itemType;
      nft.searchIsWearableHead = isWearableHead(wearable.category);
      nft.searchIsWearableAccessory = isWearableAccessory(wearable.category);
      nft.searchWearableCategory = wearable.category;
      nft.searchWearableBodyShapes = wearable.bodyShapes;
      nft.searchWearableRarity = wearable.rarity;
    }
  }

  return nft;
}
