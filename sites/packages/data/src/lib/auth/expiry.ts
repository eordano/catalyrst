import type { AuthIdentity } from "./types";

export function isIdentityExpired(
  identity: Pick<AuthIdentity, "expiration">,
  now: number = Date.now(),
): boolean {
  const exp = Date.parse(identity.expiration);
  return !Number.isFinite(exp) || exp <= now;
}
