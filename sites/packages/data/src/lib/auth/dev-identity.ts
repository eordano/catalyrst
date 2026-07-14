import { generatePrivateKey } from "viem/accounts";

import { isDevHost } from "./dev-host";
import { createIdentityFromPrivateKey } from "./identity";
import { setIdentity } from "./session";
import type { AuthIdentity } from "./types";

export { isDevHost } from "./dev-host";

export class DevSignInBlockedError extends Error {
  constructor() {
    super(
      "Burner sign-in is dev-only. It is disabled on this (non-local) origin; " +
        "pass { allowNonDev: true } only from trusted headless tooling.",
    );
    this.name = "DevSignInBlockedError";
  }
}

export async function signInWithPrivateKey(
  signerPrivateKey?: `0x${string}`,
  opts: { expirationMs?: number; allowNonDev?: boolean } = {},
): Promise<AuthIdentity> {
  if (!opts.allowNonDev && !isDevHost()) throw new DevSignInBlockedError();
  const { expirationMs } = opts;
  const pk = signerPrivateKey ?? generatePrivateKey();
  const id = await createIdentityFromPrivateKey(pk, { expirationMs });
  if (isDevHost()) {
    try {
      window.localStorage.setItem(DEV_SIGNER_PK_KEY, pk);
    } catch {
    }
  }
  setIdentity(id);
  return id;
}

const DEV_SIGNER_PK_KEY = "dcl:auth:dev-signer-pk:v1";

type DevTypedData = {
  domain: Record<string, unknown>;
  types: Record<string, Array<{ name: string; type: string }>>;
  primaryType: string;
  message: Record<string, unknown>;
};

export function hasDevSigner(): boolean {
  if (!isDevHost()) return false;
  try {
    const pk = window.localStorage.getItem(DEV_SIGNER_PK_KEY);
    return !!pk && pk.startsWith("0x");
  } catch {
    return false;
  }
}

export async function devSignTypedData(
  typedData: DevTypedData,
  from: string,
): Promise<string | null> {
  if (!isDevHost()) return null;
  let pk: string | null = null;
  try {
    pk = window.localStorage.getItem(DEV_SIGNER_PK_KEY);
  } catch {
    return null;
  }
  if (!pk || !pk.startsWith("0x")) return null;
  const { privateKeyToAccount } = await import("viem/accounts");
  const account = privateKeyToAccount(pk as `0x${string}`);
  if (account.address.toLowerCase() !== from.toLowerCase()) return null;
  const types: Record<string, Array<{ name: string; type: string }>> = {
    ...typedData.types,
  };
  delete types.EIP712Domain;
  return account.signTypedData({
    domain: typedData.domain,
    types,
    primaryType: typedData.primaryType,
    message: typedData.message,
  } as unknown as Parameters<typeof account.signTypedData>[0]);
}
