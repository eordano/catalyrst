import { createPublicClient, custom, encodeFunctionData, erc20Abi, type Address, type Hex } from "viem";
import type { PublicationState } from "@ui/generated/catalyst/builder/PublicationState";
import { PublicationStateSchema } from "../generated-schemas/builder";
import type { Eip1193Provider } from "../../auth/wallet";
import { COLLECTION_MANAGER, COLLECTION_MANA, fetchPublicationPreparation, quoteCollectionPublication, type PublicationQuote } from "./collection-publication";

type Session = { address: string; provider: Eip1193Provider; fetch: (path: string, init?: RequestInit) => Promise<Response> };
type Timing = { pollMs?: number; timeoutMs?: number; storage?: Pick<Storage, "getItem" | "setItem" | "removeItem"> };
export type CollectionPublishResult = { txHash: string; contractAddress: string; simulated: false; curationSubmitted: false };

export function collectionPublishWallet(getSession: () => Session, timing: Timing = {}) {
  const pending = new Map<string, Hex>();
  const uncertain = new Set<string>();
  let storage = timing.storage;
  try { storage ??= typeof window === "undefined" ? undefined : window.localStorage; } catch {}
  function key(owner: string, id: string, phase: string) { return `dcl:collection-publish:137:${owner.toLowerCase()}:${id}:${phase}`; }
  function read(key: string): Hex | undefined {
    let value = pending.get(key);
    try { value ??= storage?.getItem(key) as Hex | undefined; } catch {}
    return value && /^0x[\da-f]{64}$/i.test(value) ? value.toLowerCase() as Hex : undefined;
  }
  function save(key: string, hash?: Hex) {
    if (hash) pending.set(key, hash); else pending.delete(key);
    try { if (hash) storage?.setItem(key, hash); else storage?.removeItem(key); } catch {}
  }
  function uncertainSend(key: string): boolean {
    try { return uncertain.has(key) || storage?.getItem(`${key}:sending`) === "1"; } catch { return uncertain.has(key); }
  }
  function markSend(key: string, active: boolean) {
    if (active) uncertain.add(key); else uncertain.delete(key);
    try { if (active) storage?.setItem(`${key}:sending`, "1"); else storage?.removeItem(`${key}:sending`); } catch {}
  }
  async function request(id: string, method: string, body?: unknown, signal?: AbortSignal, suffix = "") {
    const response = await getSession().fetch(`/v1/collections/${encodeURIComponent(id)}/publication${suffix}`, {
      method, cache:"no-store", headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body), signal,
    });
    const result = await response.json();
    if (!response.ok) throw new Error(typeof result.message === "string" ? result.message : "Builder could not update the publication. Retry to resume it.");
    return result.data as unknown;
  }
  async function status(id: string, signal?: AbortSignal): Promise<PublicationState | null> {
    return PublicationStateSchema.nullable().parse(await request(id, "GET", undefined, signal, "/status"));
  }
  function session() {
    const s = getSession();
    if (!/^0x[\da-f]{40}$/i.test(s.address)) throw new Error("Sign in with the wallet that owns this collection.");
    const assertOwner = () => {
      if (getSession().address.toLowerCase() !== s.address.toLowerCase()) throw new Error("Your account changed. Sign in with the collection owner and retry.");
    };
    const assertWallet = async (signal?: AbortSignal) => {
      signal?.throwIfAborted(); assertOwner();
      const accounts = await s.provider.request({method:"eth_accounts"});
      if (!Array.isArray(accounts) || String(accounts[0]).toLowerCase() !== s.address.toLowerCase()) throw new Error("Your wallet account changed. Select the collection owner and retry.");
      if (BigInt(String(await s.provider.request({method:"eth_chainId"}))) !== 137n) throw new Error("Switch your wallet to Polygon and retry.");
    };
    return {...s, assertOwner, assertWallet, client:createPublicClient({transport:custom(s.provider, {retryCount:0})})};
  }
  async function connect(s: ReturnType<typeof session>, signal?: AbortSignal) {
    signal?.throwIfAborted(); s.assertOwner();
    if (BigInt(String(await s.provider.request({method:"eth_chainId"}))) !== 137n) {
      await s.provider.request({method:"wallet_switchEthereumChain", params:[{chainId:"0x89"}]});
    }
    await s.assertWallet(signal);
  }
  const pause = () => new Promise(resolve => setTimeout(resolve, timing.pollMs ?? 3000));
  async function send(s: ReturnType<typeof session>, key: string, to: Address, data: Hex, signal?: AbortSignal, beforeBroadcast?: () => Promise<unknown>) {
    let hash = read(key);
    if (!hash) {
      if (uncertainSend(key)) throw new Error("A wallet request is unresolved. Check wallet activity and recover its transaction hash before retrying.");
      await s.assertWallet(signal);
      const tx = {from:s.address, to, data};
      await s.provider.request({method:"eth_call", params:[tx, "latest"]});
      await s.assertWallet(signal);
      await beforeBroadcast?.();
      markSend(key, true);
      let result: unknown;
      try { result = await s.provider.request({method:"eth_sendTransaction", params:[tx]}); }
      catch (error) { if ((error as {code?:number})?.code === 4001) markSend(key, false); throw error; }
      if (typeof result !== "string" || !/^0x[\da-f]{64}$/i.test(result)) throw new Error("The wallet did not return a transaction hash. Check its activity before retrying.");
      hash = result.toLowerCase() as Hex;
      save(key, hash);
      markSend(key, false);
    }
    return hash;
  }
  async function approve(s: ReturnType<typeof session>, id: string, amount: bigint, signal?: AbortSignal) {
    const allowance = () => s.client.readContract({address:COLLECTION_MANA, abi:erc20Abi, functionName:"allowance", args:[s.address as Address, COLLECTION_MANAGER]});
    const approvalKey = key(s.address, id, `approve:${amount}`);
    if (!read(approvalKey) && await allowance() === amount) { markSend(approvalKey, false); return; }
    const hash = await send(s, approvalKey, COLLECTION_MANA, encodeFunctionData({abi:erc20Abi, functionName:"approve", args:[COLLECTION_MANAGER, amount]}), signal);
    const deadline = Date.now() + (timing.timeoutMs ?? 300_000);
    for (;;) {
      await s.assertWallet(signal);
      const receipt = await s.provider.request({method:"eth_getTransactionReceipt", params:[hash]}) as {status?:string} | null;
      if (receipt) {
        if (receipt.status !== "0x1") { save(approvalKey); throw new Error("MANA approval reverted. Retry from your wallet."); }
        if (await allowance() !== amount) throw new Error("The exact MANA approval could not be verified. Check your wallet and retry.");
        save(approvalKey); return;
      }
      if (Date.now() >= deadline) throw new Error("MANA approval is still pending. Retry to wait for the same transaction.");
      await pause();
    }
  }
  async function confirm(s: ReturnType<typeof session>, id: string, hash: string, signal?: AbortSignal): Promise<CollectionPublishResult> {
    hash = hash.toLowerCase();
    const deadline = Date.now() + (timing.timeoutMs ?? 300_000);
    for (;;) {
      signal?.throwIfAborted(); s.assertOwner();
      const result = PublicationStateSchema.parse(await request(id, "PUT", {tx_hash:hash}, signal));
      if (result.status === "published" && result.contract_address && result.tx_hash === hash) {
        save(key(s.address, id, "publish"));
        return {txHash:hash, contractAddress:result.contract_address, simulated:false, curationSubmitted:false};
      }
      if (result.status === "reverted") {
        save(key(s.address, id, "publish"));
        throw new Error("The collection transaction reverted. Review the fee and retry.");
      }
      if (Date.now() >= deadline) throw new Error("Your publication is awaiting Polygon finality. Retry to check the same transaction.");
      await pause();
    }
  }
  return {
    status,
    async quote({id, signal}: {id:string; signal?:AbortSignal}) {
      const s = session(); await connect(s, signal);
      const fetchImpl: typeof fetch = (input, init) => s.fetch(String(input), init);
      const preparation = await fetchPublicationPreparation(id, {base:"", fetchImpl, signal});
      return quoteCollectionPublication(preparation, s.provider, s.address, signal);
    },
    async recover({id, txHash, signal}: {id:string; txHash:string; signal?:AbortSignal}) {
      if (!/^0x[\da-f]{64}$/i.test(txHash)) throw new Error("Enter a valid Polygon transaction hash.");
      const s = session();
      const publicationKey = key(s.address, id, "publish");
      save(publicationKey, txHash as Hex); markSend(publicationKey, false);
      return confirm(s, id, txHash, signal);
    },
    async cancel(id: string, signal?: AbortSignal) {
      const s = session();
      if (read(key(s.address, id, "publish")) || uncertainSend(key(s.address, id, "publish"))) throw new Error("A transaction was submitted. Retry to check its receipt before making changes.");
      if ((await status(id, signal))?.status === "signing") throw new Error("A wallet request is unresolved. Recover its transaction before canceling.");
      await request(id, "DELETE", undefined, signal);
    },
    async publish({id, quote, signal, prepare}: {id:string; quote?:PublicationQuote; signal?:AbortSignal; prepare?:(quote:PublicationQuote, signal?:AbortSignal)=>Promise<void>}): Promise<CollectionPublishResult> {
      const s = session();
      const existing = await status(id, signal);
      s.assertOwner();
      if (existing?.status === "published" && existing.tx_hash && existing.contract_address) {
        save(key(s.address, id, "publish"));
        return {txHash:existing.tx_hash, contractAddress:existing.contract_address, simulated:false, curationSubmitted:false};
      }
      const pendingHash = existing?.status === "submitted" ? existing.tx_hash : read(key(s.address, id, "publish"));
      if (pendingHash) return confirm(s, id, pendingHash, signal);
      if (existing?.status === "signing") throw new Error("A wallet request is unresolved. Recover its transaction hash before retrying.");
      if (!quote || quote.preparation.id !== id || quote.preparation.creator.toLowerCase() !== s.address.toLowerCase()) throw new Error("Review this collection's current publication fee first.");
      await connect(s, signal);
      const checkFee = async () => {
        const fresh = await quoteCollectionPublication(quote.preparation, s.provider, s.address, signal);
        if (fresh.totalWei !== quote.totalWei) throw new Error("The publication fee changed. Go back to review the new fee before signing.");
        if (fresh.transaction.data !== quote.transaction.data) throw new Error("The publication transaction changed. Review the collection again.");
      };
      await checkFee();
      const balance = await s.client.readContract({address:COLLECTION_MANA, abi:erc20Abi, functionName:"balanceOf", args:[s.address as Address]});
      if (balance < BigInt(quote.totalWei)) throw new Error(`You need ${quote.totalMana} MANA on Polygon to publish this collection.`);
      await request(id, "POST", {revision:quote.preparation.revision}, signal);
      try {
        await prepare?.(quote, signal);
        await approve(s, id, BigInt(quote.totalWei), signal);
        await checkFee();
        const hash = await send(s, key(s.address, id, "publish"), COLLECTION_MANAGER, quote.transaction.data, signal, () => request(id, "PATCH", {revision:quote.preparation.revision}, signal));
        return await confirm(s, id, hash, signal);
      } catch (error) {
        if ((error as {code?:number})?.code === 4001 && !read(key(s.address, id, "publish"))) {
          await request(id, "DELETE", undefined, signal).catch(() => undefined);
        }
        throw error;
      }
    },
  };
}
