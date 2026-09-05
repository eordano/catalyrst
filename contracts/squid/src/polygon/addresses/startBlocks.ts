import { ChainId } from "@dcl/schemas";

export const startBlockByNetwork: Record<string, Record<string, number>> = {
  [ChainId.MATIC_MAINNET]: {
    Factory: 15202000,
    FactoryV3: 28121692,
    Marketplace: 8828687,
    MarketplaceV2: 8828687,
    Bid: 8828687,
    BidV2: 8828687,
    Spoke: 78255530,
  },
  [ChainId.MATIC_AMOY]: {
    Factory: 5763249,
    FactoryV3: 5763249,
    Marketplace: 14517370,
    MarketplaceV2: 5706656,
    BidV1: 21834173,
    BidV2: 5706662,
    Spoke: 5706662, // we won't track the Spoke events for AMOY
  },
};
