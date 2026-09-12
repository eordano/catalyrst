import { privateKeyToAccount } from "viem/accounts";

import { toStoredIdentity, type StoredAuthIdentity } from "@ui/data/auth/identity";

import { signedFetch } from "./signer";
import type { AuthIdentity } from "./types";

export const DEEPLINK_IDENTITY_EXPIRATION_MS = 30 * 24 * 60 * 60 * 1000;

export type HandoffIdentity = StoredAuthIdentity & {
  ephemeralIdentity: { address: string; privateKey: string; publicKey: string };
};

export function toHandoffIdentity(identity: AuthIdentity): HandoffIdentity {
  const stored = toStoredIdentity(identity);
  return {
    ...stored,
    ephemeralIdentity: {
      ...stored.ephemeralIdentity,
      publicKey: privateKeyToAccount(identity.ephemeral.privateKey).publicKey,
    },
  };
}

export type IdentityHandoffResponse = { identityId: string; expiration: string };

export async function postIdentityHandoff(
  identity: AuthIdentity,
  authApiUrl: string,
  opts: { isMobile?: boolean } = {},
): Promise<IdentityHandoffResponse> {
  const res = await signedFetch(identity, `${authApiUrl}/identities`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      identity: toHandoffIdentity(identity),
      isMobile: opts.isMobile ?? false,
    }),
  });
  const body = (await res.json().catch(() => null)) as
    | { identityId?: unknown; expiration?: unknown; error?: unknown }
    | null;
  if (!res.ok) {
    throw new Error(
      typeof body?.error === "string" && body.error ? body.error : "Failed to create identity",
    );
  }
  if (typeof body?.identityId !== "string" || !body.identityId) {
    throw new Error("The sign-in server returned no identity id.");
  }
  return {
    identityId: body.identityId,
    expiration: typeof body.expiration === "string" ? body.expiration : "",
  };
}
