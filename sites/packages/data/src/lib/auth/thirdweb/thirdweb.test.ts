import { recoverMessageAddress } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  completeEmailLogin,
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

describe("vendored thirdweb client — request contract", () => {
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

describe("ADR-44 bridge — enclave login produces a catalyrst-valid chain", () => {
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
