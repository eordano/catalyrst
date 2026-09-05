import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { hashTypedData } from "viem";
import { privateKeyToAccount } from "viem/accounts";

import { setThirdwebSession } from "../../auth/thirdweb";
import {
  PURCHASE_INTENT_CHAIN_ID,
  PURCHASE_INTENT_CURRENCY,
  PURCHASE_INTENT_TTL_MS,
  buildPurchaseIntent,
  canonicalItems,
  intentLineFromCart,
  purchaseIntentTypedData,
  signPurchaseIntent,
  type PurchaseIntent,
} from "./purchase-intent";

const VECTOR_PK =
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const VECTOR_SIGNER = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const VECTOR_ITEMS =
  '[["0x59a90bad9570ecd08895f132daf7b79696337f61","12",2],["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","3",1]]';
const VECTOR_DIGEST =
  "0xcc577fb0844b7f8a0163e4daf32481bed2beca29d87c1634aa43b42ed34bca1c";
const VECTOR_SIG =
  "0x29ababcea69bb9464958c8ccd3b34dce8c82c44c52e80ec0c02a49891344d8da6951071404ce24a3975a4111511adf8bd313d3e9ab5072e89299e2f351800ef71b";

const VECTOR_INTENT: PurchaseIntent = {
  buyer: VECTOR_SIGNER,
  items: VECTOR_ITEMS,
  totalCredits: "3",
  currency: "CREDITS",
  nonce: "idem-vector-0001",
  expiresAt: 1767225600,
};

function fakeLocalStorage(seed: Record<string, string> = {}) {
  const m = new Map<string, string>(Object.entries(seed));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.THIRDWEB_CLIENT_ID;
});

describe("canonicalItems", () => {
  it("sorts by (collection, itemId) and lowercases collections", () => {
    const out = canonicalItems([
      { collection: "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", itemId: "3", qty: 1 },
      { collection: "0x59a90bad9570ecd08895f132daf7b79696337f61", itemId: "12", qty: 2 },
    ]);
    expect(out).toBe(VECTOR_ITEMS);
  });

  it("derives the collection from the urn when the cart line lacks one", () => {
    const line = intentLineFromCart({
      itemId: "7",
      urn: "urn:decentraland:matic:collections-v2:0xABC0000000000000000000000000000000000abc:7",
      qty: 1,
    });
    expect(line.collection).toBe("0xabc0000000000000000000000000000000000abc");
  });
});

describe("buildPurchaseIntent", () => {
  it("binds buyer (lowercased), canonical items, nonce and a bounded expiry", () => {
    const now = 1767224000_000;
    const intent = buildPurchaseIntent({
      buyer: "0xF39FD6E51AAD88F6F4CE6AB8827279CFFFB92266",
      lines: [
        { collection: "0x59a90bad9570ecd08895f132daf7b79696337f61", itemId: "12", qty: 2 },
        { collection: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", itemId: "3", qty: 1 },
      ],
      totalCredits: "3",
      nonce: "idem-1",
      now,
    });
    expect(intent.buyer).toBe(VECTOR_SIGNER);
    expect(intent.items).toBe(VECTOR_ITEMS);
    expect(intent.currency).toBe(PURCHASE_INTENT_CURRENCY);
    expect(intent.nonce).toBe("idem-1");
    expect(intent.expiresAt).toBe(Math.floor((now + PURCHASE_INTENT_TTL_MS) / 1000));
  });
});

describe("EIP-712 vector (TS signs, Rust recovers)", () => {
  it("hashes the typed data to the digest the Rust verifier recomputes", () => {
    const typed = purchaseIntentTypedData(VECTOR_INTENT);
    const digest = hashTypedData(
      typed as unknown as Parameters<typeof hashTypedData>[0],
    );
    expect(digest).toBe(VECTOR_DIGEST);
  });

  it("signs deterministically to the signature the Rust verifier recovers", async () => {
    const account = privateKeyToAccount(VECTOR_PK);
    expect(account.address.toLowerCase()).toBe(VECTOR_SIGNER);
    const typed = purchaseIntentTypedData(VECTOR_INTENT);
    const types = { ...typed.types };
    delete types.EIP712Domain;
    const sig = await account.signTypedData({
      domain: typed.domain,
      types,
      primaryType: typed.primaryType,
      message: typed.message,
    } as unknown as Parameters<typeof account.signTypedData>[0]);
    expect(sig).toBe(VECTOR_SIG);
  });
});

describe("signPurchaseIntent routing", () => {
  beforeEach(() => {
    process.env.THIRDWEB_CLIENT_ID = "test-client-id";
  });

  it("signs through the server-side sign proxy with the exact typed data shown in the sheet", async () => {
    vi.stubGlobal("window", { localStorage: fakeLocalStorage() });
    setThirdwebSession({ token: "jwt-abc", address: VECTOR_SIGNER });
    const f = vi.fn(
      async () =>
        new Response(JSON.stringify({ signature: "0xenclave-sig" }), {
          status: 200,
        }),
    );
    vi.stubGlobal("fetch", f);

    const signed = await signPurchaseIntent(VECTOR_INTENT);
    expect(signed.signature).toBe("0xenclave-sig");
    expect(signed.intent).toEqual(VECTOR_INTENT);

    const [url, init] = f.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/internal/thirdweb-sign");
    const body = JSON.parse(init.body as string);
    expect(body.kind).toBe("typedData");
    expect(body.token).toBe("jwt-abc");
    expect(body.from).toBe(VECTOR_SIGNER);
    expect(body.chainId).toBe(PURCHASE_INTENT_CHAIN_ID);
    expect(body.typedData.primaryType).toBe("PurchaseIntent");
    expect(body.typedData.types.EIP712Domain).toBeUndefined();
    expect(body.typedData.message).toEqual(VECTOR_INTENT);
  });

  it("falls back to the dev burner signer on dev hosts and produces the vector signature", async () => {
    vi.stubGlobal("window", {
      location: { hostname: "localhost" },
      localStorage: fakeLocalStorage({ "dcl:auth:dev-signer-pk:v1": VECTOR_PK }),
    });

    const signed = await signPurchaseIntent(VECTOR_INTENT);
    expect(signed.signature).toBe(VECTOR_SIG);
  });

  it("throws (no fake signature) when no signer is available", async () => {
    vi.stubGlobal("window", {
      location: { hostname: "catalyst.example.com" },
      localStorage: fakeLocalStorage(),
    });
    await expect(signPurchaseIntent(VECTOR_INTENT)).rejects.toThrow(/wallet/i);
  });
});
