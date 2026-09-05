import { ChainId, Network } from "@dcl/schemas";
import { getCategory } from "../common/utils/category";
import { EthereumInMemoryState } from "./types";
import { Category } from "../model";
import { getAddresses } from "../common/utils/addresses";
import { Contract as MarketplaceContract } from "../abi/Marketplace";
import { Contract as ERC721BidContract } from "../abi/ERC721Bid";
import { Context } from "./processor";
import { BlockData } from "./processor";
import { startBlockByNetwork } from "./data/contracts/start-blocks";

export const getBatchInMemoryState: () => EthereumInMemoryState = () => ({
  transfers: new Map(),
  collectionIds: new Set(),
  mints: new Map(),
  itemIds: new Map(),
  tokenIds: new Map(),
  landTokenIds: new Set(),
  estateTokenIds: new Set(),
  ensTokenIds: new Set(),
  accountIds: new Set(),
  analyticsIds: new Set(),
  transferEvents: new Map(),
  estateEvents: [],
  parcelEvents: [],
  ensEvents: [],
  markteplaceEvents: [],
  bidIds: new Set(),
});

export const addEventToStateIdsBasedOnCategory = (
  nftAddress: string,
  assetId: bigint,
  {
    landTokenIds,
    estateTokenIds,
    ensTokenIds,
    tokenIds,
  }: Pick<
    EthereumInMemoryState,
    "landTokenIds" | "estateTokenIds" | "ensTokenIds" | "tokenIds"
  >
) => {
  const category = getCategory(Network.ETHEREUM, nftAddress);
  if (category === Category.parcel) {
    landTokenIds.add(assetId);
  } else if (category === Category.estate) {
    estateTokenIds.add(assetId);
  } else if (category === Category.ens) {
    ensTokenIds.add(assetId);
  } else {
    tokenIds.set(nftAddress, [...(tokenIds.get(nftAddress) || []), assetId]);
  }
};

export let marketplaceOwnerCutPerMillion: bigint | null = null;
export let bidOwnerCutPerMillion: bigint | null = null;

export const getMarketplaceOwnerCutPerMillion = () => {
  return marketplaceOwnerCutPerMillion;
};

export const getBidOwnerCutPerMillion = () => {
  return bidOwnerCutPerMillion;
};

export const setMarketplaceOwnerCutPerMillion = (value: bigint) => {
  marketplaceOwnerCutPerMillion = value;
};

export const setBidOwnerCutPerMillion = (value: bigint) => {
  bidOwnerCutPerMillion = value;
};

const getContractOwnerCutPerMillion = async (
  ctx: Context,
  block: BlockData,
  contract: "Marketplace" | "ERC721Bid"
) => {
  const addresses = getAddresses(Network.ETHEREUM);
  const c =
    contract === "ERC721Bid"
      ? new ERC721BidContract(ctx, block.header, addresses.Marketplace)
      : new MarketplaceContract(ctx, block.header, addresses.Marketplace);

  const value = await c.ownerCutPerMillion();
  if (contract === "Marketplace") {
    marketplaceOwnerCutPerMillion = value;
  } else {
    bidOwnerCutPerMillion = value;
  }
};

export const getOwnerCutsValues = async (ctx: Context, block: BlockData) => {
  const chainId = process.env.ETHEREUM_CHAIN_ID || ChainId.ETHEREUM_MAINNET;
  if (marketplaceOwnerCutPerMillion === null) {
    const marketplaceContractCreation =
      startBlockByNetwork[chainId].MarketplaceProxy;
    if (block.header.height >= marketplaceContractCreation) {
      await getContractOwnerCutPerMillion(ctx, block, "Marketplace");
    }
  }
  if (bidOwnerCutPerMillion === null) {
    const bidERC721Creation = startBlockByNetwork[chainId].ERC721Bid;
    if (block.header.height >= bidERC721Creation) {
      await getContractOwnerCutPerMillion(ctx, block, "ERC721Bid");
    }
  }
  return {
    marketplaceOwnerCutPerMillion,
    bidOwnerCutPerMillion,
  };
};
