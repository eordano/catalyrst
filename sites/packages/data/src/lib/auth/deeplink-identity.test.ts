import { afterEach, describe, expect, it, vi } from "vitest";

import { createIdentityFromPrivateKey } from "./identity";
import { postIdentityHandoff, toHandoffIdentity } from "./deeplink-identity";

const SIGNER_KEY =
  "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318" as const;

afterEach(() => {
  vi.unstubAllGlobals();
});

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("toHandoffIdentity", () => {
  it("emits the persisted shape the client reads back, with a public key", async () => {
    const identity = await createIdentityFromPrivateKey(SIGNER_KEY);
    const handoff = toHandoffIdentity(identity);
    expect(Object.keys(handoff).sort()).toEqual(["authChain", "ephemeralIdentity", "expiration"]);
    expect(handoff.ephemeralIdentity.address).toBe(identity.ephemeral.address);
    expect(handoff.ephemeralIdentity.privateKey).toBe(identity.ephemeral.privateKey);
    expect(handoff.ephemeralIdentity.publicKey).toMatch(/^0x04[0-9a-f]{128}$/);
    expect(handoff.expiration).toBe(identity.expiration);
    expect(handoff.authChain).toEqual(identity.authChain);
    expect("signer" in handoff).toBe(false);
  });
});

describe("postIdentityHandoff", () => {
  it("posts the identity with signed-fetch headers and returns the identity id", async () => {
    const identity = await createIdentityFromPrivateKey(SIGNER_KEY);
    const fetchMock = vi.fn(async () =>
      jsonResponse(201, {
        identityId: "9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b",
        expiration: "2026-09-04T13:00:00.000Z",
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const result = await postIdentityHandoff(identity, "https://auth-api.catalyst.example.com");

    expect(result).toEqual({
      identityId: "9b2c1a1e-4c3d-4f5e-8a6b-7c8d9e0f1a2b",
      expiration: "2026-09-04T13:00:00.000Z",
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("https://auth-api.catalyst.example.com/identities");
    expect(init.method).toBe("POST");

    const headers = new Headers(init.headers);
    expect(headers.get("content-type")).toBe("application/json");
    expect(headers.get("x-identity-timestamp")).toMatch(/^\d+$/);
    expect(headers.get("x-identity-metadata")).toBe("{}");
    const link0 = JSON.parse(headers.get("x-identity-auth-chain-0") ?? "null");
    const link1 = JSON.parse(headers.get("x-identity-auth-chain-1") ?? "null");
    const link2 = JSON.parse(headers.get("x-identity-auth-chain-2") ?? "null");
    expect(link0).toEqual({ type: "SIGNER", payload: identity.signer, signature: "" });
    expect(link1.type).toBe("ECDSA_EPHEMERAL");
    expect(link2.type).toBe("ECDSA_SIGNED_ENTITY");
    expect(link2.payload).toBe(
      `post:/identities:${headers.get("x-identity-timestamp")}:{}`.toLowerCase(),
    );

    const body = JSON.parse(String(init.body));
    expect(body.isMobile).toBe(false);
    expect(body.identity).toEqual(toHandoffIdentity(identity));
  });

  it("surfaces the server error text when the post is refused", async () => {
    const identity = await createIdentityFromPrivateKey(SIGNER_KEY);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(403, { error: "Request sender does not match identity owner" })),
    );
    await expect(postIdentityHandoff(identity, "/auth-api")).rejects.toThrow(
      "Request sender does not match identity owner",
    );
  });

  it("rejects a success response that carries no identity id", async () => {
    const identity = await createIdentityFromPrivateKey(SIGNER_KEY);
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(201, { ok: true })));
    await expect(postIdentityHandoff(identity, "/auth-api")).rejects.toThrow(
      "The sign-in server returned no identity id.",
    );
  });
});
