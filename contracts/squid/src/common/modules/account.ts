import { Account, Network } from "../../model";

export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

export function createAccount(
  id: string,
  network: Network = Network.ETHEREUM
): Account {
  return new Account({
    id: `${id}-${network}`,
    address: id,
    sales: 0,
    purchases: 0,
    earned: BigInt(0),
    spent: BigInt(0),
    primarySales: 0,
    primarySalesEarned: BigInt(0),
    royalties: BigInt(0),
    collections: 0,
    uniqueAndMythicItems: [],
    uniqueAndMythicItemsTotal: 0,
    creatorsSupported: [],
    creatorsSupportedTotal: 0,
    uniqueCollectors: [],
    uniqueCollectorsTotal: 0,
    totalCurations: 0,
    isCommitteeMember: false,
    network,
  });
}

export function createOrLoadAccount(
  accounts: Map<string, Account>,
  id: string,
  network: Network = Network.ETHEREUM
): Account {
  const accountId = `${id}-${network}`;
  let account = accounts.get(accountId);

  if (!account) {
    account = createAccount(id, network);
  }

  accounts.set(accountId, account);
  return account;
}
