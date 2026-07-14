import { ChainId, Network } from "@dcl/schemas";

export const getAddresses = (network: Network) => {
  const chainId =
    network === Network.ETHEREUM
      ? process.env.ETHEREUM_CHAIN_ID
      : process.env.POLYGON_CHAIN_ID;

  const networkName = network === Network.ETHEREUM ? "eth" : "polygon";
  const chainName =
    network === Network.ETHEREUM
      ? chainId === ChainId.ETHEREUM_MAINNET.toString()
        ? "mainnet"
        : "sepolia"
      : chainId === ChainId.MATIC_MAINNET.toString()
      ? "mainnet"
      : "amoy";

  const path = `../../${networkName}/addresses/${chainName}`;
  return require(path);
};
