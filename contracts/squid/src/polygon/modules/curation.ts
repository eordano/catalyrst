import { ChainId } from "@dcl/schemas";

export function getCurationId(
  collectionAddress: string,
  transactionHash: string,
  logIndex: string
): string {
  return collectionAddress + "-" + transactionHash + "-" + logIndex;
}

export function getBlockWhereRescueItemsStarted(): bigint {
  const chainId = parseInt(
    process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET.toString()
  ) as ChainId;

  if (chainId === ChainId.MATIC_MAINNET) {
    return BigInt(20841775);
  }

  if (chainId === ChainId.MATIC_AMOY) {
    return BigInt(5706678);
  }

  return BigInt(0);
}
