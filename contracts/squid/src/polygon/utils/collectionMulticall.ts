import { Multicall, AggregateTuple } from "../../abi/multicall";
import { functions as CollectionV2Functions } from "../abi/CollectionV2";
import type { Context, Block } from "../processor";

const CALLS_PER_COLLECTION = 9;

const MULTICALL_CONTRACT = "0xcA11bde05977b3631167028862bE2a173976CA11";
export const POLYGON_MULTICALL_CREATION_BLOCK = 25770160;

export interface CollectionData {
  address: string;
  name: string;
  symbol: string;
  owner: string;
  creator: string;
  isCompleted: boolean;
  isApproved: boolean;
  isEditable: boolean;
  baseURI: string;
  chainId: bigint;
}

export async function fetchCollectionDataMulticall(
  ctx: Context,
  blockHeader: Block,
  collectionAddresses: string[]
): Promise<Map<string, CollectionData>> {
  if (collectionAddresses.length === 0) {
    return new Map();
  }

  if (blockHeader.height < POLYGON_MULTICALL_CREATION_BLOCK) {
    console.log(`\u{26A0}\u{FE0F} Block ${blockHeader.height} is before multicall creation, falling back to individual calls`);
    return new Map();
  }

  const multicall = new Multicall(ctx, blockHeader, MULTICALL_CONTRACT);
  const results = new Map<string, CollectionData>();

  const calls: AggregateTuple[] = [];
  
  for (const address of collectionAddresses) {
    calls.push([CollectionV2Functions.name, address, {}]);
    calls.push([CollectionV2Functions.symbol, address, {}]);
    calls.push([CollectionV2Functions.owner, address, {}]);
    calls.push([CollectionV2Functions.creator, address, {}]);
    calls.push([CollectionV2Functions.isCompleted, address, {}]);
    calls.push([CollectionV2Functions.isApproved, address, {}]);
    calls.push([CollectionV2Functions.isEditable, address, {}]);
    calls.push([CollectionV2Functions.baseURI, address, {}]);
    calls.push([CollectionV2Functions.getChainId, address, {}]);
  }

  const multicallStart = performance.now();
  
  try {
    const rawResults = await multicall.tryAggregate(calls, 100);
    
    const multicallDuration = performance.now() - multicallStart;
    const fmt = (ms: number) => ms >= 1000 ? `${(ms/1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
    console.log(`\u{2705} Multicall for ${collectionAddresses.length} collections (${calls.length} calls): ${fmt(multicallDuration)}`);

    for (let i = 0; i < collectionAddresses.length; i++) {
      const address = collectionAddresses[i].toLowerCase();
      const baseIndex = i * CALLS_PER_COLLECTION;

      const allSuccess = rawResults
        .slice(baseIndex, baseIndex + CALLS_PER_COLLECTION)
        .every((r) => r.success);
      
      if (!allSuccess) {
        console.log(`\u{26A0}\u{FE0F} Multicall failed for collection ${address.slice(0, 10)}, will use fallback`);
        continue;
      }

      results.set(address, {
        address,
        name: rawResults[baseIndex + 0].value as string,
        symbol: rawResults[baseIndex + 1].value as string,
        owner: (rawResults[baseIndex + 2].value as string).toLowerCase(),
        creator: (rawResults[baseIndex + 3].value as string).toLowerCase(),
        isCompleted: rawResults[baseIndex + 4].value as boolean,
        isApproved: rawResults[baseIndex + 5].value as boolean,
        isEditable: rawResults[baseIndex + 6].value as boolean,
        baseURI: rawResults[baseIndex + 7].value as string,
        chainId: rawResults[baseIndex + 8].value as bigint,
      });
    }
  } catch (e: any) {
    console.error(`\u{274C} Multicall failed completely, will use fallback: ${e.message}`);
    return new Map();
  }

  return results;
}

