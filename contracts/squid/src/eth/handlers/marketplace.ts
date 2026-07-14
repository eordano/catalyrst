import { BlockData } from "../processor";
import { Network } from "@dcl/schemas";
import {
  OrderCancelledEventArgs,
  OrderCreatedEventArgs,
  OrderSuccessfulEventArgs,
} from "../../abi/Marketplace";
import * as MarketplaceV3ABI from "../../abi/DecentralandMarketplaceEthereum";
import { getCategory } from "../../common/utils/category";
import {
  cancelActiveOrder,
  getNFTId,
  updateNFTOrderProperties,
} from "../../common/utils";
import {
  Account,
  AnalyticsDayData,
  Category,
  Count,
  Item,
  NFT,
  Order,
  OrderStatus,
  Sale,
  Network as ModelNetwork,
  SaleType,
} from "../../model";
import { ORDER_SALE_TYPE, trackSale } from "../modules/analytics";
import { Context } from "../processor";
import { buildCountFromOrder } from "../modules/count";
import { TradedEventArgs } from "../../abi/DecentralandMarketplaceEthereum";
import { getAddresses } from "../../common/utils/addresses";
import {
  getTradeEventData,
  getTradeEventType,
  TradeType,
} from "../../common/utils/marketplaceV3";
import { normalizeTimestamp } from "../../common/utils/utils";

export type MarkteplaceEvents =
  | OrderCreatedEventArgs
  | OrderSuccessfulEventArgs
  | OrderCancelledEventArgs;

export function handleOrderCreated(
  event: OrderCreatedEventArgs,
  block: BlockData,
  contractAddress: string,
  txHash: string,
  orders: Map<string, Order>,
  nfts: Map<string, NFT>,
  counts: Map<string, Count>
): void {
  const { assetId, nftAddress, id, seller, priceInWei, expiresAt } = event;

  const category = getCategory(Network.ETHEREUM, nftAddress);
  const nftId = getNFTId(
    nftAddress,
    assetId.toString(),
    category !== Category.wearable ? category : undefined
  );
  const nft = nfts.get(nftId);
  if (nft) {
    const orderId = id;

    const order = new Order({ id: orderId });
    order.network = ModelNetwork.ETHEREUM;
    order.marketplaceAddress = contractAddress;
    order.status = OrderStatus.open;
    order.category = category as Category;
    order.nft = nft;
    order.item = nft.item;
    order.nftAddress = nftAddress;
    order.tokenId = assetId;
    order.txHash = txHash;
    order.owner = seller;
    order.price = priceInWei;
    order.expiresAt = expiresAt;

    order.expiresAtNormalized = normalizeTimestamp(expiresAt);

    order.blockNumber = BigInt(block.header.height);
    const timestamp = BigInt(block.header.timestamp / 1000);
    order.createdAt = timestamp;
    order.updatedAt = timestamp;

    if (nft.activeOrder) {
      const oldOrder = orders.get(nft.activeOrder.id);
      if (oldOrder) {
        cancelActiveOrder(oldOrder, timestamp);
      } else {
        console.log(`ERROR: Order not found when trying to cancel order ${id}`);
      }
    }
    nft.updatedAt = timestamp;
    updateNFTOrderProperties(nft, order);

    buildCountFromOrder(order, counts);
    orders.set(orderId, order);
  } else {
    console.log(`ERROR: NFT not found for order created ${nftId}`);
  }
}

export async function handleOrderSuccessful(
  ctx: Context,
  event: OrderSuccessfulEventArgs,
  block: BlockData,
  txHash: string,
  ownerCutPerMillionValue: bigint,
  orders: Map<string, Order>,
  nfts: Map<string, NFT>,
  accounts: Map<string, Account>,
  analytics: Map<string, AnalyticsDayData>,
  counts: Map<string, Count>,
  sales: Map<string, Sale>,
  items: Map<string, Item>
): Promise<void> {
  const { assetId, buyer, id, nftAddress, seller, totalPrice } = event;
  const category = getCategory(Network.ETHEREUM, nftAddress);
  const nftId = getNFTId(
    nftAddress,
    assetId.toString(),
    category !== Category.wearable ? category : undefined
  );
  const orderId = id;
  const order = orders.get(orderId);
  if (!order) {
    console.log(`ERROR: Order not found for order successful ${orderId}`);
    return;
  }

  order.category = category as Category;
  order.status = OrderStatus.sold;
  order.buyer = buyer;
  order.price = totalPrice;
  order.blockNumber = BigInt(block.header.height);
  const timestamp = BigInt(block.header.timestamp / 1000);
  order.updatedAt = timestamp;

  const nft = nfts.get(nftId);
  if (!nft) {
    console.log(`ERROR: NFT not found for order successful ${nftId}`);
    return;
  }

  const buyerAccount = accounts.get(`${buyer}-${ModelNetwork.ETHEREUM}`);
  if (buyerAccount) {
    nft.owner = buyerAccount;
    nft.ownerAddress = buyerAccount.address;
  } else {
    console.log("ERROR: Buyer account not found for order successful");
  }

  nft.updatedAt = timestamp;
  updateNFTOrderProperties(nft!, order!);

  await trackSale(
    ctx,
    block.header,
    ORDER_SALE_TYPE,
    buyer,
    seller,
    nft.id,
    order.price,
    ownerCutPerMillionValue,
    BigInt(block.header.timestamp / 1000),
    txHash,
    nfts,
    sales,
    accounts,
    analytics,
    counts,
    items
  );
}

export async function handleTraded(
  ctx: Context,
  event: TradedEventArgs,
  block: BlockData,
  txHash: string,
  nfts: Map<string, NFT>,
  accounts: Map<string, Account>,
  analytics: Map<string, AnalyticsDayData>,
  counts: Map<string, Count>,
  sales: Map<string, Sale>,
  items: Map<string, Item>
): Promise<void> {
  const tradeType = getTradeEventType(event, Network.ETHEREUM);
  const tradeData = getTradeEventData(event, Network.ETHEREUM);
  // Nothing to index: not an order or a bid (a giveaway has no payment leg).
  if (!tradeData) {
    return;
  }
  const { collectionAddress, price, buyer, seller, tokenId } = tradeData;

  if (!tokenId) {
    console.log(`ERROR: tokenId not found in trade event`);
    return;
  }

  const category = getCategory(Network.ETHEREUM, collectionAddress);
  const nftId = getNFTId(
    collectionAddress,
    tokenId.toString(),
    category !== Category.wearable ? category : undefined
  );

  const timestamp = BigInt(block.header.timestamp / 1000);

  const nft = nfts.get(nftId);
  if (!nft) {
    console.log(`ERROR: NFT not found for trade ${nftId}`);
    return;
  }

  const buyerAccount = accounts.get(`${buyer}-${ModelNetwork.ETHEREUM}`);
  if (buyerAccount) {
    nft.owner = buyerAccount;
    nft.ownerAddress = buyerAccount.address;
  } else {
    console.log("ERROR: Buyer account not found for order successful");
  }

  nft.updatedAt = timestamp;

  const addresses = getAddresses(Network.ETHEREUM);
  const marketplaceV3Contract = new MarketplaceV3ABI.Contract(
    ctx,
    block.header,
    addresses.MarketplaceV3
  );

  await trackSale(
    ctx,
    block.header,
    tradeType === TradeType.Bid ? SaleType.bid : SaleType.order,
    buyer,
    seller,
    nft.id,
    price,
    await marketplaceV3Contract.feeRate(),
    BigInt(block.header.timestamp / 1000),
    txHash,
    nfts,
    sales,
    accounts,
    analytics,
    counts,
    items
  );
}

export function handleOrderCancelled(
  event: OrderCancelledEventArgs,
  block: BlockData,
  nfts: Map<string, NFT>,
  orders: Map<string, Order>
): void {
  const { assetId, id, nftAddress, seller } = event;
  const category = getCategory(Network.ETHEREUM, nftAddress);
  const nftId = getNFTId(
    nftAddress,
    assetId.toString(),
    category !== Category.wearable ? category : undefined
  );
  const orderId = id;

  const nft = nfts.get(nftId);
  const order = orders.get(orderId);

  if (nft && order) {
    order.category = category as Category;
    order.status = OrderStatus.cancelled;
    order.blockNumber = BigInt(block.header.height);
    const timestamp = BigInt(block.header.timestamp / 1000);
    order.updatedAt = timestamp;

    nft.updatedAt = timestamp;
    updateNFTOrderProperties(nft, order);
  } else if (!nft) {
    console.log(
      `ERROR: NFT not found for order cancelled orderId: ${id}, nftId: ${nftId}`
    );
  } else if (!order) {
    console.log(
      `ERROR: Order not found for order cancelled orderId: ${id}, nftId: ${nftId}`
    );
  }
}

export const isOrderCreatedEvent = (
  event: MarkteplaceEvents
): event is OrderCreatedEventArgs =>
  "id" in event && "assetId" in event && "seller" in event;
