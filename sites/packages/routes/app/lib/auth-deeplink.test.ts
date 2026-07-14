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

describe("deep-link request flags", () => {
  it("enables the flow on flow=deeplink, case-insensitively", () => {
    expect(isDeepLinkFlowEnabled(new URLSearchParams("flow=deeplink"))).toBe(true);
    expect(isDeepLinkFlowEnabled(new URLSearchParams("flow=DeepLink"))).toBe(true);
    expect(isDeepLinkFlowEnabled(new URLSearchParams("flow=polling"))).toBe(false);
    expect(isDeepLinkFlowEnabled(new URLSearchParams(""))).toBe(false);
  });

  it("treats a bare or true bridgeOnly as enabled and anything else as off", () => {
    expect(isBridgeOnlyEnabled(new URLSearchParams("bridgeOnly"))).toBe(true);
    expect(isBridgeOnlyEnabled(new URLSearchParams("bridgeOnly=true"))).toBe(true);
    expect(isBridgeOnlyEnabled(new URLSearchParams("bridgeOnly=TRUE"))).toBe(true);
    expect(isBridgeOnlyEnabled(new URLSearchParams("bridgeOnly=false"))).toBe(false);
    expect(isBridgeOnlyEnabled(new URLSearchParams(""))).toBe(false);
  });

  it("returns the raw authRequestId query value", () => {
    expect(getAuthRequestId(new URLSearchParams("authRequestId=abc-123"))).toBe("abc-123");
    expect(getAuthRequestId(new URLSearchParams(""))).toBeNull();
  });

  it("never starts the request recovery for the deep-link handoff", () => {
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
  it("builds the signin deep link with the identity and the route id", () => {
    expect(getSigninDeeplink(undefined, IDENTITY_ID, false, REQUEST_ID)).toBe(
      `decentraland://open?signin=${IDENTITY_ID}&authRequestId=${REQUEST_ID}`,
    );
  });

  it("adds bridgeOnly=true when the page was opened with the flag", () => {
    expect(getSigninDeeplink(undefined, IDENTITY_ID, true, REQUEST_ID)).toBe(
      `decentraland://open?signin=${IDENTITY_ID}&bridgeOnly=true&authRequestId=${REQUEST_ID}`,
    );
  });

  it("never carries dclenv for a production deployment", () => {
    expect(getSigninDeeplink(undefined, IDENTITY_ID)).not.toContain("dclenv");
    expect(getExplorerDeeplink()).toBe("decentraland://");
  });

  it("percent-encodes the identity id", () => {
    expect(getSigninDeeplink(undefined, "a b&c")).toBe("decentraland://open?signin=a+b%26c");
  });

  it("builds the bare explorer deep link with only the flags it was given", () => {
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
  it("targets the auth-api fanout host the client also reads from", () => {
    expect(authApiUrlFor("catalyst.example.com")).toBe("https://auth-api.catalyst.example.com");
    expect(authApiUrlFor("Example.COM")).toBe("https://auth-api.example.com");
  });

  it("keeps the same-origin prefix on dev origins", () => {
    expect(authApiUrlFor("localhost")).toBe("/auth-api");
    expect(authApiUrlFor("localhost:5173")).toBe("/auth-api");
    expect(authApiUrlFor("dev.catalyst.example.com:5173")).toBe("/auth-api");
    expect(authApiUrlFor("")).toBe("/auth-api");
  });

  it("lets AUTH_API_URL override the derived host, trailing slashes stripped", () => {
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
  const sender = "0xabc0000000000000000000000000000000000001";
  return {
    connect: vi.fn(async () => sender),
    cachedIdentity: () => null,
    createIdentity: vi.fn(async (who: string) => identityFor(who)),
    postIdentity: vi.fn(async () => ({ identityId: IDENTITY_ID })),
    isUserRejection: (err: unknown) => (err as { code?: number })?.code === 4001,
    ...overrides,
  };
}

describe("completeDeepLinkSignIn", () => {
  it("mints and posts a fresh identity for the connected wallet", async () => {
    const d = deps();
    const outcome = await completeDeepLinkSignIn(d);
    expect(outcome.kind).toBe("ok");
    if (outcome.kind !== "ok") return;
    expect(outcome.identityId).toBe(IDENTITY_ID);
    expect(d.createIdentity).toHaveBeenCalledWith("0xabc0000000000000000000000000000000000001");
    expect(d.postIdentity).toHaveBeenCalledWith(outcome.identity);
  });

  it("reuses the identity minted for this handoff without prompting the wallet again", async () => {
    const cached = identityFor("0xabc0000000000000000000000000000000000001");
    const d = deps({ cachedIdentity: () => cached });
    const outcome = await completeDeepLinkSignIn(d);
    expect(outcome.kind).toBe("ok");
    expect(d.createIdentity).not.toHaveBeenCalled();
    expect(d.postIdentity).toHaveBeenCalledWith(cached);
  });

  it("maps a wallet rejection to the denied outcome", async () => {
    const d = deps({
      connect: async () => {
        throw { code: 4001, message: "User rejected the request" };
      },
    });
    expect(await completeDeepLinkSignIn(d)).toEqual({ kind: "denied" });
    expect(d.postIdentity).not.toHaveBeenCalled();
  });

  it("maps a refused ephemeral signature to the denied outcome", async () => {
    const d = deps({
      createIdentity: vi.fn(async () => {
        throw { code: 4001, message: "User rejected the request" };
      }),
    });
    expect(await completeDeepLinkSignIn(d)).toEqual({ kind: "denied" });
    expect(d.postIdentity).not.toHaveBeenCalled();
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
    expect(outcome.identity.signer).toBe("0xabc0000000000000000000000000000000000001");
  });
});
