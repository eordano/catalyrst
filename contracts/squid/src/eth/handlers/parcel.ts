import { BlockData } from "../processor";
import { Network } from "@dcl/schemas";
import { UpdateEventArgs } from "../../abi/LANDRegistry";
import { getNFTId } from "../../common/utils/nft";
import {
  Category,
  Data,
  NFT,
  Parcel,
  Network as ModelNetwork,
} from "../../model";
import { getAddresses } from "../../common/utils/addresses";
import { DataType, buildData } from "../../common/utils";
import { Coordinate } from "../../types";
import { getParcelText } from "../LANDs/utils";

export function handleUpdate(
  event: UpdateEventArgs,
  block: BlockData,
  parcels: Map<string, Parcel>,
  nfts: Map<string, NFT>,
  coordinates: Map<bigint, Coordinate>,
  datas: Map<string, Data>
): void {
  const { assetId, data } = event;
  const parcelId = assetId.toString();
  const addresses = getAddresses(Network.ETHEREUM);

  const id = getNFTId(addresses.LANDRegistry, parcelId, Category.parcel);

  let parcel = parcels.get(id);
  if (!parcel) {
    console.log(`ERROR: id not found ${id} for Parcel on handleUpdate`);
    parcel = new Parcel({ id });
    parcels.set(id, parcel);
  }
  parcel.rawData = data;
  const parcelData = buildData(id, data, DataType.PARCEL);
  if (parcelData) {
    parcel.data = parcelData;
    datas.set(parcelData.id, parcelData);

    const coords = coordinates.get(assetId);
    if (coords) {
      parcel.x = coords._0;
      parcel.y = coords._1;
    }

    let nft = nfts.get(id);
    if (!nft) {
      console.log(`ERROR: id not found ${id} for NFT`);
      nft = new NFT({ id, network: ModelNetwork.ETHEREUM });
      nfts.set(id, nft);
    }
    nft.name = parcelData.name;
    if (parcelData.name) {
      nft.searchText = getParcelText(parcel, parcelData.name);
    }
    nft.updatedAt = BigInt(block.header.timestamp / 1000);
  } else {
    console.log(`ERROR: parcelData not found for parcel ${id}`);
  }
}
