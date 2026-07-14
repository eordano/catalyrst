import { ChainId, Network } from "@dcl/schemas";
import { assertNotNull } from "@subsquid/util-internal";
import { portalSource } from "../common/utils/portal";
import { DataSourceBuilder, FieldSelection } from "@subsquid/evm-stream";
import * as evmObjects from "@subsquid/evm-objects";
import { RpcClient } from "@subsquid/rpc-client";
import { createLogger, Logger } from "@subsquid/logger";
import { Store } from "@subsquid/typeorm-store";
import { getAddresses } from "../common/utils/addresses";
import { getBlockRange } from "../config";
import * as landRegistryAbi from "../abi/LANDRegistry";
import * as estateRegistryAbi from "../abi/EstateRegistry";
import * as erc721Abi from "../abi/ERC721";
import * as marketplaceAbi from "../abi/Marketplace";
import * as dclRegistrarAbi from "../abi/DCLRegistrar";
import * as dclControllerV2 from "../abi/DCLControllerV2";
import * as erc721BidAbi from "../abi/ERC721Bid";
import * as MarketplaceV3 from "../abi/DecentralandMarketplaceEthereum";
import * as SpokeABI from "../abi/Spoke";

const addresses = getAddresses(Network.ETHEREUM);
const chainId = process.env.ETHEREUM_CHAIN_ID || ChainId.ETHEREUM_MAINNET;

// SQD Network Portal dataset (replaces the deprecated v2 archive gateway). See portalSource for
// the shared-vs-public endpoint selection and why it hinges on SQD_PORTAL_API_KEY.
const PORTAL_DATASET = `ethereum-${
  chainId == ChainId.ETHEREUM_MAINNET ? "mainnet" : "sepolia"
}`;
const RPC_ENDPOINT = process.env.RPC_ENDPOINT_ETH;
const FROM_BLOCK = getBlockRange(Network.ETHEREUM).from;

// Field selection for the Portal stream. Portal fetches ONLY these fields -- unlike
// the v2 gateway it does not merge a default set -- so anything a handler reads must
// be listed here (required fields like log topic indices are always present).
export const fields = {
  block: { timestamp: true },
  log: { address: true, topics: true, data: true, transactionHash: true },
  transaction: { hash: true, from: true, to: true, input: true },
} satisfies FieldSelection;
export type Fields = typeof fields;

// RPC client for contract-state reads (owner(), tokenURI multicall, cuts, ...).
// Portal only ingests logs/blocks; eth_call still goes through the RPC endpoint,
// so Portal (data) and RPC (state) run as two independent channels.
export const rpc = new RpcClient({
  url: assertNotNull(RPC_ENDPOINT, "RPC_ENDPOINT_ETH is not set"),
  capacity: 10,
  rateLimit: 10,
});

export const logger: Logger = createLogger("sqd:eth");

// A ChainContext for the generated ABI contract wrappers (ContractBase), backed by
// the RPC client above. Shape matches @subsquid/evm-abi's `Chain` interface.
export const chainContext = {
  _chain: {
    client: {
      call: <T = any>(method: string, params?: unknown[]): Promise<T> =>
        rpc.call<T>(method, params as any[]),
    },
  },
};

// Types kept API-compatible with the previous evm-processor exports so handlers and
// util files do not need to change. `Block` is the block HEADER (matching the prior
// naming); `Context` is the augmented batch context, including `_chain` for RPC.
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

const erc721TransferTopics = [
  erc721Abi.events[
    "Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)"
  ].topic,
  erc721Abi.events[
    "Transfer(address indexed,address indexed,uint256 indexed,address,bytes)"
  ].topic,
  erc721Abi.events["Transfer(address indexed,address indexed,uint256 indexed)"]
    .topic,
  erc721Abi.events["Transfer(address indexed,address indexed,uint256)"].topic,
  erc721Abi.events.OwnershipTransferred.topic,
  erc721Abi.events.AddWearable.topic,
];

export const dataSource = new DataSourceBuilder()
  .setPortal(portalSource(PORTAL_DATASET))
  .setBlockRange({ from: FROM_BLOCK })
  .setFields(fields)
  .addLog({
    where: {
      address: [
        addresses.LANDRegistry,
        addresses.EstateRegistry,
        addresses.DCLRegistrar,
        ...Object.values(addresses.collections as string[]),
      ],
      topic0: erc721TransferTopics,
    },
  })
  .addLog({
    where: {
      address: [addresses.LANDRegistry, addresses.EstateRegistry],
      topic0: [
        landRegistryAbi.events.Update.topic,
        estateRegistryAbi.events.CreateEstate.topic,
        estateRegistryAbi.events.AddLand.topic,
        estateRegistryAbi.events.RemoveLand.topic,
        estateRegistryAbi.events.Update.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.Marketplace],
      topic0: [
        marketplaceAbi.events.OrderCreated.topic,
        marketplaceAbi.events.OrderSuccessful.topic,
        marketplaceAbi.events.OrderCancelled.topic,
        marketplaceAbi.events.ChangedOwnerCutPerMillion.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.DCLRegistrar],
      topic0: [dclRegistrarAbi.events.NameRegistered.topic],
    },
  })
  .addLog({
    where: {
      address: [addresses.ERC721Bid],
      topic0: [
        erc721BidAbi.events.BidAccepted.topic,
        erc721BidAbi.events.BidCreated.topic,
        erc721BidAbi.events.BidCancelled.topic,
        erc721BidAbi.events.ChangedOwnerCutPerMillion.topic,
      ],
    },
  })
  .addLog({
    where: {
      address: [addresses.DCLControllerV2],
      topic0: [dclControllerV2.events.NameBought.topic],
    },
  })
  .addLog({
    where: {
      address: [addresses.MarketplaceV3],
      topic0: [MarketplaceV3.events.Traded.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.MarketplaceV3_V2],
      topic0: [MarketplaceV3.events.Traded.topic],
    },
    include: { transaction: true },
  })
  .addLog({
    where: {
      address: [addresses.Spoke],
      topic0: [SpokeABI.events.OrderFilled.topic],
    },
    include: { transaction: true },
  })
  .build();
