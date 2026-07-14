import { Network } from "@dcl/schemas";
import { getAddresses } from "../../common/utils/addresses";
import { buildCount } from "../modules/count";
import { Count } from "../../model";

export function handleInitializeWearablesV1(counts: Map<string, Count>): void {
  const addresses = getAddresses(Network.ETHEREUM);
  let count = buildCount(counts);

  const collectionsV1 = Object.values(addresses.collections);

  if (count.started) {
    count.collectionTotal += collectionsV1.length;
  }

  count.started = 1;
  counts.set(count.id, count);
}
