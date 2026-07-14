import { check } from "@ui/validate";

import { ThirdwebSessionSchema } from "../../persisted-schemas";

export const THIRDWEB_SESSION_KEY = "dcl:auth:thirdweb:v1";

export type ThirdwebSession = {
  token: string;
  address: string;
};

/** The try covers the read and the parse only, so the dev throw from `check`
 *  is not swallowed by the catch that exists to tolerate a bad blob. The
 *  token/address guard stays as the production fallback. */
export function getThirdwebSession(): ThirdwebSession | null {
  if (typeof window === "undefined") return null;
  let parsed: unknown;
  try {
    const raw = window.localStorage.getItem(THIRDWEB_SESSION_KEY);
    if (!raw) return null;
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  const session = check(ThirdwebSessionSchema, parsed, "persisted/thirdweb-session");
  if (!session?.token || !session?.address) return null;
  return session;
}

export function setThirdwebSession(session: ThirdwebSession | null): void {
  if (typeof window === "undefined") return;
  try {
    if (session) {
      window.localStorage.setItem(THIRDWEB_SESSION_KEY, JSON.stringify(session));
    } else {
      window.localStorage.removeItem(THIRDWEB_SESSION_KEY);
    }
  } catch {
  }
}

export function clearThirdwebSession(): void {
  setThirdwebSession(null);
}
