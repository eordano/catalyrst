import { z } from "zod";
import { createPublicClient, custom, domainSeparator, numberToHex, parseAbi, recoverTypedDataAddress, type Hex } from "viem";
import type { LinkedPublicationCheque as StoredCheque } from "@ui/generated/catalyst/builder/LinkedPublicationCheque";
import type { Eip1193Provider } from "../../auth/wallet";
import type { DraftOptions } from "./drafts";
import { linkedProviderId, linkedProviderSlots, listLinkedProviders } from "./linked-providers";

export const LINKED_REGISTRY = "0x1C436C1EFb4608dFfDC8bace99d2B03c314f3348";
const Intent = z.object({
  thirdPartyId: linkedProviderId,
  qty: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
  salt: z.string().regex(/^0x[\da-f]{64}$/i).transform(value => value.toLowerCase() as Hex),
});
export type LinkedPublicationIntent = z.input<typeof Intent>;
export type LinkedPublicationCheque = StoredCheque & { salt: Hex; signature: Hex };
const domain = {
  name: "Decentraland Third Party Registry", version: "1",
  verifyingContract: LINKED_REGISTRY, salt: numberToHex(137, { size: 32 }),
} as const;
const types = {
  EIP712Domain: [
    { name: "name", type: "string" }, { name: "version", type: "string" },
    { name: "verifyingContract", type: "address" }, { name: "salt", type: "bytes32" },
  ],
  ConsumeSlots: [
    { name: "thirdPartyId", type: "string" }, { name: "qty", type: "uint256" }, { name: "salt", type: "bytes32" },
  ],
} as const;
const registryAbi = parseAbi(["function domainSeparator() view returns (bytes32)"]);

type Session = { address: string; provider: Eip1193Provider; fetch: DraftOptions["fetch"] };

export function linkedPublishWallet(getSession: () => Session) {
  return async (input: LinkedPublicationIntent, signal?: AbortSignal): Promise<LinkedPublicationCheque> => {
    const intent = Intent.parse(input);
    const session = getSession();
    if (!/^0x[\da-f]{40}$/i.test(session.address)) throw new Error("Sign in with the collection owner before publishing.");
    async function assertWallet() {
      signal?.throwIfAborted();
      if (getSession().address.toLowerCase() !== session.address.toLowerCase()) throw new Error("Your account changed. Sign in with the collection owner and retry.");
      const accounts = await session.provider.request({ method: "eth_accounts" });
      if (!Array.isArray(accounts) || String(accounts[0]).toLowerCase() !== session.address.toLowerCase()) throw new Error("Select the collection owner's wallet and retry.");
      if (BigInt(String(await session.provider.request({ method: "eth_chainId" }))) !== 137n) throw new Error("Switch your wallet to Polygon and retry.");
      signal?.throwIfAborted();
      if (getSession().address.toLowerCase() !== session.address.toLowerCase()) throw new Error("Your account changed. Sign in with the collection owner and retry.");
    }
    signal?.throwIfAborted();
    if (BigInt(String(await session.provider.request({ method: "eth_chainId" }))) !== 137n) {
      await session.provider.request({ method: "wallet_switchEthereumChain", params: [{ chainId: "0x89" }] });
    }
    await assertWallet();
    const opts = { fetch: session.fetch, signal };
    const providers = await listLinkedProviders(session.address, opts);
    if (!providers.some(provider => provider.id === intent.thirdPartyId)) throw new Error("You no longer manage this linked provider. Ask its owner to restore your access.");
    const slots = await linkedProviderSlots(intent.thirdPartyId, opts);
    if (slots < intent.qty) throw new Error(`This provider has ${slots} available item slots; this publication needs ${intent.qty}.`);
    const client = createPublicClient({ transport: custom(session.provider, { retryCount: 0 }) });
    const actualDomain = await client.readContract({ address: LINKED_REGISTRY, abi: registryAbi, functionName: "domainSeparator" });
    if (actualDomain.toLowerCase() !== domainSeparator({ domain })) throw new Error("The Polygon registry's signing domain does not match. Publication has been stopped.");
    await assertWallet();
    const data = { domain, types, primaryType: "ConsumeSlots" as const, message: intent };
    const result = await session.provider.request({ method: "eth_signTypedData_v4", params: [session.address, JSON.stringify(data)] });
    const signature = z.string().regex(/^0x[\da-f]{130}$/i).parse(result) as Hex;
    if ((await recoverTypedDataAddress({ ...data, message: { ...intent, qty: BigInt(intent.qty) }, signature })).toLowerCase() !== session.address.toLowerCase()) throw new Error("The authorization was signed by a different wallet. Select the collection owner and retry.");
    await assertWallet();
    return { qty: intent.qty, salt: intent.salt, signature };
  };
}
