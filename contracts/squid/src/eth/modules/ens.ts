import { ENS, NFT } from "../../model";

export function buildENSFromNFT(nft: NFT): ENS {
  const ens = new ENS({ id: nft.id });

  ens.tokenId = nft.tokenId;
  ens.owner = nft.owner;

  return ens;
}
