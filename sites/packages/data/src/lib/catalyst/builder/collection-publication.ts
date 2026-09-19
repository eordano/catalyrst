import { createPublicClient, custom, encodeFunctionData, formatUnits, parseAbi, type Address, type Hex } from "viem";
import type { PublicationPreparation } from "@ui/generated/catalyst/builder/PublicationPreparation";
import type { Eip1193Provider } from "../../auth/wallet";
import { getJSON, type GetOptions } from "../client";
import { PublicationPreparationSchema } from "../generated-schemas/builder";

export const COLLECTION_MANAGER = "0x9d32aac179153a991e832550d9f96441ea27763a";
export const COLLECTION_FACTORY = "0x3195e88ae10704b359764cb38e429d24f1c2f781";
export const COLLECTION_FORWARDER = "0xbf6755a83c0dcdbb2933a96ea778e00b717d7004";
export const COLLECTION_MANA = "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4";
export const COLLECTION_BASE_URI = "https://peer.decentraland.org/lambdas/collections/standard/erc721/";

export const collectionManagerAbi = parseAbi([
  "function acceptedToken() view returns (address)",
  "function rarities() view returns (address)",
  "function createCollection(address forwarder, address factory, bytes32 salt, string name, string symbol, string baseURI, address creator, (string rarity, uint256 price, address beneficiary, string metadata)[] items)",
]);
export const collectionRaritiesAbi = parseAbi([
  "function getRarityByName(string rarity) view returns ((string name, uint256 maxSupply, uint256 price))",
]);

export type PublicationQuote = {
  preparation: PublicationPreparation;
  blockNumber: string;
  rarities: string;
  totalWei: string;
  totalMana: string;
  lines: { rarity: string; count: number; feeWei: string; mana: string }[];
  transaction: { from: Address; to: Address; data: Hex };
};

export async function fetchPublicationPreparation(id: string, opts: GetOptions): Promise<PublicationPreparation> {
  const response = await getJSON<{ data: unknown }>(`/v1/collections/${encodeURIComponent(id)}/publication`, opts);
  return PublicationPreparationSchema.parse(response.data);
}

export async function quoteCollectionPublication(
  preparation: PublicationPreparation,
  provider: Eip1193Provider,
  owner: string,
  signal?: AbortSignal,
): Promise<PublicationQuote> {
  signal?.throwIfAborted();
  if (preparation.chain_id !== 137 || preparation.manager.toLowerCase() !== COLLECTION_MANAGER
    || preparation.factory.toLowerCase() !== COLLECTION_FACTORY || preparation.forwarder.toLowerCase() !== COLLECTION_FORWARDER
    || preparation.base_uri !== COLLECTION_BASE_URI) throw new Error("Collection publication configuration has changed. Reload before publishing.");
  if (!/^0x[\da-f]{40}$/i.test(owner) || preparation.creator.toLowerCase() !== owner.toLowerCase()) throw new Error("Use the wallet that owns this collection.");
  if (!preparation.items.length || preparation.items.length > 50) throw new Error("Publish a collection containing 1 to 50 items.");
  if (BigInt(String(await provider.request({ method: "eth_chainId" }))) !== 137n) throw new Error("Switch your wallet to Polygon to check the publication fee.");
  const client = createPublicClient({ transport: custom(provider, { retryCount: 0 }) });
  const blockNumber = await client.getBlockNumber({ cacheTime: 0 });
  const [token, rarities] = await Promise.all([
    client.readContract({ address: COLLECTION_MANAGER, abi: collectionManagerAbi, functionName: "acceptedToken", blockNumber }),
    client.readContract({ address: COLLECTION_MANAGER, abi: collectionManagerAbi, functionName: "rarities", blockNumber }),
  ]);
  if (token.toLowerCase() !== COLLECTION_MANA) throw new Error("The collection manager no longer accepts the expected Polygon MANA token.");
  const counts = new Map<string, number>();
  for (const item of preparation.items) counts.set(item.rarity, (counts.get(item.rarity) ?? 0) + 1);
  const lines = await Promise.all([...counts].map(async ([rarity, count]) => {
    const result = await client.readContract({ address: rarities, abi: collectionRaritiesAbi, functionName: "getRarityByName", args: [rarity], blockNumber });
    if (result.name.toLowerCase() !== rarity.toLowerCase() || result.maxSupply === 0n) throw new Error(`The ${rarity} rarity is not available for publication.`);
    return { rarity, count, feeWei: result.price.toString(), mana: formatUnits(result.price * BigInt(count), 18) };
  }));
  signal?.throwIfAborted();
  if (BigInt(String(await provider.request({ method: "eth_chainId" }))) !== 137n) throw new Error("Your wallet network changed. Check the publication fee again.");
  const total = lines.reduce((sum, line) => sum + BigInt(line.feeWei) * BigInt(line.count), 0n);
  const data = encodeFunctionData({ abi: collectionManagerAbi, functionName: "createCollection", args: [
    COLLECTION_FORWARDER, COLLECTION_FACTORY, preparation.salt as Hex, preparation.name, preparation.symbol,
    preparation.base_uri, preparation.creator as Address,
    preparation.items.map(item => ({ rarity: item.rarity, price: BigInt(item.price), beneficiary: item.beneficiary as Address, metadata: item.metadata })),
  ] });
  return { preparation, rarities, blockNumber: blockNumber.toString(), totalWei: total.toString(), totalMana: formatUnits(total, 18), lines,
    transaction: { from: owner as Address, to: COLLECTION_MANAGER, data } };
}
