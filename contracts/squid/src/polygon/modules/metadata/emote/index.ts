import {
  Emote,
  EmoteCategory,
  Item,
  Metadata,
  NFT,
  WearableBodyShape,
  WearableRarity,
  EmoteOutcomeType,
} from "../../../../model";
import { isValidBodyShape } from "../wearable";
import {
  DANCE,
  FUN,
  GREETINGS,
  HORROR,
  MISCELLANEOUS,
  POSES,
  REACTIONS,
  STUNT,
} from "./categories";
import { mapOutcomeToString, OUTCOMES } from "./outcomes";

export function buildEmoteItem(
  item: Item,
  emotes: Map<string, Emote>
): Emote | null {
  const id = item.id;
  const data = item.rawMetadata.split(":");
  const dataHasValidLength =
    data.length == 6 || data.length == 7 || data.length == 8 || data.length == 9;
  if (dataHasValidLength && isValidBodyShape(data[5].split(","))) {
    let emote = emotes.get(id);

    if (!emote) {
      emote = new Emote({ id });
    }

    emote.collection = item.collection.id;
    emote.name = data[2];
    emote.description = data[3];
    emote.rarity = item.rarity as WearableRarity;
    const isValidCategory = isValidEmoteCategory(data[4]);
    if (!isValidCategory) {
      console.log(
        `ERROR: Invalid Emote Category ${data[4]} for item ${id} and data ${data} with rawMetadata ${item.rawMetadata}`
      );
    }
    emote.category = (isValidCategory ? data[4] : DANCE) as EmoteCategory;
    emote.bodyShapes = data[5].split(",") as WearableBodyShape[];
    emote.loop =
      data.length >= 7 && isValidLoopValue(data[6]) && data[6] == "1"
        ? true
        : false;
    const isOutcomeType = data.length >= 8 && OUTCOMES.includes(data[7])
    emote.hasGeometry = data.length >= 8 && !isOutcomeType && data[7].includes("g")
    emote.hasSound = data.length >= 8 && !isOutcomeType && data[7].includes("s")
    emote.outcomeType = handleEmoteOutcomeType(data)

    return emote;
  }

  return null;
}

const handleEmoteOutcomeType = (data: string[]): EmoteOutcomeType | null => {
  if (data.length >= 8 && OUTCOMES.includes(data[7])) {
    return mapOutcomeToString(data[7]) as EmoteOutcomeType
  }

  if (data.length >= 9 && OUTCOMES.includes(data[8])) {
    return mapOutcomeToString(data[8]) as EmoteOutcomeType
  }

  return null
}

function isValidEmoteCategory(category: string): boolean {
  if (
    category == DANCE ||
    category == STUNT ||
    category == GREETINGS ||
    category == FUN ||
    category == POSES ||
    category == REACTIONS ||
    category == HORROR ||
    category == MISCELLANEOUS
  ) {
    return true;
  }

  console.log(`ERROR: Invalid Emote Category ${category}`);

  return false;
}

function isValidLoopValue(value: string): boolean {
  if (value == "0" || value == "1") {
    return true;
  }

  console.log("ERROR: Invalid Emote Loop value {}", [value]);

  return false;
}

export function setItemEmoteSearchFields(
  item: Item,
  metadatas: Map<string, Metadata>,
  emotes: Map<string, Emote>
): Item {
  if (!item.metadata) {
    return item;
  }
  const metadata = metadatas.get(item.metadata.id);
  if (metadata) {
    if (metadata.emote) {
      let emote = emotes.get(metadata.emote.id);
      if (emote) {
        item.searchText = `${emote.name} ${emote.description}`.toLowerCase();
        item.searchEmoteCategory = emote.category;
        item.searchEmoteLoop = emote.loop;
        item.searchEmoteBodyShapes = emote.bodyShapes;
        item.searchEmoteRarity = emote.rarity;
        item.searchEmoteHasSound = emote.hasSound;
        item.searchEmoteHasGeometry = emote.hasGeometry;
        item.searchEmoteOutcomeType = emote.outcomeType;
      }
      item.searchItemType = item.itemType;
    } else {
      console.log("ERROR: Emote not found for item {}", [item.id]);
    }
  }

  return item;
}

export function setNFTEmoteSearchFields(
  nft: NFT,
  metadatas: Map<string, Metadata>
): NFT {
  if (!nft.metadata) {
    console.log("ERROR: Metadata not found for NFT:", nft.id);
    return nft;
  }
  const metadata = metadatas.get(nft.metadata.id);
  if (metadata) {
    if (metadata.emote) {
      const emote = metadata.emote
      if (emote) {
        nft.searchText = `${emote.name} ${emote.description}`.toLowerCase();
        nft.searchEmoteCategory = emote.category;
        nft.searchEmoteLoop = emote.loop;
        nft.searchEmoteBodyShapes = emote.bodyShapes;
        nft.searchEmoteRarity = emote.rarity;
      }
      nft.searchItemType = nft.itemType;
    } else {
      console.log("ERROR: Emote not found for NFT: ", nft.id);
    }
  } else {
    console.log("ERROR: Metadata not found for NFT:", nft.id);
  }

  return nft;
}
