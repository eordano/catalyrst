import { afterEach, describe, expect, test } from "vitest";
import { recoverMessageAddress } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

import { CatalystError } from "../catalyst/client";
import { IDENTITY_STORAGE_KEY, type StoredAuthIdentity } from "./engineLogin";
import { loadStoredIdentity, signedFetchHeaders } from "./signedFetchLocal";

async function makeStoredIdentity(expirationMs = Date.now() + 86_400_000): Promise<{
  stored: StoredAuthIdentity;
  ephemeralAddress: string;
}> {
  const root = privateKeyToAccount(generatePrivateKey());
  const ephKey = generatePrivateKey();
  const eph = privateKeyToAccount(ephKey);
  const expiration = new Date(expirationMs).toISOString();
  const message = [
    "Decentraland Login",
    `Ephemeral address: ${eph.address.toLowerCase()}`,
    `Expiration: ${expiration}`,
  ].join("\n");
  const signature = await root.signMessage({ message });
  return {
    stored: {
      ephemeralIdentity: { address: eph.address.toLowerCase(), privateKey: ephKey },
      expiration,
      authChain: [
        { type: "SIGNER", payload: root.address.toLowerCase(), signature: "" },
        { type: "ECDSA_EPHEMERAL", payload: message, signature },
      ],
    },
    ephemeralAddress: eph.address,
  };
}

/** Fails the test when the call resolves, so "it threw" stays part of the claim. */
async function rejectionOf(p: Promise<unknown>): Promise<unknown> {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("expected the call to reject");
}

afterEach(() => {
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
});

describe("signedFetchHeaders", () => {
  test("appends a verifiable ECDSA_SIGNED_ENTITY link over the canonical", async () => {
    const { stored, ephemeralAddress } = await makeStoredIdentity();
    const ts = 1_751_500_000_000;
    const headers = await signedFetchHeaders("post", "/v1/communities", {
      identity: stored,
      now: () => ts,
    });

    expect(headers["x-identity-timestamp"]).toBe(String(ts));
    expect(headers["x-identity-metadata"]).toBe("{}");
    const link0 = JSON.parse(headers["x-identity-auth-chain-0"] ?? "{}") as { type?: string };
    const link1 = JSON.parse(headers["x-identity-auth-chain-1"] ?? "{}") as { type?: string };
    expect(link0.type).toBe("SIGNER");
    expect(link1.type).toBe("ECDSA_EPHEMERAL");

    const entity = JSON.parse(headers["x-identity-auth-chain-2"] ?? "{}") as {
      type?: string;
      payload?: string;
      signature?: `0x${string}`;
    };
    expect(entity.type).toBe("ECDSA_SIGNED_ENTITY");
    expect(entity.payload).toBe(`post:/v1/communities:${ts}:{}`);

    const recovered = await recoverMessageAddress({
      message: entity.payload ?? "",
      signature: entity.signature ?? "0x",
    });
    expect(recovered.toLowerCase()).toBe(ephemeralAddress.toLowerCase());
  });

  test("lowercases mixed-case canonical inputs (server compares lowercased)", async () => {
    const { stored } = await makeStoredIdentity();
    const headers = await signedFetchHeaders("POST", "/v1/Communities", {
      identity: stored,
      now: () => 42,
    });
    const entity = JSON.parse(headers["x-identity-auth-chain-2"] ?? "{}") as { payload?: string };
    expect(entity.payload).toBe("post:/v1/communities:42:{}");
  });

  test("no stored identity -> 401 CatalystError", async () => {
    localStorage.removeItem(IDENTITY_STORAGE_KEY);
    const err = await rejectionOf(signedFetchHeaders("post", "/v1/communities"));
    expect(err).toBeInstanceOf(CatalystError);
    expect((err as CatalystError).status).toBe(401);
  });

  test("expired stored identity is rejected at load and at signing", async () => {
    const { stored } = await makeStoredIdentity(Date.now() - 1000);
    localStorage.setItem(IDENTITY_STORAGE_KEY, JSON.stringify(stored));
    expect(loadStoredIdentity()).toBeNull();
    const err = await rejectionOf(signedFetchHeaders("post", "/v1/communities"));
    expect((err as CatalystError).status).toBe(401);
  });

  test("valid persisted identity loads from localStorage and signs", async () => {
    const { stored } = await makeStoredIdentity();
    localStorage.setItem(IDENTITY_STORAGE_KEY, JSON.stringify(stored));
    expect(loadStoredIdentity()?.ephemeralIdentity.address).toBe(
      stored.ephemeralIdentity.address,
    );
    const headers = await signedFetchHeaders("get", "/v1/communities");
    expect(headers["x-identity-auth-chain-2"]).toBeTruthy();
  });
});
