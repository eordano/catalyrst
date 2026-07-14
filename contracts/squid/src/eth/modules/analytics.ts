import { createOrLoadAccount } from "../../common/modules/account";
import { ONE_MILLION } from "../../common/utils/utils";
import {
  Account,
  AnalyticsDayData,
  Count,
  Item,
  NFT,
  Network,
  Sale,
  SaleType,
} from "../../model";
import { buildCountFromSale } from "./count";
import { getOrCreateAnalyticsDayData } from "../../common/modules/analytics";
import { getOwner } from "../../common/utils/nft";
import { getOperation } from "../../polygon/modules/analytics";
import { Block, Context } from "../processor";

export let BID_SALE_TYPE = "bid";
export let ORDER_SALE_TYPE = "order";

// check if the buyer in a sale was a third party provider (to pay with credit card, cross chain, etc)
export function isThirdPartySale(buyer: string): boolean {
  if (
    buyer == "0xea749fd6ba492dbc14c24fe8a3d08769229b896c" || // Axelar Ethereum old contract
    buyer == "0xad6cea45f98444a922a2b4fe96b8c90f0862d2f4" // Axelar Ethereum new contract
  ) {
    return true;
  }
  return false;
}

export async function trackSale(
  ctx: Context,
  block: Block,
  type: string,
  buyer: string,
  seller: string,
  nftId: string,
  price: bigint,
  feesCollectorCut: bigint,
  timestamp: bigint,
  txHash: string,
  nfts: Map<string, NFT>,
  sales: Map<string, Sale>,
  accounts: Map<string, Account>,
  analytics: Map<string, AnalyticsDayData>,
  counts: Map<string, Count>,
  items: Map<string, Item>
): Promise<void> {
  if (price === BigInt(0)) {
    console.log("INFO: ignoring zero price sale");
    return;
  }

  const count = buildCountFromSale(price, feesCollectorCut, counts);
  counts.set(count.id, count);

  const nft = nfts.get(nftId);

  const saleId = `${BigInt(count.salesTotal).toString()}-${Network.ETHEREUM}`;
  const sale = new Sale({ id: saleId });
  sale.type = type as SaleType;
  sale.seller = seller;
  sale.price = price;
  if (nft) {
    sale.realBuyer = buyer;
    sale.operation = getOperation(buyer);
    sale.buyer = isThirdPartySale(buyer)
      ? await getOwner(ctx, block, nft.contractAddress, nft.tokenId)
      : buyer;
    sale.nft = nft;
    sale.searchTokenId = nft.tokenId;
    sale.searchContractAddress = nft.contractAddress;
    sale.searchCategory = nft.category;

    let item: Item | undefined | null = nft.item;
    if (
      !item &&
      nft.itemBlockchainId !== undefined &&
      nft.itemBlockchainId !== null
    ) {
      // Item ID format is typically: collectionAddress-blockchainId
      const itemId = `${nft.contractAddress}-${nft.itemBlockchainId}`;
      item = items.get(itemId);
    }

    if (item) {
      sale.item = item;
      sale.searchItemId = item.blockchainId;
    }
  } else {
    console.log("ERROR: NFT not found for sale", nftId);
  }
  sale.timestamp = timestamp;
  sale.txHash = txHash;
  sale.network = Network.ETHEREUM;
  sales.set(saleId, sale);

  const buyerAccount = createOrLoadAccount(accounts, buyer);
  if (buyerAccount) {
    buyerAccount.purchases += 1;
    buyerAccount.spent = buyerAccount.spent + price;
  }

  const sellerAccount = createOrLoadAccount(accounts, seller);
  if (sellerAccount) {
    sellerAccount.sales += 1;
    sellerAccount.earned = sellerAccount.earned + price;
  }

  if (nft) {
    nft.soldAt = timestamp;
    nft.sales += 1;
    nft.volume = nft.volume + price;
    nft.updatedAt = timestamp;
    nfts.set(nftId, nft);
  }

  const analyticsDayData = updateAnalyticsDayData(
    sale,
    feesCollectorCut,
    analytics
  );
  analytics.set(analyticsDayData.id, analyticsDayData);
}

export function updateAnalyticsDayData(
  sale: Sale,
  feesCollectorCut: bigint,
  analytics: Map<string, AnalyticsDayData>
): AnalyticsDayData {
  const analyticsDayData = getOrCreateAnalyticsDayData(
    sale.timestamp,
    analytics
  );
  analyticsDayData.sales += 1;
  analyticsDayData.volume = analyticsDayData.volume + sale.price;
  analyticsDayData.daoEarnings =
    analyticsDayData.daoEarnings +
    (feesCollectorCut * sale.price) / ONE_MILLION;

  return analyticsDayData;
}
