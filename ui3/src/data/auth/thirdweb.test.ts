import { afterEach, describe, expect, test, vi } from "vitest";

import {
  ThirdwebError,
  completeEmailLogin,
  makeInAppSigner,
  parseAuthResult,
  thirdwebClientId,
  thirdwebSignProxyUrl,
} from "./thirdweb";
import {
  SignProxyOkSchema,
  ThirdwebAuthResultSchema,
} from "./thirdwebSchema";

afterEach(() => {
  vi.unstubAllGlobals();
  delete (window as { __DCL_PUBLIC__?: unknown }).__DCL_PUBLIC__;
});

describe("config resolution", () => {
  test("the injected SSR global wins over the same-origin defaults", () => {
    expect(thirdwebSignProxyUrl()).toBe("/internal/thirdweb-sign");
    window.__DCL_PUBLIC__ = {
      thirdwebClientId: "cid-from-ssr",
      thirdwebSignProxy: "https://catalyst.example.com/internal/thirdweb-sign",
    };
    expect(thirdwebClientId()).toBe("cid-from-ssr");
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

  test("a proxy 503 surfaces as a ThirdwebError with the server message, never a validation error", async () => {
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
    let err: unknown;
    try {
      await signer.personalSign("m");
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(ThirdwebError);
    expect(String((err as Error).message)).not.toMatch(/validation failed/);
  });
});

const oldTwFetchGuard = (_v: unknown) => true;

const oldSignatureGuard = (v: unknown) =>
  Boolean((v as { signature?: unknown } | null)?.signature);

describe("upstream drift at the thirdweb boundaries", () => {
  const authResult = (over: Record<string, unknown> = {}) => ({
    isNewUser: false,
    token: "jwt-123",
    userId: "u1",
    walletAddress: "0xAbC0000000000000000000000000000000000001",
    type: "email",
    ...over,
  });

  const without = (o: Record<string, unknown>, key: string) => {
    const copy = { ...o };
    delete copy[key];
    return copy;
  };

  test("the schemas catch every drift shape the old guards waved through", () => {
    const authCases: [string, unknown, boolean][] = [
      ["what thirdweb sends today", authResult(), true],
      ["a field thirdweb added since", authResult({ profiles: [] }), true],
      ["token wrapped in an object", authResult({ token: { jwt: "jwt-123" } }), false],
      [
        "walletAddress renamed to address",
        without(authResult({ address: "0xabc" }), "walletAddress"),
        false,
      ],
      ["a declared field stopped arriving", without(authResult(), "type"), false],
    ];
    for (const [name, value, shouldPass] of authCases) {
      expect(ThirdwebAuthResultSchema.safeParse(value).success, `auth-complete: ${name}`).toBe(shouldPass);
      expect(oldTwFetchGuard(value)).toBe(true);
    }
    const signCases: [string, unknown, boolean][] = [
      ["what the proxy sends today", { signature: "0xsigned" }, true],
      ["signature split into r/s/v", { signature: { r: "0x1", s: "0x2", v: 27 } }, false],
      ["signature arrived as bytes", { signature: [1, 2, 3] }, false],
    ];
    for (const [name, value, shouldPass] of signCases) {
      expect(SignProxyOkSchema.safeParse(value).success, `sign-proxy: ${name}`).toBe(shouldPass);
      expect(oldSignatureGuard(value)).toBe(true);
    }
  });

  test("both boundaries report drift by name rather than failing later", async () => {
    window.__DCL_PUBLIC__ = { thirdwebClientId: "cid" };
    vi.stubGlobal(
      "fetch",
      async () =>
        new Response(JSON.stringify(authResult({ token: { jwt: "jwt-123" } })), {
          status: 200,
        }),
    );
    await expect(completeEmailLogin("a@b.com", "123456")).rejects.toThrow(
      /external-http\/thirdweb\/auth-complete/,
    );
    vi.stubGlobal(
      "fetch",
      async () =>
        new Response(JSON.stringify({ signature: { r: "0x1", s: "0x2" } }), {
          status: 200,
        }),
    );
    const signer = makeInAppSigner({ token: "t", walletAddress: "0xabc" });
    await expect(signer.personalSign("m")).rejects.toThrow(
      /external-http\/thirdweb\/sign-proxy/,
    );
  });
});

describe("parseAuthResult (social redirect return)", () => {
  test("reads the flat, storedToken and cookieString shapes, and null for anything else", () => {
    expect(
      parseAuthResult(JSON.stringify({ token: "t1", walletAddress: "0x1" })),
    ).toEqual({ token: "t1", walletAddress: "0x1" });
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
    expect(parseAuthResult("not-json")).toBeNull();
    expect(parseAuthResult(JSON.stringify({ token: "only-token" }))).toBeNull();
    expect(parseAuthResult(JSON.stringify(null))).toBeNull();
  });
});
