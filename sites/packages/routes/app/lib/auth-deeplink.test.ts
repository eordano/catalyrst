import { describe, expect, it, vi } from "vitest";

import type { AuthIdentity } from "@data/lib/auth/types";

import {
  authApiUrlFor,
  completeDeepLinkSignIn,
  getAuthRequestId,
  getExplorerDeeplink,
  getSigninDeeplink,
  isBridgeOnlyEnabled,
  isDeepLinkFlowEnabled,
  isValidUuidV4,
  resolveAuthApiUrl,
  startsRequestRecovery,
  type DeepLinkSignInDeps,
} from "./auth-deeplink";

const REQUEST_ID = "123e4567-e89b-42d3-a456-426614174000";
const IDENTITY_ID = "9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b";
const SENDER = "0xabc0000000000000000000000000000000000001";

describe("deep-link request flags", () => {
  it("reads flow, bridgeOnly and authRequestId from the query and never recovers a request for the handoff", () => {
    for (const [query, enabled] of [
      ["flow=deeplink", true],
      ["flow=DeepLink", true],
      ["flow=polling", false],
      ["", false],
    ] as const) {
      expect(isDeepLinkFlowEnabled(new URLSearchParams(query)), query).toBe(enabled);
    }
    for (const [query, enabled] of [
      ["bridgeOnly", true],
      ["bridgeOnly=true", true],
      ["bridgeOnly=TRUE", true],
      ["bridgeOnly=false", false],
      ["", false],
    ] as const) {
      expect(isBridgeOnlyEnabled(new URLSearchParams(query)), query).toBe(enabled);
    }
    expect(getAuthRequestId(new URLSearchParams("authRequestId=abc-123"))).toBe("abc-123");
    expect(getAuthRequestId(new URLSearchParams(""))).toBeNull();

    expect(startsRequestRecovery({ isDeepLink: true, valid: true })).toBe(false);
    expect(startsRequestRecovery({ isDeepLink: false, valid: true })).toBe(true);
    expect(startsRequestRecovery({ isDeepLink: false, valid: false })).toBe(false);
  });

  it("only accepts canonical UUID v4 route ids", () => {
    expect(isValidUuidV4(REQUEST_ID)).toBe(true);
    expect(isValidUuidV4(REQUEST_ID.toUpperCase())).toBe(true);
    expect(isValidUuidV4("123e4567-e89b-12d3-a456-426614174000")).toBe(false);
    expect(isValidUuidV4("not-a-uuid")).toBe(false);
    expect(isValidUuidV4("")).toBe(false);
  });
});

describe("client deep links", () => {
  it("builds the signin deep link from the identity, the route id and bridgeOnly, percent-encoded and without dclenv", () => {
    expect(getSigninDeeplink(undefined, IDENTITY_ID, false, REQUEST_ID)).toBe(
      `decentraland://open?signin=${IDENTITY_ID}&authRequestId=${REQUEST_ID}`,
    );
    expect(getSigninDeeplink(undefined, IDENTITY_ID, true, REQUEST_ID)).toBe(
      `decentraland://open?signin=${IDENTITY_ID}&bridgeOnly=true&authRequestId=${REQUEST_ID}`,
    );
    expect(getSigninDeeplink(undefined, IDENTITY_ID)).not.toContain("dclenv");
    expect(getSigninDeeplink(undefined, "a b&c")).toBe("decentraland://open?signin=a+b%26c");
  });

  it("builds the bare explorer deep link with only the flags it was given", () => {
    expect(getExplorerDeeplink()).toBe("decentraland://");
    expect(getExplorerDeeplink(undefined, true)).toBe("decentraland://?bridgeOnly=true");
    expect(getExplorerDeeplink(undefined, false, "req-1")).toBe(
      "decentraland://?authRequestId=req-1",
    );
    expect(getExplorerDeeplink("dcl-creator-hub://", true, "req-1")).toBe(
      "dcl-creator-hub://?bridgeOnly=true&authRequestId=req-1",
    );
  });
});

describe("authApiUrlFor", () => {
  it("targets the auth-api fanout host, keeps the same-origin prefix on dev origins and lets AUTH_API_URL override", () => {
    expect(authApiUrlFor("catalyst.example.com")).toBe("https://auth-api.catalyst.example.com");
    expect(authApiUrlFor("Example.COM")).toBe("https://auth-api.example.com");
    for (const host of ["localhost", "localhost:5173", "dev.catalyst.example.com:5173", ""]) {
      expect(authApiUrlFor(host), host).toBe("/auth-api");
    }
    expect(resolveAuthApiUrl("https://auth.example.com/", "app.example.com")).toBe(
      "https://auth.example.com",
    );
    expect(resolveAuthApiUrl("  ", "catalyst.example.com")).toBe("https://auth-api.catalyst.example.com");
    expect(resolveAuthApiUrl(undefined, "localhost:5173")).toBe("/auth-api");
  });
});

function identityFor(signer: string): AuthIdentity {
  return {
    signer,
    ephemeral: {
      address: "0x2222222222222222222222222222222222222222",
      privateKey: "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    },
    expiration: new Date(Date.now() + 60_000).toISOString(),
    authChain: [
      { type: "SIGNER", payload: signer, signature: "" },
      { type: "ECDSA_EPHEMERAL", payload: "Decentraland Login", signature: "0xsig" },
    ],
  };
}

function deps(overrides: Partial<DeepLinkSignInDeps> = {}): DeepLinkSignInDeps {
  return {
    connect: vi.fn(async () => SENDER),
    cachedIdentity: () => null,
    createIdentity: vi.fn(async (who: string) => identityFor(who)),
    postIdentity: vi.fn(async () => ({ identityId: IDENTITY_ID })),
    isUserRejection: (err: unknown) => (err as { code?: number })?.code === 4001,
    ...overrides,
  };
}

describe("completeDeepLinkSignIn", () => {
  it("re-posts the identity already minted for this handoff, otherwise mints and posts a fresh one", async () => {
    const cached = identityFor(SENDER);
    const reused = deps({ cachedIdentity: () => cached });
    expect((await completeDeepLinkSignIn(reused)).kind).toBe("ok");
    expect(reused.createIdentity).not.toHaveBeenCalled();
    expect(reused.postIdentity).toHaveBeenCalledWith(cached);

    const fresh = deps();
    const outcome = await completeDeepLinkSignIn(fresh);
    expect(outcome.kind).toBe("ok");
    if (outcome.kind !== "ok") return;
    expect(outcome.identityId).toBe(IDENTITY_ID);
    expect(fresh.createIdentity).toHaveBeenCalledWith(SENDER);
    expect(fresh.postIdentity).toHaveBeenCalledWith(outcome.identity);
  });

  it("maps a wallet rejection at connect or at the ephemeral signature to the denied outcome", async () => {
    const rejection = { code: 4001, message: "User rejected the request" };
    const atConnect = deps({
      connect: async () => {
        throw rejection;
      },
    });
    expect(await completeDeepLinkSignIn(atConnect)).toEqual({ kind: "denied" });
    expect(atConnect.postIdentity).not.toHaveBeenCalled();

    const atSignature = deps({
      createIdentity: vi.fn(async () => {
        throw rejection;
      }),
    });
    expect(await completeDeepLinkSignIn(atSignature)).toEqual({ kind: "denied" });
    expect(atSignature.postIdentity).not.toHaveBeenCalled();
  });

  it("reports other wallet failures as wallet errors", async () => {
    const d = deps({
      connect: async () => {
        throw new Error("No browser wallet found.");
      },
    });
    expect(await completeDeepLinkSignIn(d)).toEqual({
      kind: "wallet_error",
      message: "No browser wallet found.",
    });
  });

  it("keeps the minted identity when the post fails so a retry can re-post it", async () => {
    const d = deps({
      postIdentity: vi.fn(async () => {
        throw new Error("Failed to create identity");
      }),
    });
    const outcome = await completeDeepLinkSignIn(d);
    expect(outcome.kind).toBe("post_error");
    if (outcome.kind !== "post_error") return;
    expect(outcome.message).toBe("Failed to create identity");
    expect(outcome.identity.signer).toBe(SENDER);
  });
});
