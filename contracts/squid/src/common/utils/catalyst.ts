import { ChainId } from "@dcl/schemas";

export function getCatalystBase(chainId: ChainId): string {
  if (
    chainId === ChainId.ETHEREUM_MAINNET ||
    chainId === ChainId.MATIC_MAINNET
  ) {
    return "https://peer.decentraland.org";
  } else if (
    chainId === ChainId.ETHEREUM_SEPOLIA ||
    chainId === ChainId.MATIC_AMOY
  ) {
    return "https://peer.decentraland.zone";
  }

  return "";
}
