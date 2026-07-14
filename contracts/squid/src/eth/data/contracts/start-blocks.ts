import { ChainId } from "@dcl/schemas";

enum ContractName {
  MANAToken = "MANAToken",
  ERC721Bid = "ERC721Bid",
  LANDProxy = "LANDProxy",
  EstateProxy = "EstateProxy",
  MarketplaceProxy = "MarketplaceProxy",
  DCLRegistrar = "DCLRegistrar",
  DCLControllerV2 = "DCLControllerV2",
}

export const startBlockByNetwork: Record<
  string,
  Record<ContractName, number>
> = {
  [ChainId.ETHEREUM_MAINNET]: {
    MANAToken: 4162050,
    ERC721Bid: 7270906,
    LANDProxy: 4944642,
    EstateProxy: 6236547,
    MarketplaceProxy: 6496012,
    DCLRegistrar: 9412979,
    DCLControllerV2: 16977347,
  },
  [ChainId.ETHEREUM_SEPOLIA]: {
    MANAToken: 3831216,
    ERC721Bid: 3831237,
    LANDProxy: 3831219,
    EstateProxy: 3831232,
    MarketplaceProxy: 3831225,
    DCLRegistrar: 3831239,
    DCLControllerV2: 3831242,
  },
};
