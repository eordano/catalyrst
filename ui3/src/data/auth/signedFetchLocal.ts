
import { CatalystError } from "../catalyst/client";
import {
  IDENTITY_STORAGE_KEY,
  isExpired,
  type StoredAuthIdentity,
} from "./engineLogin";
import { signAuthChain } from "./signerCore";

function isStoredIdentity(v: unknown): v is StoredAuthIdentity {
  const s = v as StoredAuthIdentity;
  return (
    typeof s === "object" &&
    s !== null &&
    typeof s.ephemeralIdentity === "object" &&
    s.ephemeralIdentity !== null &&
    typeof s.ephemeralIdentity.privateKey === "string" &&
    s.ephemeralIdentity.privateKey.length > 0 &&
    typeof s.expiration === "string" &&
    Array.isArray(s.authChain)
  );
}

export function loadStoredIdentity(now = Date.now()): StoredAuthIdentity | null {
  try {
    const raw =
      typeof localStorage !== "undefined"
        ? localStorage.getItem(IDENTITY_STORAGE_KEY)
        : null;
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!isStoredIdentity(parsed) || isExpired(parsed, now)) return null;
    return parsed;
  } catch {
    return null;
  }
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
