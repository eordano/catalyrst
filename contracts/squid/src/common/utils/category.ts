import { Network } from "@dcl/schemas";
import { Category } from "../../model";
import { getAddresses } from "./addresses";

export function getCategory(network: Network, contractAddress: string): string {
  const addresses = getAddresses(network);

  let category = "";

  if (contractAddress == addresses.LANDRegistry) {
    category = Category.parcel;
  } else if (contractAddress == addresses.EstateRegistry) {
    category = Category.estate;
  } else if (contractAddress == addresses.DCLRegistrar) {
    category = Category.ens;
  } else if (Object.values(addresses.collections).includes(contractAddress)) {
    category = Category.wearable;
  } else {
    category = contractAddress;
  }

  return category;
}
