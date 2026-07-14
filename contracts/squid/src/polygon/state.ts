import { ChainId, Network } from "@dcl/schemas";
import { PolygonInMemoryState } from "./types";
import { Sale } from "../model";
import { getAddresses } from "../common/utils/addresses";
import { Contract as MarketplaceContract } from "./abi/Marketplace";
import { Contract as MarketplaceV2Contract } from "./abi/MarketplaceV2";
import { Contract as MarketplaceV3Contract } from "./abi/DecentralandMarketplacePolygon";
import { Contract as CollectionStoreContract } from "./abi/CollectionStore";
import { Contract as ERC721BidV2Contract } from "./abi/ERC721BidV2";
import { Block, Context } from "./processor";

const chainId = +(process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET);

export const getBatchInMemoryState: () => PolygonInMemoryState = () => ({
  curations: new Map(),
  mints: new Map(),
  transfers: new Map(),
  transferGiftCandidates: new Map(),
  sales: new Map<string, Sale>(),
  squidRouterOrders: new Map(),
  tokenIds: new Map(),
  accountIds: new Set(),
  collectionIds: new Set(),
  analyticsIds: new Set(),
  itemDayDataIds: new Set(),
  bidIds: new Set(),
  consumedIssueLogs: new Set(),
  itemIds: new Map(),
  transferEvents: new Map(),
  collectionFactoryEvents: [],
  events: [],
  committeeEvents: [],
  rarityEvents: [],
});

export type StoreContractData = {
  fee: bigint | undefined;
  feeOwner: string | undefined;
};

export type MarketplaceContractData = {
  ownerCutPerMillion: bigint | undefined;
  owner: string | undefined;
};

export type MarketplaceV2ContractData = {
  feesCollectorCutPerMillion: bigint | undefined;
  feesCollector: string | undefined;
  royaltiesCutPerMillion: bigint | undefined;
};

export type BidContractData = {
  ownerCutPerMillion: bigint | undefined;
  owner: string | undefined;
};

export type BidV2ContractData = {
  feesCollectorCutPerMillion: bigint | undefined;
  feesCollector: string | undefined;
  royaltiesCutPerMillion: bigint | undefined;
};

export let marketplaceContractData: MarketplaceContractData = {
  ownerCutPerMillion: undefined,
  owner: undefined,
};

export let marketplaceV2ContractData: MarketplaceV2ContractData = {
  feesCollectorCutPerMillion: undefined,
  feesCollector: undefined,
  royaltiesCutPerMillion: undefined,
};

export let bidV2ContractData: BidV2ContractData = {
  feesCollectorCutPerMillion: undefined,
  feesCollector: undefined,
  royaltiesCutPerMillion: undefined,
};

export let storeContractData: StoreContractData = {
  fee: undefined,
  feeOwner: undefined,
};

export type MarketplaceV3ContractData = {
  feeCollector: string | undefined;
  feeRate: bigint | undefined;
  royaltiesRate: bigint | undefined;
};

export let marketplaceV3ContractData: MarketplaceV3ContractData = {
  feeCollector: undefined,
  feeRate: undefined,
  royaltiesRate: undefined,
};

/**
 * Fee configuration of the V3 marketplace, read from the chain ONCE and kept current from the
 * contract's own FeeCollectorUpdated / FeeRateUpdated / RoyaltiesRateUpdated events.
 *
 * handleTraded used to read all three per Traded event -- three sequential eth_calls for values
 * that change roughly never. Against the RPC client's rate limit that was ~0.3s per trade, and
 * because the calls were not attributed to any rpcTime bucket it showed up as unexplained
 * "event loop" time: 548s of a 671s batch on a prod backfill through 2025 blocks.
 *
 * Unlike the other getters here there is no start-block guard, and none is needed: this is only
 * ever called from handleTraded, and a Traded event cannot exist before the contract does.
 *
 * Deliberately NOT wrapped in try/catch, unlike its siblings. These values land in the Sale's
 * money columns (feesCollectorCut, royaltiesCut), so a read failure must fail the batch and be
 * retried -- swallowing it would either skip the sale or record it with empty fees.
 */
export const getMarketplaceV3ContractData = async (
  ctx: Context,
  block: Block
): Promise<{ feeCollector: string; feeRate: bigint; royaltiesRate: bigint }> => {
  let { feeCollector, feeRate, royaltiesRate } = marketplaceV3ContractData;
  if (
    feeCollector === undefined ||
    feeRate === undefined ||
    royaltiesRate === undefined
  ) {
    console.log("INFO: Fetching marketplace v3 contract data for first time");
    const addresses = getAddresses(Network.MATIC);
    const c = new MarketplaceV3Contract(ctx, block, addresses.MarketplaceV3);
    [feeCollector, feeRate, royaltiesRate] = await Promise.all([
      c.feeCollector(),
      c.feeRate(),
      c.royaltiesRate(),
    ]);
    marketplaceV3ContractData.feeCollector = feeCollector;
    marketplaceV3ContractData.feeRate = feeRate;
    marketplaceV3ContractData.royaltiesRate = royaltiesRate;
  }
  return { feeCollector, feeRate, royaltiesRate };
};

export const setMarketplaceV3FeeCollector = (value: string) => {
  marketplaceV3ContractData.feeCollector = value;
};

export const setMarketplaceV3FeeRate = (value: bigint) => {
  marketplaceV3ContractData.feeRate = value;
};

export const setMarketplaceV3RoyaltiesRate = (value: bigint) => {
  marketplaceV3ContractData.royaltiesRate = value;
};

// CollectionStore contract creation blocks
const START_BLOCK_COLLECTION_STORE: Record<number, number> = {
  [ChainId.MATIC_AMOY]: 5706656, // Same as MarketplaceV2 for testnet
  [ChainId.MATIC_MAINNET]: 15202567,
};

export const getStoreContractData = async (ctx: Context, block: Block) => {
  const contractStartingBlock = START_BLOCK_COLLECTION_STORE[chainId];
  
  // Only fetch if contract exists at this block height
  if (
    (storeContractData.fee === undefined ||
      storeContractData.feeOwner === undefined) &&
    block.height >= contractStartingBlock
  ) {
    console.log("INFO: Fetching store contract data for first time");
    const addresses = getAddresses(Network.MATIC);
    const storeContract = new CollectionStoreContract(
      ctx,
      block,
      addresses.CollectionStore
    );
    try {
      storeContractData.fee = await storeContract.fee();
      storeContractData.feeOwner = await storeContract.feeOwner();
    } catch (e: any) {
      // The contract may not be readable at this (historical) block on some RPC
      // providers -- e.g. fee() returns 0x and decoding throws. Leave the data
      // undefined and retry on a later batch rather than crashing the processor.
      console.log(`WARN: could not fetch store contract data: ${e.message}`);
    }
  }
  return storeContractData;
};

const START_BLOCK_MARKETPLACEV1: Record<number, number> = {
  [ChainId.MATIC_AMOY]: 14517370,
  [ChainId.MATIC_MAINNET]: 15202000,
};

export const getMarketplaceContractData = async (
  ctx: Context,
  block: Block
) => {
  const contractStartingBlock = START_BLOCK_MARKETPLACEV1[chainId];
  
  // Only fetch if contract exists at this block height (and only on mainnet)
  if (
    chainId === ChainId.MATIC_MAINNET && // there's no contract for AMOY
    (marketplaceContractData.ownerCutPerMillion === undefined ||
      marketplaceContractData.owner === undefined) &&
    block.height >= contractStartingBlock
  ) {
    console.log("INFO: Fetching Marketplace v1 contract data for first time");
    const addresses = getAddresses(Network.MATIC);
    const c = new MarketplaceContract(ctx, block, addresses.Marketplace);
    try {
      marketplaceContractData.ownerCutPerMillion = await c.ownerCutPerMillion();
      marketplaceContractData.owner = await c.owner();
    } catch (e: any) {
      console.log(`WARN: could not fetch marketplace contract data: ${e.message}`);
    }
  }
  return marketplaceContractData;
};

const START_BLOCK_MARKETPLACEV2: Record<number, number> = {
  [ChainId.MATIC_AMOY]: 5706656,
  [ChainId.MATIC_MAINNET]: 22514900,
};

export const getMarketplaceV2ContractData = async (
  ctx: Context,
  block: Block
) => {
  const contractStartingBlock = START_BLOCK_MARKETPLACEV2[chainId];
  if (
    (marketplaceV2ContractData.feesCollectorCutPerMillion === undefined ||
      marketplaceV2ContractData.feesCollector === undefined ||
      marketplaceV2ContractData.royaltiesCutPerMillion === undefined) &&
    block.height >= contractStartingBlock
  ) {
    console.log("INFO: Fetching marketplace v2 contract data for first time");
    const addresses = getAddresses(Network.MATIC);
    const c = new MarketplaceV2Contract(ctx, block, addresses.MarketplaceV2);
    try {
      marketplaceV2ContractData.feesCollectorCutPerMillion =
        await c.feesCollectorCutPerMillion();
      marketplaceV2ContractData.feesCollector = await c.feesCollector();
      marketplaceV2ContractData.royaltiesCutPerMillion =
        await c.royaltiesCutPerMillion();
    } catch (e: any) {
      console.log(`WARN: could not fetch marketplace v2 contract data: ${e.message}`);
    }
  }
  return marketplaceV2ContractData;
};

const START_BLOCK_BIDV2: Record<number, number> = {
  [ChainId.MATIC_AMOY]: 5706662,
  [ChainId.MATIC_MAINNET]: 22913743,
};

export const getBidV2ContractData = async (ctx: Context, block: Block) => {
  const contractStartingBlock = START_BLOCK_BIDV2[chainId];
  if (
    (bidV2ContractData.feesCollectorCutPerMillion === undefined ||
      bidV2ContractData.feesCollector === undefined ||
      bidV2ContractData.royaltiesCutPerMillion === undefined) &&
    block.height >= contractStartingBlock
  ) {
    console.log("INFO: Fetching bid v2 contract data for first time");
    const addresses = getAddresses(Network.MATIC);
    const c = new ERC721BidV2Contract(ctx, block, addresses.BidV2);
    try {
      bidV2ContractData.feesCollectorCutPerMillion =
        await c.feesCollectorCutPerMillion();
      bidV2ContractData.feesCollector = await c.feesCollector();
      bidV2ContractData.royaltiesCutPerMillion =
        await c.royaltiesCutPerMillion();
    } catch (e: any) {
      console.log(`WARN: could not fetch bid v2 contract data: ${e.message}`);
    }
  }
  return bidV2ContractData;
};

export const setStoreFee = (fee: bigint) => {
  storeContractData.fee = fee;
};

export const setStoreFeeOwner = (feeOwner: string) => {
  storeContractData.feeOwner = feeOwner;
};

export let marketplaceOwnerCutPerMillion: bigint | null = null;
export let marketplaceV2OwnerCutPerMillion: bigint | null = null;
export let bidOwnerCutPerMillion: bigint | null = null;

export const getMarketplaceOwnerCutPerMillion = () => {
  return marketplaceOwnerCutPerMillion;
};

export const getMarketplaceV2OwnerCutPerMillion = () => {
  return marketplaceV2OwnerCutPerMillion;
};

export const getBidOwnerCutPerMillion = () => {
  return bidOwnerCutPerMillion;
};

export const setMarketplaceOwnerCutPerMillion = (value: bigint) => {
  marketplaceOwnerCutPerMillion = value;
};

export const setMarketplaceV2OwnerCutPerMillion = (value: bigint) => {
  marketplaceV2OwnerCutPerMillion = value;
};

export const setBidOwnerCutPerMillion = (value: bigint) => {
  bidOwnerCutPerMillion = value;
};
