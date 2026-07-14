import { getCatalystBase } from "../../common/utils/catalyst";
import { ChainId, Network } from "@dcl/schemas";
import {
  NFT,
  WearableBodyShape,
  WearableCategory,
  WearableRarity,
  Wearable as WearableModel,
  Network as ModelNetwork,
  Metadata,
  ItemType,
  Item,
} from "../../model";
import {
  Wearable,
  atari_launch,
  binance_us_collection,
  china_flying,
  community_contest,
  cybermike_cybersoldier_set,
  cz_mercenary_mtz,
  dappcraft_moonminer,
  dc_meta,
  dc_niftyblocksmith,
  dcg_collection,
  dcl_launch,
  dg_atari_dillon_francis,
  dg_fall_2020,
  dg_summer_2020,
  dgtble_headspace,
  digital_alchemy,
  ethermon_wearables,
  exclusive_masks,
  halloween_2019,
  halloween_2020,
  mch_collection,
  meme_dontbuythis,
  mf_sammichgamer,
  ml_liondance,
  ml_pekingopera,
  moonshot_2020,
  pm_dreamverse_eminence,
  pm_outtathisworld,
  rac_basics,
  release_the_kraken,
  rtfkt_x_atari,
  stay_safe,
  sugarclub_yumi,
  tech_tribal_marc0matic,
  threelau_basics,
  winklevoss_capital,
  wonderzone_meteorchaser,
  wonderzone_steampunk,
  wz_wonderbot,
  xmas_2019,
  xmas_2020,
  xmash_up_2020,
  wearable_test,
} from "../data/wearables";
import { Wearable as WearableEntity } from "../../model";
import { getNetwork as getURNNetwork } from "../../common/utils/network";

export function buildWearableFromNFT(nft: NFT): WearableEntity {
  // https://wearable-api.decentraland.org/v2/standards/erc721-metadata/collections/halloween_2019/wearables/funny_skull_mask/1
  let wearableId = "";
  if (nft.tokenURI) {
    wearableId = getWearableIdFromTokenURI(nft.tokenURI);
    if (wearableId === "") {
      console.log(
        `ERROR: Could not get a wearable id from tokenURI ${nft.tokenURI} and nft ${nft.id}`
      );
      return new WearableEntity({ id: "" });
    }
  } else {
    console.log(`ERROR: NFT ${nft.id} does not have a tokenURI`);
  }

  const allCollections: Wearable[][] = [
    atari_launch,
    binance_us_collection,
    china_flying,
    community_contest,
    cybermike_cybersoldier_set,
    cz_mercenary_mtz,
    dappcraft_moonminer,
    dc_meta,
    dc_niftyblocksmith,
    dcg_collection,
    dcl_launch,
    dg_atari_dillon_francis,
    dg_fall_2020,
    dg_summer_2020,
    dgtble_headspace,
    digital_alchemy,
    ethermon_wearables,
    exclusive_masks,
    halloween_2019,
    halloween_2020,
    mch_collection,
    meme_dontbuythis,
    mf_sammichgamer,
    ml_liondance,
    ml_pekingopera,
    moonshot_2020,
    pm_dreamverse_eminence,
    pm_outtathisworld,
    rac_basics,
    release_the_kraken,
    rtfkt_x_atari,
    stay_safe,
    sugarclub_yumi,
    tech_tribal_marc0matic,
    threelau_basics,
    winklevoss_capital,
    wonderzone_meteorchaser,
    wonderzone_steampunk,
    wz_wonderbot,
    xmas_2019,
    xmas_2020,
    xmash_up_2020,
  ];
  const collectionNames: string[] = [
    "atari_launch",
    "binance_us_collection",
    "china_flying",
    "community_contest",
    "cybermike_cybersoldier_set",
    "cz_mercenary_mtz",
    "dappcraft_moonminer",
    "dc_meta",
    "dc_niftyblocksmith",
    "dcg_collection",
    "dcl_launch",
    "dg_atari_dillon_francis",
    "dg_fall_2020",
    "dg_summer_2020",
    "dgtble_headspace",
    "digital_alchemy",
    "ethermon_wearables",
    "exclusive_masks",
    "halloween_2019",
    "halloween_2020",
    "mch_collection",
    "meme_dontbuythis",
    "mf_sammichgamer",
    "ml_liondance",
    "ml_pekingopera",
    "moonshot_2020",
    "pm_dreamverse_eminence",
    "pm_outtathisworld",
    "rac_basics",
    "release_the_kraken",
    "rtfkt_x_atari",
    "stay_safe",
    "sugarclub_yumi",
    "tech_tribal_marc0matic",
    "3lau_basics", // threelau_basics
    "winklevoss_capital",
    "wonderzone_meteorchaser",
    "wonderzone_steampunk",
    "wz_wonderbot",
    "xmas_2019",
    "xmas_2020",
    "xmash_up_2020",
  ];
  for (let i = 0; i < allCollections.length; i++) {
    const wearable = findWearable(wearableId, allCollections[i]);

    if (wearable.id === wearableId) {
      wearable.id = nft.id;
      wearable.collection = collectionNames[i];
      wearable.owner = nft.owner;
      return wearable;
    }
  }

  console.log(
    `ERROR Could not find a wearable for the id ${wearableId} found on the tokenURI ${nft.tokenURI} and nft ${nft.id}`
  );
  return new WearableEntity({ id: "" });
}

export function getWearableImage(wearable: WearableEntity): string {
  const chainId = parseInt(
    process.env.ETHEREUM_CHAIN_ID || ChainId.ETHEREUM_MAINNET.toString()
  ) as ChainId;
  const baseURI = getCatalystBase(chainId);
  const urn = getWearableURN(wearable);

  return baseURI + "/lambdas/collections/contents/" + urn + "/thumbnail";
}

export function getWearableURN(wearable: WearableEntity): string {
  return (
    "urn:decentraland:" +
    getURNNetwork(Network.ETHEREUM) +
    ":collections-v1:" +
    wearable.collection +
    ":" +
    wearable.representationId
  );
}

function findWearable(id: string, collection: Wearable[]): WearableEntity {
  for (let i = 0; i < collection.length; i++) {
    const representation = collection[i];
    if (id === representation.id) {
      // TODO: representation.toEntity()
      const wearable = new WearableEntity({ id });
      wearable.representationId = representation.id;
      wearable.name = representation.name;
      wearable.description = representation.description;
      wearable.category = representation.category as WearableCategory;
      wearable.rarity = representation.rarity as WearableRarity;
      wearable.bodyShapes = representation.bodyShapes as WearableBodyShape[];
      wearable.network = ModelNetwork.ETHEREUM;
      return wearable;
    }
  }

  return new WearableEntity({ id: "" });
}

export function getWearableIdFromTokenURI(tokenURI: string): string {
  let splitted = tokenURI.split("/");

  // https://wearable-api.decentraland.org/v2/standards/erc721-metadata/collections/halloween_2019/wearables/funny_skull_mask/1
  // or
  // dcl://halloween_2019/vampire_feet/55
  if (splitted.length == 11 || splitted.length == 5) {
    let ids = splitted.slice(-2);
    return ids[0];
  }

  return "";
}

export function getWearableV1Representation(
  wearableId: string
): WearableEntity | null {
  if (!wearableId) {
    return null;
  }

  const allCollections: Wearable[][] = [
    binance_us_collection,
    china_flying,
    community_contest,
    cybermike_cybersoldier_set,
    cz_mercenary_mtz,
    dappcraft_moonminer,
    dc_meta,
    dc_niftyblocksmith,
    dcg_collection,
    dcl_launch,
    dg_fall_2020,
    dg_summer_2020,
    dgtble_headspace,
    digital_alchemy,
    ethermon_wearables,
    exclusive_masks,
    halloween_2019,
    mch_collection,
    mf_sammichgamer,
    ml_pekingopera,
    moonshot_2020,
    pm_dreamverse_eminence,
    pm_outtathisworld,
    stay_safe,
    sugarclub_yumi,
    tech_tribal_marc0matic,
    wonderzone_meteorchaser,
    wonderzone_steampunk,
    wz_wonderbot,
    xmas_2019,
    halloween_2020,
    xmas_2020,
    xmash_up_2020,
    release_the_kraken,
    threelau_basics,
    meme_dontbuythis,
    ml_liondance,
    atari_launch,
    rtfkt_x_atari,
    rac_basics,
    winklevoss_capital,
    dg_atari_dillon_francis,
    wearable_test,
  ];

  for (let i = 0; i < allCollections.length; i++) {
    const wearable = findWearable(wearableId, allCollections[i]);
    if (wearable != null && wearable.id == wearableId) {
      return wearable;
    }
  }

  return null;
}

export function buildWearableV1(
  item: Item,
  representation: WearableEntity
): WearableModel {
  let wearable = new WearableModel({ id: representation.id });

  wearable.collection = item.collection.id;
  wearable.name = representation.name;
  wearable.description = representation.description;
  wearable.rarity = representation.rarity as WearableRarity;
  wearable.category = representation.category as WearableCategory;
  wearable.bodyShapes = representation.bodyShapes as WearableBodyShape[];
  wearable.network = ModelNetwork.ETHEREUM;

  return wearable;
}

export function buildWearableV1Metadata(
  item: Item,
  representation: WearableEntity,
  wearables: Map<string, WearableModel>,
  metadatas: Map<string, Metadata>
): Metadata {
  const metadata = new Metadata({ id: representation.id });
  metadata.network = ModelNetwork.ETHEREUM;

  const wearable = buildWearableV1(item, representation);
  wearables.set(wearable.id, wearable);

  metadata.itemType = ItemType.wearable_v1;
  metadata.wearable = wearable;

  metadatas.set(metadata.id, metadata);

  return metadata;
}

export function getIssuedIdFromTokenURI(tokenURI: string): number {
  let splitted = tokenURI.split("/");

  // https://wearable-api.decentraland.org/v2/standards/erc721-metadata/collections/halloween_2019/wearables/funny_skull_mask/1
  // or
  // dcl://halloween_2019/vampire_feet/55
  if (splitted.length == 11 || splitted.length == 5) {
    let issuedId = splitted.slice(-1);
    return parseInt(issuedId[0], 10);
  }

  return 0;
}
