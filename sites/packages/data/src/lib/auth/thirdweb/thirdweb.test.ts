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

function lastCall(fn: FetchMock): { url: string; init: RequestInit } {
  const call = fn.mock.calls[0];
  expect(call).toBeDefined();
  const [url, init] = call as [string, RequestInit];
  return { url, init };
}

function headerOf(init: RequestInit, name: string): string | undefined {
  return (init.headers as Record<string, string>)[name];
}

beforeEach(() => {
  process.env.THIRDWEB_CLIENT_ID = CLIENT_ID;
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.THIRDWEB_CLIENT_ID;
});

describe("vendored thirdweb client \u{2014} request contract", () => {
  it("initiateEmailLogin POSTs {method,email} with x-client-id", async () => {
    const f = stubFetch(async () => new Response("", { status: 200 }));
    await initiateEmailLogin("a@b.com");
    const { url, init } = lastCall(f);
    expect(url).toBe("https://api.thirdweb.com/v1/auth/initiate");
    expect(init.method).toBe("POST");
    expect(headerOf(init, "x-client-id")).toBe(CLIENT_ID);
    expect(headerOf(init, "authorization")).toBeUndefined();
    expect(JSON.parse(init.body as string)).toEqual({
      method: "email",
      email: "a@b.com",
    });
  });

  it("completeEmailLogin returns {token,walletAddress}", async () => {
    stubFetch(
      async () =>
        new Response(
          JSON.stringify({
            isNewUser: true,
            token: "jwt-123",
            userId: "u1",
            walletAddress: "0xAbC0000000000000000000000000000000000001",
            type: "email",
          }),
          { status: 200 },
        ),
    );
    const res = await completeEmailLogin("a@b.com", "654321");
    expect(res.token).toBe("jwt-123");
    expect(res.walletAddress).toBe(
      "0xAbC0000000000000000000000000000000000001",
    );
  });

  it("signMessageEnclave sends bearer + {from,chainId,message}, returns signature", async () => {
    const f = stubFetch(
      async () =>
        new Response(JSON.stringify({ result: { signature: "0xdead" } }), {
          status: 200,
        }),
    );
    const sig = await signMessageEnclave("jwt-123", "0xabc", "hello", 1);
    const { url, init } = lastCall(f);
    expect(url).toBe("https://api.thirdweb.com/v1/wallets/sign-message");
    expect(headerOf(init, "authorization")).toBe("Bearer jwt-123");
    expect(headerOf(init, "x-client-id")).toBe(CLIENT_ID);
    expect(JSON.parse(init.body as string)).toEqual({
      from: "0xabc",
      chainId: 1,
      message: "hello",
    });
    expect(sig).toBe("0xdead");
  });

  it("signTypedDataEnclave forwards the EIP-712 payload with bearer auth", async () => {
    const f = stubFetch(
      async () =>
        new Response(JSON.stringify({ result: { signature: "0xbeef" } }), {
          status: 200,
        }),
    );
    const typed = {
      domain: { name: "Market", chainId: "137" },
      types: { Order: [{ name: "id", type: "uint256" }] },
      primaryType: "Order",
      message: { id: "7" },
    };
    const sig = await signTypedDataEnclave("jwt-123", "0xabc", typed, 137);
    const { url, init } = lastCall(f);
    expect(url).toBe("https://api.thirdweb.com/v1/wallets/sign-typed-data");
    expect(headerOf(init, "authorization")).toBe("Bearer jwt-123");
    const body = JSON.parse(init.body as string);
    expect(body).toEqual({ from: "0xabc", chainId: 137, ...typed });
    expect(sig).toBe("0xbeef");
  });

  it("throws when no client id is configured", async () => {
    delete process.env.THIRDWEB_CLIENT_ID;
    stubFetch(async () => new Response("", { status: 200 }));
    await expect(initiateEmailLogin("a@b.com")).rejects.toThrow(
      /client id/i,
    );
  });

  it("surfaces the thirdweb error message + status on failure", async () => {
    stubFetch(
      async () =>
        new Response(
          JSON.stringify({
            message: "The API key was not found.",
            correlationId: "abc",
          }),
          { status: 401 },
        ),
    );
    await expect(completeEmailLogin("a@b.com", "1")).rejects.toThrow(
      "The API key was not found.",
    );
  });
});

// Every case below is drift the shipped code accepted, and each asserts BOTH
// halves: the schema rejects it, and what shipped did not. Three guards shipped
// and none of them looked at a type -- `return parsed as T` in twFetch checked
// nothing, `out.result.signature` dereferenced whatever came back, and
// getWalletForToken wrapped the lot in one try/catch that turned any surprise
// into "no wallet".
describe("upstream drift at the thirdweb boundaries", () => {
  /** twFetch's `return parsed as T`: a cast, so every payload got through. */
  const oldTwFetchGuard = (_v: unknown) => true;

  /**
   * getWalletForToken as it shipped: the whole body inside one try/catch, so
   * an address of the wrong type threw on `.toLowerCase()` and came out as
   * null -- indistinguishable from a token that had genuinely expired, which
   * is what the callback route then told the user.
   */
  const oldWalletRead = (body: unknown): string | null => {
    try {
      const out = body as { result?: { address?: string }; address?: string };
      const addr = out.result?.address ?? out.address ?? null;
      return addr ? addr.toLowerCase() : null;
    } catch {
      return null;
    }
  };

  const authResult = {
    isNewUser: false,
    token: "jwt-123",
    userId: "u1",
    walletAddress: "0xAbC0000000000000000000000000000000000001",
    type: "email",
  };

  it("auth-complete: a token that became a wrapper object", () => {
    const drift = { ...authResult, token: { jwt: "jwt-123" } };
    expect(ThirdwebAuthResultSchema.safeParse(drift).success).toBe(false);
    expect(oldTwFetchGuard(drift)).toBe(true);
  });

  it("auth-complete: a field thirdweb added is still accepted", () => {
    expect(
      ThirdwebAuthResultSchema.safeParse({ ...authResult, profiles: [] }).success,
    ).toBe(true);
  });

  /** `out.result.signature`, typed `string` and never checked. */
  const oldEnclaveRead = (body: unknown): string =>
    (body as { result: { signature: string } }).result.signature;

  // A signing failure reported in-band. The old deref handed `null` back typed
  // as `string`, and it went into the auth chain as the signature.
  it("enclave-sign: a null signature inside a 200", () => {
    const drift = { result: { signature: null } };
    expect(EnclaveSignatureSchema.safeParse(drift).success).toBe(false);
    expect(oldEnclaveRead(drift)).toBeNull();
  });

  // This one the old code did notice, but only as a TypeError from a property
  // access, naming neither thirdweb nor the endpoint that changed.
  it("enclave-sign: the result envelope flattened away", () => {
    const drift = { signature: "0xdead" };
    expect(EnclaveSignatureSchema.safeParse(drift).success).toBe(false);
    expect(() => oldEnclaveRead(drift)).toThrow(TypeError);
  });

  it("wallets-me: an address that arrived as an object", () => {
    const drift = { result: { address: { value: "0xabc" } } };
    expect(WalletsMeSchema.safeParse(drift).success).toBe(false);
    // The old read did not reject it -- it answered "no wallet".
    expect(oldWalletRead(drift)).toBeNull();
  });

  it("signMessageEnclave reports the boundary rather than signing with null", async () => {
    stubFetch(
      async () =>
        new Response(JSON.stringify({ result: { signature: null } }), { status: 200 }),
    );
    await expect(signMessageEnclave("jwt-123", "0xabc", "hello", 1)).rejects.toThrow(
      /external-http\/thirdweb\/enclave-sign/,
    );
  });

  it("getWalletForToken reports the boundary instead of blaming the session", async () => {
    stubFetch(
      async () =>
        new Response(JSON.stringify({ result: { address: { value: "0xabc" } } }), {
          status: 200,
        }),
    );
    await expect(getWalletForToken("jwt-123")).rejects.toThrow(
      /external-http\/thirdweb\/wallets-me/,
    );
  });

  // The request catch must keep swallowing a transport failure: that is the
  // signed-out path, not drift.
  it("getWalletForToken still answers null when the request itself fails", async () => {
    stubFetch(async () => {
      throw new Error("network down");
    });
    await expect(getWalletForToken("jwt-123")).resolves.toBeNull();
  });

  // The order inside proxySign is load-bearing: validating the success shape
  // before the status test would answer a configuration failure with a
  // complaint about a missing signature.
  it("a 503 from the sign proxy stays a ThirdwebError", async () => {
    stubFetch(
      async () =>
        new Response(JSON.stringify({ error: "THIRDWEB_SECRET_KEY unset" }), {
          status: 503,
        }),
    );
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

describe("ADR-44 bridge \u{2014} enclave login produces a catalyrst-valid chain", () => {
  it("ECDSA_EPHEMERAL signature recovers to the enclave wallet address", async () => {
    const walletAccount = privateKeyToAccount(generatePrivateKey());
    const walletAddress = walletAccount.address;

    stubFetch(async (_url, init) => {
      const body = JSON.parse(init.body as string) as { message: string };
      const signature = await walletAccount.signMessage({
        message: body.message,
      });
      return new Response(JSON.stringify({ signature }), { status: 200 });
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
