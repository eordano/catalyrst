import { createPublicClient, custom, encodeFunctionData, erc20Abi, formatUnits, keccak256, parseAbi, parseUnits, stringToHex, type Address, type Hex } from "viem";
import type { Eip1193Provider } from "../../auth/wallet";
import { NAME_ECONOMICS, NAME_REGEX } from "./names";

export const NAME_CONTROLLER = "0xbe92b49aee993adea3a002adcda189a2b7dec56c";
export const NAME_MANA = "0x0f5d2fb29fb7d3cfee444a200298f468908cc942";
export const NAME_REGISTRAR = NAME_ECONOMICS.registrarContractAddress;
export const nameControllerAbi = parseAbi([
  "function PRICE() view returns (uint256)",
  "function acceptedToken() view returns (address)",
  "function registrar() view returns (address)",
  "function register(string name, address beneficiary)",
]);
export const nameRegistrarAbi = parseAbi([
  "function available(string name) view returns (bool)",
  "function ownerOf(uint256 tokenId) view returns (address)",
]);

type Wallet = { provider: Eip1193Provider; address: string };
type Args = { name: string; priceMana?: string; signal?: AbortSignal };

export function nameClaimWallet(getWallet: () => Wallet, timing: { pollMs?: number; timeoutMs?: number; storage?: Pick<Storage, "getItem" | "setItem" | "removeItem"> } = {}) {
  const pending = new Map<string, Hex>();
  let storage = timing.storage;
  try { storage ??= typeof window === "undefined" ? undefined : window.sessionStorage; } catch {}
  const storageKey = (key: string) => `dcl:name-claim:1:${key.toLowerCase()}`;
  function readPending(key: string): Hex | undefined {
    if (pending.has(key)) return pending.get(key);
    let hash: string | null = null;
    try { hash = storage?.getItem(storageKey(key)) ?? null; } catch {}
    return hash && /^0x[\da-f]{64}$/i.test(hash) ? hash as Hex : undefined;
  }
  function savePending(key: string, hash?: Hex) {
    if (hash) pending.set(key, hash); else pending.delete(key);
    try {
      if (hash) storage?.setItem(storageKey(key), hash);
      else storage?.removeItem(storageKey(key));
    } catch {}
  }
  async function connect(signal?: AbortSignal) {
    signal?.throwIfAborted();
    const { provider, address } = getWallet();
    if (!/^0x[\da-f]{40}$/i.test(address)) throw new Error("Sign in with the wallet that will own this NAME.");
    const assertAccount = async () => {
      const accounts = await provider.request({ method: "eth_accounts" });
      if (!Array.isArray(accounts) || String(accounts[0]).toLowerCase() !== address.toLowerCase()) throw new Error("Your wallet account changed. Sign in with the selected wallet and retry.");
    };
    await assertAccount();
    if (BigInt(String(await provider.request({ method: "eth_chainId" }))) !== 1n) {
      await provider.request({ method: "wallet_switchEthereumChain", params: [{ chainId: "0x1" }] });
    }
    const assertWallet = async () => {
      signal?.throwIfAborted();
      await assertAccount();
      if (BigInt(String(await provider.request({ method: "eth_chainId" }))) !== 1n) throw new Error("Switch your wallet to Ethereum Mainnet and retry.");
    };
    await assertWallet();
    const client = createPublicClient({ transport: custom(provider, { retryCount: 0 }) });
    const [price, token, registrar] = await Promise.all([
      client.readContract({ address: NAME_CONTROLLER, abi: nameControllerAbi, functionName: "PRICE" }),
      client.readContract({ address: NAME_CONTROLLER, abi: nameControllerAbi, functionName: "acceptedToken" }),
      client.readContract({ address: NAME_CONTROLLER, abi: nameControllerAbi, functionName: "registrar" }),
    ]);
    if (token.toLowerCase() !== NAME_MANA || registrar.toLowerCase() !== NAME_REGISTRAR) throw new Error("The NAME controller configuration has changed. Registration is unavailable.");
    return { provider, address: address as Address, client, price, assertWallet };
  }
  function validate(name: string) {
    if (!NAME_REGEX.test(name)) throw new Error("Choose 2\u201315 letters or numbers for your NAME.");
  }
  async function transaction(wallet: Awaited<ReturnType<typeof connect>>, key: string, to: Address, data: Hex) {
    let hash = readPending(key);
    if (!hash) {
      await wallet.assertWallet();
      const tx = { from: wallet.address, to, data };
      await wallet.provider.request({ method: "eth_call", params: [tx, "latest"] });
      await wallet.assertWallet();
      const result = await wallet.provider.request({ method: "eth_sendTransaction", params: [tx] });
      if (typeof result !== "string" || !/^0x[\da-f]{64}$/i.test(result)) throw new Error("The wallet returned an invalid transaction hash.");
      hash = result as Hex;
      savePending(key, hash);
    }
    const deadline = Date.now() + (timing.timeoutMs ?? 300_000);
    for (;;) {
      await wallet.assertWallet();
      const receipt = await wallet.provider.request({ method: "eth_getTransactionReceipt", params: [hash] }) as { status?: string } | null;
      if (receipt) {
        if (receipt.status !== "0x1") {
          savePending(key);
          throw new Error(`Transaction ${hash} reverted. Check your wallet and retry.`);
        }
        return hash;
      }
      if (Date.now() >= deadline) throw new Error(`Transaction ${hash} is still pending. Retry to check the same transaction.`);
      await new Promise(resolve => setTimeout(resolve, timing.pollMs ?? 3000));
    }
  }
  async function available(wallet: Awaited<ReturnType<typeof connect>>, name: string) {
    return wallet.client.readContract({ address: NAME_REGISTRAR, abi: nameRegistrarAbi, functionName: "available", args: [name.toLowerCase()] });
  }
  function checkPrice(price: bigint, quoted?: string) {
    if (!quoted || parseUnits(quoted, 18) !== price) throw new Error("The NAME price changed. Go back and check availability again.");
  }
  return {
    async check({ name, signal }: Args) {
      validate(name);
      const wallet = await connect(signal);
      const pendingRegistration = !!readPending(`mint:${wallet.address}:${name.toLowerCase()}`);
      return { available: pendingRegistration || await available(wallet, name), priceMana: formatUnits(wallet.price, 18), pendingRegistration };
    },
    async approve({ name, priceMana, signal }: Args) {
      validate(name);
      const wallet = await connect(signal);
      checkPrice(wallet.price, priceMana);
      if (!await available(wallet, name)) throw new Error("This NAME has already been registered. Go back and choose another.");
      const args = [wallet.address, NAME_CONTROLLER] as const;
      const allowance = () => wallet.client.readContract({ address: NAME_MANA, abi: erc20Abi, functionName: "allowance", args });
      const key = `approve:${wallet.address}:${wallet.price}`;
      if (await allowance() >= wallet.price) { savePending(key); return; }
      const balance = await wallet.client.readContract({ address: NAME_MANA, abi: erc20Abi, functionName: "balanceOf", args: [wallet.address] });
      if (balance < wallet.price) throw new Error(`You need ${priceMana} MANA on Ethereum to register this NAME.`);
      await transaction(wallet, key, NAME_MANA,
        encodeFunctionData({ abi: erc20Abi, functionName: "approve", args: [NAME_CONTROLLER, wallet.price] }));
      if (await allowance() < wallet.price) throw new Error("The approval confirmed but the MANA allowance is still insufficient.");
      savePending(key);
    },
    async mint({ name, priceMana, signal }: Args) {
      validate(name);
      const wallet = await connect(signal);
      checkPrice(wallet.price, priceMana);
      const key = `mint:${wallet.address}:${name.toLowerCase()}`;
      if (!readPending(key)) {
        if (!await available(wallet, name)) throw new Error("This NAME has already been registered. Go back and choose another.");
        const allowance = await wallet.client.readContract({ address: NAME_MANA, abi: erc20Abi, functionName: "allowance", args: [wallet.address, NAME_CONTROLLER] });
        if (allowance < wallet.price) throw new Error("Approve MANA before registering this NAME.");
      }
      const txHash = await transaction(wallet, key, NAME_CONTROLLER,
        encodeFunctionData({ abi: nameControllerAbi, functionName: "register", args: [name, wallet.address] }));
      const token = BigInt(keccak256(stringToHex(name.toLowerCase())));
      const owner = await wallet.client.readContract({ address: NAME_REGISTRAR, abi: nameRegistrarAbi, functionName: "ownerOf", args: [token] });
      if (owner.toLowerCase() !== wallet.address.toLowerCase()) throw new Error("The transaction confirmed, but NAME ownership could not be verified. Retry to check it again.");
      savePending(key);
      return { txHash, tokenId: token.toString(), simulated: false };
    },
  };
}
