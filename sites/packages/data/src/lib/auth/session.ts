import { check } from "@ui/validate";

import { AuthIdentitySchema } from "../persisted-schemas";

import { isIdentityExpired } from "./expiry";
import type { AuthIdentity } from "./types";
import { clearWalletCookie, serializeWalletCookie } from "./wallet-cookie";

function syncWalletCookie(identity: AuthIdentity | null): void {
  if (typeof document === "undefined") return;
  document.cookie = identity
    ? serializeWalletCookie(identity.signer, identity.expiration)
    : clearWalletCookie();
}

export const SESSION_STORAGE_KEY = "dcl:auth:identity:v1";

/**
 * Only the read and the parse sit inside the try. `check` throws in dev on
 * purpose, and a catch wide enough to cover it would turn that throw back into
 * "signed out" -- detection wired in and never firing.
 *
 * The hand guard below stays: `check` returns the ORIGINAL value when it
 * rejects outside dev, so removing it would hand a drifted blob to the signing
 * path instead of treating it as absent.
 */
function readStorage(): AuthIdentity | null {
  if (typeof window === "undefined") return null;
  let parsed: unknown;
  try {
    const raw = window.localStorage.getItem(SESSION_STORAGE_KEY);
    if (!raw) return null;
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  const identity = check(AuthIdentitySchema, parsed, "persisted/auth-identity");
  if (
    !identity?.signer ||
    !identity?.ephemeral?.privateKey ||
    !Array.isArray(identity.authChain)
  ) {
    return null;
  }
  if (isIdentityExpired(identity)) {
    try {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
    } catch {
    }
    return null;
  }
  return identity;
}

function writeStorage(identity: AuthIdentity | null): void {
  if (typeof window === "undefined") return;
  try {
    if (identity) {
      window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(identity));
    } else {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
    }
  } catch {
  }
  syncWalletCookie(identity);
}


let current: AuthIdentity | null = null;
let hydrated = false;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function ensureHydrated() {
  if (hydrated || typeof window === "undefined") return;
  current = readStorage();
  hydrated = true;
}

export function getIdentity(): AuthIdentity | null {
  ensureHydrated();
  return current;
}

export function setIdentity(identity: AuthIdentity | null): void {
  current = identity;
  hydrated = true;
  writeStorage(identity);
  emit();
}

export function clearIdentity(): void {
  setIdentity(null);
}

export function subscribe(listener: () => void): () => void {
  ensureHydrated();
  listeners.add(listener);
  const onStorage = (e: StorageEvent) => {
    if (e.key === SESSION_STORAGE_KEY) {
      current = readStorage();
      syncWalletCookie(current);
      emit();
    }
  };
  if (typeof window !== "undefined" && listeners.size === 1) {
    window.addEventListener("storage", onStorage);
  }
  return () => {
    listeners.delete(listener);
    if (typeof window !== "undefined" && listeners.size === 0) {
      window.removeEventListener("storage", onStorage);
    }
  };
}

export function getServerSnapshot(): AuthIdentity | null {
  return null;
}
