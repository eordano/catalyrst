import { BlockData } from "../processor";
import { Contract as LANDRegistryContract } from "../../abi/LANDRegistry";
import { Estate, NFT, Parcel } from "../../model";
import { Context } from "../processor";
import { p as proximities } from "../data/lands/proximities";
import { Coordinate } from "../../types";

export async function decodeTokenId(
  ctx: Context,
  block: BlockData,
  contractAddress: string,
  assetId: bigint
): Promise<bigint[]> {
  const contract = new LANDRegistryContract(ctx, block.header, contractAddress);
  const coordinate = await contract.decodeTokenId(assetId);
  return [coordinate._0, coordinate._1];
}

export function buildParcelFromNFT(nft: NFT, coordinates: Coordinate): Parcel {
  const parcel = new Parcel({ id: nft.id });

  parcel.x = coordinates._0;
  parcel.y = coordinates._1;
  parcel.tokenId = nft.tokenId;
  parcel.owner = nft.owner;
  return parcel;
}

export function buildEstateFromNFT(nft: NFT): Estate {
  let estate = new Estate({ id: nft.id });

  estate.tokenId = nft.tokenId;
  estate.owner = nft.owner;
  estate.size = 0;
  estate.parcelDistances = [];
  estate.adjacentToRoadCount = 0;

  return estate;
}

export function getEstateImage(estate: Estate): string {
  return (
    "https://api.decentraland.org/v1/estates/" +
    estate.tokenId.toString() +
    "/map.png"
  );
}

export function getParcelText(parcel: Parcel, name: string): string {
  let text = parcel.x.toString() + "," + parcel.y.toString();
  if (name != "") {
    text += "," + name.toLowerCase();
  }
  return text;
}

export function getParcelImage(parcel: Parcel): string {
  return (
    "https://api.decentraland.org/v1/parcels/" +
    parcel.x.toString() +
    "/" +
    parcel.y.toString() +
    "/map.png"
  );
}

export function getDistanceToPlaza(parcel: Parcel): number {
  let coord = getParcelText(parcel, "");
  if (proximities.has(coord)) {
    let proximity = proximities.get(coord)!;
    return proximity.p;
  }
  return -1;
}

export function getAdjacentToRoad(parcel: Parcel): boolean {
  let coord = getParcelText(parcel, "");
  if (proximities.has(coord)) {
    let proximity = proximities.get(coord)!;
    return proximity.r;
  }
  return false;
}

export function isInBounds(x: bigint, y: bigint): boolean {
  let lowerBound = -150;
  let upperBound = 150;
  return (
    x >= lowerBound && x <= upperBound && y >= lowerBound && y <= upperBound
  );
}
