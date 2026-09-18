import { recoverMessageAddress } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  EnclaveSignatureSchema,
  ThirdwebAuthResultSchema,
  WalletsMeSchema,
} from "@ui/data/auth/thirdwebSchema";

import {
  completeEmailLogin,
  getWalletForToken,
  initiateEmailLogin,
  signMessageEnclave,
  signTypedDataEnclave,
} from "./api";
import { makeInAppSigner } from "./signer";
import { createIdentityWith } from "../identity";

const CLIENT_ID = "test-client-id";

type FetchMock = ReturnType<typeof vi.fn>;

function stubFetch(
  responder: (url: string, init: RequestInit) => Promise<Response>,
): FetchMock {
  const fn = vi.fn(responder);
  vi.stubGlobal("fetch", fn);
  return fn;
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status });
}

function lastCall(fn: FetchMock): { url: string; init: RequestInit } {
  const call = fn.mock.calls[fn.mock.calls.length - 1];
  expect(call).toBeDefined();
  const [url, init] = call as [string, RequestInit];
  return { url, init };
}

function headerOf(init: RequestInit, name: string): string | undefined {
  return (init.headers as Record<string, string>)[name];
}

const AUTH_RESULT = {
  isNewUser: false,
  token: "jwt-123",
  userId: "u1",
  walletAddress: "0xAbC0000000000000000000000000000000000001",
  type: "email",
};

beforeEach(() => {
  process.env.THIRDWEB_CLIENT_ID = CLIENT_ID;
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.THIRDWEB_CLIENT_ID;
});

describe("vendored thirdweb client request contract", () => {
  it("initiateEmailLogin POSTs {method,email} with x-client-id and completeEmailLogin returns {token,walletAddress}", async () => {
    const f = stubFetch(async (url) =>
      url.endsWith("/initiate") ? new Response("", { status: 200 }) : json({ ...AUTH_RESULT, isNewUser: true }),
    );
    await initiateEmailLogin("a@b.com");
    const { url, init } = lastCall(f);
    expect(url).toBe("https://api.thirdweb.com/v1/auth/initiate");
    expect(init.method).toBe("POST");
    expect(headerOf(init, "x-client-id")).toBe(CLIENT_ID);
    expect(headerOf(init, "authorization")).toBeUndefined();
    expect(JSON.parse(init.body as string)).toEqual({ method: "email", email: "a@b.com" });

    const res = await completeEmailLogin("a@b.com", "654321");
    expect(res.token).toBe("jwt-123");
    expect(res.walletAddress).toBe("0xAbC0000000000000000000000000000000000001");
  });

  it("enclave signing sends bearer + client id with {from,chainId,...payload} and returns the signature", async () => {
    const f = stubFetch(async (url) =>
      json({ result: { signature: url.endsWith("sign-message") ? "0xdead" : "0xbeef" } }),
    );
    expect(await signMessageEnclave("jwt-123", "0xabc", "hello", 1)).toBe("0xdead");
    const msg = lastCall(f);
    expect(msg.url).toBe("https://api.thirdweb.com/v1/wallets/sign-message");
    expect(headerOf(msg.init, "authorization")).toBe("Bearer jwt-123");
    expect(headerOf(msg.init, "x-client-id")).toBe(CLIENT_ID);
    expect(JSON.parse(msg.init.body as string)).toEqual({ from: "0xabc", chainId: 1, message: "hello" });

    const typed = {
      domain: { name: "Market", chainId: "137" },
      types: { Order: [{ name: "id", type: "uint256" }] },
      primaryType: "Order",
      message: { id: "7" },
    };
    expect(await signTypedDataEnclave("jwt-123", "0xabc", typed, 137)).toBe("0xbeef");
    const td = lastCall(f);
    expect(td.url).toBe("https://api.thirdweb.com/v1/wallets/sign-typed-data");
    expect(headerOf(td.init, "authorization")).toBe("Bearer jwt-123");
    expect(JSON.parse(td.init.body as string)).toEqual({ from: "0xabc", chainId: 137, ...typed });
  });

  it("throws without a configured client id and surfaces the thirdweb error message on failure", async () => {
    delete process.env.THIRDWEB_CLIENT_ID;
    stubFetch(async () => new Response("", { status: 200 }));
    await expect(initiateEmailLogin("a@b.com")).rejects.toThrow(/client id/i);
    process.env.THIRDWEB_CLIENT_ID = CLIENT_ID;
    stubFetch(async () => json({ message: "The API key was not found.", correlationId: "abc" }, 401));
    await expect(completeEmailLogin("a@b.com", "1")).rejects.toThrow("The API key was not found.");
  });
});

describe("upstream drift at the thirdweb boundaries", () => {
  it("the schemas reject every known drift shape and still accept added fields", () => {
    const rejected: [string, boolean][] = [
      [
        "auth-complete token wrapper",
        ThirdwebAuthResultSchema.safeParse({ ...AUTH_RESULT, token: { jwt: "jwt-123" } }).success,
      ],
      ["enclave-sign null signature", EnclaveSignatureSchema.safeParse({ result: { signature: null } }).success],
      ["enclave-sign flattened envelope", EnclaveSignatureSchema.safeParse({ signature: "0xdead" }).success],
      ["wallets-me address object", WalletsMeSchema.safeParse({ result: { address: { value: "0xabc" } } }).success],
    ];
    expect(rejected.filter(([, ok]) => ok).map(([label]) => label)).toEqual([]);
    expect(ThirdwebAuthResultSchema.safeParse({ ...AUTH_RESULT, profiles: [] }).success).toBe(true);
  });

  it("the client reports the drifted boundary instead of signing with null or blaming the session, and still answers null on a network failure", async () => {
    stubFetch(async () => json({ result: { signature: null } }));
    await expect(signMessageEnclave("jwt-123", "0xabc", "hello", 1)).rejects.toThrow(
      /external-http\/thirdweb\/enclave-sign/,
    );
    stubFetch(async () => json({ result: { address: { value: "0xabc" } } }));
    await expect(getWalletForToken("jwt-123")).rejects.toThrow(/external-http\/thirdweb\/wallets-me/);
    stubFetch(async () => {
      throw new Error("network down");
    });
    await expect(getWalletForToken("jwt-123")).resolves.toBeNull();
  });

  it("a 503 from the sign proxy stays a ThirdwebError", async () => {
    stubFetch(async () => json({ error: "THIRDWEB_SECRET_KEY unset" }, 503));
    const signer = makeInAppSigner({ token: "t", walletAddress: "0xabc" });
    let err: unknown;
    try {
      await signer.personalSign("m");
    } catch (e) {
      err = e;
    }
    expect((err as Error).name).toBe("ThirdwebError");
    expect(String((err as Error).message)).not.toMatch(/validation failed/);
  });
});

describe("ADR-44 bridge: enclave login produces a catalyrst-valid chain", () => {
  it("ECDSA_EPHEMERAL signature recovers to the enclave wallet address", async () => {
    const walletAccount = privateKeyToAccount(generatePrivateKey());
    const walletAddress = walletAccount.address;
    stubFetch(async (_url, init) => {
      const body = JSON.parse(init.body as string) as { message: string };
      const signature = await walletAccount.signMessage({ message: body.message });
      return json({ signature });
    });

    const signer = makeInAppSigner({ token: "jwt-123", walletAddress });
    const identity = await createIdentityWith(signer.address, signer.personalSign);

    expect(identity.authChain).toHaveLength(2);
    const [signerLink, ephLink] = identity.authChain;
    expect(signerLink?.type).toBe("SIGNER");
    expect(signerLink?.payload.toLowerCase()).toBe(walletAddress.toLowerCase());
    expect(ephLink?.type).toBe("ECDSA_EPHEMERAL");
    if (!ephLink) throw new Error("missing ephemeral link");
    const recovered = await recoverMessageAddress({
      message: ephLink.payload,
      signature: ephLink.signature as `0x${string}`,
    });
    expect(recovered.toLowerCase()).toBe(walletAddress.toLowerCase());
    expect(identity.signer).toBe(walletAddress.toLowerCase());
    expect(identity.ephemeral.address).not.toBe(walletAddress.toLowerCase());
  });
});
