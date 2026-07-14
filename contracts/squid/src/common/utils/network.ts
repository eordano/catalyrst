import { ChainId, Network } from "@dcl/schemas";

export function getNetwork(network: Network): string {
  const chainId =
    network === Network.ETHEREUM
      ? process.env.ETHEREUM_CHAIN_ID
      : process.env.POLYGON_CHAIN_ID;
  const chainName =
    network === Network.ETHEREUM
      ? chainId === ChainId.ETHEREUM_MAINNET.toString()
        ? "ethereum"
        : "sepolia"
      : chainId === ChainId.MATIC_MAINNET.toString()
      ? "matic"
      : "amoy";
  return chainName;
}

const baseDecentralandURN = "urn:decentraland:";

export function getURNForCollectionV2(
  network: Network,
  collectionAddress: string
): string {
  return (
    baseDecentralandURN +
    getNetwork(network) +
    ":collections-v2:" +
    collectionAddress
  );
}
