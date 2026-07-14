import {
  Contract as ERC721Contract,
  TransferEventArgs_2,
} from "../../abi/ERC721";
import {
  Account,
  Category,
  Count,
  ENS,
  Estate,
  NFT,
  Order,
  OrderStatus,
  Parcel,
  Wearable,
  Network as ModelNetwork,
} from "../../model";
import { getCategory } from "./category";
import { Network } from "@dcl/schemas";
import { bigint } from "../../model/generated/marshal";
import { getAddresses } from "./addresses";
import {
  buildEstateFromNFT,
  buildParcelFromNFT,
  getAdjacentToRoad,
  getDistanceToPlaza,
  getEstateImage,
  getParcelImage,
  getParcelText,
  isInBounds,
} from "../../eth/LANDs/utils";
import { createAccount, createOrLoadAccount } from "../modules/account";
import { Coordinate } from "../../types";
import {
  buildWearableFromNFT,
  getWearableImage,
} from "../../eth/modules/wearable";
import { buildENSFromNFT } from "../../eth/modules/ens";
import { buildCountFromNFT } from "../../eth/modules/count";
import {
  isWearableAccessory,
  isWearableHead,
} from "../modules/metadata/wearable";
import { normalizeTimestamp } from "./utils";

export function getNFTId(
  contractAddress: string,
  tokenId: string,
  category?: string
): string {
  return category
    ? category + "-" + contractAddress + "-" + tokenId
    : contractAddress + "-" + tokenId;
}

// Minimal structural types for RPC contract reads: only what ContractBase needs.
// Both the Polygon (evm-processor) and ETH (Portal + rpc-client) contexts satisfy
// these, so this helper stays shared across processors without coupling to either.
type RpcContext = {
  _chain: {
    client: { call: <T = any>(method: string, params?: unknown[]) => Promise<T> };
  };
};
type RpcBlock = { height: number };

export async function getTokenURI(
  ctx: RpcContext,
  block: RpcBlock,
  contractAddress: string,
  tokenId: bigint
): Promise<string> {
  const contract = new ERC721Contract(ctx, block, contractAddress);
  const tokenURI = await contract.tokenURI(tokenId);
  return tokenURI;
}

export async function getOwner(
  ctx: RpcContext,
  block: RpcBlock,
  contractAddress: string,
  tokenId: bigint
): Promise<string> {
  const contract = new ERC721Contract(ctx, block, contractAddress);
  return await contract.ownerOf(tokenId);
}

export function updateNFTOrderProperties(nft: NFT, order: Order): void {
  if (order.status == OrderStatus.open) {
    addNFTOrderProperties(nft, order);
  } else if (
    order.status == OrderStatus.sold ||
    order.status == OrderStatus.cancelled
  ) {
    clearNFTOrderProperties(nft);
  }
}

export function addNFTOrderProperties(nft: NFT, order: Order) {
  nft.activeOrder = order;
  nft.searchOrderStatus = order.status;
  nft.searchOrderPrice = order.price;
  nft.searchOrderCreatedAt = order.createdAt;
  nft.searchOrderExpiresAt = order.expiresAt;
  nft.searchOrderExpiresAtNormalized = normalizeTimestamp(order.expiresAt);
}

export function clearNFTOrderProperties(nft: NFT): void {
  nft.activeOrder = null;
  nft.searchOrderStatus = null;
  nft.searchOrderPrice = null;
  nft.searchOrderCreatedAt = null;
  nft.searchOrderExpiresAt = null;
  nft.searchOrderExpiresAtNormalized = null;
}

export function cancelActiveOrder(order: Order, now: bigint): Order {
  if (
    order &&
    (order.status == OrderStatus.open ||
      order.status == OrderStatus.transferred)
  ) {
    // Here we are setting old orders as cancelled, because the smart contract allows new orders to be created
    // and they just overwrite them in place. But the subgraph stores all orders ever
    // you can also overwrite ones that are expired
    order.status = OrderStatus.cancelled;
    order.updatedAt = now;
  }
  return order;
}

export function isMint(from: string): boolean {
  return from === "0x0000000000000000000000000000000000000000";
}
