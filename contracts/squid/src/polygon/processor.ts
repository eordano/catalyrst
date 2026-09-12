import { assertNotNull } from "@subsquid/util-internal";
import { portalSource } from "../common/utils/portal";
import { DataSourceBuilder, FieldSelection } from "@subsquid/evm-stream";
import * as evmObjects from "@subsquid/evm-objects";
import { RpcClient } from "@subsquid/rpc-client";
import { createLogger, Logger } from "@subsquid/logger";
import { Store } from "@subsquid/typeorm-store";
import { ChainId, Network } from "@dcl/schemas";
import * as CollectionFactoryABI from "./abi/CollectionFactory";
import * as RaritiesWithOracleABI from "./abi/RaritiesWithOracle";
import * as CollectionFactoryV3ABI from "./abi/CollectionFactoryV3";
import * as MarketplaceABI from "./abi/Marketplace";
import * as BidABI from "./abi/ERC721Bid";
import * as MarketplaceV2ABI from "./abi/MarketplaceV2";
import * as CommitteeABI from "./abi/Committee";
import * as CollectionV2ABI from "./abi/CollectionV2";
import * as RaritiesABI from "./abi/Rarity";
import * as CollectionManagerABI from "./abi/CollectionManager";
import * as OffChainMarketplace from "./abi/DecentralandMarketplacePolygon";
import * as OffChainMarketplaceV3 from "./abi/DecentralandMarketplacePolygonV3";
import * as CreditsManagerABI from "./abi/CreditsManager";
import * as SpokeABI from "../abi/Spoke";
import { getBlockRange } from "../config";
import { getAddresses } from "../common/utils/addresses";
import { loadCollections } from "./utils/loaders";
import { startBlockByNetwork } from "./addresses/startBlocks";

const addresses = getAddresses(Network.MATIC);
const chainId = process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET;

const PORTAL_DATASET = `polygon-${
  chainId == ChainId.MATIC_MAINNET ? "mainnet" : "amoy-testnet"
}`;
const RPC_ENDPOINT = process.env.RPC_ENDPOINT_POLYGON;
const collections = loadCollections();

export const fields = {
  block: { timestamp: true },
  log: { address: true, topics: true, data: true, transactionHash: true },
  transaction: { hash: true, from: true, to: true, input: true },
} satisfies FieldSelection;
export type Fields = typeof fields;

export const rpc = new RpcClient({
  url: assertNotNull(RPC_ENDPOINT, "RPC_ENDPOINT_POLYGON is not set"),
  capacity: 10,
  rateLimit: 10,
});

export const logger: Logger = createLogger("sqd:polygon");

export const chainContext = {
  _chain: {
    client: {
      call: <T = any>(method: string, params?: unknown[]): Promise<T> =>
        rpc.call<T>(method, params as any[]),
    },
  },
};

export type Block = evmObjects.BlockHeader<Fields>;
export type Log = evmObjects.Log<Fields>;
export type Transaction = evmObjects.Transaction<Fields>;
export type BlockData = evmObjects.Block<Fields>;
export interface Context {
  _chain: typeof chainContext._chain;
  store: Store;
  log: Logger;
  blocks: BlockData[];
  isHead: boolean;
}

const collectionV2Topics = [
  CollectionV2ABI.events.SetGlobalMinter.topic,
  CollectionV2ABI.events.SetGlobalManager.topic,
  CollectionV2ABI.events.SetItemMinter.topic,
  CollectionV2ABI.events.SetItemManager.topic,
  CollectionV2ABI.events.AddItem.topic,
  CollectionV2ABI.events.RescueItem.topic,
  CollectionV2ABI.events.UpdateItemData.topic,
  CollectionV2ABI.events.Issue.topic,
  CollectionV2ABI.events.SetApproved.topic,
  CollectionV2ABI.events.SetEditable.topic,
  CollectionV2ABI.events.Complete.topic,
  CollectionV2ABI.events.CreatorshipTransferred.topic,
  CollectionV2ABI.events.OwnershipTransferred.topic,
  CollectionV2ABI.events.Transfer.topic,
];

export const dataSource = new DataSourceBuilder()
  .setPortal(portalSource(PORTAL_DATASET))
  .setBlockRange(getBlockRange(Network.MATIC))
  .setFields(fields)
  .addLog({
    where: {
      address: [addresses.CollectionFactory, addresses.CollectionFactoryV3],
      topic0: [
        CollectionFactoryABI.events.ProxyCreated.topic,
        CollectionFactoryV3ABI.events.ProxyCreated.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.Marketplace, addresses.MarketplaceV2],
      topic0: [
        MarketplaceABI.events.OrderCreated.topic,
        MarketplaceABI.events.OrderSuccessful.topic,
        MarketplaceABI.events.OrderCancelled.topic,
        MarketplaceV2ABI.events.OrderCreated.topic,
        MarketplaceV2ABI.events.OrderSuccessful.topic,
        MarketplaceV2ABI.events.OrderCancelled.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.Bid, addresses.BidV2],
      topic0: [
        BidABI.events.BidCreated.topic,
        BidABI.events.BidAccepted.topic,
        BidABI.events.BidCancelled.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.OldCommittee, addresses.Committee],
      topic0: [CommitteeABI.events.MemberSet.topic],
    },
  })
  .addLog({
    where: { address: collections.addresses, topic0: collectionV2Topics },
    include: { transaction: true },
    range: {
      from: startBlockByNetwork[parseInt(chainId.toString())].Factory,
      to: collections.height,
    },
  })
  .addLog({
    where: { topic0: collectionV2Topics },
    include: { transaction: true },
    range: { from: collections.height + 1 },
  })
  .addLog({
    where: {
      address: [addresses.Rarity, addresses.RaritiesWithOracle],
      topic0: [
        RaritiesABI.events.AddRarity.topic,
        RaritiesABI.events.UpdatePrice.topic,
        RaritiesWithOracleABI.events.AddRarity.topic,
        RaritiesWithOracleABI.events.UpdatePrice.topic,
      ],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.CollectionManager],
      topic0: [CollectionManagerABI.events.RaritiesSet.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.OffChainMarketplace],
      topic0: [OffChainMarketplace.events.Traded.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.OffChainMarketplaceV2],
      topic0: [OffChainMarketplace.events.Traded.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.OffChainMarketplaceV3],
      topic0: [OffChainMarketplaceV3.events.Traded.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.OffChainMarketplace],
      topic0: [
        OffChainMarketplace.events.FeeCollectorUpdated.topic,
        OffChainMarketplace.events.FeeRateUpdated.topic,
        OffChainMarketplace.events.RoyaltiesRateUpdated.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: addresses.CreditsManager,
      topic0: [CreditsManagerABI.events.CreditUsed.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.Spoke],
      topic0: [SpokeABI.events.OrderCreated.topic],
    },
    include: { transaction: true },
    range: {
      from: startBlockByNetwork[parseInt(chainId.toString())].Spoke,
    },
  })
  .build();
