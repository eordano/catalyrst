import { beforeEach, describe, expect, it, vi } from "vitest";
import { encodeFunctionData, pad, toHex } from "viem";

vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, postJSON: vi.fn() };
});

import { postJSON } from "../client";
import {
  buildCreateOrder,
  buildSellOrder,
  failClosedCreate,
  type CreateOrderDeps,
  type OwnedAsset,
  type SellOrder,
} from "./sell";
import { manaToWei } from "./money";
import { APPROVAL_FOR_ALL_ABI } from "./sell-chain";
import {
  buildListingTrade,
  offchainMarketplaceFor,
  tradeTypedData,
  type TradeTypedData,
} from "./trade";
import type { Eip1193Provider } from "../../auth/wallet";
import type { AuthIdentity } from "../../auth/types";

const mPostJSON = vi.mocked(postJSON);

const SELLER = "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295";
const NFT = "0x2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f";
const MARKET = offchainMarketplaceFor(137);
const NOW = 1_800_000_000_000;
const SIGNATURE = `0x${"ab".repeat(65)}`;
const APPROVAL_TX = `0x${"11".repeat(32)}`;

const IDENTITY: AuthIdentity = {
  signer: SELLER,
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

const ASSET: OwnedAsset = {
  id: `${NFT}-101`,
  contractAddress: NFT,
  tokenId: "101",
  itemId: "0",
  issuedId: "101",
  activeOrderId: null,
  owner: SELLER,
  name: "Test Wearable",
  category: "wearable",
  rarity: "epic",
  network: "MATIC",
  chainId: 137,
  image: null,
  urn: null,
  bodyShape: "Unisex",
  isOnSale: false,
};

function order(overrides: Partial<SellOrder> = {}): SellOrder {
  return {
    ...buildSellOrder({
      asset: ASSET,
      priceMana: 1500,
      expiresAt: NOW + 30 * 86_400_000,
    }),
    ...overrides,
  };
}

const BOOL_TRUE = pad("0x01", { size: 32 });
const BOOL_FALSE = pad("0x00", { size: 32 });

type Call = { method: string; params?: unknown[] };

type ChainState = {
  chainId: number;
  approved: boolean;
  approveAfterSend: boolean;
  receipt: { status?: string; blockNumber?: string } | null;
  receiptAfter: number;
  sendFails?: string;
};

function fakeProvider(state: Partial<ChainState> = {}) {
  const chain: ChainState = {
    chainId: 137,
    approved: false,
    approveAfterSend: true,
    receipt: { status: "0x1", blockNumber: "0x2a" },
    receiptAfter: 0,
    ...state,
  };
  const calls: Call[] = [];
  let receiptPolls = 0;

  const isApprovedForAllSelector = encodeFunctionData({
    abi: APPROVAL_FOR_ALL_ABI,
    functionName: "isApprovedForAll",
    args: [SELLER as `0x${string}`, MARKET.address],
  }).slice(0, 10);

  const provider: Eip1193Provider = {
    async request({ method, params }) {
      calls.push({ method, params });
      if (method === "eth_chainId") return toHex(chain.chainId);
      if (method === "eth_call") {
        const { to, data } = (params?.[0] ?? {}) as { to: string; data: string };
        if (to.toLowerCase() === NFT && data.startsWith(isApprovedForAllSelector)) {
          return chain.approved ? BOOL_TRUE : BOOL_FALSE;
        }
        if (to.toLowerCase() === MARKET.address) {
          return data.length === 10
            ? pad("0x03", { size: 32 })
            : pad("0x07", { size: 32 });
        }
        throw new Error(`unexpected eth_call to ${to}`);
      }
      if (method === "eth_sendTransaction") {
        if (chain.sendFails) throw new Error(chain.sendFails);
        if (chain.approveAfterSend) chain.approved = true;
        return APPROVAL_TX;
      }
      if (method === "eth_getTransactionReceipt") {
        receiptPolls += 1;
        return receiptPolls > chain.receiptAfter ? chain.receipt : null;
      }
      throw new Error(`unexpected rpc ${method}`);
    },
  };

  return { provider, calls, chain };
}

function deps(overrides: Partial<CreateOrderDeps> = {}): CreateOrderDeps {
  const { provider } = fakeProvider();
  return {
    identity: IDENTITY,
    provider,
    address: SELLER,
    now: () => NOW,
    approval: { pollIntervalMs: 0, sleep: async () => {} },
    signTrade: async () => SIGNATURE,
    ...overrides,
  };
}

beforeEach(() => {
  mPostJSON.mockReset();
  mPostJSON.mockResolvedValue({ data: { id: "trade-abc" } });
});

describe("failClosedCreate is what the route still ships", () => {
  it("throws instead of fabricating a listing", async () => {
    await expect(failClosedCreate({ order: order() })).rejects.toThrow(
      "listing unavailable: order relayer not configured",
    );
    expect(mPostJSON).not.toHaveBeenCalled();
  });
});

describe("buildCreateOrder \u{2014} refusals before anything is signed", () => {
  it("requires a signed-in identity", async () => {
    await expect(
      buildCreateOrder(deps({ identity: null }))({ order: order() }),
    ).rejects.toThrow("listing unavailable: sign in first");
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("requires a connected wallet", async () => {
    await expect(
      buildCreateOrder(deps({ provider: null }))({ order: order() }),
    ).rejects.toThrow("listing unavailable: connect a browser wallet first");
  });

  it("refuses when the connected wallet is not the signed-in account", async () => {
    await expect(
      buildCreateOrder(
        deps({ address: "0x1111111111111111111111111111111111111111" }),
      )({ order: order() }),
    ).rejects.toThrow("not the signed-in account");
  });

  it("refuses when the connected wallet does not own the item", async () => {
    await expect(
      buildCreateOrder(deps())({
        order: order({ owner: "0x2222222222222222222222222222222222222222" }),
      }),
    ).rejects.toThrow("does not own this item");
  });

  it("refuses when the wallet is on another chain", async () => {
    const { provider } = fakeProvider({ chainId: 1 });
    await expect(
      buildCreateOrder(deps({ provider }))({ order: order() }),
    ).rejects.toThrow("switch the wallet to Polygon (chain 137); it is on chain 1");
  });

  it("refuses a chain with no off-chain marketplace deployment", async () => {
    await expect(
      buildCreateOrder(deps())({ order: order({ chainId: 42161 }) }),
    ).rejects.toThrow("no off-chain marketplace is deployed on chain 42161");
  });

  it("refuses an expiration that is already past", async () => {
    await expect(
      buildCreateOrder(deps())({ order: order({ expiresAt: NOW - 1 }) }),
    ).rejects.toThrow("the expiration date is in the past");
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("refuses a zero price", async () => {
    await expect(
      buildCreateOrder(deps())({ order: order({ price: "0" }) }),
    ).rejects.toThrow("the price must be above zero");
  });

  it("refuses an estate listing that carries no fingerprint", () => {
    expect(() =>
      buildListingTrade({
        signer: SELLER,
        chainId: 1,
        contractAddress: "0x959e104e1a4db6317fa58f8295f586e1a978c297",
        tokenId: "9",
        priceWei: manaToWei(1500),
        expiresAt: NOW + 1000,
        effectiveAt: NOW,
        contractSignatureIndex: 0,
        signerSignatureIndex: 0,
      }),
    ).toThrow("an estate listing needs the estate fingerprint");
  });
});

describe("buildCreateOrder \u{2014} approval driven by real receipts", () => {
  it("skips the approval transaction when the marketplace is already approved", async () => {
    const { provider, calls } = fakeProvider({ approved: true });
    const result = await buildCreateOrder(deps({ provider }))({ order: order() });

    expect(calls.some((c) => c.method === "eth_sendTransaction")).toBe(false);
    expect(result.approvalTxHash).toBeNull();
    expect(result.order.id).toBe("trade-abc");
  });

  it("declares the signer and intent catalyrst-market authorizes POST /v1/trades on", async () => {
    const { provider } = fakeProvider({ approved: true });
    await buildCreateOrder(deps({ provider }))({ order: order() });

    const lastCall = mPostJSON.mock.calls.at(-1);
    expect(lastCall?.[0]).toBe("/market/v1/trades");
    expect((lastCall?.[2] as { metadata?: unknown })?.metadata).toEqual({
      signer: "dcl:marketplace",
      intent: "dcl:create-trade",
    });
  });

  it("sends setApprovalForAll for the off-chain marketplace and waits for the receipt", async () => {
    const { provider, calls } = fakeProvider({ receiptAfter: 2 });
    const result = await buildCreateOrder(deps({ provider }))({ order: order() });

    const sent = calls.find((c) => c.method === "eth_sendTransaction");
    const tx = sent?.params?.[0] as { from: string; to: string; data: string };
    expect(tx.to).toBe(NFT);
    expect(tx.from).toBe(SELLER);
    expect(tx.data).toBe(
      encodeFunctionData({
        abi: APPROVAL_FOR_ALL_ABI,
        functionName: "setApprovalForAll",
        args: [MARKET.address, true],
      }),
    );
    expect(
      calls.filter((c) => c.method === "eth_getTransactionReceipt").length,
    ).toBe(3);
    expect(result.approvalTxHash).toBe(APPROVAL_TX);
  });

  it("aborts on a reverted approval without signing or posting anything", async () => {
    const { provider, calls } = fakeProvider({
      receipt: { status: "0x0", blockNumber: "0x2a" },
    });
    const signTrade = vi.fn(async () => SIGNATURE);

    await expect(
      buildCreateOrder(deps({ provider, signTrade }))({ order: order() }),
    ).rejects.toThrow("did not succeed (status 0x0)");

    expect(signTrade).not.toHaveBeenCalled();
    expect(mPostJSON).not.toHaveBeenCalled();
    expect(calls.some((c) => c.method === "eth_call")).toBe(true);
  });

  it("gives up honestly when the approval never confirms", async () => {
    let clock = NOW;
    const { provider } = fakeProvider({ receipt: null });
    await expect(
      buildCreateOrder(
        deps({
          provider,
          approval: {
            pollIntervalMs: 0,
            timeoutMs: 10,
            now: () => (clock += 5),
            sleep: async () => {},
          },
        }),
      )({ order: order() }),
    ).rejects.toThrow("was not confirmed in time; nothing was signed");
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("refuses when the confirmed approval did not take effect", async () => {
    const { provider } = fakeProvider({ approveAfterSend: false });
    await expect(
      buildCreateOrder(deps({ provider }))({ order: order() }),
    ).rejects.toThrow("confirmed but the marketplace is still not approved");
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("propagates a wallet rejection of the approval", async () => {
    const { provider } = fakeProvider({ sendFails: "User rejected the request" });
    await expect(
      buildCreateOrder(deps({ provider }))({ order: order() }),
    ).rejects.toThrow("User rejected the request");
    expect(mPostJSON).not.toHaveBeenCalled();
  });
});

describe("buildCreateOrder \u{2014} the signed trade", () => {
  it("signs the EIP-712 Trade the seller is actually agreeing to", async () => {
    const seen: TradeTypedData[] = [];
    const { provider } = fakeProvider({ approved: true });
    await buildCreateOrder(
      deps({
        provider,
        signTrade: async (typedData) => {
          seen.push(typedData);
          return SIGNATURE;
        },
      }),
    )({ order: order() });

    expect(seen).toHaveLength(1);
    const typedData = seen[0];
    expect(typedData.primaryType).toBe("Trade");
    expect(typedData.domain).toEqual({
      name: "DecentralandMarketplacePolygon",
      version: "1.0.0",
      verifyingContract: MARKET.address,
      salt: pad(toHex(137), { size: 32 }),
    });
    expect(typedData.message.sent).toEqual([
      { assetType: 3, contractAddress: NFT, value: "101", extra: "0x" },
    ]);
    expect(typedData.message.received).toEqual([
      {
        assetType: 1,
        contractAddress: MARKET.manaAddress,
        value: manaToWei(1500),
        extra: "0x",
        beneficiary: SELLER,
      },
    ]);
    expect(typedData.message.checks.uses).toBe(1);
    expect(typedData.message.checks.expiration).toBe(
      String(Math.floor((NOW + 30 * 86_400_000) / 1000)),
    );
    expect(typedData.message.checks.effective).toBe(String(NOW / 1000));
    expect(typedData.message.checks.contractSignatureIndex).toBe(3);
    expect(typedData.message.checks.signerSignatureIndex).toBe(7);
  });

  it("rejects a signature the wallet could not have produced", async () => {
    const { provider } = fakeProvider({ approved: true });
    await expect(
      buildCreateOrder(deps({ provider, signTrade: async () => "0xshort" }))({
        order: order(),
      }),
    ).rejects.toThrow("the wallet returned an invalid trade signature");
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("posts the signed trade to the marketplace and returns its id", async () => {
    const { provider } = fakeProvider({ approved: true });
    const result = await buildCreateOrder(deps({ provider }))({ order: order() });

    expect(mPostJSON).toHaveBeenCalledTimes(1);
    const [path, body] = mPostJSON.mock.calls[0];
    expect(path).toBe("/market/v1/trades");
    expect(body).toMatchObject({
      signer: SELLER,
      network: "MATIC",
      chainId: 137,
      type: "public_nft_order",
      signature: SIGNATURE,
    });
    expect(result.order.id).toBe("trade-abc");
    expect(result.order.marketplaceAddress).toBe(MARKET.address);
    expect(result.order.price).toBe(manaToWei(1500));
  });

  it("fails closed when the marketplace answers without a trade id", async () => {
    mPostJSON.mockResolvedValue({ ok: true });
    const { provider } = fakeProvider({ approved: true });
    await expect(
      buildCreateOrder(deps({ provider }))({ order: order() }),
    ).rejects.toThrow("the marketplace did not return a trade id");
  });

  it("surfaces the marketplace error rather than inventing a listing", async () => {
    mPostJSON.mockRejectedValue(new Error("Catalyst request failed: 404"));
    const { provider } = fakeProvider({ approved: true });
    await expect(
      buildCreateOrder(deps({ provider }))({ order: order() }),
    ).rejects.toThrow("Catalyst request failed: 404");
  });
});

describe("the signed payload is real EIP-712", () => {
  it("recovers to the seller when signed by a local key", async () => {
    const { privateKeyToAccount } = await import("viem/accounts");
    const { recoverTypedDataAddress } = await import("viem");
    const account = privateKeyToAccount(`0x${"7".repeat(64)}`);

    const seen: TradeTypedData[] = [];
    const { provider } = fakeProvider({ approved: true });
    await buildCreateOrder(
      deps({
        provider,
        identity: { ...IDENTITY, signer: account.address },
        address: account.address,
        signTrade: async (typedData, address) => {
          seen.push(typedData);
          const { EIP712Domain: _domain, ...types } = typedData.types;
          return account.signTypedData({
            domain: typedData.domain,
            types,
            primaryType: typedData.primaryType,
            message: typedData.message,
          } as unknown as Parameters<typeof account.signTypedData>[0]);
        },
      }),
    )({ order: order({ owner: account.address }) });

    const typedData = seen[0];
    const { EIP712Domain: _domain, ...types } = typedData.types;
    const posted = mPostJSON.mock.calls[0][1] as { signature: string };
    const recovered = await recoverTypedDataAddress({
      domain: typedData.domain,
      types,
      primaryType: typedData.primaryType,
      message: typedData.message,
      signature: posted.signature as `0x${string}`,
    } as unknown as Parameters<typeof recoverTypedDataAddress>[0]);

    expect(recovered.toLowerCase()).toBe(account.address.toLowerCase());
  });
});

describe("tradeTypedData \u{2014} units and padding", () => {
  it("keeps bytes32 fields padded and price exact", () => {
    const typedData = tradeTypedData({
      signer: SELLER as `0x${string}`,
      network: "MATIC",
      chainId: 137,
      type: "public_nft_order",
      checks: {
        uses: 1,
        expiration: NOW + 1000,
        effective: NOW,
        salt: "0x01",
        contractSignatureIndex: 0,
        signerSignatureIndex: 0,
        allowedRoot: "0x",
        externalChecks: [],
      },
      sent: [
        {
          assetType: 3,
          contractAddress: NFT as `0x${string}`,
          tokenId: "101",
          extra: "",
        },
      ],
      received: [
        {
          assetType: 1,
          contractAddress: MARKET.manaAddress,
          amount: manaToWei("0.1"),
          extra: "",
          beneficiary: SELLER as `0x${string}`,
        },
      ],
    });

    expect(typedData.message.checks.salt).toHaveLength(66);
    expect(typedData.message.checks.allowedRoot).toBe(`0x${"0".repeat(64)}`);
    expect(typedData.message.received[0].value).toBe("100000000000000000");
  });

  it("manaToWei carries the decimal the seller sees, with no float drift of its own", () => {
    expect(manaToWei("0.1")).toBe("100000000000000000");
    expect(manaToWei(0.1 + 0.2)).toBe(manaToWei("0.30000000000000004"));
    expect(manaToWei(0.1 + 0.2)).toBe("300000000000000040");
    expect(manaToWei("0.000000000000000001")).toBe("1");
    expect(manaToWei(1500)).toBe("1500000000000000000000");
    expect(manaToWei(0)).toBe("0");
    expect(manaToWei(-1)).toBe("0");
    expect(manaToWei(Number.NaN)).toBe("0");
  });
});
