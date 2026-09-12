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
  "0xa1691afad71b9a92d329f1a95c39d3077d8f2f5f",
  "0x037566bc90f85e76587e1b07f9184585f09c1420",
  "0x6a03991dfa9d661ef7ad3c6f88b31f16e5a282cf",
  "0xe9f961e6ded4e1476bbee4faab886d63a2493eb9",
];

function isCreditSale(buyer: string): boolean {
  return CREDIT_CONTRACTS.includes(buyer);
}

export function isTransakOperation(buyer: string): boolean {
  return [
    "0xed038688ecf1193f8d9717eb3930f0bf0d745cb4",
    "0xcb9bd5acd627e8fccf9eb8d4ba72aeb1cd8ff5ef",
    "0x4a598b7ec77b1562ad0df7dc64a162695ce4c78a",
    "0xab88cd272863b197b48762ea283f24a13f6586dd",
  ].includes(buyer);
}

export function isAxelarOperation(buyer: string): boolean {
  return [
    "0xea749fd6ba492dbc14c24fe8a3d08769229b896c",
    "0xad6cea45f98444a922a2b4fe96b8c90f0862d2f4",
  ].includes(buyer);
}

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
  if (price === BigInt(0)) {
    return;
  }

  const count = buildCount(counts, Network.POLYGON);

  count.salesTotal += 1;
  count.salesManaTotal = count.salesManaTotal + price;

  counts.set(count.id, count);

  let item = items.get(itemId);
  const nft = nfts.get(nftId);
  if (!item && !!nft?.item) {
    item = nft.item;
  }
  if (!item || !nft) {
    console.log(`ERROR: NFT or Item not found for sale ${nftId} ${itemId}`);
    return;
  }

  const saleId = `${BigInt(count.salesTotal).toString()}-${Network.POLYGON}`;
  const sale = new Sale({ id: saleId });
  sale.type = type;
  sale.realBuyer = buyer;
  sale.operation = getOperation(buyer);
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

  sale.feesCollector = Buffer.from(feesCollector.slice(2), "hex");
  sale.royaltiesCollector = Buffer.from(ZERO_ADDRESS.slice(2), "hex");
  sale.feesCollectorCut = (feesCollectorCut * sale.price) / ONE_MILLION;
  sale.royaltiesCut = (royaltiesCut * sale.price) / ONE_MILLION;

  const totalFees = sale.feesCollectorCut + sale.royaltiesCut;

  count.royaltiesManaTotal = count.royaltiesManaTotal + totalFees;

  if (royaltiesCut > BigInt(0)) {
    if (item.beneficiary !== ZERO_ADDRESS || item.creator !== ZERO_ADDRESS) {
      const royaltiesCollectorAddress =
        item.beneficiary !== ZERO_ADDRESS ? item.beneficiary : item.creator;

      sale.royaltiesCollector = Buffer.from(
        royaltiesCollectorAddress.slice(2),
        "hex"
      );
      const royaltiesCollectorAccount = createOrLoadAccount(
        accounts,
        royaltiesCollectorAddress,
        Network.POLYGON
      );
      royaltiesCollectorAccount.earned =
        royaltiesCollectorAccount.earned + sale.royaltiesCut;
      royaltiesCollectorAccount.royalties =
        royaltiesCollectorAccount.royalties + sale.royaltiesCut;
    } else {
      sale.feesCollectorCut = sale.feesCollectorCut + sale.royaltiesCut;
      sale.royaltiesCut = BigInt(0);
    }
  }

  count.creatorEarningsManaTotal =
    count.creatorEarningsManaTotal +
    (sale.type == SaleType.mint
      ? sale.price - sale.feesCollectorCut
      : sale.royaltiesCut);

  count.daoEarningsManaTotal =
    count.daoEarningsManaTotal +
    (sale.type == SaleType.mint ? sale.feesCollectorCut : BigInt(0));

  sales.set(saleId, sale);

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

  const sellerAccount = createOrLoadAccount(accounts, seller, Network.POLYGON);
  sellerAccount.sales += 1;
  sellerAccount.earned = sellerAccount.earned + (price - totalFees);
  sellerAccount.uniqueCollectors = updateUniqueCollectorsSet(
    sellerAccount.uniqueCollectors,
    buyer
  );
  sellerAccount.uniqueCollectorsTotal = sellerAccount.uniqueCollectors.length;

  const feesCollectorAccount = createOrLoadAccount(
    accounts,
    feesCollector,
    Network.POLYGON
  );
  feesCollectorAccount.earned =
    feesCollectorAccount.earned + sale.feesCollectorCut;
  feesCollectorAccount.royalties =
    feesCollectorAccount.royalties + sale.feesCollectorCut;

  item.soldAt = timestamp;
  item.sales += 1;
  item.volume = item.volume + price;
  item.updatedAt = timestamp;
  item.uniqueCollectors = updateUniqueCollectorsSet(
    item.uniqueCollectors,
    buyer
  );
  item.uniqueCollectorsTotal = item.uniqueCollectors.length;

  nft.soldAt = timestamp;
  nft.sales += 1;
  nft.volume = nft.volume + price;
  nft.updatedAt = timestamp;

  if (type == SaleType.mint) {
    buildCountFromPrimarySale(counts, price);
    const creatorAccount = createOrLoadAccount(
      accounts,
      item.creator,
      Network.POLYGON
    );
    creatorAccount.primarySales += 1;
    creatorAccount.primarySalesEarned =
      creatorAccount.primarySalesEarned + (price - totalFees);
  } else {
    buildCountFromSecondarySale(counts, price);
  }

  const analyticsDayData = updateAnalyticsDayData(analytics, sale);
  analytics.set(analyticsDayData.id, analyticsDayData);

  const itemDayData = updateItemDayData(itemDayDatas, sale, item);
  itemDayDatas.set(itemDayData.id, itemDayData);

  const buyerAccountsDayData = updateBuyerAccountsDayData(
    accountsDayDatas,
    sale,
    item
  );
  accountsDayDatas.set(buyerAccountsDayData.id, buyerAccountsDayData);

  const creatorsAccountsDayData = updateCreatorAccountsDayData(
    accountsDayDatas,
    sale,
    price - totalFees,
    item.collection.id
  );
  accountsDayDatas.set(creatorsAccountsDayData.id, creatorsAccountsDayData);
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
      ? analyticsDayData.creatorsEarnings + (sale.price - sale.feesCollectorCut)
      : analyticsDayData.creatorsEarnings + sale.royaltiesCut;

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
  const dayID = timestamp / BigInt(86400);
  const dayStartTimestamp = dayID * BigInt(86400);
  const itemDayDataId = dayID.toString() + "-" + itemId;

  let itemDayData = itemsDayDatas.get(itemDayDataId);
  if (!itemDayData) {
    itemDayData = new ItemsDayData({ id: itemDayDataId });
    itemDayData.date = +dayStartTimestamp.toString();
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
