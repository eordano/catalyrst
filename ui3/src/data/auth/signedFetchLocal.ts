
import { CatalystError } from "../catalyst/client";
import {
  IDENTITY_STORAGE_KEY,
  isExpired,
  parseStoredIdentity,
  type StoredAuthIdentity,
} from "./engineLogin";
import { signAuthChain } from "./signerCore";

/**
 * One key, one rule. The stored identity is judged only by
 * `parseStoredIdentity`; a second guard here would be a second rule over the
 * same blob, and only one of them could carry the validation.
 */
export function loadStoredIdentity(now = Date.now()): StoredAuthIdentity | null {
  let raw: string | null = null;
  try {
    raw =
      typeof localStorage !== "undefined"
        ? localStorage.getItem(IDENTITY_STORAGE_KEY)
        : null;
  } catch {
    return null;
  }
  return parseStoredIdentity(raw, now);
}

export type SignedFetchHeaderOpts = {
  metadata?: string;
  now?: () => number;
  identity?: StoredAuthIdentity;
};

export async function signedFetchHeaders(
  method: string,
  path: string,
  opts: SignedFetchHeaderOpts = {},
): Promise<Record<string, string>> {
  const now = opts.now ?? Date.now;
  const identity = opts.identity ?? loadStoredIdentity(now());
  if (!identity || isExpired(identity, now())) {
    throw new CatalystError("Sign in with a wallet to create a community", path, 401);
  }

  const ts = String(now());
  const metadata = opts.metadata ?? "{}";
  const { headers } = await signAuthChain(
    identity.ephemeralIdentity.privateKey as `0x${string}`,
    identity.authChain,
    method,
    path,
    ts,
    metadata,
  );
  return headers;
}
