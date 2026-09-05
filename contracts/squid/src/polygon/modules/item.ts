import { ChainId } from "@dcl/schemas";
import { getCatalystBase } from "../../common/utils/catalyst";
import { Item } from "../../model";

export function getItemId(contractAddress: string, itemId: string): string {
  return contractAddress + "-" + itemId;
}

export function getItemImage(item: Item): string {
  const chainId = parseInt(
    process.env.POLYGON_CHAIN_ID || ChainId.MATIC_MAINNET.toString()
  ) as ChainId;
  const baseURI = getCatalystBase(chainId);

  return baseURI + "/lambdas/collections/contents/" + item.urn + "/thumbnail";
}

export function removeItemMinter(item: Item, minter: string): Array<string> {
  return item.minters.filter((i) => i != minter);

}
