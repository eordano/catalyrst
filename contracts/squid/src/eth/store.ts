import { In } from "typeorm";
import {
  Account,
  AnalyticsDayData,
  Bid,
  Count,
  Data,
  ENS,
  Estate,
  NFT,
  Order,
  Parcel,
  Wearable,
  Network as ModelNetwork,
  Item,
  Collection,
  Metadata,
} from "../model";
import { Context } from "./processor";
import { getAddresses } from "../common/utils/addresses";
import { Network } from "@dcl/schemas";
import { EthereumInMemoryState } from "./types";

export const getStoredData = async (
  ctx: Context,
  ids: Pick<
    EthereumInMemoryState,
    | "accountIds"
    | "landTokenIds"
    | "estateTokenIds"
    | "ensTokenIds"
    | "tokenIds"
    | "analyticsIds"
    | "bidIds"
    | "collectionIds"
    | "itemIds"
  >
) => {
  const {
    accountIds,
    ensTokenIds,
    estateTokenIds,
    landTokenIds,
    tokenIds,
    analyticsIds,
    bidIds,
    collectionIds,
    itemIds,
  } = ids;

  const accountIdsToLookFor = [...accountIds].map(
    (id) => `${id}-${ModelNetwork.ETHEREUM}`
  );
  // console.log("accountIdsToLookFor: ", accountIdsToLookFor);

  const accounts = await ctx.store
    .findBy(Account, {
      id: In(accountIdsToLookFor),
      network: ModelNetwork.ETHEREUM,
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));
  // console.log("accounts: ", accounts);

  const parcels = await ctx.store
    .findBy(Parcel, { tokenId: In([...landTokenIds]) })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const estates = await ctx.store
    .findBy(Estate, { tokenId: In([...estateTokenIds]) })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const addresses = getAddresses(Network.ETHEREUM);

  // grab ids from all nfts to query
  const nftIds = [
    ...Array.from(estates.values()).map(
      (e) => `estate-${addresses.EstateRegistry}-${e.tokenId}`
    ),
    ...Array.from(parcels.values()).map(
      (p) => `parcel-${addresses.LANDRegistry}-${p.tokenId}`
    ),
    // ...Array.from(tokenIds.entries())
    //   .map(([contractAddress, tokenId]) =>
    //     tokenId.map((id) => `wearable-${contractAddress}-${id}`)
    //   )
    //   .flat(),
    ...Array.from(tokenIds.entries()) // for the NFTs created by the handleTransferWearableV1 that won't have the prefix added
      .map(([contractAddress, tokenId]) =>
        tokenId.map((id) => `${contractAddress}-${id}`)
      )
      .flat(),
    ...Array.from(ensTokenIds.values()).map(
      (tokenId) => `ens-${addresses.DCLRegistrar}-${tokenId}`
    ),
  ];

  const nfts = await ctx.store
    .find(NFT, {
      relations: {
        owner: true,
        activeOrder: true,
        metadata: true,
        item: true,
      },
      where: {
        id: In(nftIds),
        network: ModelNetwork.ETHEREUM,
      },
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const orders = await ctx.store
    .findBy(Order, {
      nft: In([...Array.from(nftIds.values())]),
      network: ModelNetwork.ETHEREUM,
      // id: In([...Array.from(nfts.values()).map((nft) => nft.activeOrder)]), // @TODO, revisit this
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const datas = await ctx.store
    .find(Data, {
      where: [
        { estate: In([...estateTokenIds]) },
        { parcel: In([...landTokenIds]) },
      ],
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const wearablesIds: string[] = [];
  Array.from(tokenIds.entries()).forEach(([contractAddress, tokenId]) => {
    for (const id of tokenId) {
      wearablesIds.push(`wearable-${contractAddress}-${id}`);
    }
  });

  const wearables = await ctx.store
    .findBy(Wearable, {
      id: In([...Array.from(wearablesIds.values())]),
      network: ModelNetwork.ETHEREUM,
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const ens = await ctx.store
    .findBy(ENS, {
      tokenId: In([...Array.from(ensTokenIds.values())]),
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const analytics = await ctx.store
    .findBy(AnalyticsDayData, {
      id: In([...Array.from(analyticsIds.values())]),
      network: ModelNetwork.ETHEREUM,
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const counts = await ctx.store
    .findBy(Count, {
      network: ModelNetwork.ETHEREUM,
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const bids = await ctx.store
    .find(Bid, {
      relations: {
        nft: true,
      },
      where: {
        id: In([...Array.from(bidIds.values())]),
        network: ModelNetwork.ETHEREUM,
      },
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const collections = await ctx.store
    .find(Collection, {
      where: {
        network: ModelNetwork.ETHEREUM,
        id: In([...collectionIds]),
      },
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const metadataIds = [
    ...Array.from(itemIds.values())
      .map((tokenIds) => tokenIds.map((id) => id.split("-")[1]))
      .flat(),
  ];

  const metadatas = await ctx.store
    .find(Metadata, {
      relations: {
        emote: true,
        wearable: true,
      },
      where: {
        network: ModelNetwork.ETHEREUM,
        id: In([...metadataIds]),
      },
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  const items = await ctx.store
    .find(Item, {
      relations: {
        collection: true,
        metadata: true,
      },
      where: [
        {
          collection: In([...collectionIds]),
          network: ModelNetwork.ETHEREUM,
        },
        {
          id: In([...itemIds.values()].flat()),
          network: ModelNetwork.ETHEREUM,
        },
      ],
    })
    .then((q) => new Map(q.map((i) => [i.id, i])));

  return {
    parcels,
    estates,
    ens,
    datas,
    accounts,
    nfts,
    orders,
    wearables,
    analytics,
    counts,
    bids,
    items,
    collections,
    metadatas,
  };
};
