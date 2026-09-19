import { describe, expect, it } from "vitest";
import { decodeFunctionData, encodeFunctionResult, erc20Abi, getAddress, keccak256, parseUnits, stringToHex, type Hex } from "viem";
import { nameClaimWallet, NAME_CONTROLLER, NAME_MANA, NAME_REGISTRAR, nameControllerAbi, nameRegistrarAbi } from "./name-claim";
import type { Eip1193Provider } from "../../auth/wallet";

const address = "0x1111111111111111111111111111111111111111";
const abi = [...erc20Abi, ...nameControllerAbi, ...nameRegistrarAbi];
function fixture(storage?: Pick<Storage, "getItem" | "setItem" | "removeItem">) {
  const state = { price: parseUnits("123.5", 18), balance: parseUnits("500", 18), allowance: 0n, available: true,
    account: address, owner: address, chain: "0x1", receipt: "0x1" as string | null, token: NAME_MANA, registrar: NAME_REGISTRAR, rejected: false };
  const sent: { to: string; data: Hex; from: string }[] = [];
  const provider: Eip1193Provider = { async request({ method, params }) {
    if (method === "eth_chainId") return state.chain;
    if (method === "eth_accounts") return [state.account];
    if (method === "wallet_switchEthereumChain") { state.chain = "0x1"; return null; }
    if (method === "eth_sendTransaction") {
      if (state.rejected) throw new Error("User rejected transaction");
      sent.push(params![0] as typeof sent[number]);
      return `0x${sent.length.toString(16).padStart(64, '0')}`;
    }
    if (method === "eth_getTransactionReceipt") {
      if (state.receipt === null) return null;
      if (state.receipt === "0x1") {
        const index = Number(BigInt(String(params![0]))) - 1;
        const tx = decodeFunctionData({ abi, data: sent[index].data });
        if (tx.functionName === "approve") state.allowance = tx.args![1] as bigint;
        if (tx.functionName === "register") state.available = false;
      }
      return { status: state.receipt };
    }
    if (method === "eth_call") {
      const tx = params![0] as { data: Hex; to: string };
      const call = decodeFunctionData({ abi, data: tx.data });
      const results: Record<string, string | bigint | boolean> = { PRICE: state.price, acceptedToken: state.token, registrar: state.registrar,
        available: state.available, allowance: state.allowance, balanceOf: state.balance, ownerOf: state.owner };
      const result = results[call.functionName];
      if (call.functionName === "approve") return encodeFunctionResult({ abi: erc20Abi, functionName: "approve", result: true });
      if (call.functionName === "register") return "0x";
      return encodeFunctionResult({ abi, functionName: call.functionName, result });
    }
    throw new Error(`Unexpected RPC: ${method}`);
  } };
  const claim = nameClaimWallet(() => ({ provider, address }), { pollMs: 0, timeoutMs: 0, storage });
  return { state, sent, claim, provider };
}
const args = { name: "MyWorld", priceMana: "123.5" };

describe("wallet-backed NAME registration", () => {
  it("reads the contract quote, approves the exact price, and verifies the registered owner", async () => {
    const { claim, sent } = fixture();
    expect(await claim.check(args)).toEqual({ available: true, priceMana: "123.5", pendingRegistration: false });
    await claim.approve(args);
    const result = await claim.mint(args);
    expect(sent).toHaveLength(2);
    expect(sent[0].to).toBe(NAME_MANA);
    expect(decodeFunctionData({ abi, data: sent[0].data })).toMatchObject({ functionName: "approve", args: [getAddress(NAME_CONTROLLER), parseUnits("123.5", 18)] });
    expect(sent[1].to).toBe(NAME_CONTROLLER);
    expect(decodeFunctionData({ abi, data: sent[1].data })).toMatchObject({ functionName: "register", args: ["MyWorld", address] });
    expect(result).toMatchObject({ simulated: false, tokenId: BigInt(keccak256(stringToHex("myworld"))).toString() });
  });
  it("switches to Ethereum and reuses an existing sufficient allowance", async () => {
    const { claim, state, sent } = fixture();
    state.chain = "0x89";
    state.allowance = state.price;
    await claim.approve(args);
    expect(state.chain).toBe("0x1");
    expect(sent).toEqual([]);
  });
  it("refuses account mismatches, controller changes, stale quotes, low balances and taken names", async () => {
    for (const change of [
      (s: ReturnType<typeof fixture>["state"]) => { s.account = NAME_CONTROLLER; },
      (s: ReturnType<typeof fixture>["state"]) => { s.token = NAME_CONTROLLER; },
      (s: ReturnType<typeof fixture>["state"]) => { s.price += 1n; },
      (s: ReturnType<typeof fixture>["state"]) => { s.balance = 0n; },
      (s: ReturnType<typeof fixture>["state"]) => { s.available = false; },
    ]) {
      const { claim, state, sent } = fixture(); change(state);
      await expect(claim.approve(args)).rejects.toThrow();
      expect(sent).toEqual([]);
    }
  });
  it("does not mint without allowance or claim success for a reverted receipt", async () => {
    const { claim, state, sent } = fixture();
    await expect(claim.mint(args)).rejects.toThrow("Approve MANA");
    state.receipt = "0x0";
    await expect(claim.approve(args)).rejects.toThrow("reverted");
    expect(sent).toHaveLength(1);
    state.receipt = "0x1";
    await claim.approve(args);
    expect(sent).toHaveLength(2);
  });
  it("resumes the same pending approval and registration after receipt timeout", async () => {
    const { claim, state, sent } = fixture();
    state.receipt = null;
    await expect(claim.approve(args)).rejects.toThrow("still pending");
    state.receipt = "0x1";
    await claim.approve(args);
    expect(sent).toHaveLength(1);
    state.receipt = null;
    await expect(claim.mint(args)).rejects.toThrow("still pending");
    state.receipt = "0x1";
    await claim.mint(args);
    expect(sent).toHaveLength(2);
  });
  it("preserves a confirmed hash if ownership verification fails and retries without another purchase", async () => {
    const { claim, state, sent } = fixture();
    await claim.approve(args);
    state.owner = NAME_CONTROLLER;
    await expect(claim.mint(args)).rejects.toThrow("ownership could not be verified");
    state.owner = address;
    await claim.mint(args);
    expect(sent).toHaveLength(2);
  });
  it("propagates wallet rejection and aborts without sending", async () => {
    const { claim, state, sent } = fixture();
    state.rejected = true;
    await expect(claim.approve(args)).rejects.toThrow("User rejected");
    await expect(claim.approve({ ...args, signal: AbortSignal.abort() })).rejects.toThrow();
    expect(sent).toEqual([]);
  });
});


it("restores an in-flight registration after reload without asking for another purchase", async () => {
  const values = new Map<string, string>();
  const storage = { getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
  const { claim, state, sent, provider } = fixture(storage);
  await claim.approve(args);
  state.receipt = null;
  await expect(claim.mint(args)).rejects.toThrow("still pending");
  const restored = nameClaimWallet(() => ({ provider, address }), { storage, timeoutMs: 0 });
  state.available = false;
  expect(await restored.check(args)).toMatchObject({ available: true, pendingRegistration: true });
  state.receipt = "0x1";
  await restored.mint(args);
  expect(sent).toHaveLength(2);
  expect(values.size).toBe(0);
});
