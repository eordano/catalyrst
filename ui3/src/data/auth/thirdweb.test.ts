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

// Every case below is drift the shipped code accepted. Two guards shipped, and
// neither looked at a type: `return parsed as T` in twFetch checked nothing at
// all, and the sign proxy's `!parsed.signature` only asked whether something
// truthy was there. Each case asserts BOTH halves -- the schema rejects it, and
// the guard that shipped waved it through -- because a case the old code
// already caught would prove nothing.

/** twFetch's `return parsed as T`: a cast, so every payload got through. */
const oldTwFetchGuard = (_v: unknown) => true;

/** proxySign's `!res.ok || !parsed.signature`, on a 200. */
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

  /** A key a JSON body simply does not carry, not one set to undefined. */
  const without = (o: Record<string, unknown>, key: string) => {
    const copy = { ...o };
    delete copy[key];
    return copy;
  };

  const authCases: [string, unknown, boolean][] = [
    ["what thirdweb sends today", authResult(), true],
    ["a field thirdweb added since", authResult({ profiles: [] }), true],
    // The token stops being the bearer string and becomes a wrapper. The old
    // cast handed it on, `Bearer [object Object]` went out on the next call,
    // and thirdweb answered 401 -- read by the UI as the user's code expiring.
    ["token wrapped in an object", authResult({ token: { jwt: "jwt-123" } }), false],
    // The v2 spelling. The old cast handed back a ThirdwebAuthResult whose
    // walletAddress was undefined, and makeInAppSigner threw on it.
    [
      "walletAddress renamed to address",
      without(authResult({ address: "0xabc" }), "walletAddress"),
      false,
    ],
    ["a declared field stopped arriving", without(authResult(), "type"), false],
  ];
  for (const [name, value, shouldPass] of authCases) {
    test(`auth-complete: ${name}`, () => {
      expect(ThirdwebAuthResultSchema.safeParse(value).success).toBe(shouldPass);
      expect(oldTwFetchGuard(value)).toBe(true);
    });
  }

  const signCases: [string, unknown, boolean][] = [
    ["what the proxy sends today", { signature: "0xsigned" }, true],
    // A split signature is what an enclave change would most plausibly look
    // like. Truthy, so the old guard passed it, and it reached the auth chain
    // stringified as "[object Object]" -- a chain that fails validation on a
    // catalyst server rather than here.
    ["signature split into r/s/v", { signature: { r: "0x1", s: "0x2", v: 27 } }, false],
    ["signature arrived as bytes", { signature: [1, 2, 3] }, false],
  ];
  for (const [name, value, shouldPass] of signCases) {
    test(`sign-proxy: ${name}`, () => {
      expect(SignProxyOkSchema.safeParse(value).success).toBe(shouldPass);
      expect(oldSignatureGuard(value)).toBe(true);
    });
  }

  test("completeEmailLogin reports the boundary rather than failing later", async () => {
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
  });

  test("personalSign reports the boundary rather than signing with an object", async () => {
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

  // The order inside proxySign is load-bearing: validating the success shape
  // before the status test would answer a configuration failure with a
  // complaint about a missing signature.
  test("a 503 stays a ThirdwebError and never becomes a validation error", async () => {
    vi.stubGlobal(
      "fetch",
      async () => new Response(JSON.stringify({ error: "secret key unset" }), { status: 503 }),
    );
    const signer = makeInAppSigner({ token: "t", walletAddress: "0xabc" });
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
