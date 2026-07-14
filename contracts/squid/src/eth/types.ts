import { BlockData, Log } from "./processor";
import * as landRegistryABI from "../abi/LANDRegistry";
import * as erc721abi from "../abi/ERC721";
import * as estateRegistryABI from "../abi/EstateRegistry";
import * as dclRegistrarABI from "../abi/DCLRegistrar";
import * as marketplaceAbi from "../abi/Marketplace";
import * as erc721BidAbi from "../abi/ERC721Bid";
import * as DCLControllerV2Abi from "../abi/DCLControllerV2";
import * as MarketplaceV3ABI from "../abi/DecentralandMarketplaceEthereum";
import { Mint, Transfer } from "../model";

export type EthereumInMemoryState = {
  mints: Map<string, Mint>;
  transfers: Map<string, Transfer>;
  collectionIds: Set<string>;
  itemIds: Map<string, string[]>;
  tokenIds: Map<string, bigint[]>;
  landTokenIds: Set<bigint>;
  estateTokenIds: Set<bigint>;
  ensTokenIds: Set<bigint>;
  accountIds: Set<string>;
  analyticsIds: Set<string>;
  bidIds: Set<string>;
  transferEvents: Map<
    string,
    { event: erc721abi.TransferEventArgs_2; block: BlockData }[]
  >;
  parcelEvents: {
    topic: string;
    event: landRegistryABI.UpdateEventArgs;
    block: BlockData;
  }[];
  ensEvents: {
    topic: string;
    event:
      | dclRegistrarABI.NameRegisteredEventArgs
      | DCLControllerV2Abi.NameBoughtEventArgs;
    block: BlockData;
    log: Log & { transactionHash: string };
  }[];
  markteplaceEvents: {
    topic: string;
    event:
      | marketplaceAbi.OrderCreatedEventArgs
      | marketplaceAbi.OrderSuccessfulEventArgs
      | marketplaceAbi.OrderCancelledEventArgs
      | erc721BidAbi.BidCreatedEventArgs
      | erc721BidAbi.BidCancelledEventArgs
      | erc721BidAbi.BidAcceptedEventArgs
      | erc721abi.TransferEventArgs_2
      | erc721abi.OwnershipTransferredEventArgs
      | erc721abi.AddWearableEventArgs
      | MarketplaceV3ABI.TradedEventArgs
      | estateRegistryABI.CreateEstateEventArgs
      | estateRegistryABI.UpdateEventArgs
      | estateRegistryABI.AddLandEventArgs
      | estateRegistryABI.RemoveLandEventArgs;
    block: BlockData;
    log: Log & { transactionHash: string };
    marketplaceOwnerCutPerMillion?: bigint | null;
    bidOwnerCutPerMillion?: bigint | null;
  }[];
};
