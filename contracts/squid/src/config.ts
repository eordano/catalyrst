import { ChainId, Network } from "@dcl/schemas";

const BLOCK_RANGES: Record<string, { from: number }> = {
  [ChainId.ETHEREUM_MAINNET]: {
    from: 4944642, // LANDProxy contract creation
  },
  [ChainId.ETHEREUM_SEPOLIA]: {
    from: 3831219, // LANDProxy contract creation
  },
  [ChainId.MATIC_MAINNET]: {
    from: 0, // @TODO replace this when implementing collections logic
  },
  [ChainId.MATIC_AMOY]: {
    from: 0, // @TODO replace this when implementing collections logic
  },
};

export const getBlockRange = (network: Network) => {
  const chainId =
    network === Network.ETHEREUM
      ? parseInt(
          process.env.ETHEREUM_CHAIN_ID || ChainId.ETHEREUM_MAINNET.toString()
        )
      : parseInt(
          process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET.toString()
        );

  return BLOCK_RANGES[chainId];
};
