import { describe, expect, it } from "vitest";
import { decodeFunctionData, encodeFunctionResult, parseUnits, type Hex } from "viem";
import type { PublicationPreparation } from "@ui/generated/catalyst/builder/PublicationPreparation";
import type { Eip1193Provider } from "../../auth/wallet";
import { COLLECTION_BASE_URI, COLLECTION_FACTORY, COLLECTION_FORWARDER, COLLECTION_MANAGER, COLLECTION_MANA,
  collectionManagerAbi, collectionRaritiesAbi, fetchPublicationPreparation, quoteCollectionPublication } from "./collection-publication";

const owner = "0x1111111111111111111111111111111111111111";
const oracle = "0x3333333333333333333333333333333333333333";
function preparation(): PublicationPreparation {
  return { id: "collection-id", revision: "revision", chain_id: 137, manager: COLLECTION_MANAGER, factory: COLLECTION_FACTORY,
    forwarder: COLLECTION_FORWARDER, salt: `0x${"ab".repeat(32)}`, name: "Collection", symbol: "DCL-CLLCTN", base_uri: COLLECTION_BASE_URI,
    creator: owner, items: ["rare", "rare", "unique"].map((rarity, i) => ({id: `${i}`, rarity, price: "1000000000000000001", beneficiary: owner,
      metadata: `1:w:Hat ${i}::hat:BaseMale`})) };
}
function fixture() {
  const state = { chain: "0x89", token: COLLECTION_MANA as Hex, unavailable: false };
  const calls: { functionName: string; to: string; block: string }[] = [];
  const provider: Eip1193Provider = { async request({ method, params }) {
    if (method === "eth_chainId") return state.chain;
    if (method === "eth_blockNumber") return "0x2a";
    if (method === "eth_call") {
      const [tx, block] = params as [{to: string; data: Hex}, string];
      const abi = [...collectionManagerAbi, ...collectionRaritiesAbi];
      const decoded = decodeFunctionData({abi, data: tx.data});
      calls.push({functionName: decoded.functionName, to: tx.to, block});
      if (decoded.functionName === "acceptedToken") return encodeFunctionResult({abi: collectionManagerAbi, functionName: "acceptedToken", result: state.token});
      if (decoded.functionName === "rarities") return encodeFunctionResult({abi: collectionManagerAbi, functionName: "rarities", result: oracle});
      if (decoded.functionName === "getRarityByName") {
        const rarity = decoded.args![0] as string;
        return encodeFunctionResult({abi: collectionRaritiesAbi, functionName: "getRarityByName", result: {
          name: rarity, maxSupply: state.unavailable ? 0n : 1000n, price: parseUnits(rarity === "rare" ? "123.500000000000000001" : "99.01", 18),
        }});
      }
    }
    throw new Error(`Unexpected RPC ${method}`);
  }};
  return {provider, state, calls};
}

describe("collection publication preparation", () => {
  it("fetches the owner endpoint and validates its wire response", async () => {
    let requested = "";
    const fetchImpl: typeof fetch = async input => { requested = String(input); return Response.json({data: preparation()}); };
    await expect(fetchPublicationPreparation("collection-id", {base: "https://builder.test", fetchImpl})).resolves.toEqual(preparation());
    expect(requested).toBe("https://builder.test/v1/collections/collection-id/publication");
    await expect(fetchPublicationPreparation("bad", {base: "https://builder.test", fetchImpl: async () => Response.json({data: {}})})).rejects.toThrow();
  });
  it("uses live rarity fees at one block and preserves exact wei and item order in calldata", async () => {
    const {provider, calls} = fixture();
    const result = await quoteCollectionPublication(preparation(), provider, owner);
    expect(result.totalMana).toBe("346.010000000000000002");
    expect(result.totalWei).toBe("346010000000000000002");
    expect(result.blockNumber).toBe("42");
    expect(calls.every(c => c.block === "0x2a")).toBe(true);
    expect(calls.filter(c => c.to.toLowerCase() === oracle)).toHaveLength(2);
    expect(result.transaction.to).toBe(COLLECTION_MANAGER);
    const call = decodeFunctionData({abi: collectionManagerAbi, data: result.transaction.data});
    expect(call.functionName).toBe("createCollection");
    if (call.functionName !== "createCollection") throw new Error("Wrong method");
    expect(call.args.slice(0, 3).map(a => String(a).toLowerCase())).toEqual([COLLECTION_FORWARDER, COLLECTION_FACTORY, preparation().salt]);
    expect(call.args[6].toLowerCase()).toBe(owner);
    expect(call.args[7].map(i => i.metadata)).toEqual(preparation().items.map(i => i.metadata));
    expect(call.args[7][0].price).toBe(1000000000000000001n);
  });
  it("rejects owner, chain and contract mismatches before quoting", async () => {
    const {provider, state} = fixture();
    for (const changes of [{creator: oracle}, {chain_id: 1}, {manager: oracle}, {factory: oracle}, {forwarder: oracle}, {base_uri: "https://bad.test/"}, {items: []}]) {
      await expect(quoteCollectionPublication({...preparation(), ...changes}, provider, owner)).rejects.toThrow();
    }
    state.chain = "0x1";
    await expect(quoteCollectionPublication(preparation(), provider, owner)).rejects.toThrow("Polygon");
  });
  it("refuses a changed fee token or unknown rarity", async () => {
    const {provider, state} = fixture();
    state.token = oracle;
    await expect(quoteCollectionPublication(preparation(), provider, owner)).rejects.toThrow("MANA");
    state.token = COLLECTION_MANA;
    state.unavailable = true;
    await expect(quoteCollectionPublication(preparation(), provider, owner)).rejects.toThrow("rarity");
  });
  it("does not convert provider failure into a zero-fee quote", async () => {
    const provider: Eip1193Provider = { async request() { throw new Error("RPC offline"); } };
    await expect(quoteCollectionPublication(preparation(), provider, owner)).rejects.toThrow("RPC offline");
    await expect(quoteCollectionPublication(preparation(), provider, owner, AbortSignal.abort())).rejects.toThrow();
  });
});
