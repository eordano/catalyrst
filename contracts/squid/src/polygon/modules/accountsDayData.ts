import { AccountsDayData, Item, Network, Sale, SaleType } from "../../model";

export function updateUniqueAndMythicItemsSet(
  uniqueAndMythicItems: string[],
  item: Item
): string[] {
  const updatedUniqueAndMythicItems = new Set<string>();
  for (let i = 0; i < uniqueAndMythicItems.length; i++) {
    updatedUniqueAndMythicItems.add(uniqueAndMythicItems[i]);
  }
  updatedUniqueAndMythicItems.add(item.id);
  return [...updatedUniqueAndMythicItems.values()];
}

export function updateCreatorsSupportedSet(
  creatorsSupported: string[],
  seller: string
): string[] {
  let uniqueCreatorsSupported = new Set<string>();
  for (let i = 0; i < creatorsSupported.length; i++) {
    uniqueCreatorsSupported.add(creatorsSupported[i]);
  }
  uniqueCreatorsSupported.add(seller);
  return [...uniqueCreatorsSupported.values()];
}

export function updateUniqueCollectorsSet(
  collectors: string[],
  buyer: string
): string[] {
  const uniqueCollectors = new Set<string>();
  for (let i = 0; i < collectors.length; i++) {
    uniqueCollectors.add(collectors[i]);
  }
  uniqueCollectors.add(buyer);
  return [...uniqueCollectors.values()];
}

export function updateUniqueCollectionsSalesSet(
  uniqueCollectionsSales: string[],
  collectionId: string
): string[] {
  let uniqueCollectionsSalesSet = new Set<string>();
  for (let i = 0; i < uniqueCollectionsSales.length; i++) {
    uniqueCollectionsSalesSet.add(uniqueCollectionsSales[i]);
  }
  uniqueCollectionsSalesSet.add(collectionId);
  return [...uniqueCollectionsSalesSet.values()];
}

// AccountsDayData
export function getOrCreateAccountsDayData(
  accountsDayDatas: Map<string, AccountsDayData>,
  timestamp: bigint,
  address: string
): AccountsDayData {
  const dayID = timestamp / BigInt(86400); // unix timestamp for start of day / 86400 giving a unique day index
  const dayStartTimestamp = dayID * BigInt(86400);
  const accountsDayDataId = dayID.toString() + "-" + address;

  let accountsDayData = accountsDayDatas.get(accountsDayDataId);
  if (!accountsDayData) {
    accountsDayData = new AccountsDayData({ id: accountsDayDataId });
    accountsDayData.date = +dayStartTimestamp.toString(); // unix timestamp for start of day
    accountsDayData.earned = BigInt(0);
    accountsDayData.spent = BigInt(0);
    // for creators
    accountsDayData.sales = 0;
    accountsDayData.uniqueCollectionsSales = [];
    accountsDayData.uniqueCollectors = [];
    accountsDayData.uniqueCollectorsTotal = 0;
    // for collectors
    accountsDayData.purchases = 0;
    accountsDayData.creatorsSupported = [];
    accountsDayData.creatorsSupportedTotal = 0;
    accountsDayData.uniqueAndMythicItems = [];
    accountsDayData.uniqueAndMythicItemsTotal = 0;

    accountsDayData.network = Network.POLYGON; //@TODO see if we remove this since it's not used in ETH
  }

  return accountsDayData;
}

export function updateBuyerAccountsDayData(
  accountsDayDatas: Map<string, AccountsDayData>,
  sale: Sale,
  item: Item
): AccountsDayData {
  const buyerAccountsDayData = getOrCreateAccountsDayData(
    accountsDayDatas,
    sale.timestamp,
    sale.buyer
  );

  // update buyer/collector day data
  buyerAccountsDayData.purchases += 1;
  buyerAccountsDayData.spent = buyerAccountsDayData.spent + sale.price;

  // track unique and mythic items
  if (item.rarity === "unique" || item.rarity === "mythic") {
    buyerAccountsDayData.uniqueAndMythicItems = updateUniqueAndMythicItemsSet(
      buyerAccountsDayData.uniqueAndMythicItems,
      item
    );
    buyerAccountsDayData.uniqueAndMythicItemsTotal =
      buyerAccountsDayData.uniqueAndMythicItems.length;
  }
  // track creators supported
  buyerAccountsDayData.creatorsSupported = updateCreatorsSupportedSet(
    buyerAccountsDayData.creatorsSupported,
    sale.seller
  );
  buyerAccountsDayData.creatorsSupportedTotal =
    buyerAccountsDayData.creatorsSupported.length;

  return buyerAccountsDayData;
}

export function updateCreatorAccountsDayData(
  accountsDayDatas: Map<string, AccountsDayData>,
  sale: Sale,
  earned: bigint,
  collectionId: string
): AccountsDayData {
  const creatorAccountsDayData = getOrCreateAccountsDayData(
    accountsDayDatas,
    sale.timestamp,
    sale.seller
  );

  // update seller/creator day data
  creatorAccountsDayData.earned = creatorAccountsDayData.earned + earned;
  // for mints, track the number of unique collectors
  if (sale.type === SaleType.mint) {
    creatorAccountsDayData.sales += 1;
    creatorAccountsDayData.uniqueCollectionsSales =
      updateUniqueCollectionsSalesSet(
        creatorAccountsDayData.uniqueCollectionsSales,
        collectionId
      );
    creatorAccountsDayData.uniqueCollectors = updateUniqueCollectorsSet(
      creatorAccountsDayData.uniqueCollectors,
      sale.buyer
    );
    creatorAccountsDayData.uniqueCollectorsTotal =
      creatorAccountsDayData.uniqueCollectors.length;
  }

  return creatorAccountsDayData;
}
