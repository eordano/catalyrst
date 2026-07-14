import { BlockData, Transaction } from "../processor";
import { Network } from "@dcl/schemas";
import {
  OrderCancelledEventArgs,
  OrderCreatedEventArgs,
  OrderSuccessfulEventArgs,
} from "../abi/Marketplace";
import { getCategory } from "../../common/utils/category";
import {
  cancelActiveOrder,
  getNFTId,
  updateNFTOrderProperties,
} from "../../common/utils";
import * as CollectionV2ABI from "../abi/CollectionV2";
import * as MarketplaceV3ABI from "../abi/DecentralandMarketplacePolygon";
import {
  Category,
  Count,
  NFT,
  Order,
  OrderStatus,
  Network as NetworkModel,
  SaleType,
} from "../../model";
import { trackSale } from "../modules/analytics";
import { PolygonInMemoryState, PolygonStoredData } from "../types";
import { buildCountFromOrder } from "../../common/modules/count";
import { getAddresses } from "../../common/utils/addresses";
import {
  MarketplaceContractData,
  MarketplaceV2ContractData,
  getMarketplaceV3ContractData,
} from "../state";
import { Context } from "../processor";
import { TradedEventArgs } from "../abi/DecentralandMarketplacePolygon";
import { handleIssue } from "./collection";
import {
  getTradeEventData,
  getTradeEventType,
  TradeAssetType,
  TradeType,
} from "../../common/utils/marketplaceV3";
import { normalizeTimestamp } from "../../common/utils/utils";
import { selectIssueLogForTrade } from "../utils/issueLog";

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

  const nftId = getNFTId(nftAddress, assetId.toString());
  const nft = nfts.get(nftId);
  if (nft) {
    const orderId = id;

    const order = new Order({ id: orderId, network: NetworkModel.POLYGON });
    order.marketplaceAddress = contractAddress;
    order.status = OrderStatus.open;
    order.category = Category.wearable;
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

    // get open order for the nft since nft.activeOrder can be undefined but an order could still be open

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

    buildCountFromOrder(order, counts, NetworkModel.POLYGON);
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
  marketplaceContractData: MarketplaceContractData,
  marketplaceV2ContractData: MarketplaceV2ContractData,
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState
): Promise<void> {
  const { assetId, buyer, id, nftAddress, seller, totalPrice } = event;
  const { orders, accounts, nfts } = storedData;

  const nftId = getNFTId(nftAddress, assetId.toString());
  const orderId = id;
  const order = orders.get(orderId);
  if (!order) {
    console.log(`ERROR: Order not found for order successful ${orderId}`);
    return;
  }

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

  const buyerAccount = accounts.get(`${buyer}-${NetworkModel.POLYGON}`);
  if (buyerAccount) {
    nft.owner = buyerAccount;
    nft.ownerAddress = buyer;
  } else {
    console.log("ERROR: Buyer account not found for order successful");
  }

  nft.updatedAt = timestamp;
  updateNFTOrderProperties(nft!, order!);

  const addresses = getAddresses(Network.MATIC);
  const isMarketplaceV1 = addresses.Marketplace === order.marketplaceAddress;

  let feesCollectorCut: bigint;
  let feesCollector: string;
  let royaltiesCut: bigint;

  if (isMarketplaceV1) {
    if (
      marketplaceContractData.ownerCutPerMillion === undefined ||
      marketplaceContractData.owner === undefined
    ) {
      console.log("ERROR: Owner cut per million not found");
      return;
    }
    feesCollectorCut = marketplaceContractData.ownerCutPerMillion;
    feesCollector = marketplaceContractData.owner;
    royaltiesCut = BigInt(0);
  } else {
    if (
      marketplaceV2ContractData.feesCollector === undefined ||
      marketplaceV2ContractData.feesCollectorCutPerMillion === undefined ||
      marketplaceV2ContractData.royaltiesCutPerMillion === undefined
    ) {
      console.log(
        "ERROR: feesCollector or feesCollectorCutPerMillion or royaltiesCutPerMillion not found"
      );
      return;
    }
    feesCollectorCut = marketplaceV2ContractData.feesCollectorCutPerMillion;
    feesCollector = marketplaceV2ContractData.feesCollector;
    royaltiesCut = marketplaceV2ContractData.royaltiesCutPerMillion;
  }

  if (nft.item) {
    await trackSale(
      ctx,
      block.header,
      storedData,
      inMemoryData,
      SaleType.order,
      buyer,
      seller,
      seller,
      nft.item.id,
      nft.id,
      order.price,
      feesCollectorCut,
      feesCollector,
      royaltiesCut,
      BigInt(block.header.timestamp / 1000),
      txHash
    );
  } else {
    console.log("ERROR: NFT not found for sale in order successful: ", nftId);
  }
}

export function handleOrderCancelled(
  event: OrderCancelledEventArgs,
  block: BlockData,
  nfts: Map<string, NFT>,
  orders: Map<string, Order>
): void {
  const { assetId, id, nftAddress, seller } = event;
  const nftId = getNFTId(nftAddress, assetId.toString());

  const nft = nfts.get(nftId);
  const order = orders.get(id);

  if (nft && order) {
    order.status = OrderStatus.cancelled;
    order.blockNumber = BigInt(block.header.height);
    const timestamp = BigInt(block.header.timestamp / 1000);
    order.updatedAt = timestamp;

    nft.updatedAt = timestamp;
    updateNFTOrderProperties(nft, order);
  } else if (!nft) {
    console.log(
      `ERROR: NFT not found for order cancelled orderId:${id}, nftId: ${nftId}`
    );
  } else if (!order) {
    console.log(
      `ERROR: Order not found for order cancelled orderId:${id}, nftId: ${nftId}`
    );
  }
}

export async function handleTraded(
  ctx: Context,
  event: TradedEventArgs,
  block: BlockData,
  transaction: Transaction & { input: string },
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState
): Promise<void> {
  const addresses = getAddresses(Network.MATIC);
  const { accounts, nfts } = storedData;

  const tradeData = getTradeEventData(event, Network.MATIC);
  const tradeType = getTradeEventType(event, Network.MATIC);
  // Nothing to index: not an order or a bid (a giveaway has no payment leg).
  if (!tradeData) {
    return;
  }
  const { assetType, collectionAddress, tokenId, buyer, price, seller } =
    tradeData;
  // Read once and kept current from the contract's own *Updated events -- see
  // getMarketplaceV3ContractData. This used to be three sequential eth_calls PER Traded event.
  const { feeCollector, feeRate, royaltiesRate } =
    await getMarketplaceV3ContractData(ctx, block.header);
  const feesCollector = feeCollector;

  if (Number(assetType) === TradeAssetType.ERC721) {
    if (!tokenId) {
      console.log("ERROR: tokenId not found in traded event");
      return;
    }
    const nftId = getNFTId(collectionAddress, tokenId.toString());
    const timestamp = BigInt(block.header.timestamp / 1000);

    const nft = nfts.get(nftId);
    if (!nft) {
      console.log(`ERROR: NFT not found for traded event ${nftId}`);
      return;
    }

    const buyerAccount = accounts.get(`${buyer}-${NetworkModel.POLYGON}`);
    if (buyerAccount) {
      nft.owner = buyerAccount;
      nft.ownerAddress = buyer;
    } else {
      console.log("ERROR: Buyer account not found for traded event");
    }

    nft.updatedAt = timestamp;

    if (nft.item) {
      await trackSale(
        ctx,
        block.header,
        storedData,
        inMemoryData,
        tradeType === TradeType.Bid ? SaleType.bid : SaleType.order,
        buyer,
        seller,
        seller,
        nft.item.id,
        nft.id,
        price,
        feeRate,
        feesCollector,
        royaltiesRate,
        BigInt(block.header.timestamp / 1000),
        transaction.hash
      );
    } else {
      console.log("ERROR: NFT not found for sale in traded event");
    }
  } else if (Number(assetType) === TradeAssetType.ITEM) {
    const addresses = getAddresses(Network.MATIC);
    const itemId = tradeData.itemId;
    if (itemId === undefined) {
      console.log("ERROR: itemId not found in traded event");
      return;
    }
    // Find the actual Issue log emitted by the collection in this same tx.
    // We can't compute issuedId from items(itemId).totalSupply at block.header
    // because that returns post-block state. When 2+ Trade-mints for the same
    // item land in the same block (different txs), every read returns the same
    // final totalSupply, so all simulated Issues collide on the same nftId and
    // the earlier mints get overwritten in storage.
    //
    // A single tx can also mint the same itemId multiple times (e.g. buying
    // several units at once). That emits one Issue log per unit (differing only
    // by issuedId/tokenId) and one Traded event per unit. Traded events are
    // processed in log order, so we consume the matching Issue logs in
    // ascending logIndex order, skipping any already matched to a previous
    // Traded event in this batch. A plain find() would return the first Issue
    // log every time and drop every mint after the first for that item.
    const issueLog = selectIssueLogForTrade<(typeof block.logs)[number]>(block.logs, {
      transactionIndex: transaction.transactionIndex,
      collectionAddress,
      itemId: BigInt(itemId),
      issueTopic: CollectionV2ABI.events.Issue.topic,
      blockHeight: block.header.height,
      consumedIssueLogs: inMemoryData.consumedIssueLogs,
    });
    if (!issueLog) {
      console.log(
        `ERROR: Issue log not found for trade tx ${transaction.hash} item ${itemId}`
      );
      return;
    }
    const issueDecoded = CollectionV2ABI.events.Issue.decode(issueLog);
    const tokenId = issueDecoded._tokenId;
    const issuedId = issueDecoded._issuedId;

    const logFromTraded = block.logs.find(
      (log) =>
        log.transactionIndex === transaction.transactionIndex &&
        log.topics[0] === MarketplaceV3ABI.events.Traded.topic
    );

    if (!logFromTraded || !logFromTraded.address) {
      console.log("ERROR: logFromTraded not found");
    } else if (
      logFromTraded.address !== addresses.MarketplaceV3 &&
      logFromTraded.address !== addresses.MarketplaceV3_V2
    ) {
      console.log("ERROR: logFromTraded is not the marketplace v3 or v3 v2");
    }

    // simulates an issue event to re-use all the logic inside the `handleIssue` function
    const issueEvent = {
      _beneficiary:
        tradeType === TradeType.Order
          ? event._trade.sent[0].beneficiary
          : event._trade.received[0].beneficiary,
      _caller: logFromTraded?.address || "",
      _itemId: itemId,
      _tokenId: tokenId,
      _issuedId: issuedId,
    };
    await handleIssue(
      ctx,
      collectionAddress,
      issueEvent,
      block.header,
      transaction,
      storedData,
      inMemoryData,
      {
        fee: feeRate,
        feeOwner: feesCollector,
      },
      event
    );
  } else {
    console.log(
      `ERROR: Asset type not supported in trade: event ${event}, tx hash ${transaction.hash} and assetType ${assetType}`
    );
  }
}
