import { afterEach, describe, expect, test, vi } from "vitest";

import {
  ThirdwebError,
  makeInAppSigner,
  parseAuthResult,
  thirdwebClientId,
  thirdwebSignProxyUrl,
} from "./thirdweb";

afterEach(() => {
  vi.unstubAllGlobals();
  delete (window as { __DCL_PUBLIC__?: unknown }).__DCL_PUBLIC__;
});

describe("config resolution", () => {
  test("thirdwebClientId prefers the injected SSR global", () => {
    window.__DCL_PUBLIC__ = { thirdwebClientId: "cid-from-ssr" };
    expect(thirdwebClientId()).toBe("cid-from-ssr");
  });

  test("sign proxy defaults to the same-origin sites route", () => {
    expect(thirdwebSignProxyUrl()).toBe("/internal/thirdweb-sign");
    window.__DCL_PUBLIC__ = { thirdwebSignProxy: "https://catalyst.example.com/internal/thirdweb-sign" };
    expect(thirdwebSignProxyUrl()).toBe(
      "https://catalyst.example.com/internal/thirdweb-sign",
    );
  });
});

describe("proxy signer", () => {
  test("personalSign POSTs the sites proxy contract and returns the signature", async () => {
    const calls: Array<{ url: string; init: RequestInit }> = [];
    vi.stubGlobal(
      "fetch",
      async (url: string, init: RequestInit) => {
        calls.push({ url, init });
        return new Response(JSON.stringify({ signature: "0xsigned" }), {
          status: 200,
        });
      },
    );
    const signer = makeInAppSigner({
      token: "jwt-token",
      walletAddress: "0xWALLET00000000000000000000000000000000aa",
    });
    const sig = await signer.personalSign("hello");
    expect(sig).toBe("0xsigned");
    expect(calls.length).toBe(1);
    expect(calls[0]?.url).toBe("/internal/thirdweb-sign");
    expect(calls[0]?.init.method).toBe("POST");
    expect(JSON.parse(String(calls[0]?.init.body))).toEqual({
      kind: "message",
      token: "jwt-token",
      from: "0xwallet00000000000000000000000000000000aa",
      message: "hello",
      chainId: 1,
    });
  });

  test("proxy errors surface as ThirdwebError with the server message", async () => {
    vi.stubGlobal(
      "fetch",
      async () =>
        new Response(
          JSON.stringify({
            error:
              "Sign-in is not fully configured on this server (THIRDWEB_SECRET_KEY unset).",
          }),
          { status: 503 },
        ),
    );
    const signer = makeInAppSigner({ token: "t", walletAddress: "0xabc" });
    await expect(signer.personalSign("m")).rejects.toMatchObject({
      name: "ThirdwebError",
      status: 503,
      message: expect.stringContaining("THIRDWEB_SECRET_KEY"),
    });
    await expect(signer.personalSign("m")).rejects.toBeInstanceOf(ThirdwebError);
  });
});

describe("parseAuthResult (social redirect return)", () => {
  test("flat {token, walletAddress}", () => {
    expect(
      parseAuthResult(JSON.stringify({ token: "t1", walletAddress: "0x1" })),
    ).toEqual({ token: "t1", walletAddress: "0x1" });
  });

  test("SDK storedToken shape (jwtToken + authDetails)", () => {
    expect(
      parseAuthResult(
        JSON.stringify({
          storedToken: {
            jwtToken: "t2",
            authDetails: { walletAddress: "0x2" },
          },
        }),
      ),
    ).toEqual({ token: "t2", walletAddress: "0x2" });
  });

  test("cookieString fallback", () => {
    expect(
      parseAuthResult(
        JSON.stringify({
          storedToken: {
            cookieString: "t3",
            authDetails: { walletAddress: "0x3" },
          },
        }),
      ),
    ).toEqual({ token: "t3", walletAddress: "0x3" });
  });

  test("garbage returns null", () => {
    expect(parseAuthResult("not-json")).toBeNull();
    expect(parseAuthResult(JSON.stringify({ token: "only-token" }))).toBeNull();
    expect(parseAuthResult(JSON.stringify(null))).toBeNull();
  });
});
