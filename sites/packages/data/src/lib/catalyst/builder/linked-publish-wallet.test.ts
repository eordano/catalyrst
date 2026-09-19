import { describe, expect, it, vi } from "vitest";
import { concat, encodeAbiParameters, keccak256, numberToHex, recoverAddress, stringToHex, toFunctionSelector, type Hex } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import type { Eip1193Provider } from "../../auth/wallet";
import { FOUNDATION_BUILDER } from "./foundation-fetch";
import { linkedPublishWallet, LINKED_REGISTRY } from "./linked-publish-wallet";

const thirdPartyId = "urn:decentraland:matic:collections-thirdparty:provider";
const intent = { thirdPartyId, qty: 2, salt: `0x${"ab".repeat(32)}` };
const hashText = (text: string) => keccak256(stringToHex(text));
const registryDomain = keccak256(encodeAbiParameters(
  [{ type: "bytes32" }, { type: "bytes32" }, { type: "bytes32" }, { type: "address" }, { type: "bytes32" }],
  [hashText("EIP712Domain(string name,string version,address verifyingContract,bytes32 salt)"),
    hashText("Decentraland Third Party Registry"), hashText("1"), LINKED_REGISTRY, numberToHex(137, { size: 32 })],
));
function fixture() {
  const account = privateKeyToAccount(generatePrivateKey());
  const state = { address: account.address as string, wallet: account.address as string, chain: "0x89", slots: 5 as unknown,
    managers: [account.address] as string[], domain: registryDomain, wrongSigner: false, reject: false,
    afterSign: () => {}, afterSlots: () => {}, switchWorks: true };
  const rpc = vi.fn<Eip1193Provider["request"]>(async ({ method, params }) => {
    if (method === "eth_chainId") return state.chain;
    if (method === "eth_accounts") return [state.wallet];
    if (method === "wallet_switchEthereumChain") { if (state.switchWorks) state.chain = "0x89"; return null; }
    if (method === "eth_call") {
      expect(params).toEqual([{ to: LINKED_REGISTRY, data: toFunctionSelector("domainSeparator()") }, "latest"]);
      return state.domain;
    }
    if (method === "eth_signTypedData_v4") {
      if (state.reject) throw Object.assign(new Error("User rejected"), { code: 4001 });
      expect(params?.[0]).toBe(account.address);
      const data = JSON.parse(String(params?.[1]));
      expect(data).toMatchObject({ primaryType: "ConsumeSlots", message: intent, domain: {
        name: "Decentraland Third Party Registry", version: "1", verifyingContract: LINKED_REGISTRY, salt: numberToHex(137, { size: 32 }),
      } });
      expect(data.domain).not.toHaveProperty("chainId");
      const signer = state.wrongSigner ? privateKeyToAccount(generatePrivateKey()) : account;
      const signature = await signer.signTypedData(data);
      state.afterSign();
      return signature;
    }
    throw new Error(`Unexpected wallet operation: ${method}`);
  });
  const fetcher = vi.fn(async (url: string) => {
    if (url === `${FOUNDATION_BUILDER}/thirdParties`) return Response.json({ data: [{ id: thirdPartyId, name: "Provider", published: true, managers: state.managers }] });
    expect(url).toBe(`${FOUNDATION_BUILDER}/thirdParties/${encodeURIComponent(thirdPartyId)}/slots`);
    state.afterSlots();
    return Response.json({ data: state.slots });
  });
  const sign = linkedPublishWallet(() => ({ address: state.address, provider: { request: rpc }, fetch: fetcher }));
  const signatures = () => rpc.mock.calls.filter(([call]) => call.method === "eth_signTypedData_v4");
  return { account, state, rpc, fetcher, sign, signatures };
}

describe("linked collection slot authorization", () => {
  it("matches Foundation's ABI-encoded cheque digest and keeps the reviewed nonce on retry", async () => {
    const f = fixture();
    const cheque = await f.sign(intent);
    expect(cheque.qty).toBe(intent.qty); expect(cheque.salt).toBe(intent.salt);
    const messageHash = keccak256(encodeAbiParameters(
      [{ type: "bytes32" }, { type: "bytes32" }, { type: "uint256" }, { type: "bytes32" }],
      [hashText("ConsumeSlots(string thirdPartyId,uint256 qty,bytes32 salt)"), hashText(thirdPartyId), 2n, intent.salt as Hex],
    ));
    const hash = keccak256(concat(["0x1901", registryDomain, messageHash]));
    expect(await recoverAddress({ hash, signature: cheque.signature })).toBe(f.account.address);
    expect(await f.sign(intent)).toEqual(cheque);
    expect(f.signatures()).toHaveLength(2);
  });
  it("switches to Polygon before signing and rejects a failed switch", async () => {
    const f = fixture(); f.state.chain = "0x1";
    await f.sign(intent);
    expect(f.rpc.mock.calls.some(([call]) => call.method === "wallet_switchEthereumChain")).toBe(true);
    const failed = fixture(); failed.state.chain = "0x1"; failed.state.switchWorks = false;
    await expect(failed.sign(intent)).rejects.toThrow("Polygon"); expect(failed.signatures()).toHaveLength(0);
  });
  it("requires current provider management and enough available slots", async () => {
    const f = fixture(); f.state.managers = [];
    await expect(f.sign(intent)).rejects.toThrow("no longer manage"); expect(f.signatures()).toHaveLength(0);
    f.state.managers = [f.account.address.toUpperCase()]; f.state.slots = 1;
    await expect(f.sign(intent)).rejects.toThrow("1 available item slots"); expect(f.signatures()).toHaveLength(0);
  });
  it.each([-1, 1.5, "5", null, Number.MAX_SAFE_INTEGER + 1])("does not interpret invalid slot count %s as permission to sign", async slots => {
    const f = fixture(); f.state.slots = slots;
    await expect(f.sign(intent)).rejects.toThrow(); expect(f.signatures()).toHaveLength(0);
  });
  it("rejects a registry with another domain before opening a signing prompt", async () => {
    const f = fixture(); f.state.domain = `0x${"00".repeat(32)}`;
    await expect(f.sign(intent)).rejects.toThrow("signing domain"); expect(f.signatures()).toHaveLength(0);
  });
  it("rejects invalid intent fields without accessing the wallet or Foundation", async () => {
    const f = fixture();
    for (const bad of [{ ...intent, qty: 0 }, { ...intent, qty: 1.5 }, { ...intent, salt: "0x123" }, { ...intent, thirdPartyId: "urn:decentraland:amoy:collections-thirdparty:provider" }]) {
      await expect(f.sign(bad)).rejects.toThrow();
    }
    expect(f.rpc).not.toHaveBeenCalled(); expect(f.fetcher).not.toHaveBeenCalled();
  });
  it("rechecks account and chain after asynchronous discovery", async () => {
    for (const change of ["account", "wallet", "chain"] as const) {
      const f = fixture();
      f.state.afterSlots = () => {
        if (change === "account") f.state.address = "0x" + "22".repeat(20);
        if (change === "wallet") f.state.wallet = "0x" + "22".repeat(20);
        if (change === "chain") f.state.chain = "0x1";
      };
      await expect(f.sign(intent)).rejects.toThrow(); expect(f.signatures()).toHaveLength(0);
    }
  });
  it("rejects a different signer and account changes while the signing prompt is open", async () => {
    const f = fixture(); f.state.wrongSigner = true;
    await expect(f.sign(intent)).rejects.toThrow("different wallet");
    f.state.wrongSigner = false; f.state.afterSign = () => { f.state.address = "0x" + "22".repeat(20); };
    await expect(f.sign(intent)).rejects.toThrow("account changed");
  });
  it("propagates wallet rejection and never submits a transaction", async () => {
    const f = fixture(); f.state.reject = true;
    await expect(f.sign(intent)).rejects.toMatchObject({ code: 4001 });
    expect(f.rpc.mock.calls.some(([call]) => call.method === "eth_sendTransaction")).toBe(false);
  });
  it("stops an aborted publication before signing", async () => {
    const f = fixture(); const abort = new AbortController();
    f.state.afterSlots = () => abort.abort();
    await expect(f.sign(intent, abort.signal)).rejects.toThrow(); expect(f.signatures()).toHaveLength(0);
  });
});
