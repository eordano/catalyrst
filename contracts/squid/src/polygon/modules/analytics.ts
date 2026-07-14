import {
  ZERO_ADDRESS,
  createOrLoadAccount,
} from "../../common/modules/account";
import { getOrCreateAnalyticsDayData } from "../../common/modules/analytics";
import {
  buildCount,
  buildCountFromPrimarySale,
  buildCountFromSecondarySale,
} from "../../common/modules/count";
import { getOwner } from "../../common/utils/nft";
import { ONE_MILLION } from "../../common/utils/utils";
import {
  AnalyticsDayData,
  Item,
  ItemsDayData,
  Network,
  Operation,
  Sale,
  SaleType,
} from "../../model";
import { Block, Context } from "../processor";
import { PolygonInMemoryState, PolygonStoredData } from "../types";
import {
  updateBuyerAccountsDayData,
  updateCreatorAccountsDayData,
  updateCreatorsSupportedSet,
  updateUniqueAndMythicItemsSet,
  updateUniqueCollectorsSet,
} from "./accountsDayData";

const CREDIT_CONTRACTS = [
  "0xa1691afad71b9a92d329f1a95c39d3077d8f2f5f", // old CreditsManager contract Amoy
  "0x037566bc90f85e76587e1b07f9184585f09c1420", // new CreditsManager contract Amoy
  "0x6a03991dfa9d661ef7ad3c6f88b31f16e5a282cf", // CreditsManager contract Mainnet
  "0xe9f961e6ded4e1476bbee4faab886d63a2493eb9", // new CreditsManager contract Mainnet
];

function isCreditSale(buyer: string): boolean {
  return CREDIT_CONTRACTS.includes(buyer);
}

export function isTransakOperation(buyer: string): boolean {
  return [
    "0xed038688ecf1193f8d9717eb3930f0bf0d745cb4", // Transak Polygon
    "0xcb9bd5acd627e8fccf9eb8d4ba72aeb1cd8ff5ef", // Transak Multicall Polygon Amoy
    "0x4a598b7ec77b1562ad0df7dc64a162695ce4c78a", // Transak Multicall Polygon Mainnet
    "0xab88cd272863b197b48762ea283f24a13f6586dd", // Transak Multicall Ethereum Mainnet
  ].includes(buyer);
}

export function isAxelarOperation(buyer: string): boolean {
  return [
    "0xea749fd6ba492dbc14c24fe8a3d08769229b896c", // Axelar Polygon & Ethereum old contract
    "0xad6cea45f98444a922a2b4fe96b8c90f0862d2f4", // Axelar Polygon & Ethereum new contract
  ].includes(buyer);
}

// check if the buyer in a sale was a third party provider (to pay with credit card, cross chain, etc)
export function isThirdPartySale(buyer: string): boolean {
  if (isTransakOperation(buyer) || isAxelarOperation(buyer)) {
    return true;
  }
  return false;
}

export function getOperation(buyer: string): Operation {
  if (isTransakOperation(buyer)) {
    return Operation.fiat;
  } else if (isAxelarOperation(buyer)) {
    return Operation.cross_chain;
  } else if (isCreditSale(buyer)) {
    return Operation.credits;
  }
  return Operation.native;
}

export async function trackSale(
  ctx: Context,
  block: Block,
  storedData: PolygonStoredData,
  inMemoryData: PolygonInMemoryState,
  type: SaleType,
  buyer: string,
  seller: string,
  beneficiary: string,
  itemId: string,
  nftId: string,
  price: bigint,
  feesCollectorCut: bigint,
  feesCollector: string,
  royaltiesCut: bigint,
  timestamp: bigint,
  txHash: string
): Promise<void> {
  const {
    counts,
    nfts,
    items,
    accounts,
    analytics,
    itemDayDatas,
    accountsDayDatas,
  } = storedData;
  const { sales } = inMemoryData;
  // ignore zero price sales
  if (price === BigInt(0)) {
    return;
  }

  // count sale
  // let count = buildCountFromSale(price);

  const count = buildCount(counts, Network.POLYGON);

  count.salesTotal += 1;
  count.salesManaTotal = count.salesManaTotal + price;

  counts.set(count.id, count);
  // count.save();

  // load entities
  let item = items.get(itemId);
  const nft = nfts.get(nftId);
  if (!item && !!nft?.item) {
    item = nft.item; // set the item coming out from the NFT
  }
  if (!item || !nft) {
    console.log(`ERROR: NFT or Item not found for sale ${nftId} ${itemId}`);
    return;
  }

  // save sale
  const saleId = `${BigInt(count.salesTotal).toString()}-${Network.POLYGON}`;
  const sale = new Sale({ id: saleId });
  sale.type = type;
  // real buyer is the buyer that is paying for the NFT
  sale.realBuyer = buyer;
  sale.operation = getOperation(buyer);
  // buyer is the address that will own the NFT (beneficiary of the NFT). If it's a third party or credit sale, we need to get the owner of the NFT
  sale.buyer =
    isThirdPartySale(buyer) || isCreditSale(buyer)
      ? await getOwner(ctx, block, nft.contractAddress, nft.tokenId)
      : buyer;
  sale.seller = seller;
  sale.beneficiary = Buffer.from(beneficiary.slice(2), "hex");
  sale.price = price;
  sale.item = item;
  sale.nft = nft;
  sale.timestamp = timestamp;
  sale.txHash = txHash;
  sale.searchItemId = item.blockchainId;
  sale.searchTokenId = nft.tokenId;
  sale.searchContractAddress = nft.contractAddress;
  sale.searchCategory = nft.category;
  sale.network = Network.POLYGON;

  // update Fees
  // console.log("feesCollector: ", feesCollector);
  // console.log("feesCollectorCut: ", feesCollectorCut);
  sale.feesCollector = Buffer.from(feesCollector.slice(2), "hex");
  sale.royaltiesCollector = Buffer.from(ZERO_ADDRESS.slice(2), "hex");
  sale.feesCollectorCut = (feesCollectorCut * sale.price) / ONE_MILLION;
  sale.royaltiesCut = (royaltiesCut * sale.price) / ONE_MILLION;

  const totalFees = sale.feesCollectorCut + sale.royaltiesCut;

  // add royalties to the count
  count.royaltiesManaTotal = count.royaltiesManaTotal + totalFees;
  // count = buildCountFromRoyalties(totalFees);
  // count.save();

  if (royaltiesCut > BigInt(0)) {
    if (item.beneficiary !== ZERO_ADDRESS || item.creator !== ZERO_ADDRESS) {
      const royaltiesCollectorAddress =
        item.beneficiary !== ZERO_ADDRESS ? item.beneficiary : item.creator;

      sale.royaltiesCollector = Buffer.from(
        royaltiesCollectorAddress.slice(2),
        "hex"
      );
      // update royalties collector account
      const royaltiesCollectorAccount = createOrLoadAccount(
        accounts,
        royaltiesCollectorAddress,
        Network.POLYGON
      );
      royaltiesCollectorAccount.earned =
        royaltiesCollectorAccount.earned + sale.royaltiesCut;
      royaltiesCollectorAccount.royalties =
        royaltiesCollectorAccount.royalties + sale.royaltiesCut;
      // royaltiesCollectorAccount.save();
    } else {
      // If there is not royalties receiver, all the fees goes to the fees collector
      sale.feesCollectorCut = sale.feesCollectorCut + sale.royaltiesCut;
      sale.royaltiesCut = BigInt(0);
    }
  }

  // we update the count here because the sale has the updated values based on the royalties reciever

  count.creatorEarningsManaTotal =
    count.creatorEarningsManaTotal +
    (sale.type == SaleType.mint
      ? sale.price - sale.feesCollectorCut
      : sale.royaltiesCut);

  count.daoEarningsManaTotal =
    count.daoEarningsManaTotal +
    (sale.type == SaleType.mint ? sale.feesCollectorCut : BigInt(0));

  sales.set(saleId, sale);

  // update buyer account
  const buyerAccount = createOrLoadAccount(accounts, buyer, Network.POLYGON);
  buyerAccount.purchases += 1;
  buyerAccount.spent = buyerAccount.spent + price;

  if (item.rarity === "unique" || item.rarity === "mythic") {
    buyerAccount.uniqueAndMythicItems = updateUniqueAndMythicItemsSet(
      buyerAccount.uniqueAndMythicItems,
      item
    );
    buyerAccount.uniqueAndMythicItemsTotal =
      buyerAccount.uniqueAndMythicItems.length;
  }
  buyerAccount.creatorsSupported = updateCreatorsSupportedSet(
    buyerAccount.creatorsSupported,
    sale.seller
  );
  buyerAccount.creatorsSupportedTotal = buyerAccount.creatorsSupported.length;

  // buyerAccount.save();

  // update seller account
  const sellerAccount = createOrLoadAccount(accounts, seller, Network.POLYGON);
  sellerAccount.sales += 1;
  sellerAccount.earned = sellerAccount.earned + (price - totalFees);
  sellerAccount.uniqueCollectors = updateUniqueCollectorsSet(
    sellerAccount.uniqueCollectors,
    buyer
  );
  // console.log(
  //   "sellerAccount.uniqueCollectors: ",
  //   sellerAccount.uniqueCollectors
  // );
  // console.log(
  //   "sellerAccount.uniqueCollectors.length: ",
  //   sellerAccount.uniqueCollectors.length
  // );
  sellerAccount.uniqueCollectorsTotal = sellerAccount.uniqueCollectors.length;

  // sellerAccount.save();

  // update fees collector account
  const feesCollectorAccount = createOrLoadAccount(
    accounts,
    feesCollector,
    Network.POLYGON
  );
  feesCollectorAccount.earned =
    feesCollectorAccount.earned + sale.feesCollectorCut;
  feesCollectorAccount.royalties =
    feesCollectorAccount.royalties + sale.feesCollectorCut;
  // feesCollectorAccount.save();

  // update item
  item.soldAt = timestamp;
  item.sales += 1;
  item.volume = item.volume + price;
  item.updatedAt = timestamp;
  item.uniqueCollectors = updateUniqueCollectorsSet(
    item.uniqueCollectors,
    buyer
  );
  item.uniqueCollectorsTotal = item.uniqueCollectors.length;

  // item.save();

  // update nft
  nft.soldAt = timestamp;
  nft.sales += 1;
  nft.volume = nft.volume + price;
  nft.updatedAt = timestamp;
  // nft.save();

  // track primary sales
  if (type == SaleType.mint) {
    buildCountFromPrimarySale(counts, price);
    // count.save();
    // track the sale and mana earned in the creator account
    const creatorAccount = createOrLoadAccount(
      accounts,
      item.creator,
      Network.POLYGON
    );
    creatorAccount.primarySales += 1;
    creatorAccount.primarySalesEarned =
      creatorAccount.primarySalesEarned + (price - totalFees);
  } else {
    // track secondary sale
    buildCountFromSecondarySale(counts, price);
  }

  // console.log("tracking sale2");

  const analyticsDayData = updateAnalyticsDayData(analytics, sale);
  // console.log("analyticsDayData: ", analyticsDayData);
  analytics.set(analyticsDayData.id, analyticsDayData);

  const itemDayData = updateItemDayData(itemDayDatas, sale, item);
  itemDayDatas.set(itemDayData.id, itemDayData);
  // itemDayData.save();

  const buyerAccountsDayData = updateBuyerAccountsDayData(
    accountsDayDatas,
    sale,
    item
  );
  accountsDayDatas.set(buyerAccountsDayData.id, buyerAccountsDayData);
  // buyerAccountsDayData.save();

  const creatorsAccountsDayData = updateCreatorAccountsDayData(
    accountsDayDatas,
    sale,
    price - totalFees,
    item.collection.id
  );
  accountsDayDatas.set(creatorsAccountsDayData.id, creatorsAccountsDayData);
  // creatorsAccountsDayData.save();
}

export function updateAnalyticsDayData(
  analytics: Map<string, AnalyticsDayData>,
  sale: Sale
): AnalyticsDayData {
  const analyticsDayData = getOrCreateAnalyticsDayData(
    sale.timestamp,
    analytics,
    Network.POLYGON
  );
  // console.log("sale.feesCollectorCut: ", sale.feesCollectorCut);
  // console.log("sale.royaltiesCut: ", sale.royaltiesCut);
  if (
    sale.feesCollectorCut === undefined ||
    sale.feesCollectorCut === null ||
    sale.royaltiesCut === undefined ||
    sale.royaltiesCut === null
  ) {
    console.log(
      "ERROR: Sale fees or royalties not set because feesCollectorCut or royaltiesCut are missing",
      sale.id
    );
    return analyticsDayData;
  }

  analyticsDayData.sales += 1;
  analyticsDayData.volume = analyticsDayData.volume + sale.price;
  analyticsDayData.creatorsEarnings =
    sale.type == SaleType.mint
      ? analyticsDayData.creatorsEarnings + (sale.price - sale.feesCollectorCut) // if it's a MINT, the creator earning is the sale price
      : analyticsDayData.creatorsEarnings + sale.royaltiesCut; // if it's a secondary sale, the creator earning is the royaltiesCut (if it's set already)

  // console.log(
  //   "sale.feesCollectorCut in updateAnalyticsDayData: ",
  //   sale.feesCollectorCut
  // );
  // console.log("analyticsDayData.daoEarnings: ", analyticsDayData.daoEarnings);
  analyticsDayData.daoEarnings =
    analyticsDayData.daoEarnings + sale.feesCollectorCut;

  return analyticsDayData;
}

export function getOrCreateItemDayData(
  itemsDayDatas: Map<string, ItemsDayData>,
  blockTimestamp: bigint,
  itemId: string
): ItemsDayData {
  const timestamp = blockTimestamp;
  const dayID = timestamp / BigInt(86400); // unix timestamp for start of day / 86400 giving a unique day index
  const dayStartTimestamp = dayID * BigInt(86400);
  const itemDayDataId = dayID.toString() + "-" + itemId;

  let itemDayData = itemsDayDatas.get(itemDayDataId);
  if (!itemDayData) {
    itemDayData = new ItemsDayData({ id: itemDayDataId });
    itemDayData.date = +dayStartTimestamp.toString(); // unix timestamp for start of day
    itemDayData.sales = 0;
    itemDayData.volume = BigInt(0);
  }

  return itemDayData as ItemsDayData;
}

export function updateItemDayData(
  itemsDayDatas: Map<string, ItemsDayData>,
  sale: Sale,
  item: Item
): ItemsDayData {
  const itemDayData = getOrCreateItemDayData(
    itemsDayDatas,
    sale.timestamp,
    item.id
  );
  itemDayData.sales += 1;
  itemDayData.volume = itemDayData.volume + sale.price;
  if (item) {
    itemDayData.searchWearableCategory = item.searchWearableCategory;
    itemDayData.searchEmoteCategory = item.searchEmoteCategory;
    itemDayData.searchRarity = item.rarity;
  }

  return itemDayData;
}
