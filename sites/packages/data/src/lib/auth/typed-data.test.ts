import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { signTypedData } from "./typed-data";
import { setThirdwebSession } from "./thirdweb";

const CLIENT_ID = "test-client-id";

function fakeLocalStorage() {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
}

const salt = "0x" + (137).toString(16).padStart(64, "0");
const META_TX = {
  domain: {
    name: "Decentraland Marketplace",
    version: "2",
    verifyingContract: "0x480a0f4e360e8964e68858dd231c2922f1df45ef",
    salt,
  },
  types: {
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "verifyingContract", type: "address" },
      { name: "salt", type: "bytes32" },
    ],
    MetaTransaction: [
      { name: "nonce", type: "uint256" },
      { name: "from", type: "address" },
      { name: "functionSignature", type: "bytes" },
    ],
  },
  primaryType: "MetaTransaction",
  message: { nonce: "0", from: "0x1", functionSignature: "0x00" },
};

beforeEach(() => {
  process.env.THIRDWEB_CLIENT_ID = CLIENT_ID;
  vi.stubGlobal("window", { localStorage: fakeLocalStorage() });
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.THIRDWEB_CLIENT_ID;
});

describe("unified signTypedData router", () => {
  it("routes to the enclave for an in-app session (strips EIP712Domain, decodes chainId from salt)", async () => {
    const from = "0x1111111111111111111111111111111111111111";
    setThirdwebSession({ token: "jwt-xyz", address: from });
    const f = vi.fn(
      async (_url: string, _init: RequestInit) =>
        new Response(JSON.stringify({ signature: "0xenc" }), { status: 200 }),
    );
    vi.stubGlobal("fetch", f);

    const sig = await signTypedData(META_TX, from);
    expect(sig).toBe("0xenc");

    const call = f.mock.calls[0];
    expect(call).toBeDefined();
    const [url, init] = call as [string, RequestInit];
    expect(url).toBe("/internal/thirdweb-sign");
    const body = JSON.parse(init.body as string);
    expect(body.kind).toBe("typedData");
    expect(body.token).toBe("jwt-xyz");
    expect(body.from).toBe(from);
    expect(body.chainId).toBe(137);
    expect(body.typedData.types.EIP712Domain).toBeUndefined();
    expect(body.typedData.types.MetaTransaction).toBeDefined();
  });

  it("falls back to the injected wallet when there is no in-app session", async () => {
    const from = "0x2222222222222222222222222222222222222222";
    await expect(signTypedData(META_TX, from)).rejects.toThrow(/wallet/i);
  });
});
